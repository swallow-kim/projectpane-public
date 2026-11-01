import { paneShortcut, type ShortcutKey } from './shortcuts';

function actionKey(code: string): string | null {
  switch (code) {
    case 'Enter': case 'NumpadEnter': return 'Enter';
    case 'F6': case 'F5': case 'Escape':
    case 'ArrowLeft': case 'ArrowRight': case 'ArrowUp': return code;
    case 'KeyL': return 'l';
    case 'KeyT': return 't';
    case 'KeyW': return 'w';
    case 'KeyP': return 'p';
    case 'KeyC': return 'c';
    case 'KeyX': return 'x';
    case 'KeyV': return 'v';
    default: return null;
  }
}

export class ImeShortcutGuard {
  private readonly held = new Set<string>();

  shouldSuppress(event: ShortcutKey & { readonly code: string }, composing: boolean): boolean {
    const key = actionKey(event.code);
    const transfer = key !== null && ['c', 'x', 'v'].includes(key) && event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey;
    if (key === null || (!transfer && key !== 'Enter' && paneShortcut({ key, ctrlKey: event.ctrlKey,
      altKey: event.altKey, metaKey: event.metaKey, shiftKey: event.shiftKey,
      isComposing: false, keyCode: 0 }, false) === null)) return false;
    if (composing || event.isComposing || event.keyCode === 229) {
      this.held.add(event.code);
      return false;
    }
    return this.held.has(event.code);
  }

  keyup(code: string): void { this.held.delete(code); }

  reset(): void { this.held.clear(); }
}
