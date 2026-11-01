import { useCallback, useEffect, useRef, useState } from 'react';
import { desktop } from '../../shared/ipc';
import type { StartupConfig } from '../../shared/startup';
import type { Workspace } from '../../shared/projects';
import type { LoadedProject } from './project-client';
import { createProjectClient } from './project-ipc-client';
import { ProjectWorkspaceController } from './project-workspace-controller';
import type { ProjectToolbarProps } from './ProjectToolbar';
import { listenForProjectClose } from './project-close';

const client = createProjectClient();
export function useProjectWorkspace(startup: StartupConfig, snapshot: (root: string) => Workspace | null,
  apply: (loaded: LoadedProject | null) => void, beforeClose: (milliseconds: number) => Promise<boolean> = async () => true) {
  const latest = useRef({ snapshot, apply, beforeClose });
  latest.current = { snapshot, apply, beforeClose };
  const [, render] = useState(0);
  const controller = useRef<ProjectWorkspaceController | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);
  const capture = useCallback((requirePersisted = false) => {
    const current = controller.current;
    const project = current?.state.selected;
    if (!project || current.state.readOnly) return !requirePersisted;
    try {
      const workspace = latest.current.snapshot(project.rootPath);
      if (workspace) current.changed(workspace);
      return true;
    } catch (error) {
      if (error instanceof Error) setLocalError('현재 작업 상태를 저장할 수 없습니다. 경로와 탭 수를 확인하세요.');
      return false;
    }
  }, []);
  useEffect(() => {
    if (!desktop) return;
    const current = new ProjectWorkspaceController(client, loaded => latest.current.apply(loaded), () => render(value => value + 1));
    controller.current = current;
    void current.initialize(startup.folders.length > 0 || startup.error !== null);
    const flush = () => { capture(); void current.closeWithin(1500); };
    window.addEventListener('pagehide', flush);
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenForProjectClose(capture, async milliseconds => {
      const started = performance.now();
      if (!await latest.current.beforeClose(milliseconds)) return false;
      return current.closeWithin(Math.max(1, milliseconds - (performance.now() - started)));
    },
      () => setLocalError('종료 전 저장을 완료하지 못했습니다. 창을 유지합니다. 저장 재시도 후 다시 닫으세요. 파일 작업은 앱을 다시 실행한 뒤 사용할 수 있습니다.')).then(stop => {
        if (disposed) stop(); else unlisten = stop;
      }).catch((error: unknown) => { if (error instanceof Error && !disposed) setLocalError('종료 전 저장 연결을 준비하지 못했습니다.'); });
    return () => { disposed = true; unlisten?.(); window.removeEventListener('pagehide', flush); current.dispose(); controller.current = null; };
  }, [startup, capture]);
  const state = controller.current?.state;
  const withCapture = (action: (current: ProjectWorkspaceController) => Promise<void>) => {
    const current = controller.current;
    if (!current) return;
    if (!capture()) return; setLocalError(null); void action(current);
  };
  const toolbar: ProjectToolbarProps = {
    projects: state?.projects ?? [], selectedId: state?.selected?.id ?? null,
    saving: state?.busy ?? true, error: localError ?? state?.error, readOnly: state?.readOnly,
    saveStatusText: !state?.selected ? '임시 작업 공간 · 자동 저장 안 함' : state.readOnly ? '복구 상태 · 저장 차단됨' : ({ dirty: '변경 사항 저장 대기', saving: '세션 저장 중…', saved: '마지막 작업 상태 저장됨', error: '세션 저장 실패' } as const)[state.status],
    onSelect: id => withCapture(current => current.select(id)),
    onCreate: ({ name, rootPath }) => withCapture(current => current.mutate(async () => {
      const workspace = latest.current.snapshot(rootPath);
      if (!workspace) throw new Error('Workspace not mounted');
      return client.create(name, rootPath, workspace);
    })),
    onRename: (_id, name) => withCapture(current => current.mutate(project => project ? client.rename(project, name) : Promise.resolve(null))),
    onDuplicate: (_id, name) => withCapture(current => current.mutate(project => project ? client.duplicate(project, name) : Promise.resolve(null))),
    onRemoveFromList: () => withCapture(current => current.mutate(async project => { if (project) await client.remove(project); return null; }, true)),
    onSaveCurrentAsPreset: () => withCapture(current => current.mutate(async project => {
      const workspace = project && latest.current.snapshot(project.rootPath);
      return project && workspace ? client.savePreset(project, workspace) : null;
    })),
    onRestorePreset: id => withCapture(current => current.select(id, 'preset')),
    onRetry: () => withCapture(current => current.retry()),
  };
  return { rootPath: state?.selected?.rootPath, available: state?.available ?? false, ready: !desktop || (state?.ready ?? false), busy: state?.busy ?? false, toolbar, capture,
    recoverAsNew: () => { const project = state?.selected; if (project) toolbar.onCreate({ name: '복구한 프로젝트', rootPath: project.rootPath }); },
    warnings: state?.warnings ?? [], closeWithin: (milliseconds: number) => controller.current?.closeWithin(milliseconds) ?? Promise.resolve(true) };
}
