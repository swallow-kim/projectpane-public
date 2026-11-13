import { useSyncExternalStore } from 'react';

export type Language = 'ko' | 'en';
export const LANGUAGE_STORAGE_KEY = 'projectpane.language.v1';
export function parseLanguage(value: unknown): Language { return value === 'en' ? 'en' : 'ko'; }
function readLanguage(): Language {
  try { return parseLanguage(globalThis.localStorage?.getItem(LANGUAGE_STORAGE_KEY)); } catch { return 'ko'; }
}
let language = readLanguage();
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
export const getLanguage = (): Language => language;
export const locale = (): string => language === 'ko' ? 'ko-KR' : 'en-US';
export function useLanguage(): Language { return useSyncExternalStore(subscribe, getLanguage, () => 'ko'); }
function applyLanguage(next: Language): void {
  language = next;
  if (typeof document !== 'undefined') document.documentElement.lang = next;
  listeners.forEach(listener => listener());
}
/** Applies immediately; false means this session works but persistence was unavailable. */
export function setLanguage(next: Language): boolean {
  if (next !== 'ko' && next !== 'en') return false;
  applyLanguage(next);
  try { globalThis.localStorage.setItem(LANGUAGE_STORAGE_KEY, next); return true; } catch { return false; }
}
export function t(ko: string, en: string, values?: Readonly<Record<string, string | number>>): string {
  const message = language === 'ko' ? ko : en;
  return values ? message.replace(/\{(\w+)\}/g, (token, key: string) => Object.hasOwn(values, key) ? String(values[key]) : token) : message;
}
if (typeof document !== 'undefined') document.documentElement.lang = language;
if (typeof window !== 'undefined' && typeof window.addEventListener === 'function') window.addEventListener('storage', event => {
  if (event.key === LANGUAGE_STORAGE_KEY || event.key === null) applyLanguage(readLanguage());
});
