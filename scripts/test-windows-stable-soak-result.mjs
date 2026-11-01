import assert from 'node:assert/strict';
import test from 'node:test';
import { soakResult } from './windows-stable-soak-result.mjs';

const completed = { stopped:false, elapsedMs:28800000, durationMs:28800000, cycles:97, intervalMs:300000 };
const verified = { osMetadata:{ status:'verified', qualification:'windows10' } };
test('unknown OS does not claim a qualified eight-hour PASS', () => {
  for (const environment of [undefined, {}, { osMetadata:{status:'unavailable',reason:'not_queried',qualification:'unverified'} }]) {
    assert.equal(soakResult({...completed, environment}), 'FUNCTIONAL_PASS_OS_UNVERIFIED');
  }
});
test('only explicit verified target qualification can qualify a complete run', () => {
  assert.equal(soakResult({...completed, environment:verified}), 'PASS');
  assert.equal(soakResult({...completed, environment:{osMetadata:{status:'verified',qualification:'another-os'}}}), 'FUNCTIONAL_PASS_OS_UNVERIFIED');
});
test('metadata absence never excuses insufficient duration or cycles', () => {
  assert.equal(soakResult({...completed, elapsedMs:23100000, cycles:78}), 'INSUFFICIENT_CYCLES');
  assert.equal(soakResult({...completed, cycles:96, environment:verified}), 'INSUFFICIENT_CYCLES');
});
test('cancellation remains stopped even after the duration threshold', () => {
  assert.equal(soakResult({...completed, stopped:true, environment:verified}), 'STOPPED_EARLY');
});
