import type { Page } from '@playwright/test';
export async function installProjects(page: Page, ephemeral = false, readOnly = false, withEntry = false, entryCount = 1, projectCount = 2) {
  await page.addInitScript(({ ephemeral, readOnly, withEntry, entryCount, projectCount }) => {
    const workspace = (root: string, layout: 1 | 2 | 4 = 1) => ({ layout, columns: 60, rows: 40, activePane: 0,
      panes: [0, 1, 2, 3].map(index => ({ id: `pane-${index}`, activeTabId: `${root}-tab-${index}`, tabs: [
        { id: `${root}-tab-${index}`, path: { kind: 'relative', value: index === 0 ? 'missing' : '' },
          view: { sort: 'name', direction: 'asc', foldersFirst: true, filter: '' }, scroll: { entryId: null, index: 0, intraRowOffset: 0 } },
      ] })) });
    const a = { id: 'alpha', name: 'Alpha', rootPath: 'C:\\Alpha', revision: 0 };
    const b = { id: 'beta', name: 'Beta', rootPath: 'C:\\Beta', revision: 0 };
    const projects = new Map([['alpha', a], ['beta', b]]);
    const presets = new Map([['alpha', workspace('alpha')], ['beta', workspace('beta', 2)]]);
    for (let i = 2; i < projectCount; i++) {
      const id = `project-${i}`;
      projects.set(id, { id, name: `아주 긴 프로젝트 이름 ${i} · 디자인과 문서 정리`, rootPath: `C:\\Project-${i}`, revision: 0 });
      presets.set(id, workspace(id));
    }
    const sessions = new Map<string, { revision: number; workspace: ReturnType<typeof workspace> }>();
    const calls: { command: string; args: unknown }[] = [];
    const callbacks = new Map<number, (event: unknown) => void>();
    const listeners = new Map<string, number>();
    let operationClose = 'open';
    let closeAttempt = 0;
    let closeReply: Promise<void> | null = null;
    let releaseCloseReply: (() => void) | null = null;
    let callbackId = 0;
    const emit = (event: string, payload: unknown) => { const id = listeners.get(event); if (id !== undefined) callbacks.get(id)?.({ event, id, payload }); };
    let lastProjectId: string | null = 'alpha';
    const load = (id: string, source = 'session') => {
      const session = sessions.get(id);
      lastProjectId = id;
      return { project: projects.get(id), workspace: source === 'session' && session ? session.workspace : presets.get(id),
        sessionRevision: session?.revision ?? 0, source, readOnly: readOnly && id === 'alpha', warnings: readOnly && id === 'alpha' ? [{ code: 'recovered_workspace', projectId: id }] : [] };
    };
    Object.assign(window, { isTauri: true, p3Calls: calls, p3Sessions: sessions, p3Emit: emit, p3Close: () => emit('projectpane-close-requested', { attempt: ++closeAttempt, timeoutMs: 2500 }),
      p3OperationClose: (closeState: string) => { operationClose = closeState; emit('projectpane-operation-close', { closeState }); },
      p3HoldCloseReply: () => { closeReply = new Promise<void>(resolve => { releaseCloseReply = resolve; }); },
      p3ReleaseCloseReply: () => { releaseCloseReply?.(); closeReply = null; },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} }, __TAURI_INTERNALS__: {
      transformCallback: (handler: (event: unknown) => void) => { callbackId++; callbacks.set(callbackId, handler); return callbackId; },
      invoke: async (command: string, args: { event?: string; handler?: number; eventId?: number; choice?: string; saved?: boolean; attempt?: number; request?: { projectId: string; expectedRevision: number; expectedProjectRevision: number; expectedSessionRevision: number; name: string; rootPath: string; preset: ReturnType<typeof workspace>; workspace: ReturnType<typeof workspace>; source: string }; identity?: { pane: number }; path?: string; offset?: number; entryId?: string }) => {
        calls.push({ command, args: structuredClone(args) });
        const request = args?.request;
        if (command === 'plugin:event|listen' && args.event && args.handler !== undefined) { listeners.set(args.event, args.handler); return args.handler; }
        if (command === 'list_copy_jobs') return [];
        if (command === 'get_operation_state') return { operations: [], closeState: operationClose, available: operationClose === 'open', progress: null };
        if (command === 'operation_close_choice') { operationClose = args.choice === 'continue' ? 'open' : args.choice === 'exit_without_results' ? 'stopping' : 'waiting'; await closeReply; return; }
        if (command === 'project_close_ready') { if (!args.saved) emit('projectpane-close-failed', { attempt: args.attempt }); return; }
        if (command === 'plugin:event|unlisten') return;
        if (command === 'startup_config') return { folders: ephemeral ? ['C:\\Temporary'] : [], error: null, warnings: [] };
        if (command === 'list_projects') return { projects: [...projects.values()], lastProjectId, warnings: [] };
        if (command === 'list_favorites') return { schemaVersion: 1, revision: 0, folders: [] };
        if (command === 'load_project' && request) return load(request.projectId, request.source);
        if (command === 'save_project_session' && request) {
          const project = projects.get(request.projectId);
          const revision = sessions.get(request.projectId)?.revision ?? 0;
          if (Reflect.get(window, 'p3FailSave') || (readOnly && request.projectId === 'alpha') || !project || project.revision !== request.expectedProjectRevision || revision !== request.expectedSessionRevision) throw { code: 'revision_conflict' };
          sessions.set(project.id, { revision: revision + 1, workspace: request.workspace });
          return { projectId: project.id, projectRevision: project.revision, sessionRevision: revision + 1 };
        }
        if (command === 'create_project' && request) {
          const id = `created-${projects.size}`;
          projects.set(id, { id, name: request.name, rootPath: request.rootPath, revision: 0 }); presets.set(id, request.preset);
          return load(id);
        }
        if (command === 'duplicate_project' && request) {
          const original = projects.get(request.projectId), preset = presets.get(request.projectId);
          if (!original || !preset) throw { code: 'not_found' };
          const id = `copy-${projects.size}`;
          projects.set(id, { ...original, id, name: request.name, revision: 0 }); presets.set(id, preset);
          return load(id);
        }
        if (command === 'edit_project' && request) {
          const old = projects.get(request.projectId);
          if (!old || old.revision !== request.expectedRevision) throw { code: 'revision_conflict' };
          const rebase = (value: ReturnType<typeof workspace>) => ({ ...value, panes: value.panes.map(pane => ({ ...pane, tabs: pane.tabs.map(tab => ({ ...tab, path: tab.path.kind === 'relative' ? { kind: 'absolute', value: old.rootPath + (tab.path.value ? '\\' + tab.path.value : '') } : tab.path })) })) });
          const preset = presets.get(old.id);
          if (preset) presets.set(old.id, rebase(preset));
          const session = sessions.get(old.id);
          if (session) sessions.set(old.id, { revision: session.revision + 1, workspace: rebase(session.workspace) });
          projects.set(old.id, { ...old, name: request.name, rootPath: request.rootPath, revision: old.revision + 1 });
          return load(old.id);
        }
        if ((command === 'rename_project' || command === 'save_project_preset') && request) {
          const old = projects.get(request.projectId);
          if (!old || old.revision !== request.expectedRevision) throw { code: 'revision_conflict' };
          const project = { ...old, name: command === 'rename_project' ? request.name : old.name, revision: old.revision + 1 };
          projects.set(project.id, project);
          if (command === 'save_project_preset') presets.set(project.id, request.workspace);
          return project;
        }
        if (command === 'remove_project' && request) { projects.delete(request.projectId); if (lastProjectId === request.projectId) lastProjectId = null; return { removedProjectId: request.projectId }; }
        if (command === 'cancel_listing') return;
        if (command === 'locate_entry') {
          const index = Number(args.entryId?.replace('entry-', ''));
          return { identity: args.identity, revision: 0, index: withEntry && Number.isInteger(index) && index >= 0 && index < entryCount ? index : null };
        }
        if (command === 'start_listing' && args.path?.endsWith('missing')) throw { code: 'not_found' };
        if (command === 'start_listing' || command === 'read_listing' || command === 'set_listing_view') return { identity: args.identity, batch: { offset: args.offset ?? 0,
          entries: withEntry ? Array.from({ length: entryCount }, (_, index) => ({ id: `entry-${index}`, name: index === 0 ? '한글.txt' : `파일-${index}.txt`, kind: 'file', sizeBytes: 12, modifiedAtMs: 0 })).slice(args.offset ?? 0, (args.offset ?? 0) + 64) : [],
          total: withEntry ? entryCount : 0, complete: true, capped: false, snapshotBytes: 128, capReason: null, revision: 0 } };
        throw { code: 'unavailable' };
      },
    } });
  }, { ephemeral, readOnly, withEntry, entryCount, projectCount });
}
