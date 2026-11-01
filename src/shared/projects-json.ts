import { z } from 'zod';
import { projectSchema, sessionSchema, settingsSchema, type Project, type Session, type Settings } from './projects-schema';
import { ProjectContractError, MAX_PROJECT_BYTES, utf8Bytes } from './projects-error';

function document<T>(schema: z.ZodType<T>, value: unknown): T {
  if (typeof value === 'object' && value !== null && 'schemaVersion' in value) {
    const version = value.schemaVersion;
    if (typeof version === 'number' && Number.isSafeInteger(version) && version > 1) throw new ProjectContractError('unsupported_schema');
  }
  const parsed = schema.safeParse(value);
  if (!parsed.success) throw new ProjectContractError(parsed.error.issues.some(issue => issue.message === 'payload_limit') ? 'payload_limit' : 'invalid_document');
  if (utf8Bytes(JSON.stringify(parsed.data)) > MAX_PROJECT_BYTES) throw new ProjectContractError('payload_limit');
  return parsed.data;
}
function json(text: string): unknown {
  if (utf8Bytes(text) > MAX_PROJECT_BYTES) throw new ProjectContractError('payload_limit');
  try {
    const value: unknown = JSON.parse(text);
    // JSON.parse otherwise discards repeated properties before Zod can see them.
    const tokens = text.match(/"(?:\\[\s\S]|[^"\\])*"|[{}[\]:,]/g) ?? [];
    const stack: (Set<string> | null)[] = [];
    for (let i = 0; i < tokens.length; i++) {
      const token = tokens[i];
      if (token === '{' || token === '[') {
        stack.push(token === '{' ? new Set() : null);
        if (stack.length >= 128) throw new ProjectContractError('invalid_document');
      } else if (token === '}' || token === ']') stack.pop();
      else if (token.startsWith('"') && tokens[i + 1] === ':') {
        const keys = stack.at(-1);
        const key: unknown = JSON.parse(token);
        if (typeof key !== 'string' || !keys || keys.has(key)) throw new ProjectContractError('invalid_document');
        keys.add(key);
      }
    }
    return value;
  } catch (error) {
    if (error instanceof SyntaxError) throw new ProjectContractError('invalid_document');
    throw error;
  }
}
export const parseProject = (value: unknown): Project => document(projectSchema, value);
export const parseSession = (value: unknown): Session => document(sessionSchema, value);
export const parseSettings = (value: unknown): Settings => document(settingsSchema, value);
export const parseProjectJson = (text: string): Project => parseProject(json(text));
export const parseSessionJson = (text: string): Session => parseSession(json(text));
export const parseSettingsJson = (text: string): Settings => parseSettings(json(text));
export const serializeProject = (value: Project): string => JSON.stringify(parseProject(value));
export const serializeSession = (value: Session): string => JSON.stringify(parseSession(value));
export const serializeSettings = (value: Settings): string => JSON.stringify(parseSettings(value));
