import { useEffect, useId, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import './project-toolbar.css';
export type ProjectSummary = { readonly id: string; readonly name: string; readonly rootPath: string };
export type ProjectCreation = { readonly name: string; readonly rootPath: string };
export type ProjectToolbarCapabilities = {
  readonly select?: boolean;
  readonly create?: boolean;
  readonly edit?: boolean;
  readonly duplicate?: boolean;
  readonly remove?: boolean;
  readonly savePreset?: boolean;
  readonly restorePreset?: boolean;
  readonly retry?: boolean;
};
/** Callbacks dispatch intent; the parent owns validation, persistence, and async errors. */
export interface ProjectToolbarProps {
  readonly projects: readonly ProjectSummary[];
  readonly selectedId: string | null;
  readonly saving?: boolean;
  readonly error?: string | null;
  readonly readOnly?: boolean;
  readonly saveStatusText?: string;
  readonly capabilities?: ProjectToolbarCapabilities;
  readonly onSelect: (id: string) => void;
  readonly onCreate: (project: ProjectCreation) => void;
  readonly onEdit: (id: string, project: ProjectCreation) => void;
  readonly onDuplicate: (id: string, name: string) => void;
  readonly onRemoveFromList: (id: string) => void;
  readonly onSaveCurrentAsPreset: (id: string) => void;
  readonly onRestorePreset: (id: string) => void;
  readonly onRetry: () => void;
}

const actions = {
  create: { label: '새 프로젝트', submit: '만들기', hint: '이름과 루트 폴더 경로를 입력하세요.' },
  edit: { label: '프로젝트 편집', submit: '변경하기', hint: '이름과 루트 경로를 변경합니다. 열려 있는 폴더와 실제 파일은 이동하지 않습니다.' },
  duplicate: { label: '복제', submit: '복제하기', hint: '프로젝트 설정을 복제합니다. 파일과 폴더는 복사하지 않습니다.' },
  remove: { label: '목록에서 제거', submit: '제거하기', hint: '파일과 폴더는 삭제되지 않습니다.' },
  savePreset: { label: '현재 상태를 프리셋으로 저장', submit: '프리셋 저장하기', hint: '현재 작업 상태를 이 프로젝트의 프리셋으로 저장합니다. 기존 프리셋은 대체됩니다.' },
  restorePreset: { label: '프리셋 복원', submit: '복원하기', hint: '현재 작업 상태를 저장된 프리셋으로 바꿉니다.' },
} as const;
type Action = keyof typeof actions;
const actionOrder: readonly Action[] = ['create', 'edit', 'duplicate', 'remove', 'savePreset', 'restorePreset'];

export function ProjectToolbar(props: ProjectToolbarProps) {
  return <ToolbarContent {...props} />;
}

function ToolbarContent(props: ProjectToolbarProps) {
  const id = useId();
  const [action, setAction] = useState<Action | null>(null);
  const opener = useRef<HTMLButtonElement | null>(null);
  const tabs = useRef<HTMLDivElement | null>(null);
  const [focusedId, setFocusedId] = useState<string | null>(props.selectedId);
  const manager = useRef<HTMLDetailsElement | null>(null);
  const selected = props.projects.find(project => project.id === props.selectedId);
  const canSelect = !props.saving && props.capabilities?.select !== false;
  const focusId = props.projects.some(project => project.id === focusedId) ? focusedId : selected?.id ?? props.projects[0]?.id;
  useEffect(() => {
    setAction(null);
    setFocusedId(props.selectedId);
    const button = tabs.current?.querySelector<HTMLButtonElement>('[aria-selected="true"]');
    button?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [props.selectedId]);
  const writable = !props.saving && !props.readOnly;
  const allowed = (candidate: Action) => writable && props.capabilities?.[candidate] !== false
    && (candidate === 'create' || selected !== undefined);
  const close = () => {
    setAction(null);
    if (opener.current && !opener.current.disabled && manager.current?.open) opener.current.focus();
    else if (manager.current) manager.current.querySelector('summary')?.focus();
    else tabs.current?.querySelector<HTMLButtonElement>('[tabindex="0"]')?.focus();
  };
  const dispatch = (draft: ProjectCreation) => {
    if (!action || !allowed(action)) return;
    switch (action) {
      case 'create': props.onCreate(draft); break;
      case 'edit': if (selected) props.onEdit(selected.id, draft); break;
      case 'duplicate': if (selected) props.onDuplicate(selected.id, draft.name); break;
      case 'remove': if (selected) props.onRemoveFromList(selected.id); break;
      case 'savePreset': if (selected) props.onSaveCurrentAsPreset(selected.id); break;
      case 'restorePreset': if (selected) props.onRestorePreset(selected.id); break;
      default: { const exhaustive: never = action; return exhaustive; }
    }
    close();
  };
  return <section className="project-toolbar" aria-label="프로젝트 관리" onKeyDown={event => event.stopPropagation()}>
    <div className="project-toolbar__row">
      <div className="project-tabs" role="tablist" aria-label="프로젝트" ref={tabs}>
        {props.projects.length === 0 && <span className="project-empty">프로젝트 없음</span>}
        {props.projects.map((project, index) => <button type="button" role="tab" key={project.id}
          data-project-id={project.id} aria-selected={project.id === selected?.id} aria-controls="project-workspace"
          tabIndex={project.id === focusId ? 0 : -1} title={project.name} disabled={!canSelect}
          onFocus={() => setFocusedId(project.id)}
          onClick={() => { if (project.id !== props.selectedId) { setAction(null); props.onSelect(project.id); } }}
          onKeyDown={event => {
            if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229 || event.ctrlKey || event.altKey || event.metaKey) return;
            const next = event.key === 'Home' ? 0 : event.key === 'End' ? props.projects.length - 1
              : event.key === 'ArrowRight' ? (index + 1) % props.projects.length
              : event.key === 'ArrowLeft' ? (index + props.projects.length - 1) % props.projects.length : null;
            if (next !== null) {
              event.preventDefault();
              tabs.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
            }
          }}>{project.name}</button>)}
      </div>
      <span className="project-save-status" role="status" aria-atomic="true" title={props.saveStatusText}>{props.saving ? '저장 중…' : props.error ? '저장 실패' : props.saveStatusText ?? '저장 상태 없음'}{props.readOnly && ' · 읽기 전용'}</span>
      <details className="project-manager" ref={manager}><summary>프로젝트 관리</summary>
      <div className="project-toolbar__actions" role="group" aria-label="프로젝트 작업">
        {actionOrder.map(candidate => <button type="button" key={candidate} disabled={!allowed(candidate)}
          aria-expanded={action === candidate} aria-controls={action === candidate ? `${id}-editor` : undefined}
          onClick={event => { opener.current = event.currentTarget; setAction(candidate); if (manager.current) manager.current.open = false; }}>
          {actions[candidate].label}
        </button>)}
      </div>
      </details>
    </div>
      {props.error && <div className="project-toolbar__error">
        <span role="alert">{props.error}</span>
        <button type="button" disabled={!writable || props.capabilities?.retry === false} onClick={props.onRetry}>다시 시도</button>
      </div>}
    {action && <ProjectEditor key={`${action}-${selected?.id ?? ''}`} id={`${id}-editor`} action={action} selected={selected}
      enabled={allowed(action)} onCancel={close} onSubmit={dispatch} />}
  </section>;
}

type EditorProps = {
  readonly id: string;
  readonly action: Action;
  readonly selected: ProjectSummary | undefined;
  readonly enabled: boolean;
  readonly onCancel: () => void;
  readonly onSubmit: (draft: ProjectCreation) => void;
};

function ProjectEditor({ id, action, selected, enabled, onCancel, onSubmit }: EditorProps) {
  const [name, setName] = useState(action === 'create' ? '' : action === 'duplicate' ? `${selected?.name ?? ''} 복사본` : selected?.name ?? '');
  const [rootPath, setRootPath] = useState(action === 'edit' ? selected?.rootPath ?? '' : '');
  const input = useRef<HTMLInputElement | null>(null);
  const cancel = useRef<HTMLButtonElement | null>(null);
  const composing = useRef(false);
  const settling = useRef(false);
  const named = action === 'create' || action === 'edit' || action === 'duplicate';
  const valid = !named || (name.trim().length > 0 && ((action !== 'create' && action !== 'edit') || rootPath.trim().length > 0));
  useEffect(() => { (input.current ?? cancel.current)?.focus(); }, []);
  const keyDown = (event: KeyboardEvent<HTMLFormElement>) => {
    const commitKey = event.key === 'Enter' || event.key === 'Escape';
    if (commitKey && (composing.current || settling.current || event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229)) {
      event.preventDefault();
      return;
    }
    if (!commitKey) settling.current = false;
    if (event.key === 'Escape') { event.preventDefault(); onCancel(); }
  };
  return <form id={id} className="project-toolbar__editor" aria-labelledby={`${id}-title`}
    aria-describedby={`${id}-hint`} onKeyDown={keyDown}
    onKeyUp={() => { if (!composing.current) settling.current = false; }}
    onCompositionStart={() => { composing.current = true; }}
    onCompositionEnd={() => { composing.current = false; settling.current = true; }}
    onPointerDown={() => { if (!composing.current) settling.current = false; }}
    onSubmit={event => {
      event.preventDefault();
      if (enabled && valid && !composing.current && !settling.current) onSubmit({ name: name.trim(), rootPath: rootPath.trim() });
    }}>
    <div className="project-toolbar__editor-heading">
      <strong id={`${id}-title`}>{actions[action].label}</strong>
      {selected && action !== 'create' && <span className="project-toolbar__target" title={selected.name}>{selected.name}</span>}
    </div>
    <p id={`${id}-hint`}>{actions[action].hint}</p>
    {named && <div className="project-toolbar__fields">
      <label htmlFor={`${id}-name`}>프로젝트 이름
        <input id={`${id}-name`} ref={input} value={name} required disabled={!enabled} autoComplete="off"
          onChange={event => setName(event.currentTarget.value)} />
      </label>
      {(action === 'create' || action === 'edit') && <label className="project-toolbar__root" htmlFor={`${id}-root`}>루트 경로
        <input id={`${id}-root`} value={rootPath} required disabled={!enabled} autoComplete="off" spellCheck={false}
          placeholder="D:\작업 폴더" onChange={event => setRootPath(event.currentTarget.value)} />
      </label>}
    </div>}
    <div className="project-toolbar__editor-actions">
      <button type="submit" className="project-toolbar__submit" disabled={!enabled || !valid}>{actions[action].submit}</button>
      <button type="button" ref={cancel} onClick={onCancel}>취소</button>
    </div>
  </form>;
}
