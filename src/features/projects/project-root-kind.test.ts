import { describe, expect, it } from 'vitest';
import { rootKind } from './project-root-kind';

describe('project root display kind', () => {
  it.each([
    'C:\\',
    'd:\\Work\\project',
    'G:/내 드라이브/작업',
    'Z:\\mapped-or-local',
    '\\\\?\\C:\\Work',
    'C:\\Work\\..\\Other',
  ])('identifies Windows drive syntax without claiming the drive is local: %s', path => {
    expect(rootKind(path)).toBe('windows');
  });

  it.each([
    '\\\\wsl$\\Ubuntu',
    '\\\\wsl.localhost\\Ubuntu\\home\\User',
    '\\\\WSL$\\Ubuntu\\home\\user',
    '\\\\WsL.LoCaLhOsT\\Debian\\home\\한글',
    '//wsl.localhost/Ubuntu/home/User',
    '\\\\?\\UNC\\wsl$\\Ubuntu\\home\\User',
    '\\\\?\\unc\\WSL.LOCALHOST\\Ubuntu\\home\\user',
  ])('identifies only the exact WSL host aliases: %s', path => {
    expect(rootKind(path)).toBe('wsl');
  });

  it.each([
    '\\\\server\\share',
    '\\\\192.168.0.20\\share\\project',
    '//nas/department/project',
    '\\\\?\\UNC\\server\\share',
    '\\\\wsl.localhost.example.com\\share',
    '\\\\my-wsl$\\share',
  ])('identifies a network UNC path: %s', path => {
    expect(rootKind(path)).toBe('network');
  });

  it.each([
    '', 'relative\\folder', '/home/user', 'C:relative', '\\root-relative',
    '\\\\server', '\\\\server\\', '\\\\wsl$', '\\\\wsl.localhost\\',
    '\\\\.\\PhysicalDrive0', '\\\\?\\Volume{123}\\', '\\\\?\\GLOBALROOT\\Device',
    '\\\\?\\UNC\\wsl$\\Ubuntu\\a\\..\\b', '\\\\?\\C:\\a\\.\\b',
    'file://server/share', 'https://example.com/folder', 'ssh://host/home',
    'C:\\bad\u0000name', 'C:\\bad\ud800name', 'C:\\CON', 'C:\\trailing.',
    '\\\\server\\share\\..\\..\\escape',
  ])('uses the unknown fallback for unsupported or incomplete paths: %s', path => {
    expect(rootKind(path)).toBe('unknown');
  });

  it('does not change the supplied path or confuse case-sensitive WSL tails', () => {
    const input = '\\\\WsL.LoCaLhOsT\\Ubuntu\\home\\User\\한글';
    const before = input;
    expect(rootKind(input)).toBe('wsl');
    expect(input).toBe(before);
    expect(rootKind(input.replace('User', 'user'))).toBe('wsl');
  });

  it('does not throw on invalid runtime input or excessive paths', () => {
    expect(rootKind(null as unknown as string)).toBe('unknown');
    expect(rootKind(42 as unknown as string)).toBe('unknown');
    expect(rootKind('C:\\' + 'a'.repeat(32769))).toBe('unknown');
  });
});
