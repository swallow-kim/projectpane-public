import { expect, it, vi } from 'vitest';
import { classifyRoots } from './use-project-root-kinds';

it('uses verified native classifications including mapped network drives', async () => {
  const call = vi.fn().mockResolvedValue([{ path: 'Z:\\Work', kind: 'network' }, { path: 'C:\\Work', kind: 'windows' }]);
  const result = await classifyRoots(['Z:\\Work', 'C:\\Work'], call);
  expect(result.get('Z:\\Work')).toBe('network');
  expect(call).toHaveBeenCalledWith('classify_project_roots', { request: { paths: ['Z:\\Work', 'C:\\Work'] } });
});
it('rejects a mismatched path, missing row or unknown protocol kind', async () => {
  for (const rows of [[{ path: 'D:\\Other', kind: 'windows' }], [], [{ path: 'C:\\Work', kind: 'magic' }]]) {
    await expect(classifyRoots(['C:\\Work'], vi.fn().mockResolvedValue(rows))).rejects.toThrow();
  }
});
it('bounds request sizes and propagates failures rather than claiming local', async () => {
  const paths = Array.from({ length: 513 }, (_, index) => `C:\\Work${index}`);
  const call = vi.fn().mockImplementation(async (_command, args) => args.request.paths.map((path: string) => ({ path, kind: 'unknown' })));
  expect((await classifyRoots(paths, call)).size).toBe(513);
  expect(call.mock.calls.map(args => args[1].request.paths.length)).toEqual([512, 1]);
  await expect(classifyRoots(paths, vi.fn().mockRejectedValue(new Error('offline')))).rejects.toThrow('offline');
});
