import { chromium, expect } from '@playwright/test';
import { parseArgs } from 'node:util';
import { openSync, closeSync, writeSync, ftruncateSync, statSync, realpathSync, readdirSync } from 'node:fs';
import { resolve, join, dirname } from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

const commands = ['start_listing', 'read_listing', 'cancel_listing', 'set_listing_view', 'locate_entry', 'activate_entry'];
const phases = ['begin', 'success', 'failure'];
const timeout = 30_000, quietMs = 500;
const require = (condition, code) => { if (!condition) throw new Error(code); };
const safe = value => Number.isSafeInteger(value) && value >= 0;
const key = row => `${row.pane}/${row.tabId}/${row.generation}`;

export function inspectSnapshot(snapshot, tabPanes, settled = true) {
  require(snapshot?.enabled && snapshot.capacity === 1024 && snapshot.dropped === 0 && snapshot.observerErrors === 0, 'observer_not_complete');
  require(Array.isArray(snapshot.events) && snapshot.events.length <= 1024 && safe(snapshot.lastSeq), 'invalid_snapshot');
  require(Array.isArray(snapshot.commands) && snapshot.commands.length === commands.length, 'invalid_aggregates');
  const counts = new Map(commands.map(command => [command, { begin: 0, success: 0, failure: 0 }]));
  const pending = new Map(), starts = new Map(), latest = new Map(), lane = new Map(), cancelled = new Set();
  let previous = 0, maxPending = 0;
  for (const row of snapshot.events) {
    require(Object.keys(row).sort().join() === 'callId,command,generation,pane,phase,seq,tabId', 'unexpected_event_fields');
    require(commands.includes(row.command) && phases.includes(row.phase), 'invalid_command_or_phase');
    require(safe(row.seq) && row.seq > previous && (!previous || row.seq === previous + 1) && safe(row.callId) && row.callId > 0, 'invalid_sequence');
    require(safe(row.pane) && row.pane < 4 && tabPanes.get(row.tabId) === row.pane && safe(row.generation) && row.generation > 0, 'invalid_identity');
    previous = row.seq; counts.get(row.command)[row.phase]++;
    if (row.phase === 'begin') {
      require(row.callId === row.seq && !pending.has(row.callId), 'duplicate_begin');
      if (row.command === 'start_listing') {
        require(row.generation > (latest.get(row.pane)?.generation ?? 0), 'generation_not_increasing');
        starts.set(key(row), row); latest.set(row.pane, row);
      } else require(starts.has(key(row)), 'missing_start_identity');
      if (row.command === 'cancel_listing') cancelled.add(key(row));
      else {
        require(!lane.has(row.pane), 'overlapping_pane_requests');
        if (row.command !== 'start_listing') require(key(latest.get(row.pane)) === key(row) && !cancelled.has(key(row)), 'inactive_generation_request');
        lane.set(row.pane, row.callId);
      }
      pending.set(row.callId, row); maxPending = Math.max(maxPending, pending.size);
    } else {
      const begin = pending.get(row.callId);
      require(begin && begin.command === row.command && key(begin) === key(row), 'unpaired_completion');
      pending.delete(row.callId);
      if (row.command !== 'cancel_listing') { require(lane.get(row.pane) === row.callId, 'completion_lane'); lane.delete(row.pane); }
      require(row.phase === 'success', 'transport_failure');
    }
  }
  require(!snapshot.events.length || previous === snapshot.lastSeq, 'missing_tail');
  const seen = new Set();
  for (const row of snapshot.commands) {
    require(commands.includes(row.command) && !seen.has(row.command), 'duplicate_command_aggregate');
    seen.add(row.command);
    for (const phase of phases) require(safe(row[phase]) && row[phase] === counts.get(row.command)[phase], 'aggregate_mismatch');
  }
  if (settled) require(pending.size === 0, 'unfinished_calls');
  return { pending: pending.size, maxPending, calls: snapshot.events.filter(row => row.phase === 'begin').length };
}

export function checkPhase(events, allowed, expectedStarts, expectedCancels) {
  const begins = events.filter(row => row.phase === 'begin');
  const retired = new Set();
  const count = (command, tabId) => begins.filter(row => row.command === command && row.tabId === tabId).length;
  for (const row of begins) {
    require(['start_listing', 'read_listing', 'cancel_listing', 'locate_entry'].includes(row.command), 'unexpected_scenario_command');
    if (row.command === 'cancel_listing') {
      require(expectedCancels.includes(row.tabId), 'unexpected_cancel');
      retired.add(key(row));
    } else {
      const leavingBeforeCancel = expectedCancels.includes(row.tabId) && !retired.has(key(row)) &&
        ['read_listing', 'locate_entry'].includes(row.command);
      require(allowed.includes(row.tabId) || leavingBeforeCancel, 'inactive_tab_or_hidden_pane_ipc');
    }
    if (row.command === 'start_listing') require(expectedStarts.includes(row.tabId), 'unexpected_start');
  }
  for (const tabId of expectedStarts) require(count('start_listing', tabId) === 1 && count('read_listing', tabId) > 0, 'missing_start_or_read');
  for (const tabId of expectedCancels) require(count('cancel_listing', tabId) > 0, 'missing_explicit_cancel');
  return Object.fromEntries(commands.map(command => [command, begins.filter(row => row.command === command).length]));
}

export async function runObservation({ port, root, output }) {
  require(Number.isInteger(port) && port > 0 && port < 65536 && root && output, 'invalid_arguments');
  const folders = Array.from({ length: 4 }, (_, n) => join(realpathSync(root), String(n)));
  const names = folders.map(folder => { require(statSync(folder).isDirectory(), 'fixture_directory'); const entries = readdirSync(folder); require(entries.length === 2500, 'fixture_count'); return new Set(entries); });
  const destination = resolve(output); require(statSync(dirname(destination)).isDirectory(), 'output_parent');
  const fd = openSync(destination, 'wx');
  const report = { schemaVersion: 1, status: 'FAIL', metric: 'normal UI frontend invoke boundary', native: true,
    realImeTested: false, input: 'CDP DOM and keyboard; no physical IME', directBenchmark: 'NOT_RUN',
    transitionContract: 'leaving generation may read/locate before cancellation begins; after cancellation or in settled inactive phases no new calls',
    rawContainsFixturePaths: true, quietMs, phases: [], snapshot: null, failure: null };
  let browser, page, bridge, stage = 'connect';
  const save = () => { ftruncateSync(fd, 0); writeSync(fd, `${JSON.stringify(report, null, 2)}\n`, 0, 'utf8'); };
  const bounded = async promise => {
    let timer;
    try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('cdp_timeout')), timeout); })]); }
    finally { clearTimeout(timer); }
  };
  const pane = n => page.locator(`section[aria-label="패널 ${n + 1}"]`);
  const address = n => pane(n).getByRole('textbox', { name: `패널 ${n + 1} 주소`, exact: true });
  const snapshot = () => bounded(page.evaluate(() => window.__PROJECTPANE_IPC_OBSERVATION__.snapshot()));
  const ui = () => bounded(page.evaluate(() => [...document.querySelectorAll('section.pane')].map((node, pane) => ({ pane, visible: !node.hidden,
    active: node.querySelector('[role="tab"][aria-selected="true"]')?.id.slice(4),
    status: node.querySelector('footer [role="status"]')?.textContent, busy: node.querySelector('[role="listbox"]')?.getAttribute('aria-busy') === 'true',
    tabs: [...node.querySelectorAll('[role="tab"]')].map(tab => ({ id: tab.id.slice(4), source: tab.dataset.source })) }))));
  const tabPanes = state => new Map(state.flatMap(p => p.tabs.map(tab => [tab.id, p.pane])));
  const ready = async (n, folder) => {
    await expect(pane(n).locator('footer [role="status"]')).toHaveText('조회 완료 · 2,500개', { timeout });
    await expect(address(n)).toHaveValue(folders[folder]);
    await expect(pane(n).getByRole('tab', { selected: true })).toHaveAttribute('title', folders[folder]);
    await expect(pane(n).locator('.file-row[data-entry-id]').first()).toBeVisible();
    require(names[folder].has(await pane(n).locator('.file-row .file-name').first().getAttribute('title')), 'fixture_row_mismatch');
  };
  const tail = async n => {
    await pane(n).getByRole('listbox').focus(); await bounded(page.keyboard.press('End'));
    const last = pane(n).locator('[aria-posinset="2500"] .file-name');
    await expect(last).toBeVisible(); await expect(last).toHaveAttribute('title', 'fixture-2500.txt');
  };
  const navigate = async (n, folder) => { await address(n).fill(folders[folder]); await address(n).press('Enter'); await ready(n, folder); await tail(n); };
  const select = async (n, id) => { await pane(n).locator(`[role="tab"][id="tab-${id}"]`).click(); await expect(pane(n).getByRole('tab', { selected: true })).toHaveAttribute('id', `tab-${id}`); };
  const layout = async count => { await page.getByRole('button', { name: `${count}패널`, exact: true }).click(); await expect(page.locator('section.pane:not([hidden])')).toHaveCount(count); };
  const settle = async () => {
    const deadline = Date.now() + timeout; let stableSince = Date.now(), signature = '';
    while (Date.now() < deadline) {
      const state = await ui(), current = await snapshot(), stats = inspectSnapshot(current, tabPanes(state), false);
      const next = JSON.stringify([state, current.lastSeq]);
      if (stats.pending || state.some(p => p.visible && p.busy) || next !== signature) { stableSince = Date.now(); signature = next; }
      if (!stats.pending && Date.now() - stableSince >= quietMs) return { state, current, stats };
      await delay(50);
    }
    throw new Error('settle_timeout');
  };
  const phase = async (id, allowed, starts, cancels, action) => {
    stage = id; const fromSeq = (await snapshot()).lastSeq;
    await action(); const { state, current, stats } = await settle();
    const begins = checkPhase(current.events.filter(row => row.seq > fromSeq), allowed, starts, cancels);
    report.snapshot = current; report.phases.push({ id, status: 'PASS', fromSeq, toSeq: current.lastSeq, allowed, starts, cancels, begins, stats, panes: state }); save();
  };
  try {
    save(); const deadline = Date.now() + timeout;
    while (!browser) {
      try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: Math.max(1, deadline - Date.now()) }); }
      catch (error) { if (Date.now() >= deadline) throw error; await delay(100); }
    }
    await expect.poll(() => browser.contexts().flatMap(context => context.pages()).length, { timeout }).toBe(1);
    page = browser.contexts().flatMap(context => context.pages())[0]; page.setDefaultTimeout(timeout);
    await expect(page.getByRole('heading', { name: 'ProjectPane', exact: true })).toBeVisible();
    bridge = await bounded(page.evaluateHandle(() => ({ object: window.__TAURI_INTERNALS__, global: Object.getOwnPropertyDescriptor(window, '__TAURI_INTERNALS__'), invoke: Object.getOwnPropertyDescriptor(window.__TAURI_INTERNALS__, 'invoke') })));
    report.bridgeBefore = await bounded(bridge.evaluate(saved => ({ native: window.isTauri === true, callable: typeof saved.invoke?.value === 'function', writable: saved.invoke?.writable, configurable: saved.invoke?.configurable, enumerable: saved.invoke?.enumerable,
      global: { writable: saved.global?.writable ?? null, configurable: saved.global?.configurable, enumerable: saved.global?.enumerable } })));
    require(report.bridgeBefore.native && report.bridgeBefore.callable, 'native_bridge_missing');
    const initial = await ui();
    require(initial.length === 4 && initial.every(p => p.visible && p.tabs.length === 2 && p.tabs.find(t => t.id === p.active)?.source === 'synthetic'), 'expected_fresh_synthetic_app');
    await bounded(page.evaluate(() => {
      const observer = window.__PROJECTPANE_IPC_OBSERVATION__;
      if (!observer || !Object.isFrozen(observer) || observer.snapshot().enabled) throw new Error('observer_not_default_off');
      observer.enable();
    }));
    await phase('synthetic_zero_ipc', [], [], [], async () => { await delay(quietMs); });
    const real = initial.map(p => p.tabs.find(t => t.source === 'real').id), synthetic = initial[0].active;
    await phase('four_real_folders', real, real, [], async () => { for (let n = 0; n < 4; n++) { await select(n, real[n]); await navigate(n, n); } });
    await phase('one_pane_cancels_hidden', [], [], real.slice(1), () => layout(1));
    await phase('hidden_panes_idle', [], [], [], async () => { await delay(1000); });
    await phase('create_30_tabs', [], [], [real[0]], async () => {
      for (let count = 3; count <= 30; count++) { await pane(0).getByRole('button', { name: '새 탭', exact: true }).click(); await expect(pane(0).getByRole('tab')).toHaveCount(count); }
    });
    const tabs = (await ui())[0].tabs; require(tabs.length === 30 && tabs.length <= 32, 'tab_bound');
    for (let index = 2; index < 30; index++) {
      await phase(`populate_tab_${index}`, [tabs[index].id], [tabs[index].id], index > 2 ? [tabs[index - 1].id] : [], async () => { await select(0, tabs[index].id); await navigate(0, index % 4); });
    }
    await phase('all_29_native_tabs_inactive', [], [], [tabs[29].id], async () => {
      await select(0, synthetic); await expect(pane(0).locator('footer [role="status"]')).toHaveText('합성 데이터 · 100,000개'); await delay(1000);
    });
    await phase('native_tab_reactivation', [real[0]], [real[0]], [], async () => { await select(0, real[0]); await ready(0, 0); await tail(0); });
    await phase('active_native_path_change', [real[0]], [real[0]], [real[0]], () => navigate(0, 1));
    await phase('retained_native_tab_switch', [tabs[29].id], [tabs[29].id], [real[0]], async () => { await select(0, tabs[29].id); await ready(0, 1); await tail(0); });
    await phase('four_pane_reactivation', [tabs[29].id, ...real.slice(1)], real.slice(1), [], async () => {
      await layout(4); for (let n = 1; n < 4; n++) { await ready(n, n); await tail(n); }
    });
    await phase('hide_again', [], [], real.slice(1), () => layout(1));
    await phase('final_inactive_quiet', [], [], [tabs[29].id], async () => { await select(0, synthetic); await delay(1000); await expect(pane(0).getByRole('tab')).toHaveCount(30); });
    report.status = 'PASS';
  } catch (error) { report.failure = { stage, message: error instanceof Error ? error.message : String(error) }; }
  finally {
    try {
      if (page) {
        report.snapshot = await snapshot();
        if (report.status === 'PASS') inspectSnapshot(report.snapshot, tabPanes(await ui()));
        if (bridge) report.bridgeAfter = await bounded(bridge.evaluate(saved => {
          const same = (a, b) => !!a && !!b && ['value', 'get', 'set', 'writable', 'configurable', 'enumerable'].every(k => a[k] === b[k]);
          const invoke = Object.getOwnPropertyDescriptor(window.__TAURI_INTERNALS__, 'invoke');
          const global = Object.getOwnPropertyDescriptor(window, '__TAURI_INTERNALS__');
          return { sameObject: saved.object === window.__TAURI_INTERNALS__, sameGlobalDescriptor: same(saved.global, global),
            global: { writable: global?.writable ?? null, configurable: global?.configurable, enumerable: global?.enumerable },
            sameInvokeDescriptor: same(saved.invoke, invoke), writable: invoke?.writable, configurable: invoke?.configurable, enumerable: invoke?.enumerable };
        }));
        require(report.bridgeAfter?.sameObject && report.bridgeAfter.sameGlobalDescriptor && report.bridgeAfter.sameInvokeDescriptor, 'bridge_changed');
      }
    } catch (error) { report.status = 'FAIL'; report.failure ??= { stage: 'final_verification', message: String(error) }; }
    finally {
      if (page) try { report.disabled = await bounded(page.evaluate(() => { window.__PROJECTPANE_IPC_OBSERVATION__?.disable(); const s = window.__PROJECTPANE_IPC_OBSERVATION__?.snapshot(); return !!s && !s.enabled && s.events.length === 0; })); require(report.disabled, 'disable_failed'); }
      catch (error) { report.status = 'FAIL'; report.failure ??= { stage: 'disable', message: String(error) }; }
      if (browser) try { await bounded(browser.close()); } catch (error) { report.status = 'FAIL'; report.failure ??= { stage: 'disconnect', message: String(error) }; }
      try { save(); } finally { closeSync(fd); }
    }
  }
  return { status: report.status, phases: report.phases.length, calls: report.snapshot?.commands.reduce((sum, row) => sum + row.begin, 0), failure: report.failure };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const { values } = parseArgs({ options: { port: { type: 'string' }, root: { type: 'string' }, output: { type: 'string' } } });
    const result = await runObservation({ port: Number(values.port), root: values.root, output: values.output });
    process.stdout.write(`${JSON.stringify(result)}\n`); if (result.status !== 'PASS') process.exitCode = 1;
  } catch (error) { process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`); process.exitCode = 1; }
}
