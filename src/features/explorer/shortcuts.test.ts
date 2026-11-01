import { describe, expect, it } from 'vitest';
import { paneShortcut, targetPane, type ShortcutKey } from './shortcuts';

const key: ShortcutKey = { key: 'F6', ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, isComposing: false, keyCode: 117 };

describe('pane shortcuts', () => {
  it.each([
    [{ ...key }, 'next'],
    [{ ...key, shiftKey: true }, 'previous'],
    [{ ...key, key: 'l', ctrlKey: true }, 'address'],
    [{ ...key, key: 'L', ctrlKey: true }, 'address'],
    [{ ...key, key: 'Escape' }, 'cancel'],
    [{ ...key, ctrlKey: true }, null],
    [{ ...key, altKey: true }, null],
    [{ ...key, metaKey: true }, null],
    [{ ...key, key: 'Escape', shiftKey: true }, null],
    [{ ...key, isComposing: true }, null],
    [{ ...key, keyCode: 229 }, null],
  ])('routes %o to %s', (event, expected) => {
    expect(paneShortcut(event, false)).toBe(expected);
  });

  it('ignores every shortcut during the composition lifecycle', () => {
    for (const event of [key, { ...key, key: 'Escape' }, { ...key, key: 'l', ctrlKey: true }]) {
      expect(paneShortcut(event, true)).toBeNull();
    }
  });

  it('defaults to the first pane and wraps in both directions', () => {
    expect(targetPane(null, 'next', 4)).toBe(0);
    expect(targetPane(null, 'previous', 4)).toBe(0);
    expect(targetPane(null, 'address', 4)).toBe(0);
    expect(targetPane(3, 'next', 4)).toBe(0);
    expect(targetPane(0, 'previous', 4)).toBe(3);
    expect(targetPane(2, 'cancel', 4)).toBe(2);
    expect(targetPane(2, 'address', 4)).toBe(2);
  });
});

it.each([
  [{ ...key, key: 't', ctrlKey: true }, 'new-tab'],
  [{ ...key, key: 'T', ctrlKey: true, shiftKey: true }, 'reopen-tab'],
  [{ ...key, key: 'P', ctrlKey: true, shiftKey: true }, 'palette'],
  [{ ...key, key: 'w', ctrlKey: true }, 'close-tab'],
  [{ ...key, key: 'ArrowLeft', altKey: true }, 'back'],
  [{ ...key, key: 'ArrowRight', altKey: true }, 'forward'],
  [{ ...key, key: 'ArrowUp', altKey: true }, 'up'],
  [{ ...key, key: 'F5' }, 'refresh'],
])('routes P2 action %o with composition and handled-event guards', (event, expected) => {
  expect(paneShortcut(event, false)).toBe(expected);
  expect(paneShortcut(event, true)).toBeNull();
  expect(paneShortcut({ ...event, defaultPrevented: true }, false)).toBeNull();
  expect(paneShortcut({ ...event, isComposing: true }, false)).toBeNull();
  expect(paneShortcut({ ...event, keyCode: 229 }, false)).toBeNull();
});

it('cycles only visible panes after a layout reduction', () => {
  expect(targetPane(1, 'next', 2)).toBe(0);
  expect(targetPane(0, 'previous', 2)).toBe(1);
  expect(targetPane(3, 'address', 1)).toBe(0);
  expect(targetPane(0, 'next', 1)).toBe(0);
});
