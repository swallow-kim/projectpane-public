import { describe, expect, it, vi } from 'vitest';
import { UpdateController, updateStatusSchema, type UpdateStatus } from './update-client';
const status: UpdateStatus = { currentVersion: '1.3.0', installed: true, phase: 'available', version: '1.3.1',
  notes: 'synthetic', error: null, received: 0, total: null, lastCheck: 1, previousInstall: null };
describe('update request admission', () => {
  it('one click lane prevents duplicate installs', async () => {
    let finish!: (value: UpdateStatus) => void;
    const request = vi.fn(() => new Promise<UpdateStatus>(resolve => { finish = resolve; }));
    const controller = new UpdateController(request);
    const first = controller.run('install_update');
    await controller.run('install_update');
    expect(request).toHaveBeenCalledTimes(1);
    finish({ ...status, phase: 'preparing' }); await first;
    expect(controller.status?.phase).toBe('preparing');
  });
  it('a delayed progress poll cannot undo a completed download and polls do not overlap', async () => {
    let finishDownload!: (value: UpdateStatus) => void;
    let finishPoll!: (value: UpdateStatus) => void;
    const request = vi.fn((command: string) => new Promise<UpdateStatus>(resolve => {
      if (command === 'download_update') finishDownload = resolve;
      else finishPoll = resolve;
    }));
    const controller = new UpdateController(request);
    const download = controller.run('download_update');
    const poll = controller.refresh();
    await controller.refresh();
    expect(request).toHaveBeenCalledTimes(2);
    finishDownload({ ...status, phase: 'ready' }); await download;
    finishPoll({ ...status, phase: 'downloading' }); await poll;
    expect(controller.status?.phase).toBe('ready');
    const next = controller.refresh();
    finishPoll({ ...status, phase: 'preparing' }); await next;
    expect(controller.status?.phase).toBe('preparing');
  });
  it('failure never produces a ready or successful install state', async () => {
    const controller = new UpdateController(vi.fn().mockRejectedValue(new Error('signature')));
    await controller.run('download_update');
    expect(controller.error).toBe('request_failed'); expect(controller.status).toBeNull(); expect(controller.busy).toBe(false);
  });
  it('unmounted views ignore late results', async () => {
    let finish!: (value: UpdateStatus) => void; const changed = vi.fn();
    const controller = new UpdateController(() => new Promise(resolve => { finish = resolve; }), changed);
    const pending = controller.run('check_update'); controller.dispose();
    const calls = changed.mock.calls.length; finish(status); await pending;
    expect(changed).toHaveBeenCalledTimes(calls); expect(controller.status).toBeNull();
  });
  it('rejects invalid transport state and negative download sizes', () => {
    expect(updateStatusSchema.safeParse({ ...status, phase: 'done' }).success).toBe(false);
    expect(updateStatusSchema.safeParse({ ...status, received: -1 }).success).toBe(false);
  });
});
