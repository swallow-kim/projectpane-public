// Reconcile every numeric row before publishing path/PID-free local evidence.
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { parseArgs } from 'node:util';
import assert from 'node:assert/strict';
const { values } = parseArgs({ options: { input: { type: 'string' }, output: { type: 'string' }, runs: { type: 'string', multiple: true } } });
if (!values.input || !values.output) throw new Error('--input and --output required');
const read = path => JSON.parse(readFileSync(path, 'utf8').replace(/^\uFEFF/, ''));
const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const stats = input => {
  const v = [...input].sort((a, b) => a - b);
  return { median: (v[(v.length - 1) >> 1] + v[v.length >> 1]) / 2, p95: v[Math.ceil(v.length * .95) - 1], max: v.at(-1) };
};
const allowedNames = new Set(['projectpane', 'projectpane-read-worker', 'msedgewebview2', 'conhost']);
const allowedRoles = new Set([...allowedNames, 'browser', 'renderer', 'gpu-process', 'utility', 'crashpad-handler']);
const report = { units: 'bytes except handles', candidateBudgetBytes: 250 * 1024 ** 2, processColumns: ['name', 'role', 'privateBytes', 'workingSetBytes', 'handles'], runs: [] };
const selected = values.runs ? new Set(values.runs) : undefined;
for (const folder of readdirSync(values.input, { withFileTypes: true }).filter(d => d.isDirectory() && (selected ? selected.has(d.name) : /^local-(baseline|current|optimized|coalesced|final)-(synthetic|real)$/.test(d.name)))) {
  const directory = join(values.input, folder.name);
  const run = read(join(directory, 'run.json'));
  assert.equal(run.status, 'passed', folder.name);
  const blocks = [];
  for (let block = 0; block <= run.blocks; block++) {
    const samplePath = join(directory, `memory-processes-${block}.json`);
    const data = read(samplePath);
    const ui = read(join(directory, `memory-ui-${block}.json`));
    const idlePath = join(directory, `memory-idle-${block}.json`);
    const idle = existsSync(idlePath) ? read(idlePath) : undefined;
    if (idle) {
      assert.equal(idle.status, 'passed');
      assert.equal(idle.observationOnly, true);
      assert.equal(idle.observationPoint, 'after process sampling; no navigation');
    }
    assert.equal(ui.status, 'passed');
    assert.equal(data.samples.length, 30);
    const samples = data.samples.map(sample => {
      assert.equal(sample.missingProcesses, 0);
      assert.equal(sample.discoveredProcesses, sample.processes.length);
      for (const key of ['privateBytes', 'workingSetBytes', 'handles']) {
        assert.ok(Number.isSafeInteger(sample[key]) && sample[key] >= 0);
        assert.ok(sample.processes.every(p => Number.isSafeInteger(p[key]) && p[key] >= 0));
        assert.equal(sample[key], sample.processes.reduce((sum, p) => sum + p[key], 0));
      }
      assert.equal(sample.processes.filter(p => p.name === 'projectpane').length, 1);
      assert.equal(sample.processes.filter(p => p.name === 'projectpane-read-worker').length, 4);
      const processes = sample.processes.map(p => {
        assert.ok(allowedNames.has(p.name) && allowedRoles.has(p.role));
        return [p.name, p.role, p.privateBytes, p.workingSetBytes, p.handles];
      });
      return { utc: sample.utc, privateBytes: sample.privateBytes, workingSetBytes: sample.workingSetBytes, handles: sample.handles, processes };
    });
    const roles = [...new Set(samples.flatMap(s => s.processes.map(p => p[1])))];
    blocks.push({ block, cumulativeCyclesPerPane: block * run.cyclesPerBlock, sourceSha256: digest(samplePath), cpu: data.cpu, ramBytes: data.ramBytes, os: data.os, build: data.build,
      ui: { cycles: ui.cycles, stress: ui.stress, rowDom: ui.rowDom, webviewBrowserVersion: ui.webviewBrowserVersion, jsHeap: ui.jsHeap, dom: ui.dom, ipcCallbacks: ui.ipcCallbacks, diagnosticCondition: ui.diagnosticCondition },
      afterSampling: idle ? { sourceSha256: digest(idlePath), rowDom: idle.rowDom, jsHeap: idle.jsHeap, dom: idle.dom, ipcCallbacks: idle.ipcCallbacks, diagnosticCondition: idle.diagnosticCondition, observationPoint: idle.observationPoint } : undefined,
      summary: Object.fromEntries(['privateBytes', 'workingSetBytes', 'handles'].map(key => [key, stats(samples.map(s => s[key]))])),
      roles: Object.fromEntries(roles.map(role => [role, { privateBytes: stats(samples.map(s => s.processes.filter(p => p[1] === role).reduce((sum, p) => sum + p[2], 0))), counts: [...new Set(samples.map(s => s.processes.filter(p => p[1] === role).length))] }])), samples });
  }
  if (run.shutdown) {
    assert.equal(run.shutdown.forcedHost, false);
    assert.equal(run.shutdown.remainingOwnedProcesses, 0);
  }
  report.runs.push({ name: folder.name, appSha256: run.appSha256, cdp: run.cdp, driver: run.driver ?? 'playwright', installed: run.installed, forcedGc: run.forcedGc, workingSetTrim: run.workingSetTrim, shutdown: run.shutdown ?? 'not recorded by earlier harness; no shutdown latency claim', blocks });
}
assert.ok(report.runs.length > 0);
if (selected) assert.equal(report.runs.length, selected.size, 'Every requested run must be present');
writeFileSync(values.output, JSON.stringify(report) + '\n');
for (const run of report.runs) console.log(run.name, run.blocks.map(b => Number((b.summary.privateBytes.median / 1024 ** 2).toFixed(2))));
