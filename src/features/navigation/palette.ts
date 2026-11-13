import { t } from '../../shared/i18n';
import { currentLocation, tabLabel, type PaneState } from '../explorer/navigation';
import type { Favorite } from '../favorites/favorites';

export type PaletteAction = { kind: 'project'; id: string } | { kind: 'tab'; pane: number; id: string }
  | { kind: 'folder'; path: string } | { kind: 'command'; command: 'new-tab' | 'reopen-tab' | 'refresh' | 'address' | 'protect' };
export type PaletteItem = { id: string; label: string; detail: string; group: string; action: PaletteAction; localize?: () => Pick<PaletteItem, 'label' | 'detail' | 'group'> };
export function localizePalette(items: readonly PaletteItem[]): PaletteItem[] {
  return items.map(item => item.localize ? { ...item, ...item.localize() } : item);
}
export function paletteItems(projects: readonly { id: string; name: string }[], panes: readonly (PaneState | undefined)[], favorites: readonly Favorite[], active: number): PaletteItem[] {
  const items: PaletteItem[] = [];
  for (const project of projects.slice(0, 512)) items.push({ id: `project-${project.id}`, group: t("프로젝트", "Projects"), label: project.name, detail: t("저장된 프로젝트로 전환", "Switch to a saved project"), action: { kind: 'project', id: project.id } });
  panes.forEach((pane, index) => pane?.tabs.forEach(tab => items.push({ id: `tab-${index}-${tab.id}`, group: t("열린 탭", "Open tabs"), label: tabLabel(tab), detail: t('패널 {number} · {path}', 'Panel {number} · {path}', { number: index + 1, path: currentLocation(tab).path || t('빈 탭', 'Empty tab') }), action: { kind: 'tab', pane: index, id: tab.id } })));
  for (const favorite of favorites.slice(0, 128)) items.push({ id: `favorite-${favorite.path}`, group: t("즐겨찾기", "Favorites"), label: favorite.name, detail: favorite.path, action: { kind: 'folder', path: favorite.path } });
  const seen = new Set<string>();
  const ordered = [panes[active], ...panes.filter((_, index) => index !== active)];
  for (const pane of ordered) for (const tab of pane?.tabs ?? []) {
    if (tab.source !== 'real') continue;
    for (const { path } of tab.history.slice(0, tab.cursor + 1).reverse()) {
      const key = path.replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();
      if (!path || seen.has(key) || seen.size >= 32) continue;
      seen.add(key);
      items.push({ id: `recent-${key}`, group: t("최근 폴더", "Recent folders"), label: path.replace(/[\\/]+$/, '').split(/[\\/]/).at(-1) || path,
        detail: path, action: { kind: 'folder', path } });
    }
  }
  const commands = [['new-tab', t("새 탭", "New tab"), 'Ctrl+T'], ['reopen-tab', t("닫은 탭 복원", "Reopen closed tab"), 'Ctrl+Shift+T'], ['refresh', t("현재 폴더 새로 고침", "Refresh current folder"), 'F5'], ['address', t("현재 주소 편집", "Edit current address"), 'Ctrl+L'], ['protect', t("현재 탭 닫기 보호 전환", "Toggle protection from closing for current tab"), t("폴더 이동은 허용 · 현재 작업 공간에만 적용", "Navigation allowed · Applies to this workspace only")]] as const;
  for (const [command, label, detail] of commands) items.push({ id: `command-${command}`, group: t("명령", "Commands"), label, detail, action: { kind: 'command', command } });
  // Keep a text resolver alongside the stable action. An open palette can then react to
  // language changes without changing user names, paths, selection, or workspace state.
  return items.map(item => {
    const action = item.action;
    const text = () => {
      if (action.kind === 'project') return { label: item.label, group: t('프로젝트', 'Projects'), detail: t('저장된 프로젝트로 전환', 'Switch to a saved project') };
      if (action.kind === 'tab') {
        const tab = panes[action.pane]?.tabs.find(tab => tab.id === action.id);
        return { label: tab ? tabLabel(tab) : item.label, group: t('열린 탭', 'Open tabs'), detail: t('패널 {number} · {path}', 'Panel {number} · {path}', { number: action.pane + 1, path: tab ? currentLocation(tab).path || t('빈 탭', 'Empty tab') : item.detail }) };
      }
      if (action.kind === 'folder') return { label: item.label, detail: item.detail, group: item.id.startsWith('recent-') ? t('최근 폴더', 'Recent folders') : t('즐겨찾기', 'Favorites') };
      const labels = {
        'new-tab': t('새 탭', 'New tab'),
        'reopen-tab': t('닫은 탭 복원', 'Reopen closed tab'),
        refresh: t('현재 폴더 새로 고침', 'Refresh current folder'),
        address: t('현재 주소 편집', 'Edit current address'),
        protect: t('현재 탭 닫기 보호 전환', 'Toggle protection from closing for current tab'),
      };
      return { label: labels[action.command], group: t('명령', 'Commands'), detail: action.command === 'protect' ? t('폴더 이동은 허용 · 현재 작업 공간에만 적용', 'Navigation allowed · Applies to this workspace only') : item.detail };
    };
    return { ...item, localize: text };
  });
}
export function searchPalette(items: readonly PaletteItem[], query: string): PaletteItem[] {
  const words = query.normalize('NFC').toLowerCase().trim().split(/\s+/).filter(Boolean);
  return items.filter(item => {
    const text = `${item.group} ${item.label} ${item.detail}`.normalize('NFC').toLowerCase();
    return words.every(word => text.includes(word));
  }).slice(0, 100);
}
