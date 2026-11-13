import { t, useLanguage } from '../../shared/i18n';
import { useLayoutEffect, useRef } from 'react';

export type FileMenuAction = { readonly label: string; readonly description?: string; readonly disabled?: boolean; readonly shortcut?: 'F2' | 'Delete' | 'Alt+Enter'; readonly run: () => void };

export function FileContextMenu({ x, y, actions, close }: {
  readonly x: number; readonly y: number; readonly actions: readonly FileMenuAction[]; readonly close: () => void;
}) {
  const language = useLanguage();
  const menu = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const element = menu.current;
    if (!element) return;
    const bounds = element.getBoundingClientRect();
    element.style.left = `${Math.max(4, Math.min(x, window.innerWidth - bounds.width - 4))}px`;
    element.style.top = `${Math.max(4, Math.min(y, window.innerHeight - bounds.height - 4))}px`;
    element.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
    const outside = (event: PointerEvent) => { if (!element.contains(event.target as Node)) close(); };
    const blur = () => close();
    document.addEventListener('pointerdown', outside, true);
    window.addEventListener('blur', blur);
    return () => { document.removeEventListener('pointerdown', outside, true); window.removeEventListener('blur', blur); };
  }, [x, y, close, language]);
  return <div ref={menu} className="file-context-menu" role="menu" aria-label={t("파일 작업 메뉴", "File actions")} style={{ left: x, top: y }}
    onContextMenu={event => event.preventDefault()} onKeyDown={event => {
      event.stopPropagation();
      const buttons = [...(menu.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
      const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
      if (event.key === 'Escape' || event.key === 'Tab') { event.preventDefault(); close(); return; }
      const index = event.key === 'ArrowDown' ? (current + 1) % buttons.length
        : event.key === 'ArrowUp' ? (current - 1 + buttons.length) % buttons.length
        : event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : null;
      if (index !== null) { event.preventDefault(); buttons[index]?.focus(); }
    }}>
    {actions.map(action => <button role="menuitem" type="button" key={action.label} title={action.description} disabled={action.disabled} aria-keyshortcuts={action.shortcut}
      onClick={() => { close(); action.run(); }}><span>{action.label}</span>{action.shortcut && <kbd aria-hidden="true">{action.shortcut}</kbd>}</button>)}
  </div>;
}
