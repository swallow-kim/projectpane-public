import { useEffect, useId, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { UpdateController } from './update-client';
import './updates.css';

const errorText: Record<string, string> = {
  check_failed: '업데이트 정보를 가져오지 못했습니다. 네트워크를 확인하고 다시 시도하세요.',
  configuration: '업데이트 설정을 확인하지 못했습니다. 릴리스 페이지에서 설치 파일을 받을 수 있습니다.',
  invalid_release: '유효한 최신 정식판 설치 파일이 아닙니다. 설치를 시작하지 않았습니다.',
  download_or_signature: '다운로드 또는 서명 확인에 실패했습니다. 설치를 시작하지 않았습니다.',
  download_limit: '설치 파일 크기가 허용 범위를 넘었습니다. 설치를 시작하지 않았습니다.',
  save_failed: '설정 저장을 확인하지 못해 설치를 중단했습니다. 상태를 확인하고 앱을 정상 종료한 뒤 다시 실행하세요.',
  workers_unconfirmed: '읽기 worker의 종료를 확인하지 못해 설치를 중단했습니다. 앱을 정상 종료한 뒤 다시 실행하세요.',
  operations_unconfirmed: '파일 작업의 안전한 종료를 확인하지 못해 설치를 중단했습니다.',
  install_failed: '설치 프로그램을 시작하지 못했습니다. 앱을 정상 종료하고 릴리스 페이지의 설치 파일을 사용하세요.',
  shell_menu_busy: 'Windows 메뉴가 닫힌 것을 확인하지 못해 설치를 중단했습니다. 메뉴를 닫고 다시 시도하세요.',
};
export function UpdateControl({ ready, persisted, onPreparing }: { readonly ready: boolean; readonly persisted: boolean; readonly onPreparing: (value: boolean) => void }) {
  const [, render] = useState(0);
  const [open, setOpen] = useState(false);
  const [linkError, setLinkError] = useState(false);
  const [cancelError, setCancelError] = useState(false);
  const controllerRef = useRef<UpdateController | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const id = useId();
  useEffect(() => {
    const controller = new UpdateController(undefined, () => render(value => value + 1));
    controllerRef.current = controller; void controller.refresh();
    return () => { controller.dispose(); controllerRef.current = null; };
  }, []);
  useEffect(() => {
    if (!ready) return;
    const timer = window.setTimeout(() => { void controllerRef.current?.run('check_update', { manual: false }); }, 5000);
    return () => window.clearTimeout(timer);
  }, [ready]);
  const controller = controllerRef.current;
  const status = controller?.status;
  const working = status?.phase === 'downloading' || status?.phase === 'preparing' || status?.phase === 'installing';
  useEffect(() => { onPreparing(status?.phase === 'preparing'); }, [status?.phase, onPreparing]);
  useEffect(() => {
    if (!open && !working) return;
    const timer = window.setInterval(() => { void controllerRef.current?.refresh(); }, 500);
    return () => window.clearInterval(timer);
  }, [open, working]);
  useEffect(() => {
    if (open) dialog.current?.showModal();
    else if (dialog.current?.open) { dialog.current.close(); button.current?.focus(); }
  }, [open]);
  const run = (command: 'check_update' | 'download_update' | 'install_update') => {
    setCancelError(false);
    void controllerRef.current?.run(command, command === 'check_update' ? { manual: true } : undefined);
  };
  const releasePage = async () => {
    setLinkError(false);
    try { await invoke('open_update_page'); } catch { setLinkError(true); }
  };
  const busy = controller?.busy || working;
  const phase = status?.phase;
  const error = status?.error;
  return <>
    <button ref={button} className="favorites-manage" title="업데이트 확인 및 설치" onClick={() => setOpen(true)}>
      {phase === 'available' || phase === 'ready' ? '새 업데이트' : '업데이트'}
    </button>
    <dialog ref={dialog} className="update-dialog" aria-labelledby={id} onKeyDown={event => event.stopPropagation()}
      onCancel={event => { event.preventDefault(); if (!working) setOpen(false); }}>
      <h2 id={id}>ProjectPane 업데이트</h2>
      <p>현재 버전 {status?.currentVersion ?? '확인 중'} · {status?.installed ? '설치형' : '설치형 확인 안 됨'}</p>
      <p className="update-hint">시작 후 하루 한 번 확인합니다. 다운로드와 설치는 선택한 경우에만 진행합니다.</p>
      {status?.previousInstall === 'completed' && <p role="status">현재 버전으로 업데이트된 것을 확인했습니다.</p>}
      {status?.previousInstall === 'unconfirmed' && <p role="alert">이전 설치의 완료를 확인하지 못했습니다. 현재 버전을 확인하세요. 설치는 자동 재시도하지 않습니다.</p>}
      {!status?.installed && <p>ZIP / 개발 실행 또는 설치 경로를 확인하지 못한 경우 앱 내 설치를 지원하지 않습니다. 릴리스 페이지의 설치 안내를 사용하세요. ZIP판은 새 버전을 별도 폴더에 풀어 사용하세요.</p>}
      {status?.installed && !persisted && <p>열린 탭을 복원하려면 현재 작업 공간을 프로젝트로 저장한 뒤 설치하세요.</p>}
      <div role="status" aria-live="polite">
        {phase === 'checking' && '새 버전을 확인하고 있습니다…'}
        {phase === 'current' && '현재 최신 정식판입니다.'}
        {phase === 'available' && `새 버전 ${status?.version}을 사용할 수 있습니다.`}
        {phase === 'downloading' && `다운로드 및 서명 확인 중… ${Math.floor((status?.received ?? 0) / 1024)} KiB`}
        {phase === 'ready' && '서명 확인 완료. 설치를 선택하면 작업 종료와 설정 저장을 확인한 뒤 앱을 다시 시작합니다.'}
        {phase === 'preparing' && '파일 작업 종료와 설정 저장을 확인하고 있습니다. 파일 작업 창에서 완료 대기 또는 정상 취소를 선택하세요.'}
        {phase === 'installing' && '설치 프로그램으로 인계합니다. 설치가 끝나면 앱이 다시 시작됩니다.'}
      </div>
      {status?.notes && <pre className="update-notes">{status.notes}</pre>}
      {(error || controller?.error) && <p role="alert">{errorText[error ?? ''] ?? '요청을 완료하지 못했습니다. 현재 상태를 확인하고 다시 시도하세요.'}</p>}
      {linkError && <p role="alert">릴리스 페이지를 열지 못했습니다: https://github.com/swallow-kim/projectpane-public/releases</p>}
      {cancelError && <p role="alert">다운로드 취소를 확인하지 못했습니다. 현재 상태를 확인하세요. 설치는 자동으로 시작되지 않습니다.</p>}
      <div className="update-actions">
        {phase === 'downloading' && <button onClick={() => { setCancelError(false); void invoke('cancel_update_download').catch(() => setCancelError(true)); }}>다운로드 취소</button>}
        <button disabled={busy} onClick={() => run('check_update')}>업데이트 확인</button>
        {status?.installed && phase === 'available' && <button disabled={busy} onClick={() => run('download_update')}>다운로드</button>}
        {status?.installed && phase === 'ready' && <button disabled={busy || !persisted} onClick={() => run('install_update')}>설치 후 다시 시작</button>}
        <button disabled={working} onClick={() => void releasePage()}>릴리스 페이지</button>
        <button disabled={working} onClick={() => setOpen(false)}>닫기</button>
      </div>
    </dialog>
  </>;
}
