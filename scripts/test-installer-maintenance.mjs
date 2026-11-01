// Compiles the production maintenance functions with a private registry namespace.
// Does not install/uninstall ProjectPane or generate native UI input.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { randomUUID, createHash } from 'node:crypto';
import assert from 'node:assert/strict';

if (process.platform !== 'win32') throw new Error('Windows NSIS runtime required');
const root = resolve('artifacts', `maintenance-${randomUUID()}`);
mkdirSync(root);
const template = readFileSync('src-tauri/installer/template.nsi', 'utf8');
const candidateVersion = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8')).version;
const functions = template.slice(template.indexOf('Var InstalledVersion'), template.indexOf('; 5. Choose install directory page'))
  .replace('Page custom PageReinstall PageLeaveReinstall', '');
const strings = template.slice(template.indexOf('LangString maintenanceTitle'), template.indexOf('Function .onInit'));
const early = template.match(/Section EarlyChecks\r?\n([\s\S]*?)SectionEnd/)[1];
assert.ok(functions.includes('Call DetectMaintenance'));
assert.doesNotMatch(functions, /NSD_CreateRadioButton|ExecWait|ExecShell|DeleteReg|Delete /);
const key = `Software\\ProjectPaneInstallerTests\\${randomUUID()}`;
const nsis = join(process.env.LOCALAPPDATA, 'tauri', 'NSIS');
const exe = join(root, 'probe.exe');
const result = join(root, 'result.txt');
const prefix = `Unicode true
Name "ProjectPane isolated maintenance probe"
OutFile "${exe}"
RequestExecutionLevel user
SilentInstall silent
!include MUI2.nsh
!include LogicLib.nsh
!include nsDialogs.nsh
!include FileFunc.nsh
!addplugindir "${join(nsis, 'Plugins/x86-unicode/additional')}"
!define PRODUCTNAME "ProjectPane isolated ${randomUUID()}"
!define MANUFACTURER "ProjectPane test fixture"
!define VERSION "${candidateVersion}"
!define ALLOWDOWNGRADES "false"
!define UNINSTKEY "${key}"
Var WixMode
Var PassiveMode
Var UpdateMode
!insertmacro MUI_LANGUAGE English
!insertmacro MUI_LANGUAGE Korean
${strings}
${functions}
Section
  SetShellVarContext current
  StrCpy $LANGUAGE 1033
  ReadRegStr $0 HKCU "${key}" "UninstallString"
  StrCmp $0 "" +2
    Abort "Fixture key already exists"
  ClearErrors
  WriteRegStr HKCU "${key}" "UninstallString" "fixture-never-executed"
  WriteRegStr HKCU "${key}" "DisplayVersion" "@VERSION@"
  IfErrors 0 +3
    SetErrorLevel 3
    Abort "Isolated registry fixture write denied; no product result"
  Call DetectMaintenance
  Call MaintenanceCheck
  FileOpen $0 "${result}" w
  FileWriteUTF16LE $0 "$InstalledVersion|$InstalledVersionLabel|$MaintenanceComparison|$MaintenancePresent|$MaintenanceMessage"
  FileClose $0
  DeleteRegKey HKCU "${key}"
  @NEXT@
SectionEnd
`;
const run = code => {
  writeFileSync(join(root, 'probe.nsi'), '\uFEFF' + code);
  execFileSync(join(nsis, 'makensis.exe'), ['/V2', join(root, 'probe.nsi')], { stdio: 'pipe' });
  const ps = `$p=Start-Process -FilePath '${exe.replaceAll("'", "''")}' -WindowStyle Hidden -PassThru; if(-not $p.WaitForExit(10000)){throw 'Probe timeout; no termination attempted'}; exit $p.ExitCode`;
  try { execFileSync('powershell.exe', ['-NoProfile', '-Command', ps], { stdio: 'pipe' }); return 0; }
  catch (error) { if (typeof error.status === 'number') return error.status; throw error; }
};
const cases = [];
for (const [version, comparison, blocked] of [['0.1.2',1,false],['0.1.8',1,false],[candidateVersion,0,false],['2.0.0',-1,true],['',1,false]]) {
  const status = run(prefix.replace('@VERSION@',version).replace('@NEXT@',''));
  assert.equal(status,0);
  const fields = readFileSync(result,'utf16le').split('|');
  assert.equal(fields[0],version); assert.equal(Number(fields[2]),comparison); assert.equal(fields[3],'1');
  assert.equal(fields[4].length > 0,blocked);
  if (!version) assert.match(fields[1],/Unknown|확인 불가/);
  cases.push({version,comparison,blocked});
}
// The exact EarlyChecks code re-reads the registry even when /S skips pages.
// Keep the fixture key until after the check using an on-exit cleanup callback.
let silent = prefix.replace('@VERSION@','2.0.0').replace('  DeleteRegKey HKCU "'+key+'"','')
  .replace('@NEXT@',early) + `\nFunction .onGUIEnd\nDeleteRegKey HKCU "${key}"\nFunctionEnd\n`;
// Abort in a silent section still needs unconditional external fixture cleanup.
try { assert.equal(run(silent),2); cases.push({silentDowngradeExit:2}); }
finally {
  execFileSync('powershell.exe',['-NoProfile','-Command',`Remove-Item -LiteralPath 'HKCU:\\${key}' -ErrorAction SilentlyContinue`],{stdio:'pipe'});
}
// Next is permitted on same-version and upgrade; no radio/removal branch exists.
for (const version of ['0.1.8',candidateVersion]) {
  const code = prefix.replace('@VERSION@',version).replace('  DeleteRegKey HKCU "'+key+'"', '  Call PageLeaveReinstall\n  DeleteRegKey HKCU "'+key+'"').replace('@NEXT@','');
  assert.equal(run(code),0); cases.push({version,nextExit:0});
}
writeFileSync(join(root,'report.json'), JSON.stringify({status:'PASS', templateSha256:createHash('sha256').update(template).digest('hex'),cases,
  installed:false,clipboardAccess:false,interactiveMouseNext:'NOT_RUN: no supported native input tool',registryNamespace:key},null,2));
console.log(`PASS ${cases.length} native maintenance cases: ${root}`);
