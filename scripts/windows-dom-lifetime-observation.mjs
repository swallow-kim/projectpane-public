import assert from 'node:assert/strict';

// Serialized into the owned synthetic test page. Records never contain strong
// references to targets or handlers. Do not add console logging of targets.
export function installDomLifetimeObservation() {
  if (Reflect.has(window, '__projectpaneLifetimeProbe')) throw Error('probe_already_installed');
  if (typeof WeakRef !== 'function' || typeof FinalizationRegistry !== 'function') throw Error('weak_observation_unavailable');
  const records = [], finalized = [], ids = new WeakMap();
  const registry = new FinalizationRegistry(id => finalized.push({ id, at: performance.now() }));
  const track = (label, target) => {
    if (!target || !['object', 'function'].includes(typeof target)) return null;
    const existing = ids.get(target);
    if (existing !== undefined) { records[existing].labels.push(label); return existing; }
    if (records.length >= 256) throw Error('weak_observation_limit');
    const id = records.length;
    records.push({ id, labels: [label], weak: new WeakRef(target) }); ids.set(target, id); registry.register(target, id);
    return id;
  };
  const storeFor = element => {
    const key = Object.keys(element).find(key => key.startsWith('__reactFiber$'));
    let fiber = key && element[key];
    for (let depth = 0; fiber && depth < 32; depth++, fiber = fiber.return) {
      let hook = fiber.memoizedState;
      for (let n = 0; hook && n < 80; n++, hook = hook.next) {
        const value = hook.memoizedState;
        if (value?.listing?.displayCache?.pages instanceof Map && typeof value.subscribe === 'function') return value;
      }
    }
    throw Error('listing_store_shape_unavailable');
  };
  const capture = label => {
    const ids = [];
    for (let pane = 0; pane < 4; pane++) {
      const root = document.querySelector(`[data-pane="${pane}"]`), list = root?.querySelector('[role=listbox]');
      if (!list) throw Error('list_missing');
      const store = storeFor(list);
      for (const [kind, target] of Object.entries({ list, toolbar: root.querySelector('.view-toolbar'), firstRow: list.querySelector('[aria-posinset]'),
        store, listing: store.listing, state: store.listing.state, cache: store.listing.displayCache,
        firstPage: store.listing.displayCache.pages.values().next().value })) {
        ids.push(track(`${label}:pane-${pane}:${kind}`, target));
      }
    }
    return ids.filter(id => id !== null);
  };
  const snapshot = () => ({ at: performance.now(), finalized: [...finalized], records: records.map(({ id, labels, weak }) => {
    const target = weak.deref();
    return { id, labels: [...labels], alive: target !== undefined, node: target instanceof Node,
      connected: target instanceof Node ? target.isConnected : null,
      tag: target instanceof Element ? target.tagName : null,
      role: target instanceof Element ? target.getAttribute('role') : null,
      arrayLength: Array.isArray(target) ? target.length : null,
      cachePages: target?.pages instanceof Map ? target.pages.size : null };
  }), stores: [0,1,2,3].map(pane => {
    const list = document.querySelector(`[data-pane="${pane}"] [role=listbox]`), store = storeFor(list), listing = store.listing;
    return { pane, storeId: ids.get(store), listingId: ids.get(listing), total: listing.state.total, status: listing.state.status,
      cachePages: listing.displayCache.pages.size, cachedEntries: [...listing.displayCache.pages.values()].reduce((n, p) => n + p.length, 0),
      replacementPages: listing.replacement?.cache.pages.size ?? 0, queuedActions: listing.actions.tasks.length, pendingWindows: listing.windows.length };
  }) });
  const retainedControl = document.createElement('div'); retainedControl.addEventListener('diagnostic-control', () => {});
  const positive = track('control:retained', retainedControl);
  track('control:unretained', document.createElement('div'));
  Reflect.set(window, '__projectpaneLifetimeProbe', { capture, snapshot, get: id => records[id]?.weak.deref(),
    subscribe: pane => storeFor(document.querySelector(`[data-pane="${pane}"] [role=listbox]`)).subscribe,
    positive, retainedControl });
  return { positive, capacity: 256, weakOnlyRecords: true };
}

export async function captureListenerTargets(session, targets) {
  const group = `projectpane-lifetime-${Date.now()}`, handles = new Set(), output = [];
  try {
    for (const target of targets) {
      const evaluated = await session.send('Runtime.evaluate', { expression: target.expression, objectGroup: group, returnByValue: false });
      assert(!evaluated.exceptionDetails, 'listener target evaluation failed');
      const object = evaluated.result;
      if (!object.objectId) { output.push({ label: target.label, unavailable: object.type }); continue; }
      handles.add(object.objectId);
      const result = await session.send('DOMDebugger.getEventListeners', { objectId: object.objectId, depth: target.depth ?? 1 });
      assert(result.listeners.length <= 4096, 'listener observation bounded');
      const listeners = result.listeners.map(listener => {
        for (const handler of [listener.handler, listener.originalHandler]) if (handler?.objectId) handles.add(handler.objectId);
        return { type: listener.type, capture: listener.useCapture, passive: listener.passive, once: listener.once,
          scriptId: listener.scriptId, line: listener.lineNumber, column: listener.columnNumber, backendNodeId: listener.backendNodeId,
          handler: listener.originalHandler?.description ?? listener.handler?.description ?? null };
      });
      output.push({ label: target.label, count: listeners.length, listeners });
    }
    return output;
  } finally {
    // Release returned handler objects explicitly as well as the target group;
    // avoid relying on protocol-version-specific group inheritance.
    const releases = await Promise.allSettled([...handles].map(objectId => session.send('Runtime.releaseObject', { objectId })));
    await session.send('Runtime.releaseObjectGroup', { objectGroup: group });
    assert(releases.every(r => r.status === 'fulfilled'), 'all diagnostic handles released');
  }
}

export async function captureSubscriptionScopes(session, pane) {
  const group = `projectpane-subscription-${pane}-${Date.now()}`, handles = new Set();
  const properties = async objectId => {
    const value = await session.send('Runtime.getProperties', { objectId, ownProperties: true });
    for (const p of [...value.result, ...(value.internalProperties ?? [])]) if (p.value?.objectId) handles.add(p.value.objectId);
    return value;
  };
  try {
    const value = await session.send('Runtime.evaluate', { expression: `window.__projectpaneLifetimeProbe.subscribe(${pane})`, objectGroup: group });
    assert(value.result.objectId && !value.exceptionDetails); handles.add(value.result.objectId);
    const internals = await properties(value.result.objectId);
    const scopes = internals.internalProperties?.find(p => p.name === '[[Scopes]]')?.value.objectId;
    assert(scopes, 'subscription scopes unavailable');
    const result = [];
    for (const scope of (await properties(scopes)).result.filter(p => /^\d+$/.test(p.name)).slice(0, 4)) {
      if (!scope.value?.objectId) continue;
      if (!/^(Closure|Block|Local|Catch)(\b|\s|$)/.test(scope.value.description ?? '')) {
        result.push({ scope: scope.name, kind: scope.value.description, skipped: true, collections: [] }); continue;
      }
      const members = (await properties(scope.value.objectId)).result;
      // Only shape and collection size descriptions, never file names or payloads.
      result.push({ scope: scope.name, kind: scope.value.description, collections: members.filter(p => /^(Set|Map)\(/.test(p.value?.description ?? '')).map(p => ({ name: p.name, description: p.value.description })) });
    }
    return result;
  } finally {
    const releases = await Promise.allSettled([...handles].map(objectId => session.send('Runtime.releaseObject', { objectId })));
    await session.send('Runtime.releaseObjectGroup', { objectGroup: group });
    assert(releases.every(r => r.status === 'fulfilled'), 'subscription handles released');
  }
}
