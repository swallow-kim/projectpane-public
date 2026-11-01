// WebView2 native file drops disable HTML5 dragging. Keep panel gestures in the
// pointer layer; enter the Shell drag loop only when leaving the app.
import { clearDropFeedback, showDropFeedback } from './drop-feedback';

export function trackPointerTransfer(start: PointerEvent, callbacks: {
  started: () => void;
  target: (x: number, y: number) => string | null;
  internal: (root: string, move: boolean) => void;
  external: () => void;
}) {
  let dragging = false;
  let cancelled = false;
  let ended = false;
  const clearVisuals = () => {
    document.body.classList.remove('file-pointer-drag');
    delete document.body.dataset.dragEffect;
    clearDropFeedback();
  };
  const stop = () => {
    ended = true;
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', up);
    window.removeEventListener('pointercancel', cancel);
    window.removeEventListener('blur', stop);
    window.removeEventListener('pointerout', out);
    window.removeEventListener('keydown', key);
    clearVisuals();
  };
  const external = () => { if (dragging) { stop(); callbacks.external(); } };
  const move = (event: PointerEvent) => {
    if (event.pointerId !== start.pointerId || cancelled) return;
    if (!(event.buttons & 1)) { stop(); return; }
    if (!dragging && Math.hypot(event.clientX - start.clientX, event.clientY - start.clientY) >= 6) {
      dragging = true; document.body.classList.add('file-pointer-drag');
      callbacks.started();
    }
    if (dragging) {
      event.preventDefault();
      document.body.dataset.dragEffect = event.shiftKey ? 'move' : 'copy';
      if (callbacks.target(event.clientX, event.clientY)) showDropFeedback(event.clientX, event.clientY, event.shiftKey ? 'move' : 'copy');
      else clearDropFeedback();
    }
    if (event.clientX < 0 || event.clientY < 0 || event.clientX >= innerWidth || event.clientY >= innerHeight) external();
  };
  const out = (event: PointerEvent) => {
    if (event.pointerId !== start.pointerId || event.relatedTarget !== null || cancelled) return;
    if (!(event.buttons & 1)) stop(); else external();
  };
  const cancel = (event: PointerEvent) => { if (event.pointerId === start.pointerId) stop(); };
  const up = (event: PointerEvent) => {
    if (event.pointerId !== start.pointerId) return;
    const root = dragging && !cancelled ? callbacks.target(event.clientX, event.clientY) : null;
    if (dragging) {
      const click = (event: MouseEvent) => { event.preventDefault(); event.stopImmediatePropagation(); };
      window.addEventListener('click', click, { capture: true, once: true });
      setTimeout(() => window.removeEventListener('click', click, true), 0);
    }
    stop();
    if (root) callbacks.internal(root, event.shiftKey);
  };
  const key = (event: KeyboardEvent) => {
    if (event.key !== 'Escape') return;
    if (!dragging) { stop(); return; }
    // Retain the release listener until this gesture ends so its generated
    // click cannot replace the selected group after an Escape cancellation.
    cancelled = true;
    clearVisuals();
  };
  window.addEventListener('pointermove', move, { passive: false });
  window.addEventListener('pointerup', up);
  window.addEventListener('pointercancel', cancel);
  window.addEventListener('blur', stop);
  window.addEventListener('pointerout', out);
  window.addEventListener('keydown', key);
  return () => { if (!ended) stop(); };
}
