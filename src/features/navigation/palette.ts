import { currentLocation, tabLabel, type PaneState } from '../explorer/navigation';
import type { Favorite } from '../favorites/favorites';

export type PaletteAction = { kind: 'project'; id: string } | { kind: 'tab'; pane: number; id: string }
  | { kind: 'folder'; path: string } | { kind: 'command'; command: 'new-tab' | 'reopen-tab' | 'refresh' | 'address' | 'protect' };
export type PaletteItem = { id: string; label: string; detail: string; group: string; action: PaletteAction };
export function paletteItems(projects: readonly { id: string; name: string }[], panes: readonly (PaneState | undefined)[], favorites: readonly Favorite[], active: number): PaletteItem[] {
  const items: PaletteItem[] = [];
  for (const project of projects.slice(0, 512)) items.push({ id: `project-${project.id}`, group: '프로젝트', label: project.name, detail: '저장된 프로젝트로 전환', action: { kind: 'project', id: project.id } });
  panes.forEach((pane, index) => pane?.tabs.forEach(tab => items.push({ id: `tab-${index}-${tab.id}`, group: '열린 탭', label: tabLabel(tab), detail: `패널 ${index + 1} · ${currentLocation(tab).path || '빈 탭'}`, action: { kind: 'tab', pane: index, id: tab.id } })));
  for (const favorite of favorites.slice(0, 128)) items.push({ id: `favorite-${favorite.path}`, group: '즐겨찾기', label: favorite.name, detail: favorite.path, action: { kind: 'folder', path: favorite.path } });
  const seen = new Set<string>();
  const ordered = [panes[active], ...panes.filter((_, index) => index !== active)];
  for (const pane of ordered) for (const tab of pane?.tabs ?? []) {
    if (tab.source !== 'real') continue;
    for (const { path } of tab.history.slice(0, tab.cursor + 1).reverse()) {
      const key = path.replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();
      if (!path || seen.has(key) || seen.size >= 32) continue;
      seen.add(key);
      items.push({ id: `recent-${key}`, group: '최근 폴더', label: path.replace(/[\\/]+$/, '').split(/[\\/]/).at(-1) || path,
        detail: path, action: { kind: 'folder', path } });
    }
  }
  const commands = [['new-tab', '새 탭', 'Ctrl+T'], ['reopen-tab', '닫은 탭 복원', 'Ctrl+Shift+T'], ['refresh', '현재 폴더 새로 고침', 'F5'], ['address', '현재 주소 편집', 'Ctrl+L'], ['protect', '현재 탭 닫기 보호 전환', '폴더 이동은 허용 · 현재 작업 공간에만 적용']] as const;
  for (const [command, label, detail] of commands) items.push({ id: `command-${command}`, group: '명령', label, detail, action: { kind: 'command', command } });
  return items;
}
export function searchPalette(items: readonly PaletteItem[], query: string): PaletteItem[] {
  const words = query.normalize('NFC').toLowerCase().trim().split(/\s+/).filter(Boolean);
  return items.filter(item => {
    const text = `${item.group} ${item.label} ${item.detail}`.normalize('NFC').toLowerCase();
    return words.every(word => text.includes(word));
  }).slice(0, 100);
}
