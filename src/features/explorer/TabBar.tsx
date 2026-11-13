import { t, useLanguage } from '../../shared/i18n';
import { Plus, X, ChevronLeft, ChevronRight, LockKeyhole, Undo2 } from 'lucide-react';
import { useEffect } from 'react';
import { MAX_TABS, currentLocation, tabLabel, type PaneState } from './navigation';

export function TabBar({ pane, state, select, close, add, move, composing, reopen, protect }: {
  readonly reopen: () => void;
  readonly protect: () => void;
  readonly pane: number;
  readonly state: PaneState;
  readonly select: (id: string) => void;
  readonly close: (id: string) => void;
  readonly add: () => void;
  readonly move: (direction: -1 | 1) => void;
  readonly composing: () => boolean;
}) {
  useLanguage();
  const index = state.tabs.findIndex(tab => tab.id === state.activeId);
  useEffect(() => {
    document.getElementById(`tab-${state.activeId}`)?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [state.activeId]);
  return <div className="tabs" aria-label={t('패널 {number} 탭', 'Pane {number} tabs', { number: pane + 1 })}>
    <span className="pane-number">{String(pane + 1).padStart(2, '0')}</span>
    <div className="tab-items" role="tablist" aria-label={t('패널 {number} 열린 탭', 'Pane {number} open tabs', { number: pane + 1 })}>
      {state.tabs.map((tab, position) => <div className="tab-item" key={tab.id}>
        <button role="tab" id={`tab-${tab.id}`} data-source={tab.source} aria-selected={tab.id === state.activeId}
          aria-controls={`pane-content-${pane}`} tabIndex={tab.id === state.activeId ? 0 : -1}
          title={currentLocation(tab).path || tabLabel(tab)} onClick={() => select(tab.id)}
          onKeyDown={event => {
            if (composing() || event.defaultPrevented || event.nativeEvent.isComposing || event.keyCode === 229 || event.ctrlKey || event.altKey || event.metaKey) return;
            const next = event.key === 'ArrowRight' ? (position + 1) % state.tabs.length
              : event.key === 'ArrowLeft' ? (position + state.tabs.length - 1) % state.tabs.length
              : event.key === 'Home' ? 0 : event.key === 'End' ? state.tabs.length - 1 : null;
            const target = next === null ? undefined : state.tabs[next];
            if (target) { event.preventDefault(); select(target.id); document.getElementById(`tab-${target.id}`)?.focus(); }
          }}>{tab.protected && <LockKeyhole size={12} aria-label={t("닫기 보호됨", "Protected from closing")} />}{tabLabel(tab)}</button>
        <button className="tab-close" disabled={tab.protected} title={tab.protected ? t("닫기 보호를 해제한 뒤 닫으세요. 폴더 탐색은 가능합니다.", "Unlock this tab before closing it. Folder navigation is still available.") : t("탭 닫기 (Ctrl+W)", "Close tab (Ctrl+W)")} aria-label={t('{label} 탭 닫기', 'Close {label} tab', { label: tabLabel(tab) })} onClick={() => close(tab.id)}><X size={12} aria-hidden="true" /></button>
      </div>)}
    </div>
    <button aria-label={t("새 탭", "New tab")} title={t("새 탭 (Ctrl+T)", "New tab (Ctrl+T)")} disabled={state.tabs.length >= MAX_TABS} onClick={add}><Plus size={15} aria-hidden="true" /></button>
    <button aria-label={t("현재 탭 닫기 보호", "Protect current tab from closing")} aria-pressed={Boolean(state.tabs[index]?.protected)} title={t("현재 작업 공간에서 닫기만 보호 · 프로젝트 전환/종료 시 초기화", "Prevent closing in this workspace only · Resets when switching projects or exiting")} onClick={protect}><LockKeyhole size={14} aria-hidden="true" /></button>
    <button aria-label={t("닫은 탭 복원", "Reopen closed tab")} title={t("닫은 탭 복원 (Ctrl+Shift+T) · 현재 작업 공간의 최근 10개", "Reopen closed tab (Ctrl+Shift+T) · Last 10 tabs in this workspace")} disabled={!state.closedTabs?.length || state.tabs.length >= MAX_TABS} onClick={reopen}><Undo2 size={14} aria-hidden="true" /></button>
    <button aria-label={t("탭 왼쪽으로 이동", "Move tab left")} disabled={index <= 0} onClick={() => move(-1)}><ChevronLeft size={14} aria-hidden="true" /></button>
    <button aria-label={t("탭 오른쪽으로 이동", "Move tab right")} disabled={index >= state.tabs.length - 1} onClick={() => move(1)}><ChevronRight size={14} aria-hidden="true" /></button>
    {state.tabs.length >= MAX_TABS && <span className="tab-limit" role="status">{t("최대 32탭", "32-tab limit")}</span>}
  </div>;
}
