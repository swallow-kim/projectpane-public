// Invented classifier records only; never dispatched to an app or counted as real IME.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { assess, scenarios } from './windows-ime-observe.mjs';
const ui = { address: 'C:\\fixture\\한글', rows: ['한글', 'other-folder'], status: '조회 완료 · 2개', alert: null,
  inputs: { address: { value: 'C:\\fixture\\한글', focused: true, selection: [13, 13] },
    filter: { value: '한글', focused: true, selection: [2, 2] } } };
function cycle(target, key = 'Enter', committed = false) {
  const common = { trusted: true, target, ui };
  const events = [
    { ...common, type: 'compositionstart', serial: 1 },
    { ...common, type: 'compositionupdate', serial: 2, data: '글' },
    { ...common, type: 'keydown', serial: 3, key: committed ? 'Process' : key,
      ctrlKey: key === 'l', isComposing: true, compositionActive: true, keyCode: 229 },
    { ...common, type: 'compositionend', serial: 4, data: '글' },
  ];
  if (committed) events.push({ ...common, type: 'keydown', serial: 5, key, ctrlKey: key === 'l', keyCode: 13 });
  events.push({ type: 'settled', keyEvent: committed ? 5 : 3, ui });
  return events;
}
test('matrix covers both inputs separately', () => {
  assert.equal(scenarios.length, 14); assert.equal(new Set(scenarios).size, 14);
});
for (const target of ['address', 'filter']) {
  test(`${target}: ordered trusted Korean composition is required`, () => {
    const events = cycle(target);
    assert.equal(assess(`${target}:composing-enter`, events, ui, ui).observed, true);
    for (const missing of ['compositionstart', 'compositionupdate', 'compositionend']) {
      assert.equal(assess(`${target}:composing-enter`, events.filter(e => e.type !== missing), ui, ui).observed, false);
    }
    assert.equal(assess(`${target}:composing-enter`, events.map(e => ({ ...e, trusted: false })), ui, ui).observed, false);
    assert.equal(assess(`${target}:composing-enter`, events.map(e => ({ ...e, target: 'other' })), ui, ui).observed, false);
  });
  test(`${target}: cancellation requires Escape and ordered sequence`, () => {
    assert.equal(assess(`${target}:composing-escape`, cycle(target, 'Escape'), ui, ui).observed, true);
    assert.equal(assess(`${target}:composing-escape`, cycle(target, 'Enter'), ui, ui).observed, false);
  });
  for (const [action, key] of [['ctrl-l', 'l'], ['f6', 'F6']]) {
    test(`${target}: composing ${action} keeps focus; committed shortcut needs its destination`, () => {
      const events = cycle(target, key);
      assert.equal(assess(`${target}:composing-${action}`, events, ui, ui).observed, true);
      const selected = structuredClone(ui);
      selected.inputs.address.selection = [0, selected.address.length]; selected.inputs.filter.focused = false;
      events.at(-1).ui = selected;
      assert.equal(assess(`${target}:composing-${action}`, events, ui, selected).observed, false);
      const committed = cycle(target, key, true); committed.at(-1).ui = selected;
      assert.equal(assess(`${target}:committed-${action}`, committed, ui, selected).observed, action === 'ctrl-l');
      assert.equal(assess(`${target}:committed-${action}`, committed.filter(e => !e.type.startsWith('composition')), ui, selected).observed, false);
    });
  }
}
function paneUi(target, focusedPane = 1, value = '', visible = [1, 2, 3, 4]) {
  const state = structuredClone(ui);
  state.panes = [1, 2, 3, 4].map(id => ({ id, visible: visible.includes(id), alert: null,
    inputs: { address: { value: id === 1 ? state.address : value,
      focused: id === focusedPane && (id !== 1 || target === 'address'),
      selection: id === 1 ? [0, state.address.length] : [0, value.length] },
    filter: { value: '', focused: id === focusedPane && id === 1 && target === 'filter', selection: [0, 0] } } }));
  state.focus = { pane: focusedPane, target: focusedPane === 1 ? target : 'address' };
  state.inputs = state.panes[0].inputs;
  return state;
}
for (const target of ['address', 'filter']) {
  function f6(result, initial = paneUi(target), transform = events => events) {
    const events = cycle(target, 'F6', true).map(e => ({ ...e, ui: initial }));
    events.at(-1).ui = result;
    return assess(`${target}:committed-f6`, transform(events), initial, result).observed;
  }
  test(`${target}: committed F6 selects next visible address, including empty`, () => {
    for (const visible of [[1, 2], [1, 2, 3, 4]]) {
      for (const value of ['', 'C:\\other']) {
        assert.equal(f6(paneUi(target, 2, value, visible), paneUi(target, 1, '', visible)), true);
      }
    }
  });
  test(`${target}: committed F6 rejects same, wrong, hidden or unselected destination`, () => {
    assert.equal(f6(paneUi('address', 1)), false);
    assert.equal(f6(paneUi(target, 3)), false);
    assert.equal(f6(paneUi(target, 3, '', [1, 2]), paneUi(target, 1, '', [1, 2])), false);
    const hiddenNext = paneUi(target, 2);
    hiddenNext.panes[1].visible = false;
    assert.equal(f6(hiddenNext), false);
    const unselected = paneUi(target, 2, 'C:\\other');
    unselected.panes[1].inputs.address.selection = [2, 2];
    assert.equal(f6(unselected), false);
    const filterFocus = paneUi(target, 2);
    filterFocus.focus.target = 'filter';
    filterFocus.panes[1].inputs.address.focused = false;
    filterFocus.panes[1].inputs.filter.focused = true;
    assert.equal(f6(filterFocus), false);
    assert.equal(f6(paneUi(target, 2), ui), false);
    const alert = paneUi(target, 2); alert.panes[1].alert = 'error';
    assert.equal(f6(alert), false);
  });
  test(`${target}: committed F6 still requires trusted Korean composition and plain F6`, () => {
    const next = paneUi(target, 2);
    for (const missing of ['compositionstart', 'compositionupdate', 'compositionend']) {
      assert.equal(f6(next, paneUi(target), events => events.filter(e => e.type !== missing)), false);
    }
    for (const change of [{ trusted: false }, { data: 'abc' }, { target: 'other' }]) {
      assert.equal(f6(next, paneUi(target), events => events.map(e => ({ ...e, ...change }))), false);
    }
    assert.equal(f6(next, paneUi(target), events => events.filter(e => e.serial !== 3)), false);
    for (const modifier of ['shiftKey', 'ctrlKey', 'altKey', 'metaKey']) {
      assert.equal(f6(next, paneUi(target), events => events.map(e => e.key === 'F6' ? { ...e, [modifier]: true } : e)), false);
    }
  });
  test(`${target}: Ctrl+L still rejects an empty or another pane's address`, () => {
    for (const result of [paneUi(target, 2), { ...ui, address: '', inputs: { address: { value: '', focused: true, selection: [0, 0] } } }]) {
      const events = cycle(target, 'l', true); events.at(-1).ui = result;
      assert.equal(assess(`${target}:committed-ctrl-l`, events, ui, result).observed, false);
    }
  });
}
test('address committed Enter requires sequence before Enter and known child listing', () => {
  const events = cycle('address', 'Enter', true);
  const final = { ...ui, rows: ['IME-child-proof.txt'], status: '조회 완료 · 1개' };
  assert.equal(assess('address:committed-enter', events, ui, final, ui.address).observed, true);
  assert.equal(assess('address:committed-enter', cycle('address'), ui, final, ui.address).observed, false);
  assert.equal(assess('address:committed-enter', events, ui, ui, ui.address).observed, false);
});
test('filter committed Enter requires applied Korean filter without navigation', () => {
  const events = cycle('filter', 'Enter', true), final = { ...ui, rows: ['한글'], status: '조회 완료 · 1개' };
  assert.equal(assess('filter:committed-enter', events, ui, final).observed, true);
  assert.equal(assess('filter:committed-enter', events, ui, ui).observed, false);
  assert.equal(assess('filter:committed-enter', events, ui, { ...final, address: 'different' }).observed, false);
});
test('filter may apply on commit, never during composition', () => {
  const events = cycle('filter'), changed = { type: 'ui-change', ui: { ...ui, rows: ['한글'] } };
  events.splice(4, 0, changed);
  assert.equal(assess('filter:composing-enter', events, ui, ui).observed, true);
  events.splice(4, 1); events.splice(3, 0, changed);
  assert.equal(assess('filter:composing-enter', events, ui, ui).observed, false);
  events.splice(3, 1); events.splice(2, 0, changed);
  assert.equal(assess('filter:composing-enter', events, ui, ui).observed, false);
});
test('229 Process with Enter code is recognized, never keyboard alone', () => {
  const events = cycle('address'); events[2].key = 'Process'; events[2].code = 'Enter';
  assert.equal(assess('address:composing-enter', events, ui, ui).observed, true);
  assert.equal(assess('address:composing-enter', events.slice(2), ui, ui).observed, false);
});
test('observer never injects input or grants UI-01/P2 completion', () => {
  const source = readFileSync(new URL('./windows-ime-observe.mjs', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /\.keyboard\.|\.mouse\.|\.click\(|\.fill\(|\.focus\(|\.dispatchEvent\(|Input\.(?:dispatch|insert|ime)|\.invoke\(/);
  assert.doesNotMatch(source, /(?:ui01Passed|p2Complete)\s*[:=]\s*true/);
  assert.match(source, /evidence\.observed && command\.physical === true && command\.outcome === true/);
  const helper = readFileSync(new URL('./windows-ime-check.ps1', import.meta.url), 'utf8');
  assert.match(helper, /if \(-not \$IsWindows -or -not \$Interactive\) \{ throw 'windows_and_explicit_interactive_required' \}/);
  assert.ok(helper.indexOf('windows_and_explicit_interactive_required') < helper.indexOf('[Diagnostics.Process]::Start'));
});
