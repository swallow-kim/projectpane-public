// Execute the production WebView2 section against owned fake installers. Registry
// reads and downloads are replaced; no runtime, registry, or user installation changes.
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { execFileSync, spawnSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { randomUUID, createHash } from 'node:crypto';

assert.equal(process.platform, 'win32');
const root = resolve('artifacts', `webview-branches-${randomUUID()}`);
mkdirSync(root);
const compiler = join(process.env.LOCALAPPDATA, 'tauri', 'NSIS', 'makensis.exe');
const template = readFileSync('src-tauri/installer/template.nsi', 'utf8');
const section = template.match(/Section WebView2\r?\n[\s\S]*?SectionEnd/)[0];
const report = { status: 'FAIL', templateSha256: createHash('sha256').update(template).digest('hex'),
  actualRuntimeInstalled: false, registryWritten: false, networkUsed: false, cases: [] };
const compile = (file, source) => {
  writeFileSync(file, '\uFEFF' + source);
  execFileSync(compiler, ['/V2', file], { stdio: 'pipe', windowsHide: true });
};
const prologue = (name, exe) => `Unicode true\nName "${name}"\nOutFile "${exe}"\nRequestExecutionLevel user\nSilentInstall silent\n`;
try {
  for (const scenario of [
    { name: 'existing-runtime', installed: true, succeeds: true },
    { name: 'download-failure', download: 'failed', succeeds: false },
    { name: 'download-cancel', download: 'cancel', succeeds: false },
    { name: 'launch-failure', missing: true, succeeds: false },
    { name: 'installer-cancel', exit: 1602, succeeds: false },
    { name: 'installer-failure', exit: 1603, succeeds: false },
    { name: 'installer-success-path-with-spaces', exit: 0, succeeds: true },
    { name: 'update-missing-runtime', update: true, succeeds: false },
  ]) {
    const dir = join(root, scenario.name + ' owned path'); mkdirSync(dir);
    const helper = join(dir, 'fake setup.exe');
    compile(join(dir, 'helper.nsi'), prologue('ProjectPane owned fake runtime', helper) + `Section\n SetErrorLevel ${scenario.exit ?? 0}\nSectionEnd\n`);
    const downloaded = join(dir, 'MicrosoftEdgeWebview2Setup.exe');
    // CopyFiles in the fake download runs after the production temporary cleanup.
    const download = scenario.missing ? '' : `CopyFiles /SILENT "${helper}" "${downloaded}"\n`;
    let code = section.replace(/ReadRegStr (\$\w+) [^\r\n]*/g, (_, variable) => `StrCpy ${variable} "${variable === '$4' && scenario.installed ? '155.0.0.0' : ''}"`)
      .replace(/NSISdl::download [^\r\n]*/, download + `Push "${scenario.download ?? 'success'}"`)
      .replaceAll('$TEMP', dir);
    assert(!/ReadRegStr|WriteReg|DeleteReg|NSISdl::download/.test(code), 'no external state access in fixture');
    const exe = join(dir, 'probe.exe'), marker = join(dir, 'continued.txt');
    const strings = [...new Set([...code.matchAll(/\$\((\w+)\)/g)].map(x => x[1]))].map(key => `LangString ${key} \u0024{LANG_ENGLISH} "${key}"`).join('\n');
    code = prologue('ProjectPane WebView2 branch fixture', exe) + `!include MUI2.nsh\n!include LogicLib.nsh\n!include x64.nsh\n!include WordFunc.nsh\n!define INSTALLWEBVIEW2MODE "downloadBootstrapper"\n!define WEBVIEW2APPGUID "fixture"\n!define WEBVIEW2INSTALLERARGS "/S"\n!define MINIMUMWEBVIEW2VERSION ""\nVar UpdateMode\n!insertmacro MUI_LANGUAGE English\n${strings}\nFunction .onInit\nStrCpy $UpdateMode ${scenario.update ? 1 : 0}\nStrCpy $1 0\nFunctionEnd\n` + code + `\nSection\nFileOpen $0 "${marker}" w\nFileWrite $0 "continued"\nFileClose $0\nSectionEnd\n`;
    compile(join(dir, 'probe.nsi'), code);
    const result = spawnSync(exe, ['/S'], { windowsHide: true, timeout: 15000 });
    const continued = existsSync(marker);
    report.cases.push({ name: scenario.name, status: continued === scenario.succeeds && !result.error ? 'PASS' : 'FAIL', exitCode: result.status, continued, error: result.error?.message });
  }
  assert(report.cases.every(x => x.status === 'PASS'), 'all runtime branches must match their expected outcome');
  report.status = 'PASS';
} catch (error) { report.error = String(error); process.exitCode = 1; }
finally { writeFileSync(join(root, 'report.json'), JSON.stringify(report, null, 2)); console.log(JSON.stringify({ ...report, root })); }
