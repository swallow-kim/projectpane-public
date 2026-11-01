import test from 'node:test';
import assert from 'node:assert/strict';
import { assess } from './windows-ime-observe.mjs';

const ui = { address: 'C:\\fixture\\한글', rows: ['한글', 'other-folder'], status: '조회 완료 · 2개', alert: null,
  inputs: { address: { value: 'C:\\fixture\\한글', focused: true, selection: [13, 13] },
    filter: { value: '', focused: false, selection: [0, 0] } } };
const policy = { allowCorrelatedUntrustedEnd: true };
function cycle() {
  const common = { target: 'address', trusted: true, ui };
  return [
    { ...common, serial: 1, type: 'compositionstart' },
    { ...common, serial: 2, type: 'compositionupdate', data: '글' },
    { ...common, serial: 3, type: 'keydown', key: 'Process', code: 'Enter', keyCode: 229, isComposing: true },
    { ...common, serial: 4, type: 'beforeinput', data: '글', inputType: 'insertCompositionText', isComposing: true },
    { ...common, serial: 5, type: 'input', data: '글', inputType: 'insertCompositionText', isComposing: true },
    { ...common, serial: 6, type: 'compositionend', data: '글', trusted: false },
    { type: 'settled', keyEvent: 3, ui },
  ];
}
test('untrusted end remains explicit and needs correlated native input plus opt-in', () => {
  assert.equal(assess('address:composing-enter', cycle(), ui, ui).observed, false);
  const result = assess('address:composing-enter', cycle(), ui, ui, undefined, policy);
  assert.equal(result.observed, true);
  assert.equal(result.endTrusted, false);
  assert.equal(result.endCorroborated, true);
});
test('missing, unrelated, mismatched, and synthetic sequence evidence stays rejected', () => {
  const mutations = [
    rows => rows.filter(r => r.type !== 'input'),
    rows => rows.filter(r => r.type !== 'beforeinput'),
    rows => rows.map(r => r.type === 'input' ? { ...r, data: '다른 값' } : r),
    rows => rows.map(r => r.type === 'input' ? { ...r, trusted: false } : r),
    rows => rows.map(r => r.type === 'input' ? { ...r, isComposing: false } : r),
    rows => rows.map(r => r.type === 'input' ? { ...r, target: 'filter' } : r),
    rows => rows.map(r => r.type === 'input' ? { ...r, inputType: 'insertFromPaste' } : r),
    rows => rows.map(r => ({ ...r, trusted: false })),
  ];
  for (const mutate of mutations) assert.equal(assess('address:composing-enter', mutate(cycle()), ui, ui, undefined, policy).observed, false);
});
test('committed shortcut uses ordered end rather than stale shadow composition flag', () => {
  const selected = structuredClone(ui);
  selected.inputs.address.selection = [0, selected.address.length];
  const events = cycle().filter(r => r.type !== 'settled');
  events[2] = { ...events[2], key: 'Process', code: 'KeyR' };
  events.push({ type: 'keydown', serial: 7, target: 'address', trusted: true, key: 'l', code: 'KeyL',
    ctrlKey: true, keyCode: 76, isComposing: false, compositionActive: true, ui });
  events.push({ type: 'settled', keyEvent: 7, ui: selected });
  const result = assess('address:committed-ctrl-l', events, ui, selected, undefined, policy);
  assert.equal(result.observed, true);
  assert.equal(result.keySerial, 7);
});

test('an end from a later composition cannot complete an interrupted earlier cycle', () => {
  const events = cycle();
  events.splice(3, 0, { ...events[0], serial: 30 });
  assert.equal(assess('address:composing-enter', events, ui, ui, undefined, policy).observed, false);
});
