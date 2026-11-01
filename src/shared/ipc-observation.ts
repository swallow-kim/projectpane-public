import type { Identity } from './contracts';

const commands = ['start_listing', 'read_listing', 'cancel_listing', 'set_listing_view', 'locate_entry', 'activate_entry'] as const;
export type IpcCommand = typeof commands[number];
type Phase = 'begin' | 'success' | 'failure';
type Completion = (phase: Exclude<Phase, 'begin'>) => void;
type Metadata = Pick<Identity, 'pane' | 'tabId' | 'generation'> & { readonly command: IpcCommand };
type Event = Metadata & { readonly seq: number; readonly callId: number; readonly phase: Phase };
type Counts = Record<Phase, number>;
const capacity = 1024;
let enabled = false;
let session = Symbol();
let events: Event[] = [];
let aggregates = new Map<IpcCommand, Counts>();
let dropped = 0;
let lastSeq = 0;
let observerErrors = 0;

function clear(): void {
  session = Symbol();
  events = [];
  aggregates = new Map();
  dropped = 0;
}

function failObservation(): void {
  enabled = false;
  clear();
  observerErrors = Math.min(Number.MAX_SAFE_INTEGER, observerErrors + 1);
}

function record(metadata: Metadata, phase: Phase, callId: number): number {
  if (lastSeq === Number.MAX_SAFE_INTEGER) {
    failObservation();
    return lastSeq;
  }
  const seq = ++lastSeq;
  const counts = aggregates.get(metadata.command) ?? { begin: 0, success: 0, failure: 0 };
  counts[phase]++;
  aggregates.set(metadata.command, counts);
  if (events.length < capacity) events.push({ ...metadata, phase, seq, callId });
  else dropped++;
  return seq;
}

export function beginIpcObservation(command: IpcCommand, identity: Identity): Completion | undefined {
  if (!enabled) return undefined;
  try {
    if (!commands.includes(command)) return undefined;
    const token = session;
    const metadata: Metadata = { command, pane: identity.pane, tabId: identity.tabId, generation: identity.generation };
    const callId = record(metadata, 'begin', lastSeq + 1);
    let completed = false;
    return phase => {
      if (!enabled || token !== session || completed) return;
      completed = true;
      try { record(metadata, phase, callId); }
      catch { failObservation(); }
    };
  } catch {
    failObservation();
    return undefined;
  }
}

const control = Object.freeze({
  enable(): void {
    clear();
    observerErrors = 0;
    enabled = true;
  },
  disable(): void {
    enabled = false;
    clear();
    observerErrors = 0;
  },
  snapshot() {
    return Object.freeze({
      enabled, capacity, dropped, lastSeq, observerErrors,
      events: Object.freeze(events.map(row => Object.freeze({ ...row }))),
      commands: Object.freeze(commands.map(command => Object.freeze({
        command, ...(aggregates.get(command) ?? { begin: 0, success: 0, failure: 0 }),
      }))),
    });
  },
});

declare global {
  interface Window {
    readonly __PROJECTPANE_IPC_OBSERVATION__?: typeof control;
  }
}

if (typeof window !== 'undefined') {
  try {
    Object.defineProperty(window, '__PROJECTPANE_IPC_OBSERVATION__', {
      value: control, enumerable: false, writable: false, configurable: false,
    });
  } catch { failObservation(); }
}
