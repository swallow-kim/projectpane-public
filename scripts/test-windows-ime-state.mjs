import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { renameSync } from 'node:fs';
import { join, resolve, sep } from 'node:path';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';
import { writeStateJson, STATE_MAX_ATTEMPTS, STATE_RETRY_DELAY_MS } from './windows-ime-state.mjs';

const evidence = resolve('artifacts/p2-ime-recovery-20261006/state');
const oldState = { status: 'old', text: '한글' };
const newState = { status: 'new', text: '완료' };
const failure = code => Object.assign(new Error(`fixture ${code}`), { code });

async function fixture(t) {
  await fs.mkdir(evidence, { recursive: true });
  const directory = await fs.mkdtemp(join(evidence, 'publisher-fixture-'));
  t.after(async () => {
    assert.ok(resolve(directory).startsWith(`${evidence}${sep}publisher-fixture-`));
    await fs.rm(directory, { recursive: true, force: true });
  });
  const path = join(directory, 'observer-state.json');
  await fs.writeFile(path, JSON.stringify(oldState));
  return { directory, path };
}

test('writes complete JSON on first publish and replacement', async t => {
  const { directory, path } = await fixture(t);
  await fs.unlink(path);
  assert.deepEqual(await writeStateJson(path, oldState), { attempts: 1, retries: 0 });
  assert.deepEqual(await writeStateJson(path, newState), { attempts: 1, retries: 0 });
  assert.deepEqual(JSON.parse(await fs.readFile(path, 'utf8')), newState);
  assert.deepEqual(await fs.readdir(directory), ['observer-state.json']);
});

test('Windows sharing failures retry asynchronously while the old complete JSON remains', async t => {
  const { directory, path } = await fixture(t);
  const waits = [];
  let calls = 0;
  const result = await writeStateJson(path, newState, {
    platform: 'win32',
    io: { ...fs, rename: async (source, target) => {
      assert.deepEqual(JSON.parse(await fs.readFile(path, 'utf8')), oldState);
      assert.deepEqual(JSON.parse(await fs.readFile(source, 'utf8')), newState);
      if (++calls <= 2) throw failure(calls === 1 ? 'EPERM' : 'EBUSY');
      await fs.rename(source, target);
    } },
    wait: async ms => { waits.push(ms); await delay(0); },
  });
  assert.deepEqual(result, { attempts: 3, retries: 2 });
  assert.deepEqual(waits, [25, 25]);
  assert.deepEqual(JSON.parse(await fs.readFile(path, 'utf8')), newState);
  assert.deepEqual(await fs.readdir(directory), ['observer-state.json']);
});

test('exhaustion has a fixed cap, preserves the last error and only removes its own temp', async t => {
  const { directory, path } = await fixture(t);
  await fs.writeFile(`${path}.tmp`, 'other writer');
  const errors = [];
  let waits = 0;
  await assert.rejects(writeStateJson(path, newState, {
    platform: 'win32',
    io: { ...fs, rename: async () => { const error = failure('EPERM'); errors.push(error); throw error; } },
    wait: async ms => { assert.equal(ms, STATE_RETRY_DELAY_MS); waits++; },
  }), error => error === errors.at(-1));
  assert.equal(STATE_MAX_ATTEMPTS, 20);
  assert.equal(errors.length, 20);
  assert.equal(waits, 19);
  assert.deepEqual(JSON.parse(await fs.readFile(path, 'utf8')), oldState);
  assert.equal(await fs.readFile(`${path}.tmp`, 'utf8'), 'other writer');
  assert.deepEqual((await fs.readdir(directory)).sort(), ['observer-state.json', 'observer-state.json.tmp']);
});

for (const [platform, code] of [
  ['win32', 'ENOENT'], ['win32', 'EACCES'], ['win32', 'ENOSPC'], ['win32', 'EIO'],
  ['win32', 'EXDEV'], ['win32', 'EINVAL'], ['linux', 'EPERM'], ['linux', 'EBUSY'],
]) test(`${platform} ${code} is surfaced unchanged without retry`, async t => {
  const { directory, path } = await fixture(t);
  const original = failure(code);
  let calls = 0;
  await assert.rejects(writeStateJson(path, newState, {
    platform,
    io: { ...fs, rename: async () => { calls++; throw original; } },
    wait: () => assert.fail('must not wait'),
  }), error => error === original);
  assert.equal(calls, 1);
  assert.deepEqual(JSON.parse(await fs.readFile(path, 'utf8')), oldState);
  assert.deepEqual(await fs.readdir(directory), ['observer-state.json']);
});

test('failed exclusive creation never unlinks an unowned temp', async t => {
  const { path } = await fixture(t);
  const original = failure('EEXIST');
  await assert.rejects(writeStateJson(path, newState, {
    io: { ...fs, open: async (temporary, flags) => {
      assert.equal(flags, 'wx');
      await fs.writeFile(temporary, 'unowned');
      throw original;
    }, unlink: () => assert.fail('no owned file to clean') },
  }), error => error === original);
});

test('write failure is not retried and removes its owned partial temp', async t => {
  const { directory, path } = await fixture(t);
  const original = failure('EPERM');
  await assert.rejects(writeStateJson(path, newState, {
    platform: 'win32', io: { ...fs, open: async (...args) => {
      const handle = await fs.open(...args);
      return { close: () => handle.close(), writeFile: async () => {
        await handle.writeFile('{'); throw original;
      } };
    }, rename: () => assert.fail('must not rename') },
    wait: () => assert.fail('must not wait'),
  }), error => error === original);
  assert.deepEqual(await fs.readdir(directory), ['observer-state.json']);
  assert.deepEqual(JSON.parse(await fs.readFile(path, 'utf8')), oldState);
});

test('invalid JSON fails before creating a temp', async t => {
  const { directory, path } = await fixture(t);
  const cycle = {}; cycle.self = cycle;
  for (const value of [undefined, cycle, 1n]) await assert.rejects(writeStateJson(path, value), TypeError);
  assert.deepEqual(await fs.readdir(directory), ['observer-state.json']);
});

test('cleanup failure preserves both errors and the original cause', async t => {
  const { path } = await fixture(t);
  const original = failure('ENOSPC'), cleanup = failure('EACCES');
  await assert.rejects(writeStateJson(path, newState, {
    io: { ...fs, rename: async () => { throw original; }, unlink: async () => { throw cleanup; } },
  }), error => error instanceof AggregateError && error.cause === original
    && error.errors[0] === original && error.errors[1] === cleanup);
});

test('simultaneous publishers use separate temps and leave complete JSON', async t => {
  const { directory, path } = await fixture(t);
  const sources = new Set();
  const io = { ...fs, rename: async (source, target) => {
    sources.add(source); await fs.rename(source, target);
  } };
  await Promise.all([writeStateJson(path, oldState, { io }), writeStateJson(path, newState, { io })]);
  assert.equal(sources.size, 2);
  assert.ok(['old', 'new'].includes(JSON.parse(await fs.readFile(path, 'utf8')).status));
  assert.deepEqual(await fs.readdir(directory), ['observer-state.json']);
});

test('real Windows held read handle blocks rename then release permits atomic publish', {
  skip: process.platform !== 'win32', timeout: 30000,
}, async t => {
  const { directory, path } = await fixture(t);
  const script = `
    $ErrorActionPreference = 'Stop'
    $handle = [System.IO.File]::Open($env:IME_STATE_FIXTURE, 'Open', 'Read', 'ReadWrite')
    try { [Console]::Out.WriteLine('READY'); [Console]::Out.Flush(); [Console]::In.ReadLine() | Out-Null }
    finally { $handle.Dispose() }
  `;
  const child = spawn('powershell.exe', ['-NoLogo', '-NoProfile', '-NonInteractive', '-EncodedCommand',
    Buffer.from(script, 'utf16le').toString('base64')], {
    windowsHide: true, env: { ...process.env, IME_STATE_FIXTURE: path }, timeout: 20000,
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  const exited = once(child, 'exit');
  exited.catch(() => {});
  let stderr = '';
  child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
  const lines = createInterface({ input: child.stdout });
  try {
    const ready = once(lines, 'line');
    await Promise.race([ready.then(([line]) => assert.equal(line, 'READY')),
      exited.then(() => assert.fail(`reader exited before ready: ${stderr}`))]);
    const legacyTemp = join(directory, 'legacy.tmp');
    await fs.writeFile(legacyTemp, JSON.stringify(newState));
    assert.throws(() => renameSync(legacyTemp, path), error => {
      t.diagnostic(`held-handle renameSync: ${error.code}`);
      return ['EPERM', 'EBUSY'].includes(error.code);
    });
    assert.deepEqual(JSON.parse(await fs.readFile(path, 'utf8')), oldState);
    let released = false;
    const result = await writeStateJson(path, newState, { wait: async ms => {
      assert.deepEqual(JSON.parse(await fs.readFile(path, 'utf8')), oldState);
      if (!released) {
        released = true; child.stdin.end('release\n');
        assert.deepEqual(await exited, [0, null], stderr);
      }
      await delay(ms);
    } });
    assert.ok(released);
    assert.deepEqual(result, { attempts: 2, retries: 1 });
    assert.deepEqual(JSON.parse(await fs.readFile(path, 'utf8')), newState);
    assert.deepEqual((await fs.readdir(directory)).sort(), ['legacy.tmp', 'observer-state.json']);
    t.diagnostic(`held-handle recovery: ${JSON.stringify(result)}`);
  } finally {
    child.stdin.end();
    if (child.exitCode === null && child.signalCode === null) child.kill();
    await exited;
    lines.close();
  }
});
