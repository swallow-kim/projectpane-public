// Functional endurance and exact-OS qualification are separate assertions.
// Missing metadata must never become a Windows 10 qualification PASS.
export function soakResult({ stopped, elapsedMs, durationMs, cycles, intervalMs, environment }) {
  if (stopped) return 'STOPPED_EARLY';
  if (elapsedMs < durationMs || cycles < Math.floor(durationMs / intervalMs) + 1) return 'INSUFFICIENT_CYCLES';
  if (environment?.osMetadata?.status !== 'verified' || environment.osMetadata.qualification !== 'windows10') {
    return 'FUNCTIONAL_PASS_OS_UNVERIFIED';
  }
  return 'PASS';
}
