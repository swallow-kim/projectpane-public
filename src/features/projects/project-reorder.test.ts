import { projectIdSchema } from '../../shared/projects';
import { expect, it, vi } from 'vitest';
import { ProjectWorkspaceController } from './project-workspace-controller';
import type { ProjectClient } from './project-client';
import { projectFixture } from './project-test-fixture';
import { deferred } from './test-deferred';

function fixture() {
  const a = projectFixture(), b = { ...a, id: projectIdSchema.parse('project-2'), name: 'Second' };
  let order = [a, b];
  const loaded = { project: a, workspace: a.preset, sessionRevision: 0, readOnly: false, warnings: [] };
  const client: ProjectClient = {
    list: vi.fn(async () => ({ projects: order, lastProjectId: a.id, warnings: [] })),
    reorder: vi.fn(async (_expected: readonly string[], ids: readonly string[]) => { order = ids.map(id => [a, b].find(item => item.id === id)!); return ids; }),
    load: vi.fn(async () => loaded), create: vi.fn(async () => loaded), edit: vi.fn(async () => loaded),
    rename: vi.fn(async () => a), duplicate: vi.fn(async () => loaded), remove: vi.fn(async () => {}),
    savePreset: vi.fn(async () => a), saveSession: vi.fn(async (_project, revision) => revision + 1),
  };
  const apply = vi.fn();
  return { a, b, client, loaded, apply, controller: new ProjectWorkspaceController(client, apply, () => {}) };
}
it('persists order only after acknowledgement, preserves selected workspace and flushes edits first', async () => {
  const { a, b, loaded, client, apply, controller } = fixture();
  await controller.initialize(false);
  controller.changed({ ...loaded.workspace, columns: 65 });
  const gate = deferred<readonly string[]>();
  client.reorder = vi.fn(() => gate.promise);
  const work = controller.reorder([b.id, a.id]);
  await vi.waitFor(() => expect(client.reorder).toHaveBeenCalledExactlyOnceWith([a.id, b.id], [b.id, a.id]));
  expect(client.saveSession).toHaveBeenCalled();
  expect(controller.state.projects.map(item => item.id)).toEqual([a.id, b.id]);
  expect(controller.state.busy).toBe(true);
  await controller.reorder([b.id, a.id]);
  expect(client.reorder).toHaveBeenCalledTimes(1);
  gate.resolve([b.id, a.id]); await work;
  expect(controller.state.projects.map(item => item.id)).toEqual([b.id, a.id]);
  expect(controller.state.selected).toBe(a);
  expect(controller.state.busy).toBe(false);
  expect(apply).toHaveBeenCalledTimes(1);
  controller.dispose();
});
it('rejects missing, duplicate and unknown ids locally, and skips an unchanged order', async () => {
  const { a, b, client, controller } = fixture(); await controller.initialize(false);
  for (const ids of [[a.id], [a.id, a.id], [a.id, 'unknown'], [a.id, b.id]]) await controller.reorder(ids);
  expect(client.reorder).not.toHaveBeenCalled();
  expect(controller.state.projects.map(item => item.id)).toEqual([a.id, b.id]); controller.dispose();
});
it('a lost acknowledgement keeps the visible order and retries by reading, never repeating the write', async () => {
  const { a, b, client, controller } = fixture(); await controller.initialize(false);
  client.reorder = vi.fn(async () => { throw new Error('lost acknowledgement'); });
  await controller.reorder([b.id, a.id]);
  expect(controller.state.error).toContain('순서 저장');
  expect(controller.state.projects.map(item => item.id)).toEqual([a.id, b.id]);
  expect(await controller.closeWithin(20)).toBe(false);
  client.list = vi.fn(async () => ({ projects: [b, a], lastProjectId: a.id, warnings: [] }));
  await controller.retry();
  expect(controller.state.projects.map(item => item.id)).toEqual([b.id, a.id]);
  expect(client.reorder).toHaveBeenCalledTimes(1);
  expect(controller.state.selected).toBe(a);
  expect(controller.state.error).toBeNull();
  expect(await controller.closeWithin(20)).toBe(true); controller.dispose();
});
it('failed session flush prevents catalog mutation', async () => {
  const { a, b, loaded, client, controller } = fixture(); await controller.initialize(false);
  client.saveSession = vi.fn(async () => { throw new Error('write failed'); });
  controller.changed({ ...loaded.workspace, columns: 65 });
  await controller.reorder([b.id, a.id]);
  expect(client.reorder).not.toHaveBeenCalled();
  expect(controller.state.projects.map(item => item.id)).toEqual([a.id, b.id]); controller.dispose();
});
it('exit waits for in-flight ordering and times out without reporting saved', async () => {
  const { a, b, client, controller } = fixture(); await controller.initialize(false);
  const gate = deferred<readonly string[]>(); client.reorder = vi.fn(() => gate.promise);
  const work = controller.reorder([b.id, a.id]);
  expect(await controller.closeWithin(5)).toBe(false);
  const closing = controller.closeWithin(500);
  gate.resolve([b.id, a.id]); await work;
  expect(await closing).toBe(true); controller.dispose();
});
it('disposed controllers ignore an eventual response', async () => {
  const { a, b, client, controller } = fixture(); await controller.initialize(false);
  const gate = deferred<readonly string[]>(); client.reorder = vi.fn(() => gate.promise);
  const work = controller.reorder([b.id, a.id]);
  await vi.waitFor(() => expect(client.reorder).toHaveBeenCalled());
  controller.dispose(); gate.resolve([b.id, a.id]); await work;
  expect(controller.state.projects.map(item => item.id)).toEqual([a.id, b.id]);
});

it('keeps an unconfirmed order warning visible after selecting a project until read-back succeeds', async () => {
  const { a, b, client, controller } = fixture(); await controller.initialize(false);
  client.reorder = vi.fn(async () => { throw new Error('unconfirmed'); });
  await controller.reorder([b.id, a.id]);
  await controller.select(a.id);
  expect(controller.state.error).toContain('순서 저장');
  expect(await controller.closeWithin(10)).toBe(false);
  await controller.retry();
  expect(controller.state.error).toBeNull();
  expect(await controller.closeWithin(10)).toBe(true); controller.dispose();
});
it('read-only recovery never changes the catalog order', async () => {
  const { a, b, loaded, client, controller } = fixture();
  client.load = vi.fn(async () => ({ ...loaded, readOnly: true }));
  await controller.initialize(false); await controller.reorder([b.id, a.id]);
  expect(client.reorder).not.toHaveBeenCalled();
  expect(controller.state.projects.map(project => project.id)).toEqual([a.id, b.id]); controller.dispose();
});
