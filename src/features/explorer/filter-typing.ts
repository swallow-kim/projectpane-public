// Space remains the list's selection shortcut; typing in the filter itself is native.
export function filterTypingKey(event: {
  key: string; ctrlKey: boolean; altKey: boolean; metaKey: boolean; repeat: boolean;
}): boolean {
  return !event.ctrlKey && !event.altKey && !event.metaKey && !event.repeat
    && Array.from(event.key).length === 1 && !/[\s\p{C}]/u.test(event.key);
}
