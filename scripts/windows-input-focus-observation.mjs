import { chromium } from '@playwright/test';
import { parseArgs } from 'node:util';
import { openSync, closeSync, writeSync, ftruncateSync, realpathSync } from 'node:fs';
import { resolve, join, dirname, sep } from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

export function installInputControl({ binding, lifetimeMs }) {
  if (document.getElementById('projectpane-input-control')) throw new Error('control_already_present');
  const panel = document.createElement('section');
  panel.id = 'projectpane-input-control';
  panel.setAttribute('aria-label', '한글 입력 대조');
  panel.innerHTML = `<style>
    #projectpane-input-control { position:fixed; inset:auto 12px 12px; z-index:2147483647;
      box-sizing:border-box; max-width:768px; padding:12px; border:1px solid #69b0ad;
      border-radius:8px; background:#19232f; color:#dce6ed;
      font:13px 'Segoe UI','Malgun Gothic',sans-serif; }
    #projectpane-input-control h2 { margin:0 0 8px; font-size:20px; }
    #projectpane-input-control p { margin:8px 0; word-break:keep-all; }
    #projectpane-input-control label { display:block; margin:8px 0; }
    #projectpane-input-control input { display:block; box-sizing:border-box; width:100%;
      padding:8px; color:#dce6ed; background:#131c26; border:1px solid #3f5161; border-radius:4px; font:inherit; }
    #projectpane-input-control button { margin:4px 4px 0 0; padding:8px;
      background:#23313f; color:#e3edf2; border:1px solid #3f5161; border-radius:5px; font:inherit; }
    #projectpane-input-control :focus-visible { outline:2px solid #88d5ca; outline-offset:2px; }
  </style>
  <h2>한글 입력 대조</h2>
  <p>일반 A에 ‘한글’을 입력한 뒤, 일반 B를 누르고 Backspace를 한 번 누르세요.</p>
  <p>두 칸은 앱의 주소·필터 처리와 연결되지 않은 일반 입력칸입니다. 입력은 지금부터 기록됩니다.</p>
  <label>일반 A<input id="pp-input-plain-a" aria-label="일반 입력 A"></label>
  <label>일반 B<input id="pp-input-plain-b" aria-label="일반 입력 B"></label>
  <p role="status">기록 준비 중</p>
  <button type="button" data-result="normal">정상 · 저장 후 종료</button>
  <button type="button" data-result="reproduced">같은 문제 · 저장 후 종료</button>
  <button type="button" data-result="cancelled">검사 중단</button>`;
  document.body.append(panel);
  const started = performance.now(), nodes = new WeakMap(), frames = new Set();
  let serial = 0, nodeSerial = 0, inputs = 0, decision = null, decisionTrusted = false;
  const targetName = element => {
    if (!(element instanceof HTMLInputElement)) return null;
    if (element.id === 'pp-input-plain-a') return 'plain-a';
    if (element.id === 'pp-input-plain-b') return 'plain-b';
    const match = element.getAttribute('aria-label')?.match(/^패널\s*([1-4]) (주소|이름 필터)$/);
    return match ? `pane-${match[1]}:${match[2] === '주소' ? 'address' : 'filter'}` : null;
  };
  const snapshot = () => ({
    decision, decisionTrusted, inputEvents: inputs, activeTarget: targetName(document.activeElement),
    fields: Array.from(document.querySelectorAll('input')).filter(input => targetName(input)).map(input => {
      if (!nodes.has(input)) nodes.set(input, ++nodeSerial);
      return { target: targetName(input), node: nodes.get(input), value: input.value.slice(0, 256),
        truncated: input.value.length > 256, selection: [input.selectionStart, input.selectionEnd] };
    }),
  });
  const emit = row => window[binding]({ ...row, time: performance.now(), ui: snapshot() });
  const listener = event => {
    const target = targetName(event.target);
    if (!target) return;
    if (event.type === 'input') inputs++;
    const number = ++serial;
    emit({ serial: number, type: event.type, target, relatedTarget: targetName(event.relatedTarget),
      trusted: event.isTrusted, key: event.key ?? null, code: event.code ?? null,
      keyCode: event.keyCode ?? null, data: typeof event.data === 'string' ? event.data.slice(0, 256) : null,
      inputType: event.inputType ?? null, isComposing: event.isComposing ?? false });
    if (event.type === 'input') {
      const frame = requestAnimationFrame(() => { frames.delete(frame); emit({ type: 'after-input-paint', inputSerial: number }); });
      frames.add(frame);
    }
  };
  const kinds = ['focusin', 'focusout', 'keydown', 'keyup', 'beforeinput', 'input',
    'compositionstart', 'compositionupdate', 'compositionend'];
  kinds.forEach(kind => document.addEventListener(kind, listener, true));
  for (const button of panel.querySelectorAll('button')) button.addEventListener('click', event => {
    decision = button.dataset.result; decisionTrusted = event.isTrusted;
    emit({ type: 'operator-decision', trusted: event.isTrusted });
  });
  const status = panel.querySelector('[role="status"]');
  const timer = setInterval(() => {
    status.textContent = `기록 중 · 입력 ${inputs}회 · 남은 시간 ${Math.max(0, Math.ceil((lifetimeMs - performance.now() + started) / 1000))}초`;
  }, 500);
  window.__projectpaneInputControl = { snapshot, dispose() {
    kinds.forEach(kind => document.removeEventListener(kind, listener, true));
    clearInterval(timer); frames.forEach(cancelAnimationFrame); panel.remove();
    delete window.__projectpaneInputControl;
  } };
}

async function main() {
  const { values } = parseArgs({ options: { port: { type: 'string' }, root: { type: 'string' }, output: { type: 'string' } } });
  if (!/^\d+$/.test(values.port ?? '') || !values.output) throw new Error('arguments_required');
  const output = resolve(values.output), boundary = realpathSync(resolve('artifacts')) + sep;
  if (!realpathSync(dirname(output)).startsWith(boundary)) throw new Error('owned_output_required');
  const summaryFile = openSync(output, 'wx');
  const rawFile = openSync(join(dirname(output), 'input-events.jsonl'), 'wx');
  const report = { status: 'STARTING', native: true, input: 'operator; passive observation', physicalImePassed: false,
    fixtureUsed: false, rawContainsTypedText: true, limit: 6000, rows: 0, dropped: 0, inputEvents: 0,
    compositionEvents: 0, decision: null, decisionTrusted: false, failure: null };
  const save = () => { ftruncateSync(summaryFile, 0); writeSync(summaryFile, JSON.stringify(report, null, 2), 0, 'utf8'); };
  let browser, session, page;
  try {
    const endpoint = `http://127.0.0.1:${values.port}`, readyDeadline = Date.now() + 30000;
    let ready = false;
    while (!ready) {
      try { ready = (await fetch(`${endpoint}/json/version`, { signal: AbortSignal.timeout(1000) })).ok; }
      catch (error) { if (Date.now() >= readyDeadline) throw error; }
      if (!ready && Date.now() >= readyDeadline) throw new Error('cdp_ready_timeout');
      if (!ready) await delay(200);
    }
    browser = await chromium.connectOverCDP(endpoint);
    const context = browser.contexts()[0];
    if (!context) throw new Error('context_missing');
    page = context.pages()[0];
    if (!page) throw new Error('page_missing');
    await page.getByRole('heading', { name: 'ProjectPane', exact: true }).waitFor();
    if (!await page.evaluate(() => window.isTauri === true)) throw new Error('native_required');
    session = await context.newCDPSession(page);
    await session.send('Runtime.enable');
    await session.send('Runtime.addBinding', { name: 'recordProjectPaneFocusProbe' });
    session.on('Runtime.bindingCalled', event => {
      if (event.name !== 'recordProjectPaneFocusProbe') return;
      if (report.rows >= report.limit) { report.dropped++; return; }
      const row = JSON.parse(event.payload);
      writeSync(rawFile, JSON.stringify({ ...row, receivedUtc: new Date().toISOString() }) + '\n');
      report.rows++;
      if (row.type === 'input' && row.trusted) report.inputEvents++;
      if (row.type?.startsWith('composition') && row.trusted) report.compositionEvents++;
    });
    await page.evaluate(installInputControl, { binding: 'recordProjectPaneFocusProbe', lifetimeMs: 600000 });
    report.status = 'READY'; report.startedUtc = new Date().toISOString(); save();
    await page.locator('#projectpane-input-control').screenshot({ path: join(dirname(output), 'control-ready.png') });
    const deadline = Date.now() + 600000;
    while (Date.now() < deadline && !report.dropped && !page.isClosed()) {
      const state = await page.evaluate(() => window.__projectpaneInputControl.snapshot());
      if (state.decision) {
        report.decision = state.decision; report.decisionTrusted = state.decisionTrusted;
        report.status = state.decision === 'cancelled' ? 'NOT_RUN' : 'OPERATOR_REPORTED';
        report.final = state; break;
      }
      save(); await delay(250);
    }
    if (report.status === 'READY') report.status = report.dropped ? 'INCOMPLETE_CAP' : 'INCOMPLETE_TIMEOUT_OR_CLOSE';
  } catch (error) { report.status = 'FAIL'; report.failure = error.message; process.exitCode = 1; }
  finally {
    for (const cleanup of [
      () => page && !page.isClosed() ? page.evaluate(() => window.__projectpaneInputControl?.dispose()) : undefined,
      () => session?.send('Runtime.removeBinding', { name: 'recordProjectPaneFocusProbe' }),
      () => session?.detach(),
      () => browser?.close(),
    ]) {
      try { await cleanup(); }
      catch (error) { report.status = 'FAIL'; report.failure ??= error.message; process.exitCode = 1; }
    }
    report.finishedUtc = new Date().toISOString(); save(); closeSync(summaryFile); closeSync(rawFile);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) await main();
