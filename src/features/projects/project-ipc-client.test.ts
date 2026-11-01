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
