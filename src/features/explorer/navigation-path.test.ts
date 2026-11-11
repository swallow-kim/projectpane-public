import { describe, expect, it } from 'vitest';
import { resolveNavigationPath } from './navigation-path';

describe('project relative navigation', () => {
  it.each([
    ['src/assets', 'C:\\Project', 'C:\\Project\\src\\assets'],
    ['.', 'C:\\Project', 'C:\\Project'],
    ['..\\sibling', 'C:\\Project', 'C:\\sibling'],
    ['src', '\\\\server\\share\\project', '\\\\server\\share\\project\\src'],
    ['../Other', '\\\\wsl.localhost\\Ubuntu\\home\\Project', '\\\\wsl.localhost\\Ubuntu\\home\\Other'],
    ['src', 'C:\\', 'C:\\src'],
  ])('resolves %s from %s', (input, root, expected) => expect(resolveNavigationPath(input, root)).toBe(expected));
  it.each(['D:\\Elsewhere', '\\\\server\\share\\other', '\\\\wsl$\\Ubuntu\\home\\Me'])('preserves explicit absolute path %s', input => {
    expect(resolveNavigationPath(input, 'C:\\Project')).toBe(input);
  });
  it.each(['C:relative', '\\ambiguous', '..\\..\\escape'])('rejects ambiguous or above-volume input %s', input => {
    expect(() => resolveNavigationPath(input, 'C:\\Project')).toThrow();
  });
  it('does not change navigation without a selected project', () => expect(resolveNavigationPath('relative')).toBe('relative'));
});
