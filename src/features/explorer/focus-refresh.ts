export const FOCUS_REFRESH_DELAY_MS = 200;

/** One revalidation after returning to the app, with no polling while a workflow is busy. */
export function bindFocusRefresh(
  host: Window,
  page: Document,
  refresh: () => void,
  ready: () => boolean,
) {
  let away = page.hidden;
  let requested = false;
  let disposed = false;
  let timer: number | undefined;
  const clear = () => { if (timer !== undefined) host.clearTimeout(timer); timer = undefined; };
  const resume = () => {
    if (disposed || !requested || page.hidden || timer !== undefined) return;
    timer = host.setTimeout(() => {
      timer = undefined;
      if (disposed || page.hidden || !ready()) return;
      requested = false;
      away = false;
      refresh();
    }, FOCUS_REFRESH_DELAY_MS);
  };
  const leave = () => { away = true; requested = false; clear(); };
  const enter = () => { if (away) { requested = true; resume(); } };
  const focus = (event: FocusEvent) => { if (event.target === host) enter(); };
  const blur = (event: FocusEvent) => { if (event.target === host) leave(); };
  const visibility = () => { if (page.hidden) leave(); else enter(); };
  host.addEventListener('focus', focus);
  host.addEventListener('blur', blur);
  host.addEventListener('compositionend', resume);
  host.addEventListener('close', resume, true);
  page.addEventListener('visibilitychange', visibility);
  return {
    resume,
    acknowledge() {
      if (!requested || disposed) return;
      requested = false;
      away = false;
      clear();
    },
    dispose() {
      disposed = true;
      clear();
      host.removeEventListener('focus', focus);
      host.removeEventListener('blur', blur);
      host.removeEventListener('compositionend', resume);
      host.removeEventListener('close', resume, true);
      page.removeEventListener('visibilitychange', visibility);
    },
  };
}
