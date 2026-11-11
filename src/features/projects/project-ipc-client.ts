import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';
import { absoluteProjectPathSchema, MAX_PROJECT_BYTES, projectIdSchema, projectRevisionSchema, workspaceSchema } from '../../shared/projects';
import type { ProjectClient } from './project-client';

const metadata = z.strictObject({ id: projectIdSchema, name: z.string().min(1).max(256), rootPath: absoluteProjectPathSchema, revision: projectRevisionSchema });
const warnings = z.array(z.strictObject({ code: z.string().max(80), projectId: projectIdSchema.nullable() })).max(1024);
const loaded = z.strictObject({ project: metadata, workspace: workspaceSchema, sessionRevision: projectRevisionSchema,
  source: z.enum(['preset', 'session']), readOnly: z.boolean(), warnings });
const list = z.strictObject({ projects: z.array(metadata).max(1024), lastProjectId: projectIdSchema.nullable(), warnings });
export class ProjectIpcError extends Error {
  constructor(readonly code: string) { super(code); this.name = 'ProjectIpcError'; }
}
export type ProjectInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
export function createProjectClient(call: ProjectInvoke = invoke): ProjectClient {
  const request = async <T>(command: string, schema: z.ZodType<T>, body?: Record<string, unknown>): Promise<T> => {
    try {
      const raw = await call(command, body ? { request: body } : undefined);
      if (new TextEncoder().encode(JSON.stringify(raw)).byteLength > MAX_PROJECT_BYTES) throw new ProjectIpcError('payload_limit');
      return schema.parse(raw);
    } catch (error) {
      if (error instanceof ProjectIpcError) throw error;
      const fault = z.object({ code: z.string() }).safeParse(error);
      throw new ProjectIpcError(fault.success ? fault.data.code : 'invalid_document');
    }
  };
  const normalize = (result: z.infer<typeof loaded>) => ({ ...result, warnings: result.warnings.map(warning => warning.code) });
  return {
    async list() { const result = await request('list_projects', list); return { ...result, warnings: result.warnings.map(warning => warning.code) }; },
    async load(id, source) { const result = await request('load_project', loaded, { projectId: projectIdSchema.parse(id), source });
      if (result.project.id !== id) throw new ProjectIpcError('invalid_document'); return normalize(result); },
    async create(name, rootPath, workspace) { return normalize(await request('create_project', loaded, { name, rootPath: absoluteProjectPathSchema.parse(rootPath), preset: workspaceSchema.parse(workspace) })); },
    async duplicate(project, name) { return normalize(await request('duplicate_project', loaded, { projectId: project.id, expectedRevision: project.revision, name })); },
    async edit(project, name, rootPath) {
      const result = await request('edit_project', loaded, { projectId: project.id, expectedRevision: project.revision, name, rootPath: absoluteProjectPathSchema.parse(rootPath) });
      if (result.project.id !== project.id) throw new ProjectIpcError('invalid_document');
      return normalize(result);
    },
    rename(project, name) { return request('rename_project', metadata, { projectId: project.id, expectedRevision: project.revision, name }); },
    async remove(project) { const result = await request('remove_project', z.strictObject({ removedProjectId: projectIdSchema }), { projectId: project.id, expectedRevision: project.revision });
      if (result.removedProjectId !== project.id) throw new ProjectIpcError('invalid_document'); },
    savePreset(project, workspace) { return request('save_project_preset', metadata, { projectId: project.id, expectedRevision: project.revision, workspace: workspaceSchema.parse(workspace) }); },
    async saveSession(project, revision, workspace) {
      const result = await request('save_project_session', z.strictObject({ projectId: projectIdSchema, projectRevision: projectRevisionSchema, sessionRevision: projectRevisionSchema }),
        { projectId: project.id, expectedProjectRevision: project.revision, expectedSessionRevision: revision, workspace: workspaceSchema.parse(workspace) });
      if (result.projectId !== project.id || result.projectRevision !== project.revision || result.sessionRevision <= revision) throw new ProjectIpcError('invalid_document');
      return result.sessionRevision;
    },
  };
}
