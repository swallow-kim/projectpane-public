import test from 'node:test';
import assert from 'node:assert/strict';
import { createInputActivity, recordInputActivity } from './windows-ime-activity.mjs';

test('pre-step input in another pane remains visible without claiming IME completion', () => {
  const activity = createInputActivity();
  recordInputActivity(activity, { type: 'input', trusted: true, data: '가', step: null,
    ui: { focus: { pane: 3, target: 'address' } } });
  assert.equal(activity.trustedInputEvents, 1);
  assert.equal(activity.outsideStepInputEvents, 1);
  assert.equal(activity.trustedHangulInputEvents, 1);
  assert.equal(activity.trustedCompositionEvents, 0);
  assert.equal(activity.inputByTarget['3:address'], 1);
  assert.equal(activity.lastInputTarget, '3:address');
  assert.equal('realImeTested' in activity, false);
  assert.equal(JSON.stringify(activity).includes('가'), false);
});

test('explicit event target takes precedence over focus after blur', () => {
  const activity = createInputActivity();
  recordInputActivity(activity, { type: 'input', trusted: true, data: '나', step: 'address:committed-enter',
    inputTarget: { pane: 2, target: 'address' }, ui: { focus: { pane: 3, target: 'filter' } } });
  assert.equal(activity.inputByTarget['2:address'], 1);
  assert.equal(activity.inputByTarget['3:filter'], 0);
  assert.equal(activity.outsideStepInputEvents, 0);
});

test('untrusted input and incomplete composition never become physical proof', () => {
  const activity = createInputActivity();
  recordInputActivity(activity, { type: 'input', trusted: false, data: '한', step: null });
  recordInputActivity(activity, { type: 'compositionstart', trusted: true });
  recordInputActivity(activity, { type: 'compositionend', trusted: false });
  assert.equal(activity.trustedInputEvents, 0);
  assert.equal(activity.untrustedInputEvents, 1);
  assert.equal(activity.trustedHangulInputEvents, 0);
  assert.equal(activity.trustedCompositionEvents, 1);
  assert.equal(activity.eventCount, 3);
  assert.equal('ui01Passed' in activity, false);
});

test('summary stays bounded and never retains payload or unknown target identifiers', () => {
  const activity = createInputActivity();
  for (let i = 0; i < 2000; i++) recordInputActivity(activity, {
    type: 'input', trusted: true, data: 'private-fixture-text', step: null,
    inputTarget: { pane: i + 5, target: 'private-target' },
  });
  assert.equal(activity.trustedInputEvents, 2000);
  assert.equal(activity.unknownTargetInputEvents, 2000);
  assert.equal(Object.keys(activity.inputByTarget).length, 8);
  assert.equal(activity.lastInputTarget, null);
  assert.equal(JSON.stringify(activity).includes('private'), false);
});
