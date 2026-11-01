import { normalizeRelativePath } from '../../shared/projects-path';

// Preserve common compound archive extensions as one editable suffix. Unknown
// formats use the last dot; a leading dot alone is part of a dotfile's name.
const compounds = ['tar.gz', 'tar.bz2', 'tar.xz', 'tar.zst', 'tar.lz', 'tar.lzma', 'tar.Z'];
export function splitRenameName(name: string, folder = false) {
  if (folder) return { stem: name, extension: '' };
  const compound = compounds.find(suffix => name.length > suffix.length + 1 && name.toLowerCase().endsWith(`.${suffix.toLowerCase()}`));
  const dot = compound ? name.length - compound.length - 1 : name.lastIndexOf('.');
  return dot > 0 ? { stem: name.slice(0, dot), extension: name.slice(dot + 1) } : { stem: name, extension: '' };
}

export function joinRenameName(stem: string, extension: string, folder = false) {
  return !folder && extension ? `${stem}.${extension}` : stem;
}

export function validRenameName(name: string): boolean {
  try {
    if (!name || name.length > 255 || /[/\\]/.test(name) || normalizeRelativePath(name) !== name) return false;
    const stem = name.split('.')[0]?.replace(/ +$/, '') ?? '';
    return !stem || normalizeRelativePath(stem) === stem;
  } catch { return false; }
}
