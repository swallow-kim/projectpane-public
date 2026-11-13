import { t, useLanguage } from '../../shared/i18n';
import { useEffect, useId, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { UpdateController } from './update-client';
import './updates.css';

const getErrorText = (): Record<string, string> => ({
  check_failed: t("업데이트 정보를 가져오지 못했습니다. 네트워크를 확인하고 다시 시도하세요.", "Could not retrieve update information. Check your network and try again."),
  configuration: t("업데이트 설정을 확인하지 못했습니다. 릴리스 페이지에서 설치 파일을 받을 수 있습니다.", "Could not verify update settings. You can download the installer from the releases page."),
  invalid_release: t("유효한 최신 정식판 설치 파일이 아닙니다. 설치를 시작하지 않았습니다.", "This is not a valid newer stable installer. Installation has not started."),
  download_or_signature: t("다운로드 또는 서명 확인에 실패했습니다. 설치를 시작하지 않았습니다.", "Download or signature verification failed. Installation has not started."),
  download_limit: t("설치 파일 크기가 허용 범위를 넘었습니다. 설치를 시작하지 않았습니다.", "The installer exceeds the allowed size. Installation has not started."),
  save_failed: t("설정 저장을 확인하지 못해 설치를 중단했습니다. 상태를 확인하고 앱을 정상 종료한 뒤 다시 실행하세요.", "Installation stopped because settings could not be confirmed saved. Check the state, close the app normally, and reopen it."),
  workers_unconfirmed: t("읽기 worker의 종료를 확인하지 못해 설치를 중단했습니다. 앱을 정상 종료한 뒤 다시 실행하세요.", "Installation stopped because the reader worker could not be confirmed stopped. Close the app normally and reopen it."),
  operations_unconfirmed: t("파일 작업의 안전한 종료를 확인하지 못해 설치를 중단했습니다.", "Installation stopped because file operations could not be confirmed safely stopped."),
  install_failed: t("설치 프로그램을 시작하지 못했습니다. 앱을 정상 종료하고 릴리스 페이지의 설치 파일을 사용하세요.", "Could not start the installer. Close the app normally and use the installer from the releases page."),
  shell_menu_busy: t("Windows 메뉴가 닫힌 것을 확인하지 못해 설치를 중단했습니다. 메뉴를 닫고 다시 시도하세요.", "Installation stopped because the Windows menu could not be confirmed closed. Close the menu and try again."),
});
export function UpdateControl({ ready, persisted, onPreparing }: { readonly ready: boolean; readonly persisted: boolean; readonly onPreparing: (value: boolean) => void }) {
  useLanguage();
  const errorText = getErrorText();
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
    <button ref={button} className="favorites-manage" title={t("업데이트 확인 및 설치", "Check for and install updates")} onClick={() => setOpen(true)}>
      {phase === 'available' || phase === 'ready' ? t("새 업데이트", "Update available") : t("업데이트", "Updates")}
    </button>
    <dialog ref={dialog} className="update-dialog" aria-labelledby={id} onKeyDown={event => event.stopPropagation()}
      onCancel={event => { event.preventDefault(); if (!working) setOpen(false); }}>
      <h2 id={id}>{t("ProjectPane 업데이트", "ProjectPane updates")}</h2>
      <p>{t("현재 버전", "Current version")} {status?.currentVersion ?? t("확인 중", "Checking")} · {status?.installed ? t("설치형", "Installed version") : t("설치형 확인 안 됨", "Installation not confirmed")}</p>
      <p className="update-hint">{t("시작 후 하루 한 번 확인합니다. 다운로드와 설치는 선택한 경우에만 진행합니다.", "Checks once a day after startup. Downloads and installs only when you choose to.")}</p>
      {status?.previousInstall === 'completed' && <p role="status">{t("현재 버전으로 업데이트된 것을 확인했습니다.", "The update to the current version is confirmed.")}</p>}
      {status?.previousInstall === 'unconfirmed' && <p role="alert">{t("이전 설치의 완료를 확인하지 못했습니다. 현재 버전을 확인하세요. 설치는 자동 재시도하지 않습니다.", "The previous installation could not be confirmed complete. Check the current version. Installation is not retried automatically.")}</p>}
      {!status?.installed && <p>{t("ZIP / 개발 실행 또는 설치 경로를 확인하지 못한 경우 앱 내 설치를 지원하지 않습니다. 릴리스 페이지의 설치 안내를 사용하세요. ZIP판은 새 버전을 별도 폴더에 풀어 사용하세요.", "In-app installation is unavailable for ZIP or development builds, or when the installation path cannot be verified. Follow the instructions on the releases page. For ZIP builds, extract the new version to a separate folder.")}</p>}
      {status?.installed && !persisted && <p>{t("열린 탭을 복원하려면 현재 작업 공간을 프로젝트로 저장한 뒤 설치하세요.", "Save the current workspace as a project before installing if you want to restore open tabs.")}</p>}
      <div role="status" aria-live="polite">
        {phase === 'checking' && t("새 버전을 확인하고 있습니다…", "Checking for a new version…")}
        {phase === 'current' && t("현재 최신 정식판입니다.", "You have the latest stable version.")}
        {phase === 'available' && t('새 버전 {version}을 사용할 수 있습니다.', 'Version {version} is available.', { version: status?.version ?? '' })}
        {phase === 'downloading' && t('다운로드 및 서명 확인 중… {size} KiB', 'Downloading and verifying signature… {size} KiB', { size: Math.floor((status?.received ?? 0) / 1024) })}
        {phase === 'ready' && t("서명 확인 완료. 설치를 선택하면 작업 종료와 설정 저장을 확인한 뒤 앱을 다시 시작합니다.", "Signature verified. Installing will confirm operations have stopped and settings are saved, then restart the app.")}
        {phase === 'preparing' && t("파일 작업 종료와 설정 저장을 확인하고 있습니다. 파일 작업 창에서 완료 대기 또는 정상 취소를 선택하세요.", "Checking that file operations have stopped and settings are saved. Wait for completion or cancel safely in the file operations dialog.")}
        {phase === 'installing' && t("설치 프로그램으로 인계합니다. 설치가 끝나면 앱이 다시 시작됩니다.", "Starting the installer. The app will restart after installation.")}
      </div>
      {status?.notes && <pre className="update-notes">{status.notes}</pre>}
      {(error || controller?.error) && <p role="alert">{errorText[error ?? ''] ?? t("요청을 완료하지 못했습니다. 현재 상태를 확인하고 다시 시도하세요.", "Could not complete the request. Check the current state and try again.")}</p>}
      {linkError && <p role="alert">{t("릴리스 페이지를 열지 못했습니다: https://github.com/swallow-kim/projectpane-public/releases", "Could not open the releases page: https://github.com/swallow-kim/projectpane-public/releases")}</p>}
      {cancelError && <p role="alert">{t("다운로드 취소를 확인하지 못했습니다. 현재 상태를 확인하세요. 설치는 자동으로 시작되지 않습니다.", "Download cancellation could not be confirmed. Check the current state. Installation will not start automatically.")}</p>}
      <div className="update-actions">
        {phase === 'downloading' && <button onClick={() => { setCancelError(false); void invoke('cancel_update_download').catch(() => setCancelError(true)); }}>{t("다운로드 취소", "Cancel download")}</button>}
        <button disabled={busy} onClick={() => run('check_update')}>{t("업데이트 확인", "Check for updates")}</button>
        {status?.installed && phase === 'available' && <button disabled={busy} onClick={() => run('download_update')}>{t("다운로드", "Download")}</button>}
        {status?.installed && phase === 'ready' && <button disabled={busy || !persisted} onClick={() => run('install_update')}>{t("설치 후 다시 시작", "Install and restart")}</button>}
        <button disabled={working} onClick={() => void releasePage()}>{t("릴리스 페이지", "Releases page")}</button>
        <button disabled={working} onClick={() => setOpen(false)}>{t("닫기", "Close")}</button>
      </div>
    </dialog>
  </>;
}
