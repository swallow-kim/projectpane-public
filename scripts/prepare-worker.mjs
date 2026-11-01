import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

const release = process.argv.includes('--release');
const target = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)?.[1];
if (!target) throw new Error('Rust host target is unavailable');
if (process.env.CARGO_BUILD_TARGET && process.env.CARGO_BUILD_TARGET !== target) throw new Error('The desktop worker must use the native host target');
execFileSync('cargo', ['build', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--bin', 'projectpane-read-worker', ...(release ? ['--release'] : [])], { stdio: 'inherit' });
const suffix = process.platform === 'win32' ? '.exe' : '';
mkdirSync('src-tauri/binaries', { recursive: true });
const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--no-deps', '--format-version', '1', '--manifest-path', 'src-tauri/Cargo.toml'], { encoding: 'utf8' }));
if (typeof metadata.target_directory !== 'string') throw new Error('Cargo target directory is unavailable');
const profile = release ? 'release' : 'debug';
const workerDirectory = process.env.CARGO_BUILD_TARGET ? join(metadata.target_directory, target, profile) : join(metadata.target_directory, profile);
copyFileSync(join(workerDirectory, `projectpane-read-worker${suffix}`), join('src-tauri', 'binaries', `projectpane-read-worker-${target}${suffix}`));
