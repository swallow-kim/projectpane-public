import { z } from 'zod';
import { viewSchema } from './listing-view';
import { absoluteProjectPathSchema, pathRefSchema } from './projects-path';
import { MAX_PROJECT_BYTES, unicode, utf8Bytes } from './projects-error';

export const projectIdSchema = z.string().regex(/^[A-Za-z0-9-]{1,80}$/).brand<'ProjectId'>();
const token = z.string().regex(/^[A-Za-z0-9-]{1,80}$/);
export const projectRevisionSchema = z.number().int().min(0).max(Number.MAX_SAFE_INTEGER);
export const scrollSchema = z.strictObject({
  entryId: z.string().min(1).max(2048).refine(value => value.length <= 2048 && unicode(value) && !value.includes('\0')).nullable(),
  index: z.number().int().min(0).max(99999),
  intraRowOffset: z.number().min(0).lt(30),
}).readonly();
export const projectTabSchema = z.strictObject({
  id: token, path: pathRefSchema, view: viewSchema.refine(view => view.filter.length <= 256 && unicode(view.filter)), scroll: scrollSchema,
}).readonly();
export const projectPaneSchema = z.strictObject({
  id: token, activeTabId: token, tabs: z.array(projectTabSchema).min(1).max(32).readonly(),
}).refine(pane => pane.tabs.some(tab => tab.id === pane.activeTabId)).readonly();
export const workspaceSchema = z.strictObject({
  layout: z.union([z.literal(1), z.literal(2), z.literal(4)]),
  columns: z.number().min(20).max(80), rows: z.number().min(20).max(80),
  activePane: z.number().int().min(0).max(3),
  panes: z.tuple([projectPaneSchema, projectPaneSchema, projectPaneSchema, projectPaneSchema]).readonly(),
}).refine(workspace => {
  const tabs = workspace.panes.flatMap(pane => pane.tabs.map(tab => tab.id));
  return workspace.activePane < workspace.layout
    && new Set(workspace.panes.map(pane => pane.id)).size === 4 && new Set(tabs).size === tabs.length;
}).readonly();
export type Workspace = z.infer<typeof workspaceSchema>;
const bounded = <T extends z.ZodType>(schema: T) => schema.refine(value => utf8Bytes(JSON.stringify(value)) <= MAX_PROJECT_BYTES, { message: 'payload_limit' });
export const projectSchema = bounded(z.strictObject({
  schemaVersion: z.literal(1), id: projectIdSchema, revision: projectRevisionSchema,
  name: z.string().min(1).max(256).refine(value => value.length <= 256 && unicode(value) && utf8Bytes(value) <= 1024 && /[^\s]/u.test(value) && !/[\x00-\x1f]/.test(value)),
  rootPath: absoluteProjectPathSchema, preset: workspaceSchema,
})).readonly();
export const sessionSchema = bounded(z.strictObject({
  schemaVersion: z.literal(1), projectId: projectIdSchema, projectRevision: projectRevisionSchema,
  revision: projectRevisionSchema, workspace: workspaceSchema,
})).readonly();
export const settingsSchema = bounded(z.strictObject({
  schemaVersion: z.literal(1), lastProjectId: projectIdSchema.nullable(),
})).readonly();
export type Project = z.infer<typeof projectSchema>;
export type Session = z.infer<typeof sessionSchema>;
export type Settings = z.infer<typeof settingsSchema>;
