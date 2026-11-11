import { describe, expect, it } from 'vitest';
import { projectSchema, parseProjectJson, parseSession, toPathRef, resolvePathRef } from './projects';
import documentedProject from '../../docs/examples/project.json';
import documentedSession from '../../docs/examples/session.json';

export function projectFixture() {
  return { schemaVersion: 1, id: 'project-1', revision: 0, name: 'Example', rootPath: 'C:\\Example',
    preset: { layout: 4, columns: 50, rows: 50, activePane: 0, panes: [0, 1, 2, 3].map(i => ({
      id: `pane-${i}`, activeTabId: `tab-${i}`, tabs: [{ id: `tab-${i}`, path: { kind: 'relative', value: '' },
        view: { sort: 'name', direction: 'asc', foldersFirst: true, filter: '' },
        scroll: { entryId: null, index: 0, intraRowOffset: 0 } }],
    })) } };
}
describe('P3 strict persisted boundary', () => {
  it('validates the documented project and session with their actual schemas', () => {
    const project = projectSchema.parse(documentedProject);
    const session = parseSession(documentedSession);
    expect(session.projectId).toBe(project.id);
    expect(session.projectRevision).toBe(project.revision);
    expect(session.workspace).not.toEqual(project.preset);
  });
  it('round trips a valid project', () => {
    const input = projectFixture();
    expect(parseProjectJson(JSON.stringify(input))).toEqual(input);
  });
  it.each([
    (p: ReturnType<typeof projectFixture>) => { p.revision = Number.MAX_SAFE_INTEGER + 1; },
    (p: ReturnType<typeof projectFixture>) => { p.preset.columns = 0; },
    (p: ReturnType<typeof projectFixture>) => { p.preset.panes[0].activeTabId = 'missing'; },
    (p: ReturnType<typeof projectFixture>) => { p.preset.panes[1].id = 'pane-0'; },
    (p: ReturnType<typeof projectFixture>) => { p.preset.panes[1].tabs[0].id = 'tab-0'; },
    (p: ReturnType<typeof projectFixture>) => { p.preset.panes[0].tabs[0].path.value = '..\\escape'; },
    (p: ReturnType<typeof projectFixture>) => { p.rootPath = 'C:relative'; },
  ])('rejects corruption without repairing', mutate => {
    const input = projectFixture();
    mutate(input);
    expect(projectSchema.safeParse(input).success).toBe(false);
  });
  it('rejects unknown fields', () => expect(projectSchema.safeParse({ ...projectFixture(), savedAt: 'old' }).success).toBe(false));
  it('distinguishes future schema', () => expect(() => parseProjectJson('{"schemaVersion":2}')).toThrow('unsupported_schema'));
});
describe('P3 lexical paths', () => {
  it('saves contained paths as relative', () => expect(toPathRef('C:\\Example', 'c:/Example/src')).toEqual({ kind: 'relative', value: 'src' }));
  it('does not confuse a prefix with containment', () => expect(toPathRef('C:\\Example', 'C:\\Examples')).toEqual({ kind: 'absolute', value: 'C:\\Examples' }));
  it('resolves UNC relative targets', () => expect(resolvePathRef('\\\\host\\share', { kind: 'relative', value: 'src' })).toBe('\\\\host\\share\\src'));
  it('rejects traversal', () => expect(() => resolvePathRef('C:\\Example', { kind: 'relative', value: '..\\escape' })).toThrow());
});


describe('WSL project path case preservation', () => {
  it.each(['wsl$', 'wsl.localhost'])('keeps case-distinct Linux folders absolute for %s', host => {
    const root = `\\\\${host}\\Ubuntu\\home\\User`;
    for (const target of [`\\\\${host}\\Ubuntu\\home\\user`, `\\\\${host}\\Ubuntu\\home\\user\\src`]) {
      const ref = toPathRef(root, target);
      expect(ref).toEqual({ kind: 'absolute', value: target });
      expect(resolvePathRef(root, ref)).toBe(target);
    }
  });
  it.each(['wsl$', 'wsl.localhost'])('preserves contained Linux component case for %s', host => {
    const root = `\\\\${host}\\Ubuntu\\home\\User`;
    expect(toPathRef(root, `${root}\\Src`)).toEqual({ kind: 'relative', value: 'Src' });
    expect(toPathRef(root, root)).toEqual({ kind: 'relative', value: '' });
    expect(toPathRef(root, `\\\\${host.toUpperCase()}\\Ubuntu\\home\\User\\Src`)).toEqual({ kind: 'relative', value: 'Src' });
  });
  it('retains case-insensitive Windows and ordinary UNC containment', () => {
    expect(toPathRef('C:\\Root', 'c:\\root\\Src')).toEqual({ kind: 'relative', value: 'Src' });
    expect(toPathRef('\\\\Server\\Share\\Root', '\\\\server\\share\\root\\Src')).toEqual({ kind: 'relative', value: 'Src' });
  });
});
