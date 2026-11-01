import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { desktop } from '../../shared/ipc';
import { parseOperationState, terminalOperation, type OperationState } from '../../shared/operations';

export function useOperationState(onFinished: () => void) {
  const [state, setState] = useState<OperationState | null>(null);
  const [error, setError] = useState(false);
  const [version, refresh] = useState(0);
  const latest = useRef(onFinished);
  latest.current = onFinished;
  const seen = useRef<Set<string> | null>(null);
  useEffect(() => {
    if (!desktop) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      let interval = 2000;
      try {
        const next = parseOperationState(await invoke<unknown>('get_operation_state'));
        if (stopped) return;
        const finished = next.operations.filter(item => terminalOperation(item.status)).map(item => item.id);
        if (seen.current && finished.some(id => !seen.current?.has(id))) latest.current();
        seen.current = new Set(finished);
        setState(next); setError(false);
        if (next.operations.some(item => !terminalOperation(item.status))) interval = 250;
      } catch { if (!stopped) setError(true); }
      if (!stopped) timer = setTimeout(() => { void poll(); }, interval);
    };
    void poll();
    return () => { stopped = true; clearTimeout(timer); };
  }, [version]);
  return { state, error, available: !error && state?.available === true, refresh: () => refresh(value => value + 1) };
}
