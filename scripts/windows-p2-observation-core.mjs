export const GROUPS = ['synthetic-tab-switch', 'native-tab-switch', 'native-folder-navigation'];
export const TIMEOUT_MS = 10_000;
export const TIMESTAMP_TOLERANCE_MS = 1;

export function makePlan() {
  return GROUPS.flatMap(group => Array.from({ length: 30 }, (_, index) => ({
    group, index, sampleId: `${group}:${String(index + 1).padStart(2, '0')}`,
    key: group === 'synthetic-tab-switch' ? 'ArrowLeft' : group === 'native-tab-switch' ? 'ArrowRight' : 'Enter',
  })));
}

export async function bounded(operation, label, timeout = TIMEOUT_MS) {
  let timer;
  try {
    return await Promise.race([operation, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`timeout:${label}`)), timeout);
    })]);
  } finally { clearTimeout(timer); }
}

export function classifySample(plan, raw) {
  const missing = reason => ({ status: 'UNVERIFIED', durationMs: null, reason });
  if (!raw || !raw.supported?.includes('first-input') || !raw.supported?.includes('event')) return missing('unsupported_or_missing');
  if (raw.sampleId !== plan.sampleId || !raw.documentId || !Number.isFinite(raw.timeOrigin)) return missing('identity');
  const same = value => value.sampleId === plan.sampleId && value.documentId === raw.documentId && value.timeOrigin === raw.timeOrigin;
  const inputs = raw.inputs ?? [];
  const input = inputs[0];
  const submitClicks = inputs.slice(1);
  if (!input || !same(input) || !input.trusted || input.type !== 'keydown' || input.key !== plan.key ||
    input.target !== plan.sampleId) return missing('trusted_input_identity');
  if (submitClicks.length > 1 || submitClicks.some(click => plan.key !== 'Enter' || !same(click) || !click.trusted ||
    click.type !== 'click' || !click.keyboardSubmit || click.detail !== 0 || !Number.isFinite(click.startTime) ||
    click.startTime < input.startTime || click.startTime > raw.readAt)) return missing('extra_trusted_input');
  if (![raw.armedAt, raw.readAt, input.startTime].every(Number.isFinite) ||
    input.startTime < raw.armedAt || input.startTime > raw.readAt) return missing('timestamp_window');
  const first = (raw.entries ?? []).filter(entry => entry.entryType === 'first-input');
  const entry = first[0];
  if (first.length !== 1 || !same(entry) || entry.name !== 'keydown' || entry.target !== input.target ||
    !Number.isFinite(entry.startTime) || Math.abs(entry.startTime - input.startTime) > TIMESTAMP_TOLERANCE_MS ||
    entry.startTime < raw.armedAt || entry.startTime > raw.readAt) return missing('first_input_match');
  if (![entry.duration, entry.processingStart, entry.processingEnd].every(Number.isFinite) || entry.duration < 0 ||
    entry.processingStart < entry.startTime || entry.processingEnd < entry.processingStart ||
    !Number.isSafeInteger(entry.interactionId) || entry.interactionId <= 0) return missing('entry_values');
  return { status: entry.duration <= 100 ? 'PASS' : 'FAIL', durationMs: entry.duration, reason: null };
}

export function summarize(rows) {
  const count = (values, key) => values.reduce((map, value) => map.set(key(value), (map.get(key(value)) ?? 0) + 1), new Map());
  const sampleCounts = count(rows, row => row.sampleId);
  const documentCounts = count(rows.filter(row => row.raw), row => row.raw.documentId);
  const classified = makePlan().map(plan => {
    const row = rows.find(value => value.sampleId === plan.sampleId);
    const result = classifySample(plan, row?.raw);
    const invalid = sampleCounts.get(plan.sampleId) !== 1 || (row?.raw && documentCounts.get(row.raw.documentId) !== 1);
    return { ...plan, ready: false, raw: null, attempted: Boolean(row), ...(row ?? {}), ...result, ...(invalid || !row?.ready || row?.error ? {
      status: 'UNVERIFIED', durationMs: null, reason: invalid ? 'missing_or_duplicate_identity' : row?.error ? 'sample_failure' : 'readiness',
    } : {}) };
  });
  const groups = GROUPS.map(group => {
    const samples = classified.filter(row => row.group === group);
    const durations = samples.filter(row => row.durationMs !== null).map(row => row.durationMs).sort((a, b) => a - b);
    const complete = durations.length === 30;
    const p95Ms = complete ? durations[Math.ceil(30 * .95) - 1] : null;
    return { group, planned: 30, reported: durations.length, missing: 30 - durations.length, p95Ms,
      status: complete ? p95Ms <= 100 ? 'PASS' : 'FAIL' : 'UNVERIFIED' };
  });
  const unexpected = rows.some(row => !makePlan().some(plan => plan.sampleId === row.sampleId));
  return { status: unexpected || groups.some(group => group.status === 'UNVERIFIED') ? 'UNVERIFIED'
    : groups.some(group => group.status === 'FAIL') ? 'FAIL' : 'PASS', groups, rows: classified };
}

// Serialized by addInitScript: keep this function independent of module scope.
export function installTiming() {
  const state = { documentId: crypto.randomUUID(), timeOrigin: performance.timeOrigin, sampleId: null,
    supported: [...PerformanceObserver.supportedEntryTypes], armedAt: null, inputs: [], entries: [], target: null };
  const identity = () => ({ sampleId: state.sampleId, documentId: state.documentId, timeOrigin: state.timeOrigin });
  const targetId = target => target && target === state.target ? state.sampleId : null;
  const observers = [];
  for (const type of ['first-input', 'event']) {
    if (!state.supported.includes(type)) continue;
    const observer = new PerformanceObserver(list => {
      for (const entry of list.getEntries()) state.entries.push({ ...identity(), entryType: entry.entryType,
        name: entry.name, target: targetId(entry.target), startTime: entry.startTime, duration: entry.duration,
        processingStart: entry.processingStart, processingEnd: entry.processingEnd, interactionId: entry.interactionId });
    });
    observer.observe(type === 'event' ? { type, buffered: true, durationThreshold: 0 } : { type, buffered: true });
    observers.push(observer);
  }
  const capture = event => {
    if (event.isTrusted) state.inputs.push({ ...identity(), type: event.type, key: event.key ?? null,
      target: targetId(event.target), trusted: true, startTime: event.timeStamp, detail: event.detail ?? null,
      keyboardSubmit: event.type === 'click' && event.detail === 0 && event.target instanceof HTMLButtonElement &&
        event.target.type === 'submit' && Boolean(state.target?.form) && event.target.form === state.target.form });
  };
  const types = ['keydown', 'pointerdown', 'mousedown', 'touchstart', 'click'];
  for (const type of types) document.addEventListener(type, capture, true);
  window.__p2Timing = {
    entries: state.entries,
    arm(sampleId, target) {
      if (state.sampleId !== null || state.inputs.length || state.entries.some(entry => entry.entryType === 'first-input')) {
        throw new Error('document_already_used');
      }
      if (!target || document.activeElement !== target) throw new Error('target_not_focused');
      state.target = target; state.sampleId = sampleId; state.armedAt = performance.now();
    },
    snapshot() { const { target, ...raw } = state; return { ...raw, readAt: performance.now() }; },
    stop() {
      for (const observer of observers) observer.disconnect();
      for (const type of types) document.removeEventListener(type, capture, true);
      state.target = null;
      return { documentId: state.documentId, disconnectedObservers: observers.length, removedListeners: types.length };
    },
  };
}
