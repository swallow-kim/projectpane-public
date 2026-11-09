import { fileActionText, type FileActionKind } from '../../shared/operations';
import { FolderPlus, Pencil, Copy, MoveRight, Trash2, ClipboardCopy, Scissors, ClipboardPaste } from 'lucide-react';

const actionIcons = { new_folder: FolderPlus, rename: Pencil, copy: Copy, move: MoveRight, recycle: Trash2 };

export function PaneFileActions({ pane, enabled, selected, recycleEligible, request, transferEnabled, canPaste, transfer }: {
  readonly pane: number; readonly enabled: boolean; readonly selected: number; readonly request: (kind: FileActionKind) => void;
  readonly transferEnabled: boolean; readonly canPaste: boolean; readonly transfer: (action: 'copy' | 'move' | 'paste') => void;
  readonly recycleEligible: boolean;
}) {
  return <div className="file-actions" role="group" aria-label={`패널 ${pane + 1} 파일 작업`} onKeyDown={event => event.stopPropagation()}>
    {(['new_folder', 'rename', 'copy', 'move', 'recycle'] as const).map(kind => { const Icon = actionIcons[kind]; return <button key={kind} type="button" title={fileActionText[kind]}
      aria-label={`패널 ${pane + 1} ${fileActionText[kind]}`} disabled={!enabled || (kind !== 'new_folder' && (selected === 0 || (kind !== 'copy' && selected > 1024))) || (kind === 'rename' && selected !== 1) || (kind === 'recycle' && !recycleEligible)}
      onClick={() => request(kind)}><Icon size={15} aria-hidden="true" /></button>; })}
    <button type="button" disabled={!enabled || !transferEnabled || !selected} aria-label="복사 담기" title="앱 안에 복사 (Ctrl+Shift+C)" onClick={() => transfer('copy')}><ClipboardCopy size={15} aria-hidden="true" /></button>
    <button type="button" disabled={!enabled || !transferEnabled || !selected || selected > 1024} aria-label="잘라내기" title="앱 안에 잘라내기 (Ctrl+X)" onClick={() => transfer('move')}><Scissors size={15} aria-hidden="true" /></button>
    <button type="button" disabled={!enabled || !transferEnabled || !canPaste} aria-label="앱에 붙여넣기" title="앱에 보관한 항목을 붙여넣기 (Ctrl+Shift+V)" onClick={() => transfer('paste')}><ClipboardPaste size={15} aria-hidden="true" /></button>
  </div>;
}
