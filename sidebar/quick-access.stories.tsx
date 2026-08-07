import type { Meta, StoryObj } from '@storybook/react-vite';
import { useEffect, useMemo } from 'react';
import { createDefaultSidebarCommandButtons } from '../shared/sidebar-commands';
import type {
  SidebarPreviousSessionItem,
  SidebarRecentProject,
  SidebarToExtensionMessage,
} from '../shared/session-grid-contract';
import { CommandPalette } from './command-palette';
import { PreviousSessionsModal } from './previous-sessions-modal';
import { RecentProjectsModal } from './recent-projects-modal';
import { createStoryPreviousSession } from './sidebar-story-fixture-helpers';
import type { WebviewApi } from './webview-api';

const STORY_PREVIOUS_SESSIONS: SidebarPreviousSessionItem[] = [
  createStoryPreviousSession({
    alias: 'Unify Quick Access styling',
    closedAt: '2026-08-07T08:15:00.000Z',
    detail: 'OpenAI Codex',
    historyId: 'quick-access-history-1',
    sessionId: 'quick-access-session-1',
    shortcutLabel: '⌘⌥1',
  }),
  createStoryPreviousSession({
    alias: 'Release follow-up',
    closedAt: '2026-08-07T07:30:00.000Z',
    detail: 'Claude Code',
    historyId: 'quick-access-history-2',
    sessionId: 'quick-access-session-2',
    shortcutLabel: '⌘⌥2',
  }),
  createStoryPreviousSession({
    alias: 'Sidebar interaction audit',
    closedAt: '2026-08-06T18:40:00.000Z',
    detail: 'Browser',
    historyId: 'quick-access-history-3',
    sessionId: 'quick-access-session-3',
    shortcutLabel: '⌘⌥3',
  }),
];

const STORY_RECENT_PROJECTS: SidebarRecentProject[] = [
  {
    path: '/Users/demo/Ghostex',
    projectId: 'quick-access-project-1',
    recentClosedAt: '2026-08-07T08:10:00.000Z',
    sessionCount: 7,
    title: 'Ghostex',
  },
  {
    path: '/Users/demo/Design System',
    projectId: 'quick-access-project-2',
    recentClosedAt: '2026-08-07T06:45:00.000Z',
    sessionCount: 3,
    title: 'Design System',
  },
  {
    path: '/Users/demo/Release Tools',
    projectId: 'quick-access-project-3',
    recentClosedAt: '2026-08-06T19:05:00.000Z',
    sessionCount: 2,
    title: 'Release Tools',
  },
];

function dispatchStoryMessage(data: unknown): void {
  window.setTimeout(() => {
    window.dispatchEvent(new MessageEvent('message', { data }));
  }, 0);
}

function useQuickAccessStoryHost(respondToRequests = true): WebviewApi {
  useEffect(() => {
    const previousWebkit = window.webkit;
    document.body.classList.add('app-modal-host-body');
    window.webkit = {
      ...previousWebkit,
      messageHandlers: {
        ...previousWebkit?.messageHandlers,
        ghostexAppModalHost: {
          postMessage: () => undefined,
        },
      },
    };
    return () => {
      document.body.classList.remove('app-modal-host-body');
      window.webkit = previousWebkit;
    };
  }, []);

  return useMemo(
    () => ({
      postMessage(message: SidebarToExtensionMessage) {
        if (!respondToRequests) {
          return;
        }
        if (message.type === 'requestPreviousSessions') {
          dispatchStoryMessage({
            previousSessions: STORY_PREVIOUS_SESSIONS,
            query: message.query,
            requestId: message.requestId,
            type: 'previousSessionsResult',
          });
          return;
        }
        if (message.type === 'requestRecentProjects') {
          dispatchStoryMessage({
            machineId: message.machineId,
            recentProjects: STORY_RECENT_PROJECTS,
            type: 'recentProjectsResult',
          });
        }
      },
    }),
    [respondToRequests]
  );
}

function CommandPaneStory() {
  const vscode = useQuickAccessStoryHost();
  return (
    <CommandPalette
      commands={createDefaultSidebarCommandButtons()}
      isOpen={true}
      onOpenChange={() => undefined}
      vscode={vscode}
    />
  );
}

function CommandPaneLoadingStory() {
  const vscode = useQuickAccessStoryHost(false);
  return (
    <CommandPalette
      commands={[]}
      initialQuery='waiting-for-command-hydration'
      isInitialLoadResolved={false}
      isOpen={true}
      onOpenChange={() => undefined}
      vscode={vscode}
    />
  );
}

function RecentProjectsStory() {
  const vscode = useQuickAccessStoryHost();
  return <RecentProjectsModal isOpen={true} onClose={() => undefined} vscode={vscode} />;
}

function RecentSessionsStory() {
  const vscode = useQuickAccessStoryHost();
  return <PreviousSessionsModal isOpen={true} onClose={() => undefined} vscode={vscode} />;
}

function RecentProjectsLoadingStory() {
  const vscode = useQuickAccessStoryHost(false);
  return <RecentProjectsModal isOpen={true} onClose={() => undefined} vscode={vscode} />;
}

function RecentSessionsLoadingStory() {
  const vscode = useQuickAccessStoryHost(false);
  return <PreviousSessionsModal isOpen={true} onClose={() => undefined} vscode={vscode} />;
}

const meta = {
  title: 'Quick Access/Visual Comparison',
  parameters: {
    layout: 'fullscreen',
  },
} satisfies Meta;

export default meta;

type Story = StoryObj<typeof meta>;

export const CommandPane: Story = { render: () => <CommandPaneStory /> };
export const CommandPaneLoading: Story = { render: () => <CommandPaneLoadingStory /> };
export const RecentProjects: Story = { render: () => <RecentProjectsStory /> };
export const RecentSessions: Story = { render: () => <RecentSessionsStory /> };
export const RecentProjectsLoading: Story = { render: () => <RecentProjectsLoadingStory /> };
export const RecentSessionsLoading: Story = { render: () => <RecentSessionsLoadingStory /> };
