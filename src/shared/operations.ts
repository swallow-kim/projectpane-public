import { z } from 'zod';
import { MAX_FRAME, identitySchema } from './contracts';
import { entryIdSchema } from './listing-view';
import { IpcFault, nativeFaultCodes } from './fault';

const token = z.string().regex(/^[A-Za-z0-9-]{1,80}$/);
export const operationStatusSchema = z.enum(['queued', 'awaiting_user_decision', 'running', 'cancel_requested', 'succeeded', 'partially_succeeded', 'cancelled', 'failed', 'interrupted_unknown']);
export const operationCloseSchema = z.enum(['open', 'deciding', 'waiting', 'stopping', 'stopped', 'blocked', 'results_unsaved']);
export type OperationCloseState = z.infer<typeof operationCloseSchema>;
export const operationCountsSchema = z.strictObject({ changed: z.number().int().min(0).max(4_294_967_295), skipped: z.number().int().min(0).max(4_294_967_295), failed: z.number().int().min(0).max(4_294_967_295), unknown: z.number().int().min(0).max(4_294_967_295) });
export const operationSnapshotSchema = z.strictObject({ id: token, projectId: token.nullable(), status: operationStatusSchema, counts: operationCountsSchema, journalSaved: z.boolean() });
const progressSchema = z.strictObject({ operationId: token, total: z.number().int().min(1).max(4_294_967_295), completed: z.number().int().min(0).max(4_294_967_295) }).refine(value => value.completed <= value.total);
export const operationStateSchema = z.strictObject({ operations: z.array(operationSnapshotSchema).max(256), closeState: operationCloseSchema, available: z.boolean(), progress: progressSchema.nullable() });
export type OperationState = z.infer<typeof operationStateSchema>;
export type OperationSnapshot = z.infer<typeof operationSnapshotSchema>;
export type CloseChoice = 'wait' | 'cancel' | 'continue' | 'exit_without_results';
export const closeNoticeSchema = z.strictObject({ closeState: operationCloseSchema });
export const operationSelectionSchema = z.strictObject({ identity: identitySchema, entryIds: z.array(entryIdSchema).min(1).max(1024) });
export type OperationSelection = z.infer<typeof operationSelectionSchema>;
export const transferPreviewSchema = z.strictObject({ token, mode: z.enum(['copy', 'move']), count: z.number().int().min(1).max(1024) });
export type TransferPreview = z.infer<typeof transferPreviewSchema>;
export type TransferMode = TransferPreview['mode'];
export const operationKindSchema = z.enum(['new_folder', 'rename', 'copy', 'copy_as', 'copy_skip_existing', 'move', 'recycle', 'compress', 'extract']);
export type FileActionKind = Exclude<z.infer<typeof operationKindSchema>, 'copy_as' | 'copy_skip_existing'>;
const path = z.string().min(1).max(32768);
const reportPath = path.refine(value => !value.includes('\0') && new TextEncoder().encode(value).byteLength <= 32768);
const decisionKey = (value: string) => value.replace(/[A-Z]/g, letter => letter.toLowerCase());
export const operationPreviewSchema = z.strictObject({ id: token, projectId: token.nullable(), kind: operationKindSchema, sources: z.array(path).max(1024), destination: path.nullable(), skippedSources: z.array(path).max(1024).optional() }).refine(preview => preview.kind === 'copy_skip_existing'
  ? preview.destination !== null && preview.sources.length > 0 && preview.skippedSources !== undefined
    && new Set(preview.sources).size === preview.sources.length && new Set(preview.skippedSources).size === preview.skippedSources.length
    && preview.skippedSources.every(source => preview.sources.includes(source))
  : preview.skippedSources === undefined);
export type OperationPreview = z.infer<typeof operationPreviewSchema>;
export const operationReportSchema = z.strictObject({
  failure: z.strictObject({ code: z.enum(nativeFaultCodes) }).optional(),
  counts: operationCountsSchema,
  evidence: z.strictObject({ performSucceeded: z.boolean(), aborted: z.boolean().nullable(), outcomesComplete: z.boolean(), cancellationConfirmed: z.boolean() }),
  items: z.array(z.strictObject({ source: reportPath, destination: reportPath.nullable(), status: z.enum(['changed', 'skipped', 'failed', 'unknown']), hresult: z.number().int().min(-2147483648).max(2147483647) })).max(2048),
  userSkipped: z.array(z.strictObject({ source: reportPath, destination: reportPath })).min(1).max(2048).optional(),
  userDecisionsOnly: z.literal(true).optional(),
}).refine(report => !report.failure || (Object.values(report.counts).every(count => count === 0) && report.items.length === 0 && !report.userSkipped?.length && !report.userDecisionsOnly && !report.evidence.performSucceeded && !report.evidence.cancellationConfirmed && report.evidence.aborted !== true)).refine(report => {
  const decisions = report.userSkipped ?? [];
  const nativeSources = new Set(report.items.map(item => decisionKey(item.source)));
  if (report.items.length + decisions.length > 2048
    || new Set(decisions.map(item => decisionKey(item.source))).size !== decisions.length
    || new Set(decisions.map(item => decisionKey(item.destination))).size !== decisions.length
    || decisions.some(item => nativeSources.has(decisionKey(item.source)) || decisionKey(item.source) === decisionKey(item.destination))) return false;
  if (report.userDecisionsOnly && (decisions.length === 0 || report.items.length > 0 || report.failure
    || report.counts.changed !== 0 || report.counts.failed !== 0 || report.counts.unknown !== 0
    || report.counts.skipped !== decisions.length || report.evidence.performSucceeded
    || report.evidence.aborted !== null || !report.evidence.outcomesComplete || report.evidence.cancellationConfirmed)) return false;
  if (!report.userDecisionsOnly && decisions.length > 0 && report.items.length === 0
    && report.counts.changed === 0 && report.counts.failed === 0 && report.counts.unknown === 0) return false;
  const retained = { changed: 0, skipped: decisions.length, failed: 0, unknown: 0 };
  for (const item of report.items) {
    if ((item.status === 'changed' && item.hresult < 0) || (item.status === 'failed' && item.hresult >= 0)) return false;
    retained[item.status]++;
  }
  return (['changed', 'skipped', 'failed', 'unknown'] as const).every(status => retained[status] <= report.counts[status]
    && (!report.evidence.outcomesComplete || retained[status] === report.counts[status]));
});
export type OperationReport = z.infer<typeof operationReportSchema>;
export type FileActionDraft = { readonly kind: FileActionKind; readonly root: string; readonly selection: OperationSelection | null; readonly transfer?: TransferPreview; readonly destination?: string; readonly copyConflict?: boolean; readonly copyFallback?: TransferPreview; readonly copyFallbackReason?: string; readonly archiveName?: string; readonly archiveFormat?: string; readonly renameEntry?: { readonly name: string; readonly kind: 'file' | 'folder' } };
export const fileActionText: Readonly<Record<FileActionKind, string>> = { new_folder: '새 폴더', rename: '이름 변경', copy: '파일 복사', move: '파일 이동', recycle: '휴지통', compress: '압축하기', extract: '압축 해제' };
export const terminalOperation = (status: z.infer<typeof operationStatusSchema>) => ['succeeded', 'partially_succeeded', 'cancelled', 'failed', 'interrupted_unknown'].includes(status);
export const removableOperation = (operation: OperationSnapshot) => operation.journalSaved && ['succeeded', 'partially_succeeded', 'cancelled', 'failed'].includes(operation.status);

export function parseOperationState(value: unknown): OperationState {
  const parsed = operationStateSchema.safeParse(value);
  if (!parsed.success || new TextEncoder().encode(JSON.stringify(parsed.data)).byteLength >= MAX_FRAME
    || new Set(parsed.data.operations.map(item => item.id)).size !== parsed.data.operations.length
    || (parsed.data.progress && !parsed.data.operations.some(item => item.id === parsed.data.progress?.operationId && ['running', 'cancel_requested'].includes(item.status)))) throw new IpcFault('protocol_error');
  return parsed.data;
}

export const operationStatusText: Readonly<Record<z.infer<typeof operationStatusSchema>, string>> = {
  queued: '대기 중', awaiting_user_decision: '확인 대기', running: '실행 중', cancel_requested: '취소 요청됨',
  succeeded: '완료', partially_succeeded: '일부 완료', cancelled: '취소됨', failed: '실패', interrupted_unknown: '결과 확인 필요',
};
