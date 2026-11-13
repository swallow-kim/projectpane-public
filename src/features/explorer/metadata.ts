import { t, locale } from '../../shared/i18n';
const knownTypes: Readonly<Record<string, readonly [string, string]>> = {
  txt: ['텍스트 문서', 'Text document'], md: ['Markdown 문서', 'Markdown document'], pdf: ['PDF 문서', 'PDF document'],
  doc: ['Word 문서', 'Word document'], docx: ['Word 문서', 'Word document'], xls: ['Excel 문서', 'Excel workbook'], xlsx: ['Excel 문서', 'Excel workbook'],
  ppt: ['PowerPoint 문서', 'PowerPoint presentation'], pptx: ['PowerPoint 문서', 'PowerPoint presentation'],
  png: ['PNG 이미지', 'PNG image'], jpg: ['JPEG 이미지', 'JPEG image'], jpeg: ['JPEG 이미지', 'JPEG image'], gif: ['GIF 이미지', 'GIF image'],
  zip: ['ZIP 압축 파일', 'ZIP archive'], exe: ['응용 프로그램', 'Application'], lnk: ['바로 가기', 'Shortcut'],
};

export function typeLabel(kind: string, name: string): string {
  if (kind === 'folder') return t('폴더', 'Folder');
  if (kind === 'link') return t('링크', 'Link');
  if (kind !== 'file') return t('확인 불가', 'Unknown');
  const dot = name.lastIndexOf('.');
  if (dot <= 0 || dot === name.length - 1) return t('파일', 'File');
  const extension = name.slice(dot + 1).toLowerCase();
  const label = Object.hasOwn(knownTypes, extension) ? knownTypes[extension] : undefined;
  return label ? t(...label) : t('{extension} 파일', '{extension} file', { extension: extension.toUpperCase() });
}

export function formatModifiedAt(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return '—';
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return '—';
  const pad = (part: number) => String(part).padStart(2, '0');
  return `${String(date.getFullYear()).padStart(4, '0')}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}
export function formatSize(bytes: number | null): string {
  if (bytes === null) return '—';
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KiB', 'MiB', 'GiB', 'TiB', 'PiB'];
  let value = bytes / 1024, unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++; }
  return `${value.toLocaleString(locale(), { maximumFractionDigits: 1 })} ${units[unit]}`;
}
