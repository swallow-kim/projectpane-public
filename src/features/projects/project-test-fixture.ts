import { projectSchema, snapshotWorkspace } from '../../shared/projects';
import { createPane } from '../explorer/navigation';
export function projectFixture() {
  return projectSchema.parse({ schemaVersion: 1, id: 'project-1', revision: 0, name: 'Example', rootPath: 'C:\\Example',
    preset: snapshotWorkspace('C:\\Example', { layout: 4, columns: 50, rows: 50, activePane: 0,
      panes: [0, 1, 2, 3].map(() => createPane('C:\\Example')) }).workspace });
}
