import { normalizeAbsolutePath } from '../../shared/projects-path';

export function normalizeExplorerPath(path: string): string {
  let ordinary = path;
  if (path.startsWith('\\\\?\\UNC\\')) ordinary = `\\\\${path.slice(8)}`;
  else if (path.startsWith('\\\\?\\')) {
    ordinary = path.slice(4);
    if (!/^[a-z]:\\/i.test(ordinary)) throw new Error('Unsupported device path');
  }
  if (path.startsWith('\\\\?\\') && ordinary.split('\\').some(part => part === '.' || part === '..')) throw new Error('Verbatim dot segment');
  return normalizeAbsolutePath(ordinary);
}

// Tauri coordinates are physical pixels. Reject malformed payloads before hit
// testing; never use a missing drop path list as permission to read clipboard.
export function nativeDrop(payload: unknown, scale: number, drop: boolean) {
  if (!payload || typeof payload !== 'object' || !Number.isFinite(scale) || scale <= 0) return null;
  const { position, paths } = payload as { position?: { x?: unknown; y?: unknown }; paths?: unknown };
  if (!position || typeof position.x !== 'number' || typeof position.y !== 'number'
    || !Number.isFinite(position.x) || !Number.isFinite(position.y)) return null;
  if (drop && (!Array.isArray(paths) || !paths.length || paths.length > 1024
    || paths.some(path => typeof path !== 'string' || !path.length || path.includes('\0')))) return null;
  return { x: position.x / scale, y: position.y / scale, paths: drop ? paths as string[] : [] };
}
