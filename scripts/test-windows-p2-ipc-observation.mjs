import test from 'node:test';
import assert from 'node:assert/strict';
import { checkPhase } from './windows-p2-ipc-observation.mjs';

const call = (command, tabId, seq, generation = 1) => ({ command, tabId, phase: 'begin', seq, generation, pane: 0, callId: seq });
const arriving = [call('start_listing', 'arriving', 576, 2), call('read_listing', 'arriving', 579, 2)];

test('retained tab switch includes the leaving tab lookup before its cancellation', () => {
  const rows = [call('locate_entry', 'leaving', 573), call('cancel_listing', 'leaving', 574), ...arriving];
  const counts = checkPhase(rows, ['arriving'], ['arriving'], ['leaving']);
  assert.equal(counts.locate_entry, 1);
  assert.equal(counts.start_listing, 1);
  assert.equal(counts.cancel_listing, 1);
});

test('leaving tab cannot begin a lookup after cancellation', () => {
  const rows = [call('cancel_listing', 'leaving', 573), call('locate_entry', 'leaving', 574), ...arriving];
  assert.throws(() => checkPhase(rows, ['arriving'], ['arriving'], ['leaving']), /inactive_tab/);
});

test('transition allowance cannot start a new listing in the leaving tab', () => {
  const rows = [call('start_listing', 'leaving', 573), call('cancel_listing', 'leaving', 574), ...arriving];
  assert.throws(() => checkPhase(rows, ['arriving'], ['arriving'], ['leaving']));
});

test('a third inactive tab cannot use the transition allowance', () => {
  const rows = [call('read_listing', 'unrelated', 573), call('cancel_listing', 'leaving', 574), ...arriving];
  assert.throws(() => checkPhase(rows, ['arriving'], ['arriving'], ['leaving']), /inactive_tab/);
});

test('settled inactivity still permits zero reads or lookups', () => {
  assert.throws(() => checkPhase([call('read_listing', 'leaving', 1)], [], [], []), /inactive_tab/);
  assert.deepEqual(checkPhase([], [], [], []), { start_listing: 0, read_listing: 0, cancel_listing: 0, set_listing_view: 0, locate_entry: 0, activate_entry: 0 });
});
