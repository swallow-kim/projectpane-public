import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';
import { MAX_FRAME } from '../../shared/contracts';
import { IpcFault, toFaultCode } from '../../shared/fault';
import { normalizeAbsolutePath, normalizeRelativePath } from '../../shared/projects-path';
import { ProjectContractError } from '../../shared/projects-error';
import { normalizeExplorerPath } from './native-drop';
import { operationPreviewSchema, operationReportSchema, operationSelectionSchema, operationSnapshotSchema, transferPreviewSchema, type FileActionDraft, type FileActionKind, type OperationPreview, type OperationSelection, type TransferMode } from '../../shared/operations';

async function parsed<T>(command: string, args: Record<string, unknown>, schema: z.ZodType<T>): Promise<T> {
  const raw: unknown = await invoke(command, args);
  const result = schema.safeParse(raw);
  if (!result.success || new TextEncoder().encode(JSON.stringify(result.data)).byteLength >= MAX_FRAME) throw new IpcFault('protocol_error');
  return result.data;
}
export type ExplorerExportAction = 'reveal' | 'properties' | 'context_menu' | 'clipboard' | 'drag' | 'absolute_path' | 'relative_path';
export async function exportToExplorer(selection: OperationSelection, action: ExplorerExportAction, basePath: string | null = null): Promise<boolean> {
  if (!operationSelectionSchema.safeParse(selection).success || new Set(selection.entryIds).size !== selection.entryIds.length
    || selection.entryIds.length > 128 || (action === 'properties' && selection.entryIds.length !== 1)) throw new IpcFault('invalid_request');
  if (action === 'relative_path') {
    try { if (basePath === null) throw new Error(); normalizeAbsolutePath(basePath); }
    catch { throw new IpcFault('invalid_path'); }
  }
  return parsed('export_to_explorer', { selection, action, basePath }, z.boolean());
}
export function clipboardSequence(): Promise<number> {
  return parsed('clipboard_sequence', {}, z.number().int().min(1).max(0xffff_ffff));
}
export async function importFromExplorer(paths: readonly string[] | null): Promise<import('../../shared/operations').TransferPreview> {
  if (paths !== null) {
    if (!paths.length || paths.length > 1024 || new TextEncoder().encode(JSON.stringify(paths)).byteLength >= MAX_FRAME) throw new IpcFault('payload_limit');
    try { paths = paths.map(normalizeExplorerPath); }
    catch { throw new IpcFault('invalid_path'); }
  }
  const result = await parsed('import_from_explorer', { paths }, transferPreviewSchema);
  if (result.mode !== 'copy') throw new IpcFault('protocol_error');
  return result;
}
export async function prepareFileAction(draft: FileActionDraft, value: string, projectId: string | null, copyName?: string, skipExisting = false): Promise<OperationPreview> {
  const validSource = draft.transfer ? draft.transfer.mode === 'copy' && draft.transfer.count > 0
    : operationSelectionSchema.safeParse(draft.selection).success && new Set(draft.selection?.entryIds).size === draft.selection?.entryIds.length;
  const sourceCount = draft.transfer?.count ?? draft.selection?.entryIds.length;
  if (skipExisting && (copyName !== undefined || draft.kind !== 'copy' || !validSource)) throw new IpcFault('invalid_request');
  if (copyName !== undefined) {
    if (draft.kind !== 'copy' || !validSource || sourceCount !== 1) throw new IpcFault('invalid_request');
    try {
      if (!copyName || copyName.length > 255 || /[/\\]/.test(copyName) || normalizeRelativePath(copyName) !== copyName) throw new IpcFault('invalid_path');
      const stem = copyName.split('.')[0]?.replace(/ +$/, '') ?? '';
      if (stem) normalizeRelativePath(stem);
      normalizeAbsolutePath(`${value}\\${copyName}`);
    } catch (error) {
      if (error instanceof ProjectContractError) throw new IpcFault('invalid_path');
      throw error;
    }
  }
  if (draft.kind === 'recycle' && (draft.transfer || !operationSelectionSchema.safeParse(draft.selection).success
    || new Set(draft.selection?.entryIds).size !== draft.selection?.entryIds.length)) throw new IpcFault('invalid_request');
  const action = skipExisting ? { kind: 'copy_skip_existing', selection: draft.selection, destination: value }
    : copyName !== undefined ? { kind: 'copy_as', selection: draft.selection, destination: value, name: copyName }
    : draft.kind === 'compress' || draft.kind === 'extract' ? { kind: 'archive', selection: draft.selection, destination: draft.root, name: value, extract: draft.kind === 'extract' }
    : draft.kind === 'recycle' ? { kind: draft.kind, selection: draft.selection }
    : draft.kind === 'new_folder' ? { kind: draft.kind, parent: draft.root, name: value }
    : draft.kind === 'rename' ? { kind: draft.kind, selection: draft.selection, name: value }
    : { kind: draft.kind, selection: draft.selection, destination: value };
  const request = draft.transfer ? { projectId, token: draft.transfer.token, destination: value,
    ...(copyName !== undefined ? { copyName } : {}), ...(skipExisting ? { skipExisting: true } : {}) } : { projectId, action };
  if (new TextEncoder().encode(JSON.stringify(request)).byteLength >= MAX_FRAME) throw new IpcFault('payload_limit');
  const preview = await parsed(draft.transfer ? 'prepare_transfer' : 'prepare_operation', { request }, operationPreviewSchema);
  const expectedKind = skipExisting ? 'copy_skip_existing' : copyName !== undefined ? 'copy_as' : draft.kind;
  // Native identity checks canonicalize the parent; the requested leaf must remain exact.
  if (preview.projectId !== projectId || preview.kind !== expectedKind || preview.sources.length !== (draft.transfer?.count ?? draft.selection?.entryIds.length ?? 0)
    || (draft.kind === 'rename' && (preview.destination?.replaceAll('/', '\\').split('\\').at(-1) !== value
      || (draft.renameEntry && preview.sources[0]?.replaceAll('/', '\\').split('\\').at(-1) !== draft.renameEntry.name)))
    || (copyName !== undefined && preview.destination?.replaceAll('/', '\\').split('\\').at(-1) !== copyName)
    || (draft.kind === 'recycle' && preview.destination !== null)) {
    await discardFileAction(preview.id);
    throw new IpcFault('protocol_error');
  }
  return preview;
}
export async function captureTransfer(mode: TransferMode, selection: OperationSelection) {
  const checked = operationSelectionSchema.safeParse(selection);
  if (!checked.success || new Set(selection.entryIds).size !== selection.entryIds.length) throw new IpcFault('invalid_request');
  const request = { mode, selection: checked.data };
  if (new TextEncoder().encode(JSON.stringify(request)).byteLength >= MAX_FRAME) throw new IpcFault('payload_limit');
  const result = await parsed('capture_transfer', { request }, transferPreviewSchema);
  if (result.mode !== mode || result.count !== selection.entryIds.length) throw new IpcFault('protocol_error');
  return result;
}
async function changeState(command: 'execute_operation' | 'cancel_operation', id: string) {
  const result = await parsed(command, { request: { id } }, operationSnapshotSchema);
  if (result.id !== id) throw new IpcFault('protocol_error');
  return result;
}
export const executeFileAction = (id: string) => changeState('execute_operation', id);
export const cancelFileAction = (id: string) => changeState('cancel_operation', id);
export const discardFileAction = (id: string): Promise<void> => invoke('discard_operation', { request: { id } });
export const loadFileActionReport = (id: string) => parsed('get_operation_report', { request: { id } }, operationReportSchema.nullable());
export async function forgetFileAction(id: string): Promise<void> {
  const removed = await parsed('forget_operation', { request: { id } }, operationSnapshotSchema.shape.id);
  if (removed !== id) throw new IpcFault('protocol_error');
}
export function operationError(error: unknown, kind?: FileActionKind | 'copy_as' | 'copy_skip_existing'): string {
  switch (toFaultCode(error)) {
    case 'destination_streams_unsupported': return '대상 저장소는 이 항목의 추가 데이터 스트림(ADS)을 보존할 수 없어 복사를 중단했습니다. 해당 항목은 새로 만들지 않았습니다. ADS를 지원하는 대상 저장소를 선택하세요.';
    case 'unsupported_provider': return '이 저장소는 안전한 복사에 필요한 파일 식별 또는 생성 기능을 지원하지 않습니다. 원본과 대상 저장소를 확인하세요.';
    case 'unsafe_reparse_point': return '선택한 항목이나 상위 경로에 심볼릭 링크 또는 재분석 지점이 있어 처리하지 않았습니다. 링크를 따라가지 않습니다. 링크가 없는 실제 원본 경로와 대상 폴더를 직접 선택하세요.';
    case 'destination_exists': return '대상에 같은 이름의 항목이 있습니다. 기존 항목은 변경하지 않았습니다. 건너뛰기 또는 다른 이름으로 복사를 선택할 수 있습니다.';
    case 'busy': return '대상 이름이 이미 있거나 다른 작업을 처리 중입니다. 경로와 작업 상태를 확인하세요.';
    case 'stale_request': return '선택한 항목이 바뀌었습니다. 목록을 새로 고친 뒤 다시 선택하세요.';
    case 'invalid_request': case 'invalid_path': return kind === 'copy_skip_existing'
      ? '기존 파일 건너뛰기는 고정 드라이브의 일반 파일만 지원합니다. 폴더·링크·원본과 같은 대상은 지원하지 않습니다.'
      : kind === 'copy_as'
      ? '새 이름으로 복사는 고정 드라이브의 일반 파일 하나만 지원합니다. 이름에는 경로 구분자를 넣을 수 없습니다. 원본과 대상 경로·파일명을 확인하세요.'
      : kind === 'recycle'
      ? '선택한 항목을 휴지통으로 보낼 수 없습니다. 고정 드라이브의 일반 파일만 다시 선택하세요. 폴더·링크·휴지통 안의 항목은 지원하지 않습니다.'
      : kind === 'compress' || kind === 'extract' ? '기본 ZIP은 일반 파일·폴더, UTF-8 이름, 최대 2,048항목·1GiB를 지원합니다. 암호화·ZIP64·링크·중복 이름·잘못된 경로는 지원하지 않습니다. 설치된 압축 앱을 선택할 수 있습니다.'
      : '경로·연결 상태·권한을 확인하세요. 링크와 자기 안으로 복사는 지원하지 않습니다. 다른 드라이브 폴더 이동은 링크·암호화·폴더 추가 데이터가 없는 로컬 NTFS 폴더만 지원합니다. WSL 이동은 같은 배포판·볼륨 안에서만 지원합니다. Windows↔WSL·다른 배포판·일반 네트워크 사이는 복사를 사용하세요.';
    case 'access_denied': return '이 항목을 변경할 권한이 없거나 다른 앱이 사용 중입니다.';
    case 'not_found': return '원본이나 대상 폴더를 찾을 수 없습니다. 목록을 새로 고친 뒤 경로를 확인하세요.';
    case 'worker_unavailable': case 'invalid_state': return '파일 작업 실행 상태를 확인할 수 없습니다. 원본과 대상 폴더를 확인한 뒤 앱을 다시 실행하세요.';
    case 'resource_limit': case 'payload_limit': return kind === 'recycle'
      ? '작업 한도에 도달했습니다. 선택한 파일 수·경로 길이·결과 기록 용량을 확인하고, 나누어 선택하거나 작업 기록을 정리하세요.'
      : '작업 한도에 도달했습니다. 폴더 안의 항목도 포함되므로 나누어 선택하거나 작업 기록을 확인하세요.';
    default: return '작업을 처리하지 못했습니다. 접수한 작업은 상단 파일 작업 버튼의 결과에서 확인하세요.';
  }
}

const archiveToolSchema = z.strictObject({ id: z.enum(['seven_zip', 'winrar', 'bandizip']), label: z.string().min(1).max(40) });
export type ArchiveTool = z.infer<typeof archiveToolSchema>;
export const listArchiveTools = () => parsed('list_archive_tools', {}, z.array(archiveToolSchema).max(3));
export const launchArchiveTool = (draft: FileActionDraft, tool: ArchiveTool['id'], name: string) => parsed('launch_archive_tool', { selection: draft.selection, tool, parent: draft.root, name, extract: draft.kind === 'extract' }, z.literal(true));
