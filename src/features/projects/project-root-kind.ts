import { normalizeAbsolutePath } from '../../shared/projects-path';

export type ProjectRootKind = 'windows' | 'wsl' | 'network' | 'unknown';

/**
 * A display hint from path syntax only, not a storage-capability or access check.
 * Drive letters may refer to mapped network drives, removable media or SUBST
 * paths, so "windows" must not be presented as proof of local storage.
 * The supplied path is never changed or returned as a normalized path.
 */
export function rootKind(rootPath: string): ProjectRootKind {
  if (typeof rootPath !== 'string' || new TextEncoder().encode(rootPath).length > 32768) return 'unknown';
  let candidate = rootPath.replaceAll('/', '\\');
  if (candidate.startsWith('\\\\?\\')) {
    const rest = candidate.slice(4);
    if (/^UNC\\/i.test(rest)) candidate = '\\\\' + rest.slice(4);
    else if (/^[A-Za-z]:\\/.test(rest)) candidate = rest;
    else return 'unknown';
    // Verbatim paths cannot be interpreted using ordinary Win32 dot-segment
    // normalization, matching the native import boundary.
    if (candidate.split('\\').some(part => part === '.' || part === '..')) return 'unknown';
  }
  try {
    const normalized = normalizeAbsolutePath(candidate);
    if (/^[A-Z]:\\/.test(normalized)) return 'windows';
    const host = normalized.slice(2).split('\\', 1)[0].toLowerCase();
    return host === 'wsl$' || host === 'wsl.localhost' ? 'wsl' : 'network';
  } catch {
    return 'unknown';
  }
}
