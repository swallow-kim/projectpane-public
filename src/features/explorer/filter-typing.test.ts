import { expect, it } from 'vitest';
import { filterTypingKey } from './filter-typing';

const event = (key: string) => ({ key, ctrlKey: false, altKey: false, metaKey: false, repeat: false });
it('starts filter editing with one printable character, preserving the original case', () => {
  for (const key of ['a', 'B', '1', '.', '한', '😀']) expect(filterTypingKey(event(key))).toBe(true);
});
it('leaves navigation, Space selection, dead keys and controls untouched', () => {
  for (const key of [' ', '\t', '\n', 'Dead', 'Process', 'Escape', 'F2', 'ArrowDown', '', 'ab']) expect(filterTypingKey(event(key))).toBe(false);
});
it('does not redirect shortcuts or an already held key', () => {
  for (const modifier of ['ctrlKey', 'altKey', 'metaKey', 'repeat']) expect(filterTypingKey({ ...event('a'), [modifier]: true })).toBe(false);
});
