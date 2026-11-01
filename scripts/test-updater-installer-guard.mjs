// Execute the production Restart Manager guard against synthetic helper executables.
// No installed application, registry, clipboard, or user data is modified.
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { execFileSync, spawn } from 'node:child_process';
import { resolve, join } from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
if (process.platform !== 'win32') throw new Error('Windows required');
const root = resolve('artifacts', 'updater-guard-' + randomUUID()); mkdirSync(root, { recursive: true });
const nsis = join(process.env.LOCALAPPDATA, 'tauri', 'NSIS');
const productionGuard = resolve('src-tauri/installer/safety.nsh');
const diagnostics = join(root, 'guard-results.txt');
const guard = join(root, 'instrumented-safety.nsh');
writeFileSync(guard, readFileSync(productionGuard, 'utf8').replace(
  /^(\s*System::Call 'RSTRTMGR::RmGetList[^\n]+)$/m,
  line => line + '\n    FileOpen $R2 "' + diagnostics + '" a\n    FileWrite $R2 "$R0|$4|$1|$UpdateMode|$R1$\\r$\\n"\n    FileClose $R2'));
function compile(name, body) {
  const exe = join(root, name + '.exe'), script = join(root, name + '.nsi');
  writeFileSync(script, '\uFEFF' + `Unicode true\nName "ProjectPane synthetic updater guard"\nOutFile "${exe}"\nRequestExecutionLevel user\nSilentInstall silent\n!include LogicLib.nsh\nVar UpdateMode\n!include "${guard}"\n${body}\n`);
  execFileSync(join(nsis, 'makensis.exe'), ['/V2', script], { stdio: 'pipe' });
  return exe;
}
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
function run(exe) {
  const child = spawn(exe, [], { windowsHide: true, stdio: 'ignore' });
  const done = new Promise((resolve, reject) => { child.once('error', reject); child.once('exit', resolve); });
  return { done, child };
}
async function host(name, milliseconds) {
  const ready = join(root, name + '.ready');
  const exe = compile(name, `Section\nFileOpen $0 "${ready}" w\nFileWrite $0 "ready"\nFileClose $0\nSleep ${milliseconds}\nSectionEnd`);
  const running = run(exe);
  for (let i = 0; i < 100 && !existsSync(ready); i++) await delay(50);
  assert.ok(existsSync(ready), 'helper must report ready before guard test');
  return { ...running, exe };
}
const cases = [];
for (const scenario of [
  { name: 'normal-handoff', mode: 1, lifetime: 1500, expected: 0 },
  { name: 'busy-not-updater', mode: 0, lifetime: 1800, expected: 2 },
  { name: 'uncertain-timeout', mode: 1, lifetime: 12500, expected: 2 },
]) {
  const running = await host(scenario.name, scenario.lifetime);
  const marker = join(root, scenario.name + '.mutated');
  const probe = compile(scenario.name + '-probe', `Section\nStrCpy $UpdateMode ${scenario.mode}\n!insertmacro ProjectPaneCheckClosed "${running.exe}"\nFileOpen $0 "${marker}" w\nFileWrite $0 "guard passed"\nFileClose $0\nSectionEnd`);
  const start = Date.now(); const result = await run(probe).done;
  const guardElapsedMs = Date.now() - start;
  if (result !== scenario.expected) { await running.done; console.log(JSON.stringify({ failed: scenario.name, result, diagnostics: existsSync(diagnostics) ? readFileSync(diagnostics, 'utf8') : 'none' })); }
  assert.equal(result, scenario.expected);
  assert.equal(existsSync(marker), scenario.expected === 0, 'blocked checks must never mutate the target');
  assert.equal(await running.done, 0, 'fixture must terminate on its own');
  cases.push({ name: scenario.name, exit: result, guardElapsedMs, elapsedIncludingHelperExitMs: Date.now() - start, mutation: existsSync(marker) });
}
writeFileSync(join(root, 'report.json'), JSON.stringify({ cases,
  productionGuardSha256: createHash('sha256').update(readFileSync(productionGuard)).digest('hex'),
  instrumentation: 'RmGetList result logging only; the production guard decisions are unchanged',
}, null, 2));
console.log(JSON.stringify({ cases, report: join(root, 'report.json') }));
