// Read/extract only: never execute the installer or touch an installed app.
// Usage: node scripts/verify-nsis-payload.mjs INSTALLER SEVEN_ZIP RELEASE_DIRECTORY
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
const [installerArg, sevenZip, releaseArg] = process.argv.slice(2);
assert.ok(installerArg && sevenZip && releaseArg, 'installer, 7-Zip and release directory are required');
const installer = path.resolve(installerArg), release = path.resolve(releaseArg);
const config = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8'));
assert.equal(path.basename(installer), `ProjectPane_${config.version}_x64-setup.exe`);
const listing = execFileSync(sevenZip, ['l', '-slt', installer], { encoding: 'utf8' });
const paths = [...listing.slice(listing.indexOf('----------')).matchAll(/^Path = (.+)$/gm)]
  .map(match => match[1].trim().replaceAll('\\', '/'));
assert.equal(paths.length, new Set(paths.map(value => value.toLowerCase())).size, 'duplicate package destinations');
for (const name of ['projectpane.exe', 'projectpane-read-worker.exe']) assert.equal(paths.filter(value => value === name).length, 1);
const required = new Map();
for (const [source, destination] of Object.entries(config.bundle.resources)) {
  const from = path.resolve('src-tauri', source);
  if (fs.statSync(from).isDirectory()) {
    for (const name of fs.readdirSync(from)) {
      assert.ok(fs.statSync(path.join(from, name)).isFile(), 'nested resource directory requires an explicit inventory');
      required.set(destination + name, path.join(from, name));
    }
  } else required.set(destination, from);
}
for (const destination of required.keys()) assert.ok(paths.includes(destination), `missing packaged notice: ${destination}`);
fs.mkdirSync('artifacts', { recursive: true });
const extracted = fs.mkdtempSync(path.resolve('artifacts', 'nsis-payload-'));
execFileSync(sevenZip, ['x', '-y', `-o${extracted}`, installer, 'projectpane.exe', 'projectpane-read-worker.exe', 'licenses/*'], { stdio: 'pipe' });
const hash = data => createHash('sha256').update(data).digest('hex');
const hashes = {};
for (const name of ['projectpane.exe', 'projectpane-read-worker.exe']) {
  const bytes = fs.readFileSync(path.join(extracted, name));
  hashes[name] = hash(bytes);
  const expected = fs.readFileSync(path.join(release, name));
  if (name === 'projectpane.exe') {
    // Tauri 2.12.1 temporarily patches this one marker for the NSIS payload,
    // then restores the standalone executable to UNK. All other bytes must match.
    const marker = Buffer.from('__TAURI_BUNDLE_TYPE_VAR_UNK');
    const offset = expected.indexOf(marker);
    assert.ok(offset >= 0 && expected.indexOf(marker, offset + 1) === -1, 'ambiguous Tauri bundle marker');
    Buffer.from('__TAURI_BUNDLE_TYPE_VAR_NSS').copy(expected, offset);
  }
  assert.equal(hashes[name], hash(expected), `${name} differs from build output beyond the exact Tauri NSIS marker`);
  for (const sensitiveRoot of [process.env.USERPROFILE, process.cwd()].filter(Boolean)) {
    for (const form of [sensitiveRoot, sensitiveRoot.replaceAll('\\', '/')]) {
      assert.ok(!bytes.includes(Buffer.from(form)) && !bytes.includes(Buffer.from(form, 'utf16le')), 'binary contains an unremapped local build path');
    }
  }
}
for (const [destination, source] of required) assert.equal(hash(fs.readFileSync(path.join(extracted, destination))), hash(fs.readFileSync(source)), `notice differs: ${destination}`);
// Preserve the source-relative license layout so installed Markdown links resolve.
for (const destination of required.keys()) {
  if (!destination.endsWith('.md')) continue;
  const text = fs.readFileSync(path.join(extracted, destination), 'utf8').replace(/```[\s\S]*?```/g, '');
  for (const [, target] of text.matchAll(/\]\(([^)]+)\)/g)) {
    if (/^(?:https?:|#)/.test(target)) continue;
    const linked = path.resolve(extracted, path.dirname(destination), target.split('#')[0]);
    assert.ok(linked.startsWith(extracted + path.sep) && fs.existsSync(linked), `broken installed notice link in ${destination}`);
  }
}
const report = { version: config.version, installerSha256: hash(fs.readFileSync(installer)), hashes,
  noticeFiles: required.size, duplicateDestinations: 0, localBuildPathMatches: 0, exactTauriNsisMarkerAccountedFor: true,
  signatureVerification: 'separate required step; this script does not verify updater signatures', installedApplicationModified: false };
fs.writeFileSync(path.join(extracted, 'report.json'), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report));
