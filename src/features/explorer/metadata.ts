const knownTypes: Readonly<Record<string, string>> = {
  txt: '텍스트 문서', md: 'Markdown 문서', pdf: 'PDF 문서',
  doc: 'Word 문서', docx: 'Word 문서', xls: 'Excel 문서', xlsx: 'Excel 문서',
  ppt: 'PowerPoint 문서', pptx: 'PowerPoint 문서',
  png: 'PNG 이미지', jpg: 'JPEG 이미지', jpeg: 'JPEG 이미지', gif: 'GIF 이미지',
  zip: 'ZIP 압축 파일', exe: '응용 프로그램', lnk: '바로 가기',
};

export function typeLabel(kind: string, name: string): string {
  if (kind === 'folder') return '폴더';
  if (kind === 'link') return '링크';
  if (kind !== 'file') return '확인 불가';
  const dot = name.lastIndexOf('.');
  if (dot <= 0 || dot === name.length - 1) return '파일';
  const extension = name.slice(dot + 1).toLowerCase();
  return (Object.hasOwn(knownTypes, extension) ? knownTypes[extension] : undefined) ?? `${extension.toUpperCase()} 파일`;
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
  return `${value.toLocaleString('ko-KR', { maximumFractionDigits: 1 })} ${units[unit]}`;
}
