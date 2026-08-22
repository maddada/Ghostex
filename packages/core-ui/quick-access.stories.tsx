import type { Meta, StoryObj } from '@storybook/react-vite';
import { useEffect, useMemo } from 'react';
import type { GxserverStashedPrompt } from '../shared/gxserver-protocol';
import { createDefaultSidebarCommandButtons } from '../shared/sidebar-commands';
import type {
  SidebarPreviousSessionItem,
  SidebarRecentProject,
  SidebarToExtensionMessage,
} from '../shared/session-grid-contract';
import { CommandPalette } from './command-palette';
import { PreviousSessionsModal } from './previous-sessions-modal';
import { RecentProjectsModal } from './recent-projects-modal';
import { useSidebarStore } from './sidebar-store';
import { createStoryPreviousSession, createStorySession } from './sidebar-story-fixture-helpers';
import { StashedPromptsModal } from './stashed-prompts-modal';
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

const STORY_OLDER_PREVIOUS_SESSIONS: SidebarPreviousSessionItem[] = [
  createStoryPreviousSession({
    alias: 'Older release investigation',
    closedAt: '2026-07-18T12:20:00.000Z',
    detail: 'OpenAI Codex',
    historyId: 'quick-access-history-older-1',
    sessionId: 'quick-access-session-older-1',
    shortcutLabel: '⌘⌥4',
  }),
];

const STORY_OPEN_SESSIONS = [
  createStorySession({
    alias: 'Quick Access live session',
    detail: 'OpenAI Codex',
    isFocused: true,
    isRunning: true,
    isVisible: true,
    lastInteractionAt: '2026-08-07T08:45:00.000Z',
    sessionId: 'quick-access-open-session-1',
    shortcutLabel: '⌘1',
  }),
  createStorySession({
    alias: 'Review current workspace',
    detail: 'Claude Code',
    isRunning: true,
    lastInteractionAt: '2026-08-07T08:00:00.000Z',
    sessionId: 'quick-access-open-session-2',
    shortcutLabel: '⌘2',
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

const STORY_SAVED_PROMPTS: GxserverStashedPrompt[] = [
  {
    content: 'Review the current implementation and suggest the smallest reliable fix.',
    createdAt: '2026-08-08T08:00:00.000Z',
    cwd: '/Users/demo/Ghostex',
    projectId: 'quick-access-project-1',
    projectName: 'Ghostex',
    promptId: 'quick-access-prompt-1',
    sessionId: 'quick-access-open-session-1',
    updatedAt: '2026-08-08T08:00:00.000Z',
  },
  {
    content: 'Check the release flow for anything that could fail after publishing.',
    createdAt: '2026-08-07T19:00:00.000Z',
    cwd: '/Users/demo/Release Tools',
    projectId: 'quick-access-project-3',
    projectName: 'Release Tools',
    promptId: 'quick-access-prompt-2',
    sessionId: null,
    updatedAt: '2026-08-07T19:00:00.000Z',
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
            cursor: message.cursor ? undefined : 'older',
            previousSessions: message.cursor ? STORY_OLDER_PREVIOUS_SESSIONS : STORY_PREVIOUS_SESSIONS,
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
          return;
        }
        if (message.type === 'requestStashedPrompts') {
          dispatchStoryMessage({
            prompts: STORY_SAVED_PROMPTS,
            requestId: message.requestId,
            type: 'stashedPromptsResult',
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
  useEffect(() => {
    useSidebarStore.setState({
      groupsById: {
        'quick-access-story-group': {
          groupId: 'quick-access-story-group',
          isActive: true,
          isFocusModeActive: false,
          layoutVisibleCount: 1,
          title: 'Ghostex',
          viewMode: 'grid',
          visibleCount: 1,
        },
      },
      sessionIdsByGroup: {
        'quick-access-story-group': STORY_OPEN_SESSIONS.map((session) => session.sessionId),
      },
      sessionsById: Object.fromEntries(STORY_OPEN_SESSIONS.map((session) => [session.sessionId, session])),
    });
    return () => {
      useSidebarStore.getState().reset();
    };
  }, []);
  return <PreviousSessionsModal isOpen={true} onClose={() => undefined} vscode={vscode} />;
}

function SavedPromptsStory() {
  const vscode = useQuickAccessStoryHost();
  return (
    <StashedPromptsModal
      isOpen={true}
      onClose={() => undefined}
      projectId='quick-access-project-1'
      sessionId='quick-access-open-session-1'
      stashHintTooltipDefaultOpen={true}
      vscode={vscode}
    />
  );
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
export const Sessions: Story = { render: () => <RecentSessionsStory /> };
export const SavedPrompts: Story = { render: () => <SavedPromptsStory /> };
export const RecentProjectsLoading: Story = { render: () => <RecentProjectsLoadingStory /> };
export const RecentSessionsLoading: Story = { render: () => <RecentSessionsLoadingStory /> };
