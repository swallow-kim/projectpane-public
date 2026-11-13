import { useOperationMessage } from './use-operation-message';
import { t, useLanguage } from '../../shared/i18n';
import { canFallbackToCopy, copyFallbackNotice } from './cut-fallback';
import { IpcFault, toFaultCode } from '../../shared/fault';
import { beginCopyJob, discardCopyCapture, stageCopy } from './copy-job-client';
import { useEffect, useId, useRef, useState } from 'react';
import { fileActionText, type FileActionDraft, type OperationPreview } from '../../shared/operations';
import { discardFileAction, executeFileAction, operationError, prepareFileAction, listArchiveTools, launchArchiveTool, type ArchiveTool } from './operation-client';
import { FavoriteDestination } from '../favorites/FavoritesDialog';
import { joinRenameName, splitRenameName, validRenameName } from './rename-name';

export function FileActionDialog({ draft: initialDraft, projectId, close, submitted }: { readonly draft: FileActionDraft; readonly projectId: string | null; readonly close: () => void; readonly submitted: () => void }) {
  useLanguage();
  const [fallback, setFallback] = useState<string | null>(null);
  const draft: FileActionDraft = fallback ? { ...initialDraft, kind: 'copy', transfer: initialDraft.copyFallback, copyFallbackCode: fallback } : initialDraft;
  const archive = draft.kind === 'compress' || draft.kind === 'extract';
  const [tools, setTools] = useState<ArchiveTool[]>([]);
  const [engine, setEngine] = useState<'windows' | ArchiveTool['id']>('windows');
  const [toolError, setToolError] = useState(false);
  const [handedOff, setHandedOff] = useState(false);
  useEffect(() => { if (!archive) return; let active = true; void listArchiveTools().then(value => { if (active) setTools(value); }).catch(() => { if (active) setToolError(true); }); return () => { active = false; }; }, [archive]);
  const builtinUnavailable = archive && draft.kind === 'extract' && draft.archiveFormat !== 'zip' && engine === 'windows';
  const rename = draft.kind === 'rename' ? draft.renameEntry : undefined;
  const folder = rename?.kind === 'folder';
  const original = splitRenameName(rename?.name ?? '', folder);
  const [value, setValue] = useState(rename ? original.stem : draft.archiveName ?? draft.destination ?? (draft.transfer ? draft.root : ''));
  const [extension, setExtension] = useState(original.extension);
  const finalName = joinRenameName(value, extension, folder);
  const extensionChanged = Boolean(rename && !folder && splitRenameName(finalName).extension.toLowerCase() !== original.extension.toLowerCase());
  const validName = !rename || (validRenameName(finalName) && Boolean(value) && finalName !== rename.name);
  const [copyWithName, setCopyWithName] = useState(false);
  const [copyName, setCopyName] = useState('');
  const [skipExisting, setSkipExisting] = useState(false);
  const [previewPage, setPreviewPage] = useState(0);
  const [preview, setPreview] = useState<OperationPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useOperationMessage();
  const [uncertain, setUncertain] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const composing = useRef(false);
  const settling = useRef(false);
  const generation = useRef(0);
  const prepared = useRef<string | null>(null);
  const sending = useRef(false);
  const id = useId();
  useEffect(() => {
    dialog.current?.showModal(); input.current?.focus(); if (rename) input.current?.select();
    return () => { generation.current++; if (prepared.current && !sending.current) void discardFileAction(prepared.current).catch(() => {}); };
  }, []);
  useEffect(() => { if (!preview && !busy) input.current?.focus(); }, [preview, busy]);
  const prepare = async () => {
    if (busy || uncertain || !value || !validName || (copyWithName && !copyName) || composing.current || settling.current) return;
    setBusy(true); setError(null);
    const epoch = generation.current;
    try {
      if (archive && engine !== 'windows') {
        if (await launchArchiveTool(draft, engine, value)) setHandedOff(true);
        setBusy(false); return;
      }
      if (builtinUnavailable) { setError(() => t("이 형식은 설치된 압축 앱에서 열어 압축을 해제하세요. Windows 기본 기능은 ZIP만 지원합니다.", "Use an installed archive app to extract this format. The built-in Windows option supports ZIP only.")); setBusy(false); return; }
      if (draft.kind === 'copy') {
        const capture = draft.selection ? await stageCopy(draft.selection, () => epoch === generation.current) : null;
        const captureId = capture?.id ?? draft.transfer?.token;
        if (!captureId) throw new Error('Missing copy source');
        try {
          if (epoch !== generation.current) return;
          sending.current = true;
          await beginCopyJob(captureId, value, projectId, copyWithName ? copyName : undefined, skipExisting);
          if (epoch === generation.current) { submitted(); close(); }
        } finally { if (capture) await discardCopyCapture(capture.id).catch(() => {}); }
        return;
      }
      if (draft.kind === 'move' && (draft.selection?.entryIds.length ?? 0) > 1024) throw new IpcFault('payload_limit');
      const result = await prepareFileAction(draft, rename ? finalName : value, projectId, copyWithName ? copyName : undefined, skipExisting);
      if (epoch !== generation.current) { await discardFileAction(result.id); return; }
      prepared.current = result.id;
      if (archive || (rename && !extensionChanged)) {
        sending.current = true;
        await executeFileAction(result.id);
        prepared.current = null; submitted(); close();
      } else setPreview(result);
    } catch (error) {
      if (epoch === generation.current && draft.kind === 'move' && !sending.current && !prepared.current && canFallbackToCopy(error) && (draft.selection || draft.copyFallback)) {
        setFallback(toFaultCode(error)); setError(null); return;
      }
      if (draft.kind === 'copy' && !['ipc_error', 'io_error', 'protocol_error', 'timeout'].includes(toFaultCode(error))) sending.current = false;
      if (epoch === generation.current) {
        if (sending.current) {
          // An accepted operation may have lost its reply. Never prepare a
          // second copy or discard the uncertain operation on dialog close.
          prepared.current = null; setUncertain(true); submitted();
          setError(() => draft.kind === 'copy' ? t("복사 실행 상태를 확인하지 못했습니다. 복사 작업에서 결과를 확인하세요. 자동으로 다시 실행하지 않습니다.", "The copy execution status could not be confirmed. Check Copy jobs for results. It will not run again automatically.") : t("복사 접수 상태를 확인하지 못했습니다. 상단 파일 작업 버튼을 확인하세요. 자동으로 다시 실행하지 않습니다.", "The copy request status could not be confirmed. Check File operations at the top. It will not run again automatically."));
        } else setError(() => operationError(error, skipExisting ? 'copy_skip_existing' : copyWithName ? 'copy_as' : draft.kind));
      }
    } finally { sending.current = false; if (epoch === generation.current) setBusy(false); }
  };
  const execute = async () => {
    if (!preview || busy) return;
    setBusy(true); setError(null); sending.current = true;
    try {
      await executeFileAction(preview.id);
      prepared.current = null; submitted(); close();
    } catch (error) { setError(() => operationError(error)); }
    finally { sending.current = false; setBusy(false); }
  };
  const revise = async () => {
    if (!preview || busy || sending.current) return;
    const epoch = generation.current;
    setBusy(true); setError(null);
    try {
      await discardFileAction(preview.id);
      if (epoch !== generation.current) return;
      prepared.current = null; setPreview(null); setPreviewPage(0);
    } catch {
      if (epoch === generation.current) setError(() => t("이전 확인을 취소하지 못했습니다. 조건 변경을 다시 누르거나 창을 닫아 주세요.", "The previous preview could not be cancelled. Click Change options again or close this dialog."));
    } finally { if (epoch === generation.current) setBusy(false); }
  };
  const named = archive || draft.kind === 'new_folder' || draft.kind === 'rename';
  return <dialog ref={dialog} className="operation-dialog" aria-labelledby={`${id}-title`} onCancel={event => { event.preventDefault(); if (!sending.current) close(); }}
    onKeyDown={event => { event.stopPropagation(); if ((event.key === 'Enter' || event.key === 'Escape') && (composing.current || settling.current || event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229)) event.preventDefault(); }}
    onKeyUp={() => { if (!composing.current) settling.current = false; }} onPointerDown={() => { if (!composing.current) settling.current = false; }}
    onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; settling.current = true; }}>
    <h2 id={`${id}-title`}>{fileActionText[draft.kind]}</h2>
    {archive && !handedOff && <><label className="operation-field">{t("압축 앱", "Archive app")}<select aria-label={t("압축 앱", "Archive app")} value={engine} disabled={busy || preview !== null} onChange={event => setEngine(event.target.value as typeof engine)}><option value="windows">{t("Windows 기본 ZIP", "Windows built-in ZIP")}</option>{tools.map(tool => <option key={tool.id} value={tool.id}>{tool.label}</option>)}</select></label>
      {toolError && <p role="status">{t("설치된 압축 앱 목록을 확인하지 못했습니다. Windows 메뉴에서 직접 선택할 수 있습니다.", "Installed archive apps could not be listed. You can choose one from the Windows menu.")}</p>}
      {builtinUnavailable && <p role="status">{t("ZIP 이외 형식은 설치된 압축 앱을 선택하세요.", "Choose an installed archive app for formats other than ZIP.")}</p>}
      {engine !== 'windows' && <p>{t("외부 앱에서 설정·진행·취소·결과를 확인합니다. WinRAR/Bandizip 압축 해제는 해당 앱에서 대상 폴더를 선택하세요. ProjectPane 작업 기록에는 완료로 기록하지 않습니다.", "Manage options, progress, cancellation, and results in the external app. For WinRAR or Bandizip extraction, choose the destination there. ProjectPane will not mark the operation as completed in its log.")}</p>}</>}
    {handedOff ? <><p role="status">{t("압축 앱으로 전달했습니다. 완료 여부와 취소는 해당 앱에서 확인하세요.", "Sent to the archive app. Check completion or cancel in that app.")}</p><button onClick={close}>{t("닫기", "Close")}</button></> : <>
    {(draft.copyFallbackCode || draft.copyFallbackReason) && <p role="alert">{draft.copyFallbackCode ? copyFallbackNotice({ code: draft.copyFallbackCode }) : draft.copyFallbackReason}</p>}
    {draft.copyConflict && <p role="alert">{t("같은 이름의 항목이 있어 복사하지 않았습니다. 아래에서 기존 파일 건너뛰기 또는 다른 이름으로 복사를 선택하세요. 덮어쓰기는 지원하지 않습니다.", "Nothing was copied because an item with the same name exists. Choose to skip existing files or copy with a different name below. Overwriting is not supported.")}</p>}
    {draft.transfer && <p>{t("앱 안에 담은 ", "Stored in app: ")}{draft.transfer.count}{t("개 · ", " · ")}{draft.transfer.mode === 'copy' ? t("복사", "Copy") : t("잘라내기", "Cut")}. {draft.transfer.mode === 'move' && t("실행을 접수하면 비워집니다. 실패한 항목은 결과를 확인한 뒤 다시 선택하세요.", "Cleared when the operation is accepted. Check results before selecting failed items again.")}</p>}
    {archive && <p>{t("Windows 기본 ZIP 엔진 · 최대 2,048항목 / 1GiB · 새 파일/폴더에만 생성합니다. 실패·취소 시 일부 출력은 남을 수 있으며 상단 파일 작업에서 결과를 확인하세요.", "Windows built-in ZIP engine · Up to 2,048 items / 1 GiB · Creates new files or folders only. Partial output may remain after failure or cancellation. Check File operations at the top for results.")}</p>}
    {draft.kind === 'move' && <p>{t("같은 WSL 배포판·볼륨 안에서는 파일·폴더를 그대로 이동합니다. Windows↔WSL·다른 배포판·일반 네트워크 사이에는 복사를 사용하세요. 다른 드라이브의 폴더 이동은 로컬 NTFS의 일반 폴더만 지원합니다(최대 2,048항목). 링크·암호화·폴더의 추가 데이터 스트림은 지원하지 않습니다. 작업 중 다른 앱에서 원본을 수정하지 마세요. 취소는 완료된 이동을 되돌리지 않습니다.", "Files and folders can be moved within the same WSL distribution and volume. Use copy between Windows and WSL, different distributions, or general network storage. Cross-drive folder moves support ordinary local NTFS folders only (up to 2,048 items). Links, encryption, and alternate data streams on folders are not supported. Do not modify the sources in other apps during the operation. Cancelling does not undo completed moves.")}</p>}
    {draft.kind === 'copy' && <p>{skipExisting ? t("고정 드라이브의 일반 파일만 지원합니다. 같은 이름의 대상 파일은 그대로 두고, 나머지를 복사합니다. 확인 뒤 대상이 바뀌면 다시 확인해야 합니다.", "Supports ordinary files on fixed drives only. Existing destination files with the same name are kept; the rest are copied. If the destination changes after review, review it again.") : copyWithName ? t("고정 드라이브의 일반 파일 하나를 새 이름으로 복사합니다. 원본은 유지하며, 같은 이름의 대상이 있으면 실행하지 않습니다.", "Copies one ordinary file on a fixed drive with a new name. The source is preserved. Nothing runs if that destination name already exists.") : t("폴더 내용도 복사하며 원본은 유지합니다. 같은 이름의 대상은 덮어쓰지 않습니다. 링크나 보존할 수 없는 추가 데이터가 있으면 해당 항목을 중단하고 결과에 표시합니다.", "Copies folder contents and preserves the originals. Existing items with the same name are not overwritten. Items containing links or extra data that cannot be preserved are stopped and listed in the results.")}</p>}
    {preview ? <>
      {rename && extensionChanged && <p role="alert">{t("확장자를 변경하거나 제거합니다. 파일 내용이나 형식은 변환되지 않으며, 연결된 앱에서 열리지 않을 수 있습니다. 최종 파일명을 확인하세요.", "You are changing or removing the extension. This does not convert the file contents or format, and the associated app may no longer open it. Check the final filename.")}</p>}
      <p>{preview.kind === 'copy_skip_existing' ? t('복사 {copied}개 · 기존 파일 유지 {skipped}개. 건너뛸 항목을 확인하세요.', 'Copy: {copied} · Keep existing: {skipped}. Review the items to skip.', { copied: preview.sources.length - (preview.skippedSources?.length ?? 0), skipped: preview.skippedSources?.length ?? 0 }) : preview.kind === 'copy_as' ? t("아래 새 이름으로 복사합니다. 기존 파일은 덮어쓰지 않습니다.", "Copies with the new name below. Existing files are not overwritten.") : t("아래 대상에 작업을 실행합니다.", "Runs the operation on the destination below.")}</p>
      <dl className="operation-paths"><dt>{t("원본 ", "Sources: ")}{preview.sources.length}{t("개", " items")}</dt><dd>{preview.sources.slice(previewPage * 16, (previewPage + 1) * 16).map(source => <div key={source}>{preview.kind === 'copy_skip_existing' && <strong>{preview.skippedSources?.includes(source) ? t("건너뜀 · ", "Skip · ") : t("복사 · ", "Copy · ")}</strong>}{source}</div>)}</dd>
        <dt>{t("대상", "Destination")}</dt><dd>{preview.destination}</dd></dl>
      {preview.sources.length > 16 && <div className="operation-dialog__actions"><button disabled={previewPage === 0 || busy} onClick={() => setPreviewPage(page => page - 1)}>{t("이전 원본", "Previous sources")}</button><span>{previewPage + 1} / {Math.ceil(preview.sources.length / 16)}</span><button disabled={(previewPage + 1) * 16 >= preview.sources.length || busy} onClick={() => setPreviewPage(page => page + 1)}>{t("다음 원본", "Next sources")}</button></div>}
      <div className="operation-dialog__actions"><button disabled={busy} onClick={() => void execute()}>{preview.kind === 'copy_skip_existing' && preview.skippedSources?.length === preview.sources.length ? t("모두 건너뛰기", "Skip all") : t("실행", "Run")}</button><button disabled={busy} onClick={() => void revise()}>{t("조건 변경", "Change options")}</button><button disabled={busy} onClick={close}>{t("취소", "Cancel")}</button></div>
    </> : <form onSubmit={event => { event.preventDefault(); void prepare(); }}>
      {!named && <FavoriteDestination disabled={busy} choose={setValue} />}
      {draft.root && <p className="operation-path">{t("현재 폴더: ", "Current folder: ")}{draft.root}</p>}
      {draft.selection && <p>{t("선택한 항목 ", "Selected items: ")}{draft.selection.entryIds.length}{t("개", " items")}</p>}
      <label className="operation-field">{rename ? t("이름", "Name") : archive ? (draft.kind === 'compress' ? t("ZIP 파일 이름", "ZIP filename") : t("새 압축 해제 폴더 이름", "New extraction folder name")) : named ? t("새 이름", "New name") : t("대상 폴더 절대 경로", "Destination folder absolute path")}<input ref={input} value={value} disabled={busy} autoComplete="off" spellCheck={false} required onChange={event => setValue(event.target.value)} /></label>
      {rename && !folder && <><label className="operation-field">{t("확장자", "Extension")}<input aria-describedby={`${id}-extension-help`} value={extension} disabled={busy} autoComplete="off" spellCheck={false} placeholder={t("없음", "None")} onChange={event => setExtension(event.target.value)} /></label><small id={`${id}-extension-help`}>{t("점(.) 없이 입력합니다. 비우면 확장자를 제거합니다.", "Enter without a dot (.). Leave blank to remove the extension.")}</small></>}
      {rename && <><p>{t("최종 파일명: ", "Final filename: ")}<output aria-label={t("최종 파일명", "Final filename")}>{finalName}</output></p>
        {!validRenameName(finalName) && <p role="alert">{t("Windows에서 사용할 수 있는 파일명을 입력하세요. 금지 문자·예약 이름·끝의 점이나 공백은 사용할 수 없습니다.", "Enter a valid Windows filename. Forbidden characters, reserved names, and trailing dots or spaces are not allowed.")}</p>}</>}
      {draft.kind === 'copy' && (draft.transfer?.count ?? draft.selection?.entryIds.length) === 1 && <>
        <label><input type="checkbox" checked={copyWithName} disabled={busy} onChange={event => { setCopyWithName(event.target.checked); if (event.target.checked) setSkipExisting(false); }} /> {t(" 다른 이름으로 복사 (파일 1개)", " Copy with a different name (one file)")}</label>
        {copyWithName && <label className="operation-field">{t("복사할 새 이름", "New copy name")}<input value={copyName} disabled={busy} autoComplete="off" spellCheck={false} maxLength={255} required onChange={event => setCopyName(event.target.value)} /></label>}
      </>}
      {draft.kind === 'copy' && <label><input type="checkbox" checked={skipExisting} disabled={busy} onChange={event => { setSkipExisting(event.target.checked); if (event.target.checked) setCopyWithName(false); }} /> {t(" 같은 이름의 기존 파일 모두 건너뛰기", " Skip all existing files with the same name")}</label>}
      <div className="operation-dialog__actions"><button type="submit" disabled={busy || uncertain || builtinUnavailable || !value || !validName || (copyWithName && !copyName)}>{busy ? t("대상 확인 중…", "Checking destination…") : rename ? extensionChanged ? t("확장자 변경 확인", "Review extension change") : t("이름 변경", "Rename") : archive ? fileActionText[draft.kind] : draft.kind === 'copy' ? t("복사", "Copy") : t("대상 확인", "Review destination")}</button><button type="button" disabled={busy && sending.current} onClick={() => { if (!sending.current) close(); }}>{uncertain ? t("닫기", "Close") : t("취소", "Cancel")}</button></div>
    </form>}
    {error && <p role="alert">{error?.()}</p>}
    </>}
  </dialog>;
}
