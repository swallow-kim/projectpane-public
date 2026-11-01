import { describe, expect, it } from 'vitest';
import { ImeShortcutGuard } from './ime-shortcut-guard';
import type { ShortcutKey } from './shortcuts';

const enter = { key: 'Enter', code: 'Enter', ctrlKey: false, altKey: false, metaKey: false,
  shiftKey: false, isComposing: false, keyCode: 13 } satisfies ShortcutKey & { readonly code: string };
const actions = [
  enter, { ...enter, code: 'NumpadEnter' }, { ...enter, key: 'F6', code: 'F6' },
  { ...enter, key: 'F6', code: 'F6', shiftKey: true }, { ...enter, key: 'F5', code: 'F5' },
  { ...enter, key: 'Escape', code: 'Escape' },
  ...['l', 't', 'w', 'c', 'x', 'v'].map(key => ({ ...enter, key, code: `Key${key.toUpperCase()}`, ctrlKey: true })),
  ...['t', 'p'].map(key => ({ ...enter, key, code: `Key${key.toUpperCase()}`, ctrlKey: true, shiftKey: true })),
  ...['ArrowLeft', 'ArrowRight', 'ArrowUp'].map(key => ({ ...enter, key, code: key, altKey: true })),
];

describe('IME physical shortcut latch', () => {
  it.each(actions)('blocks the $code action echo until its physical release', event => {
    const guard = new ImeShortcutGuard();
    // Given a Process event handled by the IME without cancelling its default.
    expect(guard.shouldSuppress({ ...event, key: 'Process', isComposing: true, keyCode: 229 }, false)).toBe(false);
    // When composition ends before the physical key is released.
    expect(guard.shouldSuppress(event, false)).toBe(true);
    expect(guard.shouldSuppress(event, false)).toBe(true);
    guard.keyup(event.code);
    // Then a new press works immediately.
    expect(guard.shouldSuppress(event, false)).toBe(false);
  });

  it.each([
    { event: enter, composing: true },
    { event: { ...enter, isComposing: true }, composing: false },
    { event: { ...enter, key: 'Process', keyCode: 229 }, composing: false },
  ])('arms for each composition signal: %o', ({ event, composing }) => {
    const guard = new ImeShortcutGuard();
    expect(guard.shouldSuppress(event, composing)).toBe(false);
    expect(guard.shouldSuppress(enter, false)).toBe(true);
  });

  it('keeps composing repeats available to the IME', () => {
    const guard = new ImeShortcutGuard();
    guard.shouldSuppress(enter, true);
    expect(guard.shouldSuppress(enter, true)).toBe(false);
    expect(guard.shouldSuppress(enter, false)).toBe(true);
  });

  it('releases only the matching physical code, including numpad separation', () => {
    const guard = new ImeShortcutGuard();
    guard.shouldSuppress(enter, true);
    guard.shouldSuppress({ ...enter, code: 'NumpadEnter' }, true);
    guard.keyup('ShiftLeft');
    expect(guard.shouldSuppress(enter, false)).toBe(true);
    guard.keyup('NumpadEnter');
    expect(guard.shouldSuppress(enter, false)).toBe(true);
    expect(guard.shouldSuppress({ ...enter, code: 'NumpadEnter' }, false)).toBe(false);
  });

  it('clears every latch when the window loses focus', () => {
    const guard = new ImeShortcutGuard();
    for (const event of actions) guard.shouldSuppress(event, true);
    guard.reset();
    for (const event of actions) expect(guard.shouldSuppress(event, false)).toBe(false);
  });

  it.each(['', 'Unidentified'])('does not fall back to key when code is %j', code => {
    const guard = new ImeShortcutGuard();
    guard.shouldSuppress({ ...enter, code }, true);
    expect(guard.shouldSuppress({ ...enter, code }, false)).toBe(false);
    expect(guard.shouldSuppress(enter, false)).toBe(false);
  });

  it.each([
    { key: 't', code: 'KeyT' }, { key: ' ', code: 'Space' }, { key: 'Backspace', code: 'Backspace' },
    { key: 'ArrowLeft', code: 'ArrowLeft' }, { key: 'Home', code: 'Home' },
    { key: 'F6', code: 'F6', ctrlKey: true }, { key: 'F5', code: 'F5', shiftKey: true },
    { key: 'Escape', code: 'Escape', shiftKey: true }, { key: 'p', code: 'KeyP', ctrlKey: true },
    { key: 'l', code: 'KeyL', metaKey: true }, { key: 'ArrowUp', code: 'ArrowUp', altKey: true, shiftKey: true },
  ])('does not arm unsupported chord $code: %o', values => {
    const guard = new ImeShortcutGuard();
    const event = { ...enter, ...values };
    guard.shouldSuppress(event, true);
    expect(guard.shouldSuppress(event, false)).toBe(false);
    const supported = actions.find(action => action.code === event.code);
    if (supported) expect(guard.shouldSuppress(supported, false)).toBe(false);
  });

  it('allows ordinary text after a modifier is released, without releasing its shortcut latch', () => {
    const guard = new ImeShortcutGuard();
    const shortcut = { ...enter, key: 't', code: 'KeyT', ctrlKey: true };
    guard.shouldSuppress(shortcut, true);
    guard.keyup('ControlLeft');
    expect(guard.shouldSuppress({ ...shortcut, ctrlKey: false }, false)).toBe(false);
    expect(guard.shouldSuppress(shortcut, false)).toBe(true);
  });

  it('leaves normal shortcuts alone when no composition keydown armed them', () => {
    const guard = new ImeShortcutGuard();
    for (const event of actions) expect(guard.shouldSuppress(event, false)).toBe(false);
  });
});
