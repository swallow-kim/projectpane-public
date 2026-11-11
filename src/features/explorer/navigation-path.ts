import { normalizeAbsolutePath } from '../../shared/projects-path';

/** Address-bar relative paths belong to the project, never to a pane's current folder. */
export function resolveNavigationPath(input: string, projectRoot?: string): string {
  if (!projectRoot || /^[A-Za-z]:[\\/]/.test(input) || /^[\\/]{2}/.test(input)) return input;
  // Drive-relative and volume-root-relative input is ambiguous; require an absolute path instead.
  if (/^[\\/]/.test(input) || input.includes(':')) throw new Error('invalid_path');
  return normalizeAbsolutePath(projectRoot.replace(/[\\/]+$/, '') + '\\' + input);
}
