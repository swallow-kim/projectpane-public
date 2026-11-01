// Use the app's bounded, metadata-only observer. Do not replace Tauri internals.
export async function startSoakObservation(page) {
  return page.evaluate(() => {
    const control = window.__PROJECTPANE_IPC_OBSERVATION__;
    if (!control) throw new Error('soak_ipc_observer_unavailable');
    control.enable();
    if (!control.snapshot().enabled) throw new Error('soak_ipc_observer_not_enabled');
  });
}

export async function readSoakObservation(page) {
  return page.evaluate(() => {
    const control = window.__PROJECTPANE_IPC_OBSERVATION__;
    if (!control) throw new Error('soak_ipc_observer_unavailable');
    return {
      ipc: control.snapshot(),
      panes: [...document.querySelectorAll('[data-pane]')].map(pane => ({
        pane: pane.getAttribute('data-pane'),
        status: pane.querySelector('footer [role="status"]')?.textContent,
        busy: pane.querySelector('[role="listbox"]')?.getAttribute('aria-busy'),
      })),
    };
  });
}

export async function stopSoakObservation(page) {
  return page.evaluate(() => window.__PROJECTPANE_IPC_OBSERVATION__?.disable());
}

export async function failedSoakObservation(page, navigation, timeoutMs = 2000) {
  let timer;
  try {
    if (!Number.isFinite(timeoutMs) || timeoutMs <= 0 || timeoutMs > 2000) throw new Error('invalid_observation_timeout');
    const evidence = await Promise.race([
      readSoakObservation(page),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('soak_failure_observation_timeout')), timeoutMs); }),
    ]);
    return { navigation, ...evidence };
  }
  catch (error) { return { navigation, observationError: String(error) }; }
  finally { clearTimeout(timer); }
}
