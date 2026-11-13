import { t, useLanguage } from '../../shared/i18n';
import { fileActionText, type FileActionKind } from '../../shared/operations';
import { FolderPlus, Pencil, Copy, MoveRight, Trash2, ClipboardCopy, Scissors, ClipboardPaste } from 'lucide-react';

const actionIcons = { new_folder: FolderPlus, rename: Pencil, copy: Copy, move: MoveRight, recycle: Trash2 };

export function PaneFileActions({ pane, enabled, selected, recycleEligible, request, transferEnabled, canPaste, transfer }: {
  readonly pane: number; readonly enabled: boolean; readonly selected: number; readonly request: (kind: FileActionKind) => void;
  readonly transferEnabled: boolean; readonly canPaste: boolean; readonly transfer: (action: 'copy' | 'move' | 'paste') => void;
  readonly recycleEligible: boolean;
}) {
  useLanguage();
  return <div className="file-actions" role="group" aria-label={t('패널 {pane} 파일 작업', 'Pane {pane} file operations', { pane: pane + 1 })} onKeyDown={event => event.stopPropagation()}>
    {(['new_folder', 'rename', 'copy', 'move', 'recycle'] as const).map(kind => { const Icon = actionIcons[kind]; return <button key={kind} type="button" title={fileActionText[kind]}
      aria-label={t('패널 {pane} {action}', 'Pane {pane} {action}', { pane: pane + 1, action: fileActionText[kind] })} disabled={!enabled || (kind !== 'new_folder' && selected === 0) || (kind === 'rename' && selected !== 1) || (kind === 'recycle' && (!recycleEligible || selected > 1024))}
      onClick={() => request(kind)}><Icon size={15} aria-hidden="true" /></button>; })}
    <button type="button" disabled={!enabled || !transferEnabled || !selected} aria-label={t("복사 담기", "Copy to app")} title={t("앱 안에 복사 (Ctrl+Shift+C)", "Copy within app (Ctrl+Shift+C)")} onClick={() => transfer('copy')}><ClipboardCopy size={15} aria-hidden="true" /></button>
    <button type="button" disabled={!enabled || !transferEnabled || !selected} aria-label={t("잘라내기", "Cut")} title={t("앱 안에 잘라내기 (Ctrl+X)", "Cut within app (Ctrl+X)")} onClick={() => transfer('move')}><Scissors size={15} aria-hidden="true" /></button>
    <button type="button" disabled={!enabled || !transferEnabled || !canPaste} aria-label={t("앱에 붙여넣기", "Paste in app")} title={t("앱에 보관한 항목을 붙여넣기 (Ctrl+Shift+V)", "Paste items stored in app (Ctrl+Shift+V)")} onClick={() => transfer('paste')}><ClipboardPaste size={15} aria-hidden="true" /></button>
  </div>;
}
