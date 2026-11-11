import { expect, it, vi } from 'vitest';
import { createProjectClient, type ProjectInvoke } from './project-ipc-client';
import { projectFixture } from './project-test-fixture';
const project = projectFixture();
const { preset: workspace, schemaVersion: _schemaVersion, ...metadata } = project;
const loaded = { project: metadata, workspace, sessionRevision: 0, source: 'session', readOnly: false, warnings: [] };

it('uses the frozen request envelope and validates session revision replies', async () => {
  const invoke = vi.fn<ProjectInvoke>().mockResolvedValue({ projectId: project.id, projectRevision: 0, sessionRevision: 3 });
  const client = createProjectClient(invoke);
  expect(await client.saveSession(project, 2, workspace)).toBe(3);
  expect(invoke).toHaveBeenCalledExactlyOnceWith('save_project_session', { request: { projectId: project.id, expectedProjectRevision: 0, expectedSessionRevision: 2, workspace } });
});
it('rejects a mismatched project in a load response', async () => {
  const client = createProjectClient(vi.fn<ProjectInvoke>().mockResolvedValue(loaded));
  await expect(client.load('other', 'session')).rejects.toThrow('invalid_document');
});
it('preserves read-only recovery warnings from the backend', async () => {
  const client = createProjectClient(vi.fn<ProjectInvoke>().mockResolvedValue({ ...loaded, readOnly: true, warnings: [{ code: 'session_recovered_read_only', projectId: project.id }] }));
  expect(await client.load(project.id, 'session')).toMatchObject({ readOnly: true, warnings: ['session_recovered_read_only'] });
});
it('rejects stale session acknowledgements', async () => {
  const client = createProjectClient(vi.fn<ProjectInvoke>().mockResolvedValue({ projectId: project.id, projectRevision: 0, sessionRevision: 2 }));
  await expect(client.saveSession(project, 2, workspace)).rejects.toThrow('invalid_document');
});
it('converts IPC faults into stable error codes without untrusted text', async () => {
  const client = createProjectClient(vi.fn<ProjectInvoke>().mockRejectedValue({ code: 'revision_conflict' }));
  await expect(client.load(project.id, 'session')).rejects.toThrow('revision_conflict');
});

it('edits name and root with revision checking and validates the returned project', async () => {
  const invoke = vi.fn<ProjectInvoke>().mockResolvedValue({ ...loaded, project: { ...metadata, name: 'Edited', rootPath: 'D:\\New', revision: 1 } });
  const client = createProjectClient(invoke);
  expect((await client.edit(metadata, 'Edited', 'D:\\New')).project.rootPath).toBe('D:\\New');
  expect(invoke).toHaveBeenCalledExactlyOnceWith('edit_project', { request: { projectId: metadata.id, expectedRevision: metadata.revision, name: 'Edited', rootPath: 'D:\\New' } });
  invoke.mockResolvedValue({ ...loaded, project: { ...metadata, id: 'wrong' } });
  await expect(client.edit(metadata, 'Edited', 'D:\\New')).rejects.toThrow('invalid_document');
});
it('rejects a relative project root before invoking the backend', async () => {
  const invoke = vi.fn<ProjectInvoke>();
  await expect(createProjectClient(invoke).edit(metadata, 'Edited', 'relative')).rejects.toThrow();
  expect(invoke).not.toHaveBeenCalled();
});
