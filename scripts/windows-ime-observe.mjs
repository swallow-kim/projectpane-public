// Passive native WebView2 sidecar. Only windows-ime-check.ps1 should launch this.
// CDP is a transport for observation, never proof that input was physical.
import { chromium } from '@playwright/test';
import { parseArgs } from 'node:util';
import { readFileSync, appendFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { createInputActivity, recordInputActivity } from './windows-ime-activity.mjs';
import { assess } from './windows-ime-assess.mjs';
import { writeStateJson } from './windows-ime-state.mjs';
import { requestedRange, completionStatus, persistFinalReport } from './windows-ime-session.mjs';
export { assess };

export const actions = ['composing-enter', 'committed-enter', 'composing-ctrl-l',
  'composing-f6', 'composing-escape', 'committed-ctrl-l', 'committed-f6'];
export const scenarios = ['address', 'filter'].flatMap(target => actions.map(action => `${target}:${action}`));
const normal = value => value?.replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();


async function main() {
  const { values: v } = parseArgs({ options: {
    port: { type: 'string' }, root: { type: 'string' }, token: { type: 'string' },
    output: { type: 'string' }, 'step-timeout': { type: 'string', default: '180' },
    'start-at': { type: 'string', default: '1' },
  } });
  if (!v.root || !v.output || !/^[a-f0-9]{32}$/.test(v.token ?? '') || !/^\d+$/.test(v.port ?? '')) throw Error('arguments');
  if (readFileSync(join(v.root, '.ime-owner'), 'utf8') !== v.token) throw Error('owner_marker');
  const timeout = Number(v['step-timeout']) * 1000;
  if (!(timeout >= 1000 && timeout <= 180000)) throw Error('step_timeout');
  const range = requestedRange(v['start-at'], scenarios.length);
  const report = { status: 'NOT_RUN', ui01Passed: false, p2Complete: false, realImeTested: false,
    requestedRange: range, publicationRetries: 0,
    compositionEndPolicy: 'Untrusted end requires correlated trusted composition input and physical operator attestation; original trust flag retained.',
    activity: createInputActivity(),
    provenance: 'Passive CDP; isTrusted is not proof of physical input. Physical provenance is operator-attested.',
    nativeCurrentPath: 'Not exposed by current DOM; address is a draft, rows/status are rendered native listing evidence.',
    steps: scenarios.map((id, i) => ({ id, target: id.split(':')[0], action: id.split(':')[1], status: 'NOT_RUN',
      ...(i < range.start - 1 ? { reason: 'outside_requested_range' } : {}) })) };
  let browser, session, active, serial = range.start - 1, lastCommand = 0, done = false;
  const records = [];
  async function publish(extra = {}) {
    const path = join(v.output, 'observer-state.json');
    const result = await writeStateJson(path, { ...report, active: active?.id, ...extra });
    report.publicationRetries += result.retries;
  }
  function record(event) {
    const entry = { ...event, step: active?.id ?? null, receivedAt: new Date().toISOString() };
    appendFileSync(join(v.output, 'events.jsonl'), `${JSON.stringify(entry)}\n`);
    recordInputActivity(report.activity, entry);
    if (active) records.push(entry);
  }
  try {
    const endpoint = `http://127.0.0.1:${Number(v.port)}`;
    const deadline = Date.now() + 30000;
    while (true) {
      try { const r = await fetch(`${endpoint}/json/version`, { signal: AbortSignal.timeout(1000) }); if (r.ok) break; } catch {}
      if (Date.now() > deadline) throw Error('cdp_ready_timeout');
      await delay(200);
    }
    browser = await chromium.connectOverCDP(endpoint, { timeout: 10000 });
    const context = browser.contexts()[0];
    if (!context) throw Error('native_context_missing');
    const page = context.pages()[0] ?? await context.waitForEvent('page', { timeout: 10000 });
    await page.getByRole('heading', { name: 'ProjectPane', exact: true }).waitFor({ timeout: 15000 });
    if (!await page.evaluate(() => window.isTauri === true)) throw Error('native_required');
    session = await context.newCDPSession(page);
    await session.send('Runtime.enable');
    const binding = `ime_${v.token}`;
    await session.send('Runtime.addBinding', { name: binding });
    session.on('Runtime.bindingCalled', event => {
      if (event.name !== binding) return;
      try { record(JSON.parse(event.payload)); } catch { report.observationError = 'event_decode'; }
    });
    // No input, focus, selection, navigation, app IPC, or OS settings are changed.
    await page.evaluate(({ binding }) => {
      const panel = () => document.querySelector('section[aria-label="패널 1"], section[aria-label="패널1"]');
      const address = () => panel()?.querySelector('input[aria-label="패널 1 주소"], input[aria-label="패널1주소"]');
      const filter = () => panel()?.querySelector('input[aria-label="패널 1 이름 필터"], input[aria-label="패널1이름필터"]');
      const snapshot = () => {
        const p = panel(), a = address(), f = filter();
        const describe = el => el ? { value: el.value, focused: document.activeElement === el, selection: [el.selectionStart, el.selectionEnd] } : null;
        let focus = null;
        const panes = Array.from(document.querySelectorAll('section.pane'), pane => {
          const label = pane.getAttribute('aria-label');
          const id = Number(label?.match(/^패널\s*(\d+)$/)?.[1]);
          const inputs = { address: pane.querySelector(`input[aria-label="${label} 주소"], input[aria-label="${label}주소"]`),
            filter: pane.querySelector(`input[aria-label="${label} 이름 필터"], input[aria-label="${label}이름필터"]`) };
          if (pane.contains(document.activeElement)) focus = { pane: id,
            target: Object.keys(inputs).find(key => inputs[key] === document.activeElement) ?? 'other' };
          return { id, visible: !pane.hidden && pane.getClientRects().length > 0,
            inputs: { address: describe(inputs.address), filter: describe(inputs.filter) },
            alert: pane.querySelector('[role="alert"]')?.textContent ?? null };
        });
        return { panes, focus, address: a?.value ?? null, inputs: { address: describe(a), filter: describe(f) }, focused: document.activeElement === a,
          selection: a ? [a.selectionStart, a.selectionEnd] : null,
          rows: Array.from(p?.querySelectorAll('[role="option"]') ?? [], e => e.textContent),
          status: p?.querySelector('footer [role="status"]')?.textContent ?? null,
          alert: p?.querySelector('[role="alert"]')?.textContent ?? null,
          nativeCurrentPath: p?.getAttribute('data-current-path') ?? null };
      };
      const composing = { address: false, filter: false };
      let serial = 0;
      const timers = new Set();
      const emit = entry => window[binding](JSON.stringify(entry));
      const listener = event => {
        const target = event.target === address() ? 'address' : event.target === filter() ? 'filter' : null;
        const inputPane = event.target instanceof HTMLInputElement ? event.target.closest('section.pane') : null;
        const paneLabel = inputPane?.getAttribute('aria-label');
        const paneId = Number(paneLabel?.match(/^패널\s*(\d+)$/)?.[1]);
        const inputLabel = event.target instanceof HTMLInputElement ? event.target.getAttribute('aria-label') : null;
        const inputKind = inputLabel === `${paneLabel} 주소` ? 'address'
          : inputLabel === `${paneLabel} 이름 필터` ? 'filter' : null;
        const inputTarget = paneId >= 1 && paneId <= 4 && inputKind ? { pane: paneId, target: inputKind } : null;
        if (target && event.type === 'compositionstart' && event.isTrusted) composing[target] = true;
        const n = ++serial;
        emit({ serial: n, type: event.type, trusted: event.isTrusted, target, inputTarget,
          inputType: event.inputType ?? null,
          key: event.key ?? '', code: event.code ?? '', keyCode: event.keyCode ?? null,
          isComposing: event.isComposing ?? false, compositionActive: target ? composing[target] : false,
          ctrlKey: event.ctrlKey ?? false, shiftKey: event.shiftKey ?? false,
          altKey: event.altKey ?? false, metaKey: event.metaKey ?? false,
          data: event.data ?? null, time: performance.now(), ui: snapshot() });
        if (target && event.type === 'compositionend') composing[target] = false;
        if (event.type === 'keydown') {
          const timer = setTimeout(() => { timers.delete(timer); emit({ type: 'settled', keyEvent: n, ui: snapshot() }); }, 750);
          timers.add(timer);
        }
      };
      const kinds = ['compositionstart', 'compositionupdate', 'compositionend', 'keydown', 'keyup', 'beforeinput', 'input', 'focusin', 'focusout'];
      kinds.forEach(kind => document.addEventListener(kind, listener, true));
      const mutations = new MutationObserver(() => emit({ type: 'ui-change', time: performance.now(), ui: snapshot() }));
      for (const pane of document.querySelectorAll('section.pane')) {
        mutations.observe(pane, { subtree: true, childList: true, characterData: true, attributes: true });
      }
      window[binding + '_observer'] = { snapshot, dispose() {
        kinds.forEach(kind => document.removeEventListener(kind, listener, true));
        mutations.disconnect();
        timers.forEach(clearTimeout); delete window[binding + '_observer'];
      } };
    }, { binding });
    const snapshot = () => page.evaluate(b => window[b + '_observer'].snapshot(), binding);
    const initial = await snapshot();
    if (normal(initial.address) !== normal(join(v.root, 'fixture'))) throw Error('fixture_address_mismatch');
    report.inputAvailability = { address: Boolean(initial.inputs.address), filter: Boolean(initial.inputs.filter) };
    await publish({ ready: true, initial });
    let publishedInputEvents = 0;
    const sessionDeadline = Date.now() + ((range.end - range.start + 1) * 2 + 1) * timeout;
    while (!done) {
      if (Date.now() > sessionDeadline) { report.reason = 'session_timeout'; break; }
      if (page.isClosed()) throw Error('app_closed');
      if (active && Date.now() > active.deadline) { report.reason = 'step_timeout'; break; }
      const inputEvents = report.activity.trustedInputEvents + report.activity.untrustedInputEvents;
      if (inputEvents !== publishedInputEvents) {
        publishedInputEvents = inputEvents;
        await publish({ ready: true });
      }
      let command;
      try { command = JSON.parse(readFileSync(join(v.output, 'command.json'), 'utf8')); } catch {}
      if (command?.token === v.token && Number.isInteger(command.seq) && command.seq > lastCommand) {
        lastCommand = command.seq;
        if (command.action === 'cancel') { report.reason = 'operator_cancelled'; break; }
        if (command.action === 'start' && !active && scenarios[serial] === command.id) {
          records.length = 0;
          active = { id: command.id, before: await snapshot(), startedAt: new Date().toISOString(), deadline: Date.now() + timeout };
          await publish({ ready: true });
        } else if (command.action === 'complete' && active) {
          await delay(850);
          const evidence = assess(active.id, records, active.before, await snapshot(), join(v.root, 'fixture', '한글'),
            { allowCorrelatedUntrustedEnd: process.platform === 'win32' && command.physical === true });
          const step = report.steps[serial];
          Object.assign(step, { status: evidence.observed && command.physical === true && command.outcome === true ? 'OBSERVED' : 'INCOMPLETE',
            startedAt: active.startedAt, completedAt: new Date().toISOString(), physicalInputAttested: command.physical === true, expectedOutcomeAttested: command.outcome === true, evidence });
          active = null; serial++;
          if (serial === scenarios.length) {
            report.status = completionStatus(report.steps, range);
            done = true;
          }
          await publish({ ready: true });
        }
      }
      await delay(100);
    }
    if (active) {
      Object.assign(report.steps[serial], { status: 'NOT_RUN', reason: report.reason,
        startedAt: active.startedAt, stoppedAt: new Date().toISOString(), physicalInputAttested: false,
        evidence: assess(active.id, records, active.before, await snapshot(), join(v.root, 'fixture', '한글')) });
    }
    for (const step of report.steps) {
      if (step.status === 'NOT_RUN' && !step.reason) step.reason = report.reason ?? 'not_started';
    }
    await page.evaluate(b => window[b + '_observer']?.dispose(), binding);
    await session.send('Runtime.removeBinding', { name: binding });
  } catch (error) {
    report.status = 'FAIL'; report.reason = error.message;
    if (active) Object.assign(report.steps[serial], { status: 'INCOMPLETE', reason: 'observation_interrupted',
      startedAt: active.startedAt, stoppedAt: new Date().toISOString(), physicalInputAttested: false,
      recordedEvents: records.length });
  }
  finally {
    if (session) await session.detach().catch(() => {});
    if (browser) await browser.close().catch(() => {});
    report.realImeTested = report.steps.some(s => s.physicalInputAttested && s.evidence?.compositionCycle && s.evidence?.korean);
    await persistFinalReport(v.output, report, publish);
    if (report.status === 'FAIL') process.exitCode = 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) await main();
