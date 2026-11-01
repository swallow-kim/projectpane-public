import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
fs.mkdirSync('artifacts/updater-review', { recursive: true });
const root = fs.mkdtempSync(path.resolve('artifacts', 'signed-install-probe-'));
fs.writeFileSync(path.join(root, 'synthetic-installer-only.txt'), 'No registry, user files, or installed application access.');
fs.writeFileSync(path.join(root, 'synthetic-project.json'), '{"fixture":true,"saved":true}');
const exe = path.join(root, 'synthetic-setup.exe');
const hostTest = 'desktop::update_package_tests::actual_plugin_hands_off_only_to_explicit_synthetic_installer';
const script = `Unicode true
Name "ProjectPane synthetic updater handoff"
OutFile "${exe}"
RequestExecutionLevel user
SilentInstall silent
InstallDir "${root}"
!include LogicLib.nsh
!include FileFunc.nsh
Var UpdateMode
!include "${path.resolve('src-tauri/installer/safety.nsh')}"
Function .onInit
  StrCpy $UpdateMode 0
  @@DOLLAR@@{GetParameters} $0
  ClearErrors
  @@DOLLAR@@{GetOptions} $0 "/UPDATE" $1
  @@DOLLAR@@{If} @@DOLLAR@@{Errors}
    SetErrorLevel 2
    Abort
  @@DOLLAR@@{EndIf}
  StrCpy $UpdateMode 1
FunctionEnd
Section
  !insertmacro ProjectPaneCheckClosed "$INSTDIR\\synthetic-host.exe"
  FileOpen $0 "$INSTDIR\\installed.marker" w
  FileWrite $0 "verified updater handed off after host exit"
  FileClose $0
  Exec '"$INSTDIR\\synthetic-host.exe" --exact ${hostTest} --ignored'
  SetErrorLevel 0
SectionEnd
`;
const nsi = path.join(root, 'fixture.nsi'); fs.writeFileSync(nsi, '\uFEFF' + script.replaceAll('@@DOLLAR@@', '$'));
execFileSync(path.join(process.env.LOCALAPPDATA, 'tauri/NSIS/makensis.exe'), ['/V2', nsi], { stdio: 'pipe' });
fs.writeFileSync('artifacts/updater-review/install-probe.json', JSON.stringify({ root, exe, hostTest }, null, 2));
console.log(JSON.stringify({ probeReady: true, root }));
