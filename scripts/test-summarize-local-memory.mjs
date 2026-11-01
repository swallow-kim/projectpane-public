import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

for (const invalid of ['none', 'null-row', 'wrong-total', 'missing-child', 'unknown-run', 'failed-idle', 'navigated-idle']) {
  test(`local memory publication ${invalid === 'none' ? 'redacts and reconciles' : 'rejects ' + invalid}`, () => {
    const directory = mkdtempSync(join(tmpdir(), 'projectpane-summary-'));
    try {
      const run = join(directory, 'local-current-real');
      mkdirSync(run);
      const processes = ['projectpane', ...Array(4).fill('projectpane-read-worker')].map((name, index) => ({ name, role: name, privateBytes: 100 + index, workingSetBytes: 200, handles: 1, id: 34567 + index }));
      const sample = { utc: '2026-10-04T00:00:00Z', privateBytes: 510, workingSetBytes: 1000, handles: 5, missingProcesses: 0, discoveredProcesses: 5, processes };
      if (invalid === 'null-row') processes[0].privateBytes = null;
      if (invalid === 'wrong-total') sample.privateBytes++;
      if (invalid === 'missing-child') sample.missingProcesses++;
      const save = (file, data) => writeFileSync(join(run, file), JSON.stringify(data));
      save('run.json', { status: 'passed', blocks: 0, cyclesPerBlock: 10, appProcessId: 34567 });
      save('memory-ui-0.json', { status: 'passed', cycles: 0 });
      save('memory-idle-0.json', {
        status: invalid === 'failed-idle' ? 'failed' : 'passed',
        observationOnly: invalid !== 'navigated-idle',
        observationPoint: 'after process sampling; no navigation',
        ipcCallbacks: 0, rowDom: 48, jsHeap: { usedSize: 1024 }, dom: { nodes: 800 },
        privatePath: 'PRIVATE-PATH-MUST-NOT-BE-PUBLISHED',
      });
      save('memory-processes-0.json', { samples: Array.from({ length: 30 }, () => sample) });
      const output = join(directory, 'report.json');
      const args = [resolve('scripts/summarize-local-memory.mjs'), '--input', directory, '--output', output];
      if (invalid === 'unknown-run') args.push('--runs', 'local-final-real');
      const result = spawnSync(process.execPath, args, { encoding: 'utf8' });
      if (invalid === 'none') {
        assert.equal(result.status, 0, result.stderr);
        const text = readFileSync(output, 'utf8');
        assert.ok(!text.includes('34567') && !text.includes('appProcessId') && !text.includes('PRIVATE-PATH'));
        const report = JSON.parse(text);
        assert.equal(report.runs[0].blocks[0].summary.privateBytes.median, 510);
        assert.equal(report.runs[0].blocks[0].afterSampling.ipcCallbacks, 0);
        assert.equal(report.runs[0].blocks[0].afterSampling.jsHeap.usedSize, 1024);
      } else assert.notEqual(result.status, 0);
    } finally { rmSync(directory, { recursive: true }); }
  });
}
