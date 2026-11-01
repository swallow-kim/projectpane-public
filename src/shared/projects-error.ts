export type ProjectContractCode = 'invalid_document' | 'unsupported_schema' | 'payload_limit' | 'invalid_path';
export class ProjectContractError extends Error {
  constructor(readonly code: ProjectContractCode) { super(code); this.name = 'ProjectContractError'; }
}
export const MAX_PROJECT_BYTES = 256 * 1024;
export const utf8Bytes = (value: string): number => new TextEncoder().encode(value).byteLength;
export function unicode(value: string): boolean {
  for (let i = 0; i < value.length; i++) {
    const unit = value.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) return false;
  }
  return true;
}
