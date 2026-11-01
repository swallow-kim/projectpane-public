const normal = value => value?.replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();

export function assess(id, events, before, after, child, { allowCorrelatedUntrustedEnd = false } = {}) {
  const [target, action] = id.split(':');
  const targeted = events.filter(e => e.target === target);
  const trusted = targeted.filter(e => e.trusted);
  const composingAtKey = new Map();
  let active = false;
  for (const event of targeted) {
    if (event.type === 'compositionstart' && event.trusted) active = true;
    if (event.type === 'compositionend') active = false;
    if (event.type === 'keydown') composingAtKey.set(event, active);
  }
  const composing = action.startsWith('composing-');
  const key = action.endsWith('ctrl-l') ? e => e.ctrlKey && (e.key?.toLowerCase() === 'l' || e.code === 'KeyL')
    : e => [e.key, e.code].includes(action.endsWith('f6') ? 'F6' : action.endsWith('escape') ? 'Escape' : 'Enter');
  const candidates = trusted.filter(e => e.type === 'keydown' && key(e)
    && (!action.endsWith('f6') || !(e.shiftKey || e.ctrlKey || e.altKey || e.metaKey))
    && (composing ? (composingAtKey.get(e) || e.isComposing || e.keyCode === 229)
      : (!composingAtKey.get(e) && !e.isComposing && e.keyCode !== 229)));
  function sequence(press) {
    const at = targeted.indexOf(press);
    const start = targeted.findLastIndex((e, i) => i < at && e.type === 'compositionstart' && e.trusted);
    const update = targeted.findIndex((e, i) => i > start && i < at && e.type === 'compositionupdate' && e.trusted);
    const end = targeted.findIndex((e, i) => i > update && e.type === 'compositionend');
    const nextStart = targeted.findIndex((e, i) => i > start && e.type === 'compositionstart');
    const endEvent = targeted[end];
    const inputIndex = targeted.findLastIndex((e, i) => i > update && i < end && e.type === 'input');
    const beforeIndex = targeted.findLastIndex((e, i) => i > update && i < inputIndex && e.type === 'beforeinput');
    const input = targeted[inputIndex], beforeInput = targeted[beforeIndex];
    const endCorroborated = Boolean(allowCorrelatedUntrustedEnd && endEvent?.trusted === false
      && input?.trusted && input.isComposing && input.inputType === 'insertCompositionText'
      && beforeInput?.trusted && beforeInput.isComposing && beforeInput.inputType === input.inputType
      && typeof input.data === 'string' && input.data === beforeInput.data && input.data === endEvent.data);
    const endTrusted = endEvent?.trusted === true;
    const valid = start >= 0 && update > start && end > update && (nextStart < 0 || end < nextStart)
      && (composing ? end > at : end < at) && (endTrusted || endCorroborated);
    const cycle = valid ? targeted.slice(start, end + 1) : [];
    return { valid, cycle, endEvent, endTrusted, endCorroborated };
  }
  const press = candidates.find(e => sequence(e).valid) ?? candidates[0];
  const { valid: compositionCycle, cycle, endEvent, endTrusted, endCorroborated } = sequence(press);
  const korean = cycle.some(e => e.trusted && /[ㄱ-힣]/u.test(e.data ?? ''));
  const guard = cycle.some(e => e.trusted && e.type === 'keydown' && (e.isComposing || e.keyCode === 229));
  const stable = s => JSON.stringify([s?.rows, s?.status, s?.nativeCurrentPath]);
  const following = press && events.find(e => e.type === 'settled' && e.keyEvent === press.serial);
  const endIndex = events.indexOf(endEvent);
  const boundary = target === 'filter' && composing && endIndex > events.indexOf(press)
    ? endIndex : events.indexOf(following) + 1;
  const during = press && following ? events.slice(events.indexOf(cycle[0] ?? press), boundary) : [];
  const field = s => s?.inputs?.[target];
  let outcome = false;
  if (press && following) {
    if (composing) outcome = Boolean(press.ui?.status && field(press.ui))
      && during.every(e => !e.ui || stable(e.ui) === stable(press.ui))
      && field(following.ui)?.focused && !following.ui?.alert
      && (target !== 'filter' || following.ui?.address === press.ui?.address)
      && (!action.includes('ctrl-l') && !action.endsWith('f6') ||
        !(field(following.ui)?.selection?.[0] === 0 && field(following.ui)?.selection?.[1] === field(following.ui)?.value?.length));
    else if (action === 'committed-enter' && target === 'address') outcome = normal(after?.address) === normal(child)
      && stable(press.ui) !== stable(after) && after?.rows?.some(row => row.includes('IME-child-proof.txt'))
      && /조회 완료/.test(after?.status ?? '');
    else if (action === 'committed-enter') outcome = after?.inputs?.filter?.value === '한글'
      && after?.address === press.ui?.address && after?.rows?.length > 0
      && after.rows.every(row => row.includes('한글')) && /조회 완료/.test(after?.status ?? '');
    else if (action === 'committed-f6') {
      const visible = press.ui?.panes?.filter(p => p.visible) ?? [];
      const source = visible.findIndex(p => p.id === press.ui?.focus?.pane);
      const expected = source >= 0 ? visible[(source + 1) % visible.length] : null;
      const destination = following.ui?.panes?.find(p => p.id === expected?.id && p.visible);
      const address = destination?.inputs?.address;
      outcome = press.ui?.focus?.pane === 1 && press.ui?.focus?.target === target
        && visible[source]?.inputs?.[target]?.focused === true
        && JSON.stringify(visible.map(p => p.id)) === JSON.stringify(following.ui?.panes?.filter(p => p.visible).map(p => p.id))
        && following.ui?.focus?.pane === expected?.id && following.ui?.focus?.target === 'address'
        && address?.focused === true && typeof address.value === 'string'
        && address.selection?.[0] === 0 && address.selection?.[1] === address.value.length
        && !destination.alert;
    } else outcome = following.ui?.inputs?.address?.focused && following.ui.inputs.address.selection?.[0] === 0
      && following.ui.inputs.address.selection?.[1] === following.ui.address?.length && following.ui.address?.length > 0;
  }
  return { target, action, endTrusted, endCorroborated, keyObserved: Boolean(press), compositionCycle, korean, guard, outcome,
    sequenceSerials: cycle.map(e => e.serial), keySerial: press?.serial ?? null,
    eventCounts: Object.fromEntries(['compositionstart', 'compositionupdate', 'compositionend', 'keydown', 'input']
      .map(type => [type, trusted.filter(e => e.type === type).length])),
    missing: [!compositionCycle && 'ordered_composition_sequence', !korean && 'korean_composition',
      !guard && 'composing_keyboard_evidence', !press && 'scenario_key', !outcome && 'expected_ui_outcome'].filter(Boolean),
    observed: Boolean(press && outcome && compositionCycle && korean && guard), baseline: before, final: after };
}
