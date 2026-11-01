import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test } from 'node:test';

const readJson = (path) => JSON.parse(readFileSync(new URL(path, import.meta.url), 'utf8'));
const config = readJson('../src-tauri/tauri.conf.json');
const p2Commands = [
  'list_archive_tools', 'launch_archive_tool', 'startup_config', 'start_listing', 'read_listing', 'cancel_listing',
  'set_listing_view', 'locate_entry', 'activate_entry', 'export_to_explorer', 'import_from_explorer', 'clipboard_sequence',
];
const projectCommands = [
  'list_favorites', 'save_favorites',
  'list_projects', 'create_project', 'load_project', 'rename_project',
  'duplicate_project', 'remove_project', 'save_project_preset',
  'save_project_session', 'project_close_ready',
];
const operationCommands = ['get_operation_state', 'prepare_operation', 'capture_transfer', 'prepare_transfer', 'execute_operation', 'cancel_operation', 'discard_operation', 'get_operation_report', 'forget_operation', 'operation_close_choice'];
const updateCommands = ['update_status', 'check_update', 'download_update', 'cancel_update_download', 'install_update', 'open_update_page'];
const appCommands = [...p2Commands, ...projectCommands, ...operationCommands, ...updateCommands, 'poll_watch'].sort();
const eventPermissions = ['core:event:allow-listen', 'core:event:allow-unlisten'];

test('updater accepts version-bound signatures and a fixed public HTTPS feed only', () => {
  const updater = config.plugins.updater;
  assert.equal(updater.requireSignedVersion, true);
  assert.equal(updater.allowDowngrades, false);
  assert.deepEqual(updater.endpoints, ['https://github.com/swallow-kim/projectpane-public/releases/latest/download/latest.json']);
  assert.equal(updater.windows.installMode, 'passive');
  assert.notEqual(updater.dangerousInsecureTransportProtocol, true);
  assert.notEqual(updater.dangerousAcceptInvalidCerts, true);
  assert.notEqual(updater.dangerousAcceptInvalidHostnames, true);
});

test('Cargo owns the worker bundle and notice paths retain their relative links', () => {
  assert.equal(config.bundle.externalBin, undefined, 'the Cargo bin must not be bundled a second time as a sidecar');
  for (const [source, destination] of Object.entries(config.bundle.resources)) {
    const relative = source.startsWith('../') ? source.slice(3) : `src-tauri/${source}`;
    assert.equal(destination, `licenses/${relative}`);
  }
  const version = readJson('../package.json').version;
  assert.equal(config.version, version);
  assert.equal(readJson('../package-lock.json').packages[''].version, version);
  const manifest = readFileSync(new URL('../src-tauri/Cargo.toml', import.meta.url), 'utf8');
  assert.equal(manifest.match(/^version = "([^"]+)"/m)?.[1], version);
});

test('Shell unit tests cannot replace or inspect the interactive system clipboard', () => {
  // OleGetClipboard returns a live wrapper, not a durable backup after replacement.
  // Test the bounded CF_HDROP parser and IDataObject directly; use a separate
  // disposable Windows session for live clipboard/Explorer acceptance tests.
  for (const file of ['explorer.rs', 'explorer/context_menu.rs']) {
    const source = readFileSync(new URL(`../src-tauri/src/platform/${file}`, import.meta.url), 'utf8');
    const tests = source.split(/#\[cfg\(test\)\]\s*mod tests\s*\{/)[1];
    assert.ok(tests, 'Shell regression tests must remain present');
    assert.doesNotMatch(tests, /\b(?:OleGetClipboard|OleSetClipboard|OleFlushClipboard|clipboard_paths|copy_to_clipboard|InvokeCommand|TrackPopupMenuEx|SendInput)\s*\(/);
  }
});

test('external move protocol remains test-only and never obtains the user clipboard', () => {
  const root = readFileSync(new URL('../src-tauri/src/platform/explorer.rs', import.meta.url), 'utf8');
  assert.match(root, /#\[cfg\(test\)\]\s*pub\(crate\) mod external_move;/);
  for (const file of ['external_move.rs', 'external_move/tests.rs']) {
    const source = readFileSync(new URL(`../src-tauri/src/platform/explorer/${file}`, import.meta.url), 'utf8');
    assert.doesNotMatch(source, /\b(?:OleGetClipboard|OleSetClipboard|OleFlushClipboard|clipboard_paths|copy_to_clipboard|SendInput)\s*\(/);
    if (file === 'external_move.rs') assert.doesNotMatch(source, /\b(?:rename|remove_file|remove_dir_all|copy)\s*\(/);
  }
});

test('guarded external move service and executor are absent from product builds', () => {
  for (const [path, module] of [
    ['services/operations/runtime/mod.rs', 'external_move'],
    ['platform/file_operations/mod.rs', 'guarded_move'],
  ]) {
    const source = readFileSync(new URL(`../src-tauri/src/${path}`, import.meta.url), 'utf8');
    assert.match(source, new RegExp(`#\\[cfg\\(test\\)\\]\\s*(?:pub\\(crate\\) )?mod ${module};`));
  }
  for (const path of [
    'services/operations/runtime/external_move.rs',
    'services/operations/runtime/external_move/tests.rs',
    'services/operations/runtime/external_move/boundary_tests.rs',
    'platform/file_operations/guarded_move.rs',
  ]) {
    const source = readFileSync(new URL(`../src-tauri/src/${path}`, import.meta.url), 'utf8');
    assert.doesNotMatch(source, /\b(?:OleGetClipboard|OleSetClipboard|OleFlushClipboard|clipboard_paths|copy_to_clipboard|SendInput)\s*\(/);
  }
});

test('installer refuses busy targets and cannot invoke legacy force-close or delete app data', () => {
  assert.equal(config.bundle.windows.allowDowngrades, false);
  assert.equal(config.bundle.windows.nsis.installMode, 'currentUser');
  assert.equal(config.bundle.windows.nsis.template, 'installer/template.nsi');
  assert.equal(config.bundle.windows.nsis.installerHooks, 'installer/safety.nsh');
  const template = readFileSync(new URL('../src-tauri/installer/template.nsi', import.meta.url), 'utf8');
  const guard = readFileSync(new URL('../src-tauri/installer/safety.nsh', import.meta.url), 'utf8');
  assert.equal(/!insertmacro CheckIfAppIsRunning|ExecWait '\$R1'|RmDir\s+\/r(?:\s|$)/i.test(template), false);
  assert.doesNotMatch(guard, /RmShutdown|TerminateProcess|taskkill/i);
  for (const section of ['Install', 'Uninstall']) {
    const body = template.match(new RegExp(`Section ${section}\\r?\\n([\\s\\S]*?)SectionEnd`))?.[1];
    assert.ok(body);
    const host = body.indexOf('!insertmacro ProjectPaneCheckClosed "$INSTDIR\\${MAINBINARYNAME}.exe"');
    const worker = body.indexOf('!insertmacro ProjectPaneCheckClosed "$INSTDIR\\projectpane-read-worker.exe"');
    const mutation = body.search(/^  (?:File |Delete )/m);
    assert.ok(host >= 0 && worker > host && mutation > worker, `${section} must guard both targets before mutation`);
  }
});

test('maintenance offers a safe update and rechecks versions for silent installers', () => {
  const template = readFileSync(new URL('../src-tauri/installer/template.nsi', import.meta.url), 'utf8');
  const maintenance = template.slice(template.indexOf('Var InstalledVersion'), template.indexOf('; 5. Choose install directory page'));
  assert.doesNotMatch(maintenance, /NSD_CreateRadioButton|ExecWait|ExecShell|DeleteReg|Delete /);
  assert.match(maintenance, /ReadRegStr \$InstalledVersion SHCTX "\$\{UNINSTKEY\}" "DisplayVersion"/);
  assert.match(maintenance, /Pop \$MaintenanceComparison/);
  assert.match(template, /현재 설치 버전 \(레지스트리\): \$InstalledVersionLabel/);
  assert.match(template, /이번 설치 버전: \$\{VERSION\}/);
  const checks = template.match(/Section EarlyChecks\r?\n([\s\S]*?)SectionEnd/)?.[1];
  assert.match(checks, /Call DetectMaintenance[\s\S]*Call MaintenanceCheck/);
  assert.match(checks, /SetErrorLevel 2\s+Abort/);
});

test('only the local main window receives the explicit application permissions', () => {
  const files = readdirSync(new URL('../src-tauri/capabilities/', import.meta.url));
  assert.deepEqual(files, ['main.json']);
  const capability = readJson('../src-tauri/capabilities/main.json');
  assert.equal(capability.local, true);
  assert.equal(capability.remote, undefined);
  assert.deepEqual(capability.windows, ['main']);
  assert.equal(capability.webviews, undefined);
  assert.deepEqual([...capability.permissions].sort(), [
    ...appCommands.map((name) => `allow-${name.replaceAll('_', '-')}`),
    ...eventPermissions,
  ].sort());
  assert.equal(config.app.security.capabilities, undefined);
});

test('packaged CSP allows only local scripts and IPC connections', () => {
  const directives = new Map(config.app.security.csp.split(';').map((part) => {
    const [name, ...values] = part.trim().split(/\s+/);
    return [name, values];
  }));
  assert.deepEqual(directives.get('default-src'), ["'self'"]);
  assert.deepEqual(directives.get('script-src'), ["'self'"]);
  assert.deepEqual(directives.get('connect-src'), ['ipc:', 'http://ipc.localhost']);
  assert.equal(config.app.security.dangerousDisableAssetCspModification, undefined);
  assert.equal(config.app.security.assetProtocol, undefined);
  assert.equal(config.app.withGlobalTauri, undefined);
  assert.equal(config.app.windows.length, 1);
  assert.equal(config.app.windows[0].create, false);
  assert.equal(config.app.windows[0].url, undefined);
});

test('Windows accepts native file drops through the guarded copy preview', () => {
  assert.equal(config.app.windows[0].dragDropEnabled, true);
});

test('custom command permissions are generated through AppManifest', () => {
  const build = readFileSync(new URL('../src-tauri/build.rs', import.meta.url), 'utf8');
  const manifest = build.match(/AppManifest::new\(\)\.commands\(&\[([\s\S]*?)\]\)/);
  assert.ok(manifest, 'application commands must have generated permissions');
  const names = [...manifest[1].matchAll(/"([a-z_]+)"/g)]
    .map((match) => match[1]).sort();
  assert.deepEqual(names, appCommands);
});

test('registered IPC commands match the explicit listing, project and operation contracts', () => {
  const desktop = readFileSync(new URL('../src-tauri/src/desktop/mod.rs', import.meta.url), 'utf8');
  const handler = desktop.match(/tauri::generate_handler!\[([\s\S]*?)\]/);
  assert.ok(handler);
  const registered = [...handler[1].matchAll(/(?:commands|projects|operations|operation_close|watch|explorer|archive_tools|updates)::([a-z_]+)/g)]
    .map((match) => match[1]).sort();
  assert.deepEqual(registered, appCommands);
  const projects = readFileSync(new URL('../src-tauri/src/desktop/projects.rs', import.meta.url), 'utf8');
  const defined = [...projects.matchAll(/#\[tauri::command\]\s+pub\s+(?:async\s+)?fn\s+([a-z_]+)/g)]
    .map((match) => match[1]).sort();
  assert.deepEqual(defined, [...projectCommands].sort());
  const operations = ['operations.rs', 'operation_close.rs'].map(file => readFileSync(new URL(`../src-tauri/src/desktop/${file}`, import.meta.url), 'utf8')).join('\n');
  const mutations = [...operations.matchAll(/#\[tauri::command\]\s+pub\s+(?:async\s+)?fn\s+([a-z_]+)/g)].map(match => match[1]).sort();
  assert.deepEqual(mutations, [...operationCommands].sort());
});

test('broad core, event emission, filesystem and opener permissions remain unavailable', () => {
  const capability = readJson('../src-tauri/capabilities/main.json');
  for (const permission of capability.permissions) {
    assert.equal(typeof permission, 'string');
    assert.doesNotMatch(permission, /(?:default|\*|^(?:fs|opener|shell):)/);
  }
  assert.deepEqual(capability.permissions.filter((permission) => permission.startsWith('core:')).sort(), eventPermissions);
});
