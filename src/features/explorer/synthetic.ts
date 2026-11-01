import type { Entry } from '../../shared/contracts';

export function syntheticRow(index: number): Entry {
  return { id: `synthetic-${index}`, name: `${String(index + 1).padStart(6, '0')} · ${index % 7 === 0 ? '한글 프로젝트' : 'ProjectPane file.txt'}`, kind: index % 7 === 0 ? 'folder' : 'file', modifiedAtMs: Date.UTC(2026, 9, 1, 12, index % 60), sizeBytes: index % 7 === 0 ? null : index * 1024 };
}
