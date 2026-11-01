// Generate public notices from locked packages. Local paths are never written.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
const version = JSON.parse(fs.readFileSync('package.json')).version;
const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--offline', '--manifest-path', 'src-tauri/Cargo.toml', '--format-version', '1', '--features', 'desktop', '--filter-platform', 'x86_64-pc-windows-msvc'], { encoding: 'utf8', maxBuffer: 24 * 1024 * 1024 }));
const nodes = new Map(metadata.resolve.nodes.map(x => [x.id, x]));
const packages = new Map(metadata.packages.map(x => [x.id, x]));
const scopes = new Map();
function visit(id, scope) {
  let set = scopes.get(id); if (!set) scopes.set(id, set = new Set());
  if (set.has(scope)) return; set.add(scope);
  for (const dependency of nodes.get(id)?.deps ?? []) for (const kind of dependency.dep_kinds) {
    if (kind.kind === 'dev') continue;
    const proc = packages.get(dependency.pkg)?.targets.some(t => t.kind.includes('proc-macro'));
    visit(dependency.pkg, kind.kind === 'build' || proc ? 'build' : scope);
  }
}
visit(metadata.resolve.root, 'runtime');
const licenseFiles = dir => fs.existsSync(dir) ? fs.readdirSync(dir, { withFileTypes: true })
  .filter(e => e.isFile() && /^(licen[sc]e|copying|notice|copyright|unlicense)(?:$|[.-])/i.test(e.name)).map(e => e.name) : [];
const rows = metadata.packages.filter(p => p.source).map(p => ({
  ecosystem: 'cargo', name: p.name, version: p.version, license: p.license,
  scope: [...(scopes.get(p.id) ?? ['dev-only'])].sort().join('+'),
  source: `https://static.crates.io/crates/${p.name}/${p.name}-${p.version}.crate`,
  dir: path.dirname(p.manifest_path),
}));
for (const [name, value] of Object.entries(JSON.parse(fs.readFileSync('package-lock.json')).packages)) {
  if (!name) continue;
  rows.push({ ecosystem: 'npm', name: value.name ?? name.split('node_modules/').at(-1), version: value.version,
    license: value.license, scope: value.dev ? 'build/test' : 'runtime', source: value.resolved, dir: path.resolve(name) });
}
const supplemental = {
  'alloc-stdlib': ['docs/licenses/alloc-stdlib-BSD.txt'],
  'webview2-com': ['docs/licenses/webview2-rs-MIT.txt'],
  'webview2-com-sys': ['docs/licenses/webview2-rs-MIT.txt'],
  'selectors': ['docs/licenses/Mozilla-MPL-2.0.txt'],
};
const texts = new Map(); const missing = [];
for (const row of rows.filter(row => row.scope.includes('runtime'))) {
  let files = licenseFiles(row.dir).map(file => path.join(row.dir, file));
  if (!files.length) files = supplemental[row.name] ?? [];
  if (!files.length) { missing.push(`${row.name} ${row.version}`); continue; }
  for (const file of files) {
    const text = fs.readFileSync(file, 'utf8').replaceAll('\r\n', '\n').replace(/[\t ]+$/gm, '');
    const hash = crypto.createHash('sha256').update(text).digest('hex');
    if (!texts.has(hash)) texts.set(hash, { text, names: [] });
    texts.get(hash).names.push(`${row.name} ${row.version} (${row.ecosystem}, ${path.basename(file)})`);
  }
}
if (missing.length) throw new Error(`Missing runtime notices: ${missing.join(', ')}`);
const header = '# Locked dependency license inventory\n\nProjectPane ' + version + '. Exact lockfiles and Windows x64 desktop dependency graph. Scope is conservative, not a byte-level linked-binary SBOM; optional packages for other platforms remain listed. Dependencies retain their own licenses.\n\n| Ecosystem | Package/version | Declared license | Scope | Exact source |\n|---|---|---|---|---|\n';
fs.writeFileSync(`docs/dependency-licenses-${version}.md`, header + rows.map(row => `| ${row.ecosystem} | ${row.name} ${row.version} | ${row.license?.replaceAll('|', '\\|')} | ${row.scope} | [source](${row.source}) |\n`).join(''));
fs.writeFileSync('docs/third-party-license-texts.md', '# Third-party license texts\n\nProjectPane ' + version + '. Upstream copyright and notices for the conservative runtime graph. Installer and Microsoft SDK notices are in licenses/. Exact corresponding source URLs, including MPL components, are in dependency-licenses-' + version + '.md.\n' +
  [...texts].map(([hash, value]) => `\n## License text ${hash.slice(0, 12)}\n\nApplies to: ${value.names.join('; ')}.\n\n\x60\x60\x60text\n${value.text}\n\x60\x60\x60\n`).join(''));
console.log(JSON.stringify({ version, packages: rows.length, runtimeNoticeGroups: texts.size, missing }));
