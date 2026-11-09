import { toFaultCode } from '../../shared/fault';
import { beginCopyJob, discardCopyCapture, stageCopy } from './copy-job-client';
import { useEffect, useId, useRef, useState } from 'react';
import { fileActionText, type FileActionDraft, type OperationPreview } from '../../shared/operations';
import { discardFileAction, executeFileAction, operationError, prepareFileAction, listArchiveTools, launchArchiveTool, type ArchiveTool } from './operation-client';
import { FavoriteDestination } from '../favorites/FavoritesDialog';
import { joinRenameName, splitRenameName, validRenameName } from './rename-name';

export function FileActionDialog({ draft, projectId, close, submitted }: { readonly draft: FileActionDraft; readonly projectId: string | null; readonly close: () => void; readonly submitted: () => void }) {
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
  const [error, setError] = useState<string | null>(null);
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
      if (builtinUnavailable) { setError('이 형식은 설치된 압축 앱에서 열어 압축을 해제하세요. Windows 기본 기능은 ZIP만 지원합니다.'); setBusy(false); return; }
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
      const result = await prepareFileAction(draft, rename ? finalName : value, projectId, copyWithName ? copyName : undefined, skipExisting);
      if (epoch !== generation.current) { await discardFileAction(result.id); return; }
      prepared.current = result.id;
      if (archive || (rename && !extensionChanged)) {
        sending.current = true;
        await executeFileAction(result.id);
        prepared.current = null; submitted(); close();
      } else setPreview(result);
    } catch (error) {
      if (draft.kind === 'copy' && !['ipc_error', 'io_error', 'protocol_error', 'timeout'].includes(toFaultCode(error))) sending.current = false;
      if (epoch === generation.current) {
        if (sending.current) {
          // An accepted operation may have lost its reply. Never prepare a
          // second copy or discard the uncertain operation on dialog close.
          prepared.current = null; setUncertain(true); submitted();
          setError(draft.kind === 'copy' ? '복사 실행 상태를 확인하지 못했습니다. 복사 작업에서 결과를 확인하세요. 자동으로 다시 실행하지 않습니다.' : '복사 접수 상태를 확인하지 못했습니다. 상단 파일 작업 버튼을 확인하세요. 자동으로 다시 실행하지 않습니다.');
        } else setError(operationError(error, skipExisting ? 'copy_skip_existing' : copyWithName ? 'copy_as' : draft.kind));
      }
    } finally { sending.current = false; if (epoch === generation.current) setBusy(false); }
  };
  const execute = async () => {
    if (!preview || busy) return;
    setBusy(true); setError(null); sending.current = true;
    try {
      await executeFileAction(preview.id);
      prepared.current = null; submitted(); close();
    } catch (error) { setError(operationError(error)); }
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
      if (epoch === generation.current) setError('이전 확인을 취소하지 못했습니다. 조건 변경을 다시 누르거나 창을 닫아 주세요.');
    } finally { if (epoch === generation.current) setBusy(false); }
  };
  const named = archive || draft.kind === 'new_folder' || draft.kind === 'rename';
  return <dialog ref={dialog} className="operation-dialog" aria-labelledby={`${id}-title`} onCancel={event => { event.preventDefault(); if (!sending.current) close(); }}
    onKeyDown={event => { event.stopPropagation(); if ((event.key === 'Enter' || event.key === 'Escape') && (composing.current || settling.current || event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229)) event.preventDefault(); }}
    onKeyUp={() => { if (!composing.current) settling.current = false; }} onPointerDown={() => { if (!composing.current) settling.current = false; }}
    onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; settling.current = true; }}>
    <h2 id={`${id}-title`}>{fileActionText[draft.kind]}</h2>
    {archive && !handedOff && <><label className="operation-field">압축 앱<select aria-label="압축 앱" value={engine} disabled={busy || preview !== null} onChange={event => setEngine(event.target.value as typeof engine)}><option value="windows">Windows 기본 ZIP</option>{tools.map(tool => <option key={tool.id} value={tool.id}>{tool.label}</option>)}</select></label>
      {toolError && <p role="status">설치된 압축 앱 목록을 확인하지 못했습니다. Windows 메뉴에서 직접 선택할 수 있습니다.</p>}
      {builtinUnavailable && <p role="status">ZIP 이외 형식은 설치된 압축 앱을 선택하세요.</p>}
      {engine !== 'windows' && <p>외부 앱에서 설정·진행·취소·결과를 확인합니다. WinRAR/Bandizip 압축 해제는 해당 앱에서 대상 폴더를 선택하세요. ProjectPane 작업 기록에는 완료로 기록하지 않습니다.</p>}</>}
    {handedOff ? <><p role="status">압축 앱으로 전달했습니다. 완료 여부와 취소는 해당 앱에서 확인하세요.</p><button onClick={close}>닫기</button></> : <>
    {draft.copyConflict && <p role="alert">같은 이름의 항목이 있어 복사하지 않았습니다. 아래에서 기존 파일 건너뛰기 또는 다른 이름으로 복사를 선택하세요. 덮어쓰기는 지원하지 않습니다.</p>}
    {draft.transfer && <p>앱 안에 담은 {draft.transfer.count}개 · {draft.transfer.mode === 'copy' ? '복사' : '잘라내기'}. {draft.transfer.mode === 'move' && '실행을 접수하면 비워집니다. 실패한 항목은 결과를 확인한 뒤 다시 선택하세요.'}</p>}
    {archive && <p>Windows 기본 ZIP 엔진 · 최대 2,048항목 / 1GiB · 새 파일/폴더에만 생성합니다. 실패·취소 시 일부 출력은 남을 수 있으며 상단 파일 작업에서 결과를 확인하세요.</p>}
    {draft.kind === 'move' && <p>같은 WSL 배포판·볼륨 안에서는 파일·폴더를 그대로 이동합니다. Windows↔WSL·다른 배포판·일반 네트워크 사이에는 복사를 사용하세요. 다른 드라이브의 폴더 이동은 로컬 NTFS의 일반 폴더만 지원합니다(최대 2,048항목). 링크·암호화·폴더의 추가 데이터 스트림은 지원하지 않습니다. 작업 중 다른 앱에서 원본을 수정하지 마세요. 취소는 완료된 이동을 되돌리지 않습니다.</p>}
    {draft.kind === 'copy' && <p>{skipExisting ? '고정 드라이브의 일반 파일만 지원합니다. 같은 이름의 대상 파일은 그대로 두고, 나머지를 복사합니다. 확인 뒤 대상이 바뀌면 다시 확인해야 합니다.' : copyWithName ? '고정 드라이브의 일반 파일 하나를 새 이름으로 복사합니다. 원본은 유지하며, 같은 이름의 대상이 있으면 실행하지 않습니다.' : '폴더 안의 항목까지 최대 2,048개를 복사합니다. 경로가 길면 더 적은 수에서 제한되며, 링크가 포함된 폴더와 기존 대상은 복사하지 않습니다.'}</p>}
    {preview ? <>
      {rename && extensionChanged && <p role="alert">확장자를 변경하거나 제거합니다. 파일 내용이나 형식은 변환되지 않으며, 연결된 앱에서 열리지 않을 수 있습니다. 최종 파일명을 확인하세요.</p>}
      <p>{preview.kind === 'copy_skip_existing' ? `복사 ${preview.sources.length - (preview.skippedSources?.length ?? 0)}개 · 기존 파일 유지 ${preview.skippedSources?.length ?? 0}개. 건너뛸 항목을 확인하세요.` : preview.kind === 'copy_as' ? '아래 새 이름으로 복사합니다. 기존 파일은 덮어쓰지 않습니다.' : '아래 대상에 작업을 실행합니다.'}</p>
      <dl className="operation-paths"><dt>원본 {preview.sources.length}개</dt><dd>{preview.sources.slice(previewPage * 16, (previewPage + 1) * 16).map(source => <div key={source}>{preview.kind === 'copy_skip_existing' && <strong>{preview.skippedSources?.includes(source) ? '건너뜀 · ' : '복사 · '}</strong>}{source}</div>)}</dd>
        <dt>대상</dt><dd>{preview.destination}</dd></dl>
      {preview.sources.length > 16 && <div className="operation-dialog__actions"><button disabled={previewPage === 0 || busy} onClick={() => setPreviewPage(page => page - 1)}>이전 원본</button><span>{previewPage + 1} / {Math.ceil(preview.sources.length / 16)}</span><button disabled={(previewPage + 1) * 16 >= preview.sources.length || busy} onClick={() => setPreviewPage(page => page + 1)}>다음 원본</button></div>}
      <div className="operation-dialog__actions"><button disabled={busy} onClick={() => void execute()}>{preview.kind === 'copy_skip_existing' && preview.skippedSources?.length === preview.sources.length ? '모두 건너뛰기' : '실행'}</button><button disabled={busy} onClick={() => void revise()}>조건 변경</button><button disabled={busy} onClick={close}>취소</button></div>
    </> : <form onSubmit={event => { event.preventDefault(); void prepare(); }}>
      {!named && <FavoriteDestination disabled={busy} choose={setValue} />}
      <p className="operation-path">현재 폴더: {draft.root}</p>
      {draft.selection && <p>선택한 항목 {draft.selection.entryIds.length}개</p>}
      <label className="operation-field">{rename ? '이름' : archive ? (draft.kind === 'compress' ? 'ZIP 파일 이름' : '새 압축 해제 폴더 이름') : named ? '새 이름' : '대상 폴더 절대 경로'}<input ref={input} value={value} disabled={busy} autoComplete="off" spellCheck={false} required onChange={event => setValue(event.target.value)} /></label>
      {rename && !folder && <><label className="operation-field">확장자<input aria-describedby={`${id}-extension-help`} value={extension} disabled={busy} autoComplete="off" spellCheck={false} placeholder="없음" onChange={event => setExtension(event.target.value)} /></label><small id={`${id}-extension-help`}>점(.) 없이 입력합니다. 비우면 확장자를 제거합니다.</small></>}
      {rename && <><p>최종 파일명: <output aria-label="최종 파일명">{finalName}</output></p>
        {!validRenameName(finalName) && <p role="alert">Windows에서 사용할 수 있는 파일명을 입력하세요. 금지 문자·예약 이름·끝의 점이나 공백은 사용할 수 없습니다.</p>}</>}
      {draft.kind === 'copy' && (draft.transfer?.count ?? draft.selection?.entryIds.length) === 1 && <>
        <label><input type="checkbox" checked={copyWithName} disabled={busy} onChange={event => { setCopyWithName(event.target.checked); if (event.target.checked) setSkipExisting(false); }} /> 다른 이름으로 복사 (파일 1개)</label>
        {copyWithName && <label className="operation-field">복사할 새 이름<input value={copyName} disabled={busy} autoComplete="off" spellCheck={false} maxLength={255} required onChange={event => setCopyName(event.target.value)} /></label>}
      </>}
      {draft.kind === 'copy' && <label><input type="checkbox" checked={skipExisting} disabled={busy} onChange={event => { setSkipExisting(event.target.checked); if (event.target.checked) setCopyWithName(false); }} /> 같은 이름의 기존 파일 모두 건너뛰기</label>}
      <div className="operation-dialog__actions"><button type="submit" disabled={busy || uncertain || builtinUnavailable || !value || !validName || (copyWithName && !copyName)}>{busy ? '대상 확인 중…' : rename ? extensionChanged ? '확장자 변경 확인' : '이름 변경' : archive ? fileActionText[draft.kind] : draft.kind === 'copy' ? '복사' : '대상 확인'}</button><button type="button" disabled={busy && sending.current} onClick={() => { if (!sending.current) close(); }}>{uncertain ? '닫기' : '취소'}</button></div>
    </form>}
    {error && <p role="alert">{error}</p>}
    </>}
  </dialog>;
}
