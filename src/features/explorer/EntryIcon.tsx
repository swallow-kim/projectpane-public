import type { Entry } from '../../shared/contracts';
import { FileText, Folder, Link, FileQuestion } from 'lucide-react';
const icons = { folder: Folder, file: FileText, link: Link, unknown: FileQuestion };
export function EntryIcon({ kind }: { readonly kind: Entry['kind'] }) {
  const Icon = icons[kind];
  return <Icon aria-hidden="true" className={kind === 'folder' ? 'folder' : 'file'} size={15} strokeWidth={1.5} />;
}
