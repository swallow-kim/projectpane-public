import { useCallback, useState } from 'react';
export type OperationMessage = (() => string) | null;
/** Store message factories so changing language also updates an existing notice. */
export function useOperationMessage(): readonly [OperationMessage, (message: OperationMessage) => void] {
  const [message, update] = useState<OperationMessage>(null);
  const setMessage = useCallback((next: OperationMessage) => update(() => next), []);
  return [message, setMessage] as const;
}
