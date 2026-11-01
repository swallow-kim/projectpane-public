// Activity is evidence of delivered events, never proof of a completed IME scenario.
export function createInputActivity() {
  return {
    eventCount: 0, trustedInputEvents: 0, untrustedInputEvents: 0,
    trustedHangulInputEvents: 0, trustedCompositionEvents: 0,
    outsideStepInputEvents: 0, unknownTargetInputEvents: 0, lastInputTarget: null,
    inputByTarget: Object.fromEntries([1, 2, 3, 4].flatMap(pane =>
      ['address', 'filter'].map(target => [`${pane}:${target}`, 0]))),
  };
}

export function recordInputActivity(activity, event) {
  activity.eventCount++;
  if (event.trusted === true && ['compositionstart', 'compositionupdate', 'compositionend'].includes(event.type)) {
    activity.trustedCompositionEvents++;
  }
  if (event.type !== 'input') return;
  if (event.trusted !== true) { activity.untrustedInputEvents++; return; }
  activity.trustedInputEvents++;
  if (/[\u1100-\u11ff\u3130-\u318f\uac00-\ud7a3]/u.test(event.data ?? '')) activity.trustedHangulInputEvents++;
  if (!event.step) activity.outsideStepInputEvents++;
  const target = event.inputTarget ?? event.ui?.focus;
  const key = `${target?.pane}:${target?.target}`;
  if (Object.hasOwn(activity.inputByTarget, key)) {
    activity.inputByTarget[key]++;
    activity.lastInputTarget = key;
  } else {
    activity.unknownTargetInputEvents++;
    activity.lastInputTarget = null;
  }
}
