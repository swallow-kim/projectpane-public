import { z } from 'zod';
import { ProjectContractError, unicode, utf8Bytes } from './projects-error';

function invalid(): never { throw new ProjectContractError('invalid_path'); }
function pathText(value: string): string {
  if (!unicode(value) || utf8Bytes(value) > 32768 || /[\x00-\x1f\x7f]/.test(value)) invalid();
  return value.replaceAll('/', '\\');
}
function component(value: string): boolean {
  return !!value && !/[<>:"|?*]/.test(value) && !/[. ]$/.test(value)
    && !/^(?:con|prn|aux|nul|conin\$|conout\$|com[1-9¹²³]|lpt[1-9¹²³])(?:\.|$)/i.test(value);
}
function parts(value: string): string[] {
  const result: string[] = [];
  for (const item of value.split('\\')) {
    if (item === '' || item === '.') continue;
    if (item === '..') { if (!result.length) invalid(); result.pop(); }
    else { if (!component(item)) invalid(); result.push(item); }
  }
  return result;
}
export function normalizeRelativePath(value: string): string {
  const clean = pathText(value);
  if (clean.startsWith('\\') || clean.includes(':')) invalid();
  return parts(clean).join('\\');
}
export function normalizeAbsolutePath(value: string): string {
  const clean = pathText(value);
  const drive = /^([A-Za-z]):\\/.exec(clean);
  if (drive) return drive[1].toUpperCase() + ':\\' + parts(clean.slice(3)).join('\\');
  if (!clean.startsWith('\\\\')) invalid();
  const [host, share, ...tail] = clean.slice(2).split('\\');
  if (!component(host ?? '') || !component(share ?? '')) invalid();
  const suffix = parts(tail.join('\\')).join('\\');
  return '\\\\' + host + '\\' + share + (suffix ? '\\' + suffix : '');
}
function validPath(value: string, absolute: boolean): boolean {
  try { absolute ? normalizeAbsolutePath(value) : normalizeRelativePath(value); return true; }
  catch (error) { if (error instanceof ProjectContractError) return false; throw error; }
}
export const absoluteProjectPathSchema = z.string().refine(value => validPath(value, true));
export const pathRefSchema = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('relative'), value: z.string().refine(value => validPath(value, false)) }),
  z.strictObject({ kind: z.literal('absolute'), value: absoluteProjectPathSchema }),
  z.strictObject({ kind: z.literal('empty'), value: z.literal('') }),
]).readonly();
export type PathRef = z.infer<typeof pathRefSchema>;
const asciiLower = (value: string) => value.replace(/[A-Z]/g, letter => letter.toLowerCase());
export function toPathRef(root: string, target: string): PathRef {
  const base = normalizeAbsolutePath(root).replace(/\\$/, '');
  if (target === '') return { kind: 'empty', value: '' };
  const full = normalizeAbsolutePath(target).replace(/\\$/, '');
  if (asciiLower(full) === asciiLower(base)) return { kind: 'relative', value: '' };
  if (asciiLower(full).startsWith(asciiLower(base) + '\\')) return { kind: 'relative', value: full.slice(base.length + 1) };
  return { kind: 'absolute', value: normalizeAbsolutePath(target) };
}
export function resolvePathRef(root: string, input: PathRef): string | null {
  const base = normalizeAbsolutePath(root);
  const parsed = pathRefSchema.safeParse(input);
  if (!parsed.success) invalid();
  const path = parsed.data;
  switch (path.kind) {
    case 'empty': return null;
    case 'absolute': return normalizeAbsolutePath(path.value);
    case 'relative': {
      const relative = normalizeRelativePath(path.value);
      return relative ? normalizeAbsolutePath(base.replace(/\\$/, '') + '\\' + relative) : base;
    }
  }
}
