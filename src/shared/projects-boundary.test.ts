import { describe, expect, it } from 'vitest';
import fixture from '../../tests/fixtures/projects/project-v1.json';
import paths from '../../tests/fixtures/projects/path-cases.json';
import corruptions from '../../tests/fixtures/projects/corruption-cases.json';
import {
  MAX_PROJECT_BYTES, normalizeAbsolutePath, normalizeRelativePath, parseProjectJson,
  parseSessionJson, parseSettingsJson, parseProject, serializeProject, serializeSession,
  serializeSettings, toPathRef, resolvePathRef, recoverWorkspace,
} from './projects';

describe('cross-language corpus', () => {
  it.each(paths.absolute)('normalizes absolute $input', ({ input, output }) => expect(normalizeAbsolutePath(input)).toBe(output));
  it.each(paths.relative)('normalizes relative $input', ({ input, output }) => expect(normalizeRelativePath(input)).toBe(output));
  it.each(paths.invalidAbsolute)('rejects absolute %s', input => expect(() => normalizeAbsolutePath(input)).toThrow('invalid_path'));
  it.each(paths.invalidRelative)('rejects relative %s', input => expect(() => normalizeRelativePath(input)).toThrow('invalid_path'));
  it.each(corruptions)('rejects $pointer corruption', ({ pointer, value }) => {
    const input: unknown = JSON.parse(JSON.stringify(fixture));
    const segments = pointer.slice(1).split('/');
    let target = input;
    for (const segment of segments.slice(0, -1)) {
      if (typeof target !== 'object' || target === null) throw new Error('bad test fixture');
      target = Reflect.get(target, segment);
    }
    if (typeof target !== 'object' || target === null) throw new Error('bad test fixture');
    Reflect.set(target, segments.at(-1) ?? '', value);
    expect(() => parseProjectJson(JSON.stringify(input))).toThrow('invalid_document');
  });
});
describe('persisted JSON boundaries', () => {
  it.each([
    { field: 'name', pointer: ['name'], limit: 256 },
    { field: 'filter', pointer: ['preset', 'panes', '0', 'tabs', '0', 'view', 'filter'], limit: 256 },
    { field: 'scroll entryId', pointer: ['preset', 'panes', '0', 'tabs', '0', 'scroll', 'entryId'], limit: 2048 },
  ])('bounds $field in UTF-16 units', ({ pointer, limit }) => {
    for (const count of [limit / 2, limit / 2 + 1]) {
      const input = structuredClone(fixture);
      let target: unknown = input;
      for (const segment of pointer.slice(0, -1)) {
        if (typeof target !== 'object' || target === null) throw new Error('bad test fixture');
        target = Reflect.get(target, segment);
      }
      if (typeof target !== 'object' || target === null) throw new Error('bad test fixture');
      Reflect.set(target, pointer.at(-1) ?? '', '😀'.repeat(count));
      if (count * 2 === limit) expect(parseProjectJson(JSON.stringify(input))).toEqual(input);
      else expect(() => parseProjectJson(JSON.stringify(input))).toThrow('invalid_document');
    }
  });
  it.each([
    '{"schemaVersion":1,"schemaVersion":2,"lastProjectId":null}',
    '{"schemaVersion":2,"lastProjectId":null,"lastProject\\u0049d":"other"}',
    '{"schemaVersion":2,"future":{"key":null,"key":true}}',
    '{"schemaVersion":2,"future":[{"key":null,"\\u006bey":true}]}',
  ])('rejects duplicates before future dispatch: %s', text => {
    for (const parse of [parseProjectJson, parseSessionJson, parseSettingsJson]) {
      expect(() => parse(text)).toThrow('invalid_document');
    }
  });
  it.each(['2', '2e0', '2.0', '9007199254740991'])('preserves well-formed future schema %s with unknown fields', version => {
    const text = `{"schemaVersion":${version},"future":[{"key":null},{"key":true}],"other":{"key":"value"}}`;
    for (const parse of [parseProjectJson, parseSessionJson, parseSettingsJson]) {
      expect(() => parse(text)).toThrow('unsupported_schema');
    }
  });
  it('round trips sessions and settings with required nulls', () => {
    const project = parseProject(fixture);
    const session = parseSessionJson(JSON.stringify({ schemaVersion: 1, projectId: project.id, projectRevision: 0, revision: 1, workspace: project.preset }));
    expect(parseSessionJson(serializeSession(session))).toEqual(session);
    const settings = parseSettingsJson('{"schemaVersion":1,"lastProjectId":null}');
    expect(parseSettingsJson(serializeSettings(settings))).toEqual(settings);
  });
  it('requires nullable keys rather than defaulting missing fields', () => {
    expect(() => parseSettingsJson('{"schemaVersion":1}')).toThrow('invalid_document');
    expect(() => parseProjectJson(JSON.stringify(fixture).replace('"entryId":null,', ''))).toThrow('invalid_document');
  });
  it('rejects duplicate JSON keys including escaped keys', () => {
    expect(() => parseSettingsJson('{"schemaVersion":1,"lastProjectId":null,"lastProject\\u0049d":"other"}')).toThrow('invalid_document');
  });
  it('allows the exact byte bound and rejects the next byte', () => {
    const text = JSON.stringify(fixture);
    const padding = ' '.repeat(MAX_PROJECT_BYTES - new TextEncoder().encode(text).byteLength);
    expect(parseProjectJson(text + padding).id).toBe(fixture.id);
    expect(() => parseProjectJson(text + padding + ' ')).toThrow('payload_limit');
  });
  it('rejects lone surrogates without lossy conversion', () => {
    expect(() => normalizeAbsolutePath('C:\\bad\ud800')).toThrow('invalid_path');
    expect(() => parseProjectJson(JSON.stringify({ ...fixture, name: '\ud800' }))).toThrow('invalid_document');
  });
  it('distinguishes empty from root and preserves outside Unicode case', () => {
    expect(resolvePathRef('C:\\Example', { kind: 'empty', value: '' })).toBeNull();
    expect(resolvePathRef('C:\\Example', { kind: 'relative', value: '' })).toBe('C:\\Example');
    expect(toPathRef('C:\\Ä', 'C:\\ä\\x').kind).toBe('absolute');
  });
  it('rejects extra nested keys and too many tabs or wrong pane count', () => {
    const pane = fixture.preset.panes[0];
    const variants = [
      { ...fixture.preset, panes: [pane] },
      { ...fixture.preset, panes: [{ ...pane, tabs: [] }, ...fixture.preset.panes.slice(1)] },
      { ...fixture.preset, panes: [{ ...pane, tabs: Array.from({ length: 33 }, (_, i) => ({ ...pane.tabs[0], id: 'test-' + i })) }, ...fixture.preset.panes.slice(1)] },
    ];
    for (const preset of variants) expect(() => parseProjectJson(JSON.stringify({ ...fixture, preset }))).toThrow();
    expect(() => parseProjectJson(JSON.stringify({ ...fixture, preset: { ...fixture.preset, history: [] } }))).toThrow();
  });
  it('preserves valid safe integer and numeric boundary values', () => {
    const project = parseProject({ ...fixture, revision: Number.MAX_SAFE_INTEGER });
    expect(parseProjectJson(serializeProject(project))).toEqual(project);
    expect(parseProjectJson(JSON.stringify(fixture).replace('"revision":0', '"revision":0e0')).revision).toBe(0);
  });
  it('does not recover missing or nonfinite ratios', () => {
    expect(() => recoverWorkspace({ ...fixture.preset, columns: undefined })).toThrow();
    expect(() => recoverWorkspace({ ...fixture.preset, columns: Infinity })).toThrow();
  });
});

it('reports payload_limit when individually valid fields exceed the document budget', () => {
  const preset = {
    ...fixture.preset,
    panes: fixture.preset.panes.map((pane, i) => ({ ...pane,
      activeTabId: 'large-' + i + '-0',
      tabs: Array.from({ length: 32 }, (_, j) => ({
        ...pane.tabs[0], id: 'large-' + i + '-' + j,
        scroll: { ...pane.tabs[0].scroll, entryId: 'x'.repeat(2048) },
      })),
    })),
  };
  expect(() => parseProject({ ...fixture, preset })).toThrow('payload_limit');
});
