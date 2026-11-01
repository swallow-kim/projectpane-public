import test from 'node:test';
import assert from 'node:assert/strict';
import { makePlan, classifySample, summarize, installTiming, bounded } from './windows-p2-observation-core.mjs';

function evidence(plan = makePlan()[0], duration = 8) {
  const identity = { sampleId: plan.sampleId, documentId: `doc-${plan.sampleId}`, timeOrigin: 1000 };
  const input = { ...identity, type: 'keydown', key: plan.key, trusted: true, target: plan.sampleId, startTime: 100.25 };
  return { ...identity, supported: ['first-input', 'event'], armedAt: 90, readAt: 120,
    inputs: [input], entries: [{ ...identity, entryType: 'first-input', name: 'keydown', target: plan.sampleId,
      startTime: 100.25, duration, interactionId: 1, processingStart: 101, processingEnd: 102 }] };
}

test('90 deterministic unique scenarios, exactly 30 per group', () => {
  const plan = makePlan();
  assert.equal(plan.length, 90);
  assert.equal(new Set(plan.map(row => row.sampleId)).size, 90);
  assert.deepEqual(plan, makePlan());
  for (const group of new Set(plan.map(row => row.group))) assert.equal(plan.filter(row => row.group === group).length, 30);
});

test('an actual rounded zero first-input is valid when event entries are absent', () => {
  assert.equal(classifySample(makePlan()[0], evidence(undefined, 0)).durationMs, 0);
  assert.equal(classifySample(makePlan()[0], evidence(undefined, 0)).status, 'PASS');
});

test('missing first-input is not zero, even with a nearby event entry 62.7ms away', () => {
  const raw = evidence();
  raw.entries[0].entryType = 'event';
  raw.entries[0].startTime += 62.7;
  const result = classifySample(makePlan()[0], raw);
  assert.equal(result.durationMs, null);
  assert.equal(result.status, 'UNVERIFIED');
});

test('identity, target, key, trust and timestamp mismatches cannot borrow evidence', () => {
  const mutations = [
    raw => { raw.entries[0].sampleId = 'other'; },
    raw => { raw.entries[0].documentId = 'other'; },
    raw => { raw.entries[0].timeOrigin++; },
    raw => { raw.entries[0].target = 'other'; },
    raw => { raw.inputs[0].key = 'x'; },
    raw => { raw.inputs[0].trusted = false; },
    raw => { raw.entries[0].startTime += 8; },
    raw => { raw.armedAt = 101; },
    raw => { raw.readAt = 99; },
    raw => { raw.inputs.unshift({ ...raw.inputs[0], sampleId: null, type: 'click' }); },
    raw => { raw.inputs.push({ ...raw.inputs[0] }); },
    raw => { raw.entries.push({ ...raw.entries[0] }); },
    raw => { raw.entries[0].duration = null; },
    raw => { raw.entries[0].interactionId = 0; },
  ];
  for (const mutate of mutations) {
    const raw = evidence(); mutate(raw);
    assert.notEqual(classifySample(makePlan()[0], raw).status, 'PASS', mutate.toString());
  }
});

test('candidate 100ms fails on measured duration rather than inserting delay', () => {
  assert.equal(classifySample(makePlan()[0], evidence(undefined, 104)).status, 'FAIL');
});

test('Enter implicit submit click is retained; unrelated or pre-key clicks are rejected', () => {
  const plan = makePlan()[60], raw = evidence(plan);
  raw.inputs.push({ ...raw.inputs[0], type: 'click', key: null, target: null, startTime: 103, detail: 0, keyboardSubmit: true });
  assert.equal(classifySample(plan, raw).status, 'PASS');
  raw.inputs[1].keyboardSubmit = false;
  assert.equal(classifySample(plan, raw).status, 'UNVERIFIED');
  raw.inputs[1].keyboardSubmit = true; raw.inputs[1].startTime = 80;
  assert.equal(classifySample(plan, raw).status, 'UNVERIFIED');
});

test('complete groups use nearest-rank p95, incomplete groups cannot pass or expose a partial p95', () => {
  const rows = makePlan().map((plan, index) => ({ ...plan, raw: evidence(plan, index % 30 === 29 ? 104 : 8), ready: true }));
  let result = summarize(rows);
  assert.equal(result.status, 'PASS');
  assert.equal(result.groups[0].p95Ms, 8);
  rows[0].raw.entries = [];
  result = summarize(rows);
  assert.equal(result.status, 'UNVERIFIED');
  assert.equal(result.groups[0].p95Ms, null);
  assert.equal(result.groups[0].missing, 1);
  assert.equal(result.rows.length, 90);
  assert.equal(summarize(rows.slice(1)).rows.length, 90);
});

test('duplicate sample/document identities and readiness failures invalidate evidence', () => {
  const rows = makePlan().map(plan => ({ ...plan, raw: evidence(plan), ready: true }));
  rows[1].raw.documentId = rows[0].raw.documentId;
  assert.equal(summarize(rows).status, 'UNVERIFIED');
  rows[1] = { ...makePlan()[1], raw: evidence(makePlan()[1]), ready: false };
  assert.equal(summarize(rows).status, 'UNVERIFIED');
  assert.equal(summarize([...rows, rows[0]]).status, 'UNVERIFIED');
});

test('bounded reads reject a never-resolving operation', async () => {
  await assert.rejects(bounded(new Promise(() => {}), 'fake-read', 15), /timeout:fake-read/);
});

test('isolated headless Chromium: synthetic setup leaves first-input available after reload', {
  skip: process.env.P2_HEADLESS_PROOF !== '1', timeout: 45_000,
}, async () => {
  const { chromium } = await import('@playwright/test');
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.addInitScript(installTiming);
    await page.route('https://p2-proof.invalid/', route => route.fulfill({ contentType: 'text/html', body:
      '<form onsubmit="event.preventDefault()"><input id="input"><button id="target">Ready</button></form><script>target.onkeydown=()=>target.textContent="Changed"</script>' }));
    await page.goto('https://p2-proof.invalid/');
    const documents = new Set();
    const results = [];
    for (const plan of [makePlan()[0], makePlan()[30], makePlan()[60]]) {
      const cleanup = await page.evaluate(() => window.__p2Timing.stop());
      assert.equal(cleanup.disconnectedObservers, 2);
      assert.equal(cleanup.removedListeners, 5);
      await page.reload();
      await page.evaluate(plan => {
        const target = document.querySelector(plan.key === 'Enter' ? '#input' : '#target');
        target.click(); target.dispatchEvent(new KeyboardEvent('keydown', { key: 'x', bubbles: true }));
        target.focus(); window.__p2Timing.arm(plan.sampleId, target);
      }, plan);
      await page.keyboard.press(plan.key);
      await page.waitForFunction(() => window.__p2Timing.entries.some(entry => entry.entryType === 'first-input'), null, { timeout: 10_000 });
      const raw = await page.evaluate(() => window.__p2Timing.snapshot());
      assert.equal(documents.has(raw.documentId), false); documents.add(raw.documentId);
      const result = classifySample(plan, raw);
      assert.notEqual(result.status, 'UNVERIFIED', JSON.stringify(raw));
      assert.equal(raw.inputs.filter(input => input.type === 'keydown').length, 1);
      assert.ok(raw.entries.filter(entry => entry.entryType === 'event').every(entry => entry.duration >= 16));
      await assert.rejects(page.evaluate(plan => window.__p2Timing.arm(plan.sampleId, document.activeElement), plan), /document_already_used/);
      results.push({ sampleId: plan.sampleId, duration: result.durationMs,
        firstInput: raw.entries.filter(entry => entry.entryType === 'first-input').length,
        events: raw.entries.filter(entry => entry.entryType === 'event').length });
    }
    const cleanup = await page.evaluate(() => window.__p2Timing.stop());
    assert.equal(documents.has(cleanup.documentId), true);
    assert.equal(cleanup.disconnectedObservers, 2);
    assert.equal(cleanup.removedListeners, 5);
    console.log('Headless observer evidence (not native P2):', JSON.stringify(results));
  } finally { await browser.close(); }
});
