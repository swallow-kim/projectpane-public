// One lightweight overlay shared by native and internal drags. It never changes
// a list's dimensions, scroll extent, or React selection state.
let highlighted: HTMLElement | null = null;
let hint: HTMLDivElement | null = null;

export function clearDropFeedback() {
  highlighted?.removeAttribute('data-drop-effect');
  highlighted = null;
  hint?.remove(); hint = null;
}

export function showDropFeedback(x: number, y: number, mode: 'copy' | 'move') {
  const list = document.elementFromPoint(x, y)?.closest<HTMLElement>('.file-list');
  if (!list) { clearDropFeedback(); return; }
  if (highlighted !== list) { clearDropFeedback(); highlighted = list; }
  list.dataset.dropEffect = mode;
  if (!hint) {
    hint = document.createElement('div'); hint.className = 'file-drop-hint';
    hint.setAttribute('role', 'status'); document.body.append(hint);
  }
  const message = mode === 'move' ? '놓으면 이동 위치를 확인합니다' : '놓으면 여기에 복사합니다';
  if (hint.textContent !== message) hint.textContent = message;
  const rect = list.getBoundingClientRect();
  hint.style.left = `${Math.max(4, rect.left + 8)}px`;
  hint.style.top = `${Math.max(4, rect.top + 8)}px`;
  hint.style.maxWidth = `${Math.max(0, rect.width - 16)}px`;
}
