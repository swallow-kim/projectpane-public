import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

export function requestedRange(value, total) {
  if (!/^[1-9]\d*$/.test(value) || Number(value) > total) throw Error('start_at');
  return { start: Number(value), end: total };
}

export function completionStatus(steps, range) {
  if (!steps.slice(range.start - 1, range.end).every(step => step.status === 'OBSERVED')) return 'INCOMPLETE';
  return range.start === 1 ? 'OBSERVED' : 'OBSERVED_SUBSET';
}

export async function persistFinalReport(output, report, publish, write = writeFileSync) {
  const save = () => write(join(output, 'ime-report.json'), JSON.stringify(report, null, 2));
  save();
  try { await publish({ finished: true }); }
  catch (error) { report.status = 'FAIL'; report.publicationError = error.message; report.reason ??= error.message; }
  save();
}
