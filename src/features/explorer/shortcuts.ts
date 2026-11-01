export type ShortcutKey = {
  readonly key: string;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly metaKey: boolean;
  readonly shiftKey: boolean;
  readonly isComposing: boolean;
  readonly keyCode: number;
  readonly defaultPrevented?: boolean;
};

export type PaneShortcut = 'address' | 'next' | 'previous' | 'cancel' | 'new-tab' | 'close-tab' | 'reopen-tab' | 'palette' | 'back' | 'forward' | 'up' | 'refresh';

export function paneShortcut(event: ShortcutKey, composing: boolean): PaneShortcut | null {
  if (event.defaultPrevented || composing || event.isComposing || event.keyCode === 229 || event.metaKey) return null;
  if (event.altKey) {
    if (event.ctrlKey || event.shiftKey) return null;
    switch (event.key) {
      case 'ArrowLeft': return 'back';
      case 'ArrowRight': return 'forward';
      case 'ArrowUp': return 'up';
      default: return null;
    }
  }
  if (event.ctrlKey) {
    if (event.shiftKey) return event.key.toLowerCase() === 't' ? 'reopen-tab' : event.key.toLowerCase() === 'p' ? 'palette' : null;
    switch (event.key.toLowerCase()) {
      case 'l': return 'address';
      case 't': return 'new-tab';
      case 'w': return 'close-tab';
      default: return null;
    }
  }
  switch (event.key) {
    case 'F6': return event.shiftKey ? 'previous' : 'next';
    case 'Escape': return event.shiftKey ? null : 'cancel';
    case 'F5': return event.shiftKey ? null : 'refresh';
    default: return null;
  }
}

export function targetPane(active: number | null, shortcut: PaneShortcut, count: number): number {
  if (active === null || active >= count) return 0;
  switch (shortcut) {
    case 'next': return (active + 1) % count;
    case 'previous': return (active + count - 1) % count;
    default: return active;
  }
}
