import type { Project, Workspace } from '../../shared/projects';

export type ProjectMetadata = Omit<Project, 'preset' | 'schemaVersion'>;
export type LoadedProject = {
  readonly project: ProjectMetadata;
  readonly workspace: Workspace;
  readonly sessionRevision: number;
  readonly readOnly: boolean;
  readonly warnings: readonly string[];
};
export type ProjectList = {
  readonly projects: readonly ProjectMetadata[];
  readonly lastProjectId: string | null;
  readonly warnings: readonly string[];
};
/** UI-facing seam; only the IPC adapter knows backend request envelopes. */
export interface ProjectClient {
  list(): Promise<ProjectList>;
  load(id: string, source: 'session' | 'preset'): Promise<LoadedProject>;
  create(name: string, rootPath: string, workspace: Workspace): Promise<LoadedProject>;
  rename(project: ProjectMetadata, name: string): Promise<ProjectMetadata>;
  duplicate(project: ProjectMetadata, name: string): Promise<LoadedProject>;
  remove(project: ProjectMetadata): Promise<void>;
  savePreset(project: ProjectMetadata, workspace: Workspace): Promise<ProjectMetadata>;
  saveSession(project: ProjectMetadata, revision: number, workspace: Workspace): Promise<number>;
}
