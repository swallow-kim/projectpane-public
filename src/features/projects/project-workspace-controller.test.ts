import { deferred } from './test-deferred';
import { expect, it, vi } from 'vitest';
import { projectSchema } from '../../shared/projects';
import { projectFixture } from './project-test-fixture';
import { ProjectWorkspaceController } from './project-workspace-controller';
import type { LoadedProject, ProjectClient } from './project-client';

function fixture() {
  const project = projectSchema.parse(projectFixture());
  const loaded: LoadedProject = { project, workspace: project.preset, sessionRevision: 0, readOnly: false, warnings: [] };
  const client: ProjectClient = {
    list: vi.fn(async () => ({ projects: [project], lastProjectId: project.id, warnings: [] })),
    load: vi.fn(async () => loaded), create: vi.fn(async () => loaded),
    rename: vi.fn(async () => project), duplicate: vi.fn(async () => loaded), remove: vi.fn(async () => {}),
    savePreset: vi.fn(async () => project), saveSession: vi.fn(async (_project, revision) => revision + 1),
  };
  const apply = vi.fn();
  return { project, loaded, client, apply, controller: new ProjectWorkspaceController(client, apply, () => {}) };
}
it('loads the initial last project before marking panes ready', async () => {
  const { controller, apply, loaded } = fixture();
  await controller.initialize(false);
  expect(apply).toHaveBeenCalledExactlyOnceWith(loaded);
  expect(controller.state.ready).toBe(true);
});
it('keeps folder startup ephemeral without loading or saving the last project', async () => {
  const { controller, client, apply, loaded } = fixture();
  await controller.initialize(true);
  controller.changed(loaded.workspace);
  expect(client.load).not.toHaveBeenCalled();
  expect(client.saveSession).not.toHaveBeenCalled();
  expect(apply).not.toHaveBeenCalled();
  expect(controller.state.ready).toBe(true);
  controller.dispose();
});
it('flushes dirty state before loading another project', async () => {
  const { controller, client, loaded } = fixture();
  await controller.initialize(false);
  const pending = deferred<number>();
  client.saveSession = vi.fn(() => pending.promise);
  controller.changed(loaded.workspace);
  const selecting = controller.select('another');
  expect(client.load).toHaveBeenCalledTimes(1);
  pending.resolve(1);
  await selecting;
  expect(client.load).toHaveBeenCalledTimes(2);
  controller.dispose();
});
it('ignores an older project load after a newer selection', async () => {
  const { controller, client, loaded, apply } = fixture();
  await controller.initialize(true);
  const older = deferred<LoadedProject>();
  client.load = vi.fn().mockReturnValueOnce(older.promise).mockResolvedValueOnce(loaded);
  const first = controller.select('old');
  await Promise.resolve();
  await controller.select('new');
  older.resolve(loaded);
  await first;
  expect(apply).toHaveBeenCalledTimes(1);
  controller.dispose();
});



it('preserves the current workspace when a metadata revision changes', async () => {
  const { controller, client, loaded, project, apply } = fixture();
  await controller.initialize(false);
  const current = { ...loaded.workspace, columns: 70 };
  controller.changed(current);
  await controller.mutate(async () => ({ ...project, name: 'Renamed', revision: 1 }));
  expect(apply).toHaveBeenCalledTimes(1);
  expect(client.saveSession).toHaveBeenLastCalledWith(expect.objectContaining({ revision: 1 }), 1, current);
  controller.dispose();
});
it('blocks automatic writes to a recovered project', async () => {
  const { controller, client, loaded } = fixture();
  client.load = vi.fn(async () => ({ ...loaded, readOnly: true }));
  await controller.initialize(false);
  controller.changed({ ...loaded.workspace, columns: 70 });
  await controller.closeWithin(100);
  expect(client.saveSession).not.toHaveBeenCalled();
  expect(controller.state.readOnly).toBe(true);
  controller.dispose();
});


it('retries a failed project load without discarding the current workspace', async () => {
  const { controller, client, loaded } = fixture();
  await controller.initialize(false);
  client.load = vi.fn().mockRejectedValueOnce(new Error('busy')).mockResolvedValueOnce(loaded);
  await controller.select('project-1');
  await controller.retry();
  expect(client.load).toHaveBeenCalledTimes(2);
  expect(controller.state.error).toBeNull();
  controller.dispose();
});

it('clears a failed autosave alert after a later automatic save succeeds', async () => {
  vi.useFakeTimers();
  const { controller, client, loaded } = fixture();
  try {
    client.saveSession = vi.fn().mockRejectedValueOnce(new Error('io_error')).mockResolvedValue(1);
    await controller.initialize(false);
    controller.changed({ ...loaded.workspace, columns: 60 });
    await vi.advanceTimersByTimeAsync(750);
    expect(controller.state.status).toBe('error');
    expect(controller.state.error).not.toBeNull();
    controller.changed({ ...loaded.workspace, columns: 70 });
    await vi.advanceTimersByTimeAsync(750);
    expect(controller.state.status).toBe('saved');
    expect(controller.state.error).toBeNull();
  } finally { controller.dispose(); vi.useRealTimers(); }
});

it('does not hide an unrelated project load failure when autosave succeeds', async () => {
  const { controller, client, loaded } = fixture();
  await controller.initialize(false);
  client.load = vi.fn().mockRejectedValue(new Error('not_found'));
  await controller.select('missing');
  const error = controller.state.error;
  controller.changed({ ...loaded.workspace, columns: 60 });
  expect(await controller.closeWithin(100)).toBe(true);
  expect(controller.state.error).toBe(error);
  expect(error).not.toBeNull();
  controller.dispose();
});
