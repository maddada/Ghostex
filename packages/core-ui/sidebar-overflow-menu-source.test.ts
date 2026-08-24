import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const sidebarAppSource = readFileSync(new URL('./sidebar-app.tsx', import.meta.url), 'utf8');
const recentProjectsModalSource = readFileSync(new URL('./recent-projects-modal.tsx', import.meta.url), 'utf8');
const groupPanelsCssSource = readFileSync(new URL('./styles/group-panels.css', import.meta.url), 'utf8');

describe('sidebar recent projects source', () => {
  test('renders recent project rows in the modal instead of sidebar sections', () => {
    expect(sidebarAppSource).not.toContain('function RecentProjectsSection(');
    expect(sidebarAppSource).not.toContain('recentProjectsByMachine.remoteByMachineId.get(machine.id)');
    expect(recentProjectsModalSource).toContain('export function RecentProjectRow(');
    expect(recentProjectsModalSource).toContain('{project.title}');
    expect(recentProjectsModalSource).toContain('{project.path}');
    expect(recentProjectsModalSource).toContain('{project.sessionCount}');
    expect(sidebarAppSource).not.toContain('recent-projects-drawer');
    expect(sidebarAppSource).not.toContain('Search recent projects');
    expect(groupPanelsCssSource).toContain('.recent-projects-section');
    expect(groupPanelsCssSource).not.toContain('.recent-projects-drawer');
    expect(sidebarAppSource).not.toContain('function SidebarReferenceSettingsButton(');
  });
});
