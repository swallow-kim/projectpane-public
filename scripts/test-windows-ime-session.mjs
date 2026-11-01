import test from 'node:test';
import assert from 'node:assert/strict';
import { requestedRange, completionStatus, persistFinalReport } from './windows-ime-session.mjs';

test('resuming selects only the requested suffix without granting a full pass', () => {
  const range = requestedRange('10', 14);
  assert.deepEqual(range, { start: 10, end: 14 });
  const steps = Array.from({ length: 14 }, (_, i) => ({ status: i < 9 ? 'NOT_RUN' : 'OBSERVED' }));
  assert.equal(completionStatus(steps, range), 'OBSERVED_SUBSET');
  assert.equal(completionStatus(steps, requestedRange('1', 14)), 'INCOMPLETE');
  steps[13].status = 'INCOMPLETE';
  assert.equal(completionStatus(steps, range), 'INCOMPLETE');
  assert.equal(completionStatus(steps.map(() => ({ status: 'OBSERVED' })), requestedRange('1', 14)), 'OBSERVED');
});
test('invalid resume values cannot silently skip scenarios', () => {
  for (const value of ['0', '15', '-1', '1.5', '10x', '', '01']) assert.throws(() => requestedRange(value, 14), /start_at/);
});
test('final report is durable before state publication and retains publication failure', async () => {
  const saved = [];
  const report = { status: 'INCOMPLETE', ui01Passed: false, p2Complete: false, steps: [{ startedAt: 'time' }] };
  await persistFinalReport('fixture', report, async () => {
    assert.equal(saved.length, 1);
    throw Error('EPERM');
  }, (_, json) => saved.push(JSON.parse(json)));
  assert.equal(saved.length, 2);
  assert.equal(saved[1].status, 'FAIL');
  assert.equal(saved[1].reason, 'EPERM');
  assert.deepEqual(saved[1].steps, report.steps);
  assert.equal(saved[1].p2Complete, false);
});
