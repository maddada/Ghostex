import { expect, test } from 'vitest';
import { buildSidebarSessionDetailsClipboardText } from './session-details-copy';
import type { SidebarSessionGroup, SidebarSessionItem } from './session-grid-contract-sidebar';

const BASE_SESSION: SidebarSessionItem = {
  activity: 'working',
  alias: 'Codex Session',
  column: 0,
  isFocused: true,
  isRunning: true,
  isVisible: true,
  row: 0,
  sessionId: 'session-1',
  shortcutLabel: '⌘1',
};

test('builds copyable sidebar session details without prompt text', () => {
  /**
   * CDXC:ContextMenus 2026-06-11-23:08:
   * Copy details includes session/project metadata needed for handoffs, but it
   * must not include the first user message because that is prompt content, not
   * a stable session identifier.
   */
  const session: SidebarSessionItem = {
    ...BASE_SESSION,
    agentIcon: 'codex',
    agentSessionId: 'agent-session-123',
    firstUserMessage: 'private prompt text',
    primaryTitle: 'Investigate startup',
    sessionPersistenceName: 'zmx-session-1',
    sessionPersistenceProvider: 'zmx',
  };
  const group = {
    projectContext: {
      canRemoveProject: true,
      editor: {
        diffStats: { additions: 0, deletions: 0, files: 0, isLoading: false, isRepo: true },
        isOpen: false,
        isSleeping: false,
        projectId: 'project-1',
        status: 'idle',
      },
      path: '/Users/madda/dev/project',
      worktree: {
        branch: 'feature/copy-details',
        name: 'copy-details',
        parentProjectId: 'project-root',
        parentProjectName: 'Project Root',
        parentProjectPath: '/Users/madda/dev/project-root',
      },
    },
    title: 'Project',
  } satisfies Pick<SidebarSessionGroup, 'projectContext' | 'title'>;

  const text = buildSidebarSessionDetailsClipboardText(session, group);

  expect(text).toContain('Title: Investigate startup');
  expect(text).toContain('Session ID: session-1');
  expect(text).toContain('Agent: Codex');
  expect(text).toContain('Agent Session ID: agent-session-123');
  expect(text).toContain('Persistence: zmx (zmx-session-1)');
  expect(text).toContain('Project Path: /Users/madda/dev/project');
  expect(text).toContain('Worktree Branch: feature/copy-details');
  expect(text).not.toContain('private prompt text');
});

test('keeps remote machine and project fields in session details', () => {
  /**
   * CDXC:ContextMenus 2026-06-30-15:22:
   * Remote Copy details must preserve the remote machine, project, and remote
   * project path from the rendered sidebar group. Those fields are the user's
   * stable handoff context for remote gxserver sessions.
   */
  const session: SidebarSessionItem = {
    ...BASE_SESSION,
    agentIcon: 'codex',
    primaryTitle: 'Remote Codex',
    sessionId: 'remote:machine-1:session:project-1:session-1',
  };
  const group = {
    projectContext: {
      canRemoveProject: false,
      editor: {
        diffStats: { additions: 0, deletions: 0, files: 0, isLoading: false, isRepo: true },
        isOpen: false,
        isSleeping: false,
        projectId: 'remote:machine-1:project:project-1',
        status: 'idle',
      },
      path: '/home/madda/remote-project',
    },
    remoteMachineContext: {
      machineId: 'machine-1',
      machineName: 'Ubuntu Builder',
    },
    title: 'Remote Project',
  } satisfies Pick<SidebarSessionGroup, 'projectContext' | 'remoteMachineContext' | 'title'>;

  const text = buildSidebarSessionDetailsClipboardText(session, group);

  expect(text).toContain('Remote Machine: Ubuntu Builder');
  expect(text).toContain('Project: Remote Project');
  expect(text).toContain('Project Path: /home/madda/remote-project');
});

test('keeps minimal session details useful', () => {
  const text = buildSidebarSessionDetailsClipboardText(BASE_SESSION);

  expect(text).toBe(
    [
      'Ghostex Session',
      'Title: Codex Session',
      'Session ID: session-1',
      'Kind: Terminal',
      'Status: running',
      'Activity: working',
    ].join('\n')
  );
});
