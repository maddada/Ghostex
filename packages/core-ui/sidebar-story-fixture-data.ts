import type { SidebarStoryFixture } from './sidebar-story-fixtures';
import { createStorySession, type SidebarStoryGroup } from './sidebar-story-fixture-helpers';
import { createDefaultSidebarProjectDiffStats } from '../shared/project-diff-stats';

function minutesAgo(minutes: number): string {
  return new Date(Date.now() - minutes * 60 * 1000).toISOString();
}

function secondsAgo(seconds: number): string {
  return new Date(Date.now() - seconds * 1000).toISOString();
}

function createStoryProjectContext(projectId: string): NonNullable<SidebarStoryGroup['projectContext']> {
  return {
    canRemoveProject: true,
    path: `/Users/story/dev/${projectId}`,
    /**
     * CDXC:ProjectDiffStats 2026-05-15-14:33:
     * Sidebar stories keep project editor state in project context because the
     * project header still renders git diff stats from that shared editor
     * contract even though the sidebar Code row is no longer visible.
     *
     * CDXC:EditorPanes 2026-05-09-17:24
     * Project editor fixture state includes load status because Storybook must
     * exercise the same host state the titlebar and native editor page consume.
     */
    editor: {
      diffStats: createDefaultSidebarProjectDiffStats(),
      isOpen: false,
      isSleeping: false,
      projectId,
      status: 'idle',
    },
    theme: 'plain-dark',
  };
}

function createStoryOpenProjectEditorContext(projectId: string): NonNullable<SidebarStoryGroup['projectContext']> {
  const projectContext = createStoryProjectContext(projectId);
  return {
    ...projectContext,
    /**
     * CDXC:EditorPanes 2026-05-15-13:58:
     * The Combined header-alignment Storybook fixture keeps the project editor
     * row visible with nonzero diff stats so the project-header placement can
     * be verified while the Code row remains a stable label.
     */
    editor: {
      ...projectContext.editor,
      diffStats: {
        additions: 5,
        deletions: 5,
        files: 2,
        isLoading: false,
        isRepo: true,
      },
      isOpen: true,
      status: 'running',
    },
  };
}

const DEFAULT_GROUPS: SidebarStoryGroup[] = [
  {
    groupId: 'group-1',
    isActive: false,
    sessions: [
      createStorySession({
        alias: 'show title in 2nd row',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        lastInteractionAt: secondsAgo(30),
        sessionId: 'session-1',
        shortcutLabel: '⌘⌥1',
      }),
      createStorySession({
        alias: 'layout drift fix',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(7),
        sessionId: 'session-2',
        shortcutLabel: '⌘⌥2',
      }),
      createStorySession({
        alias: 'Harbor Vale',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(18),
        sessionId: 'session-3',
        shortcutLabel: '⌘⌥3',
      }),
    ],
    title: 'Main',
  },
  {
    groupId: 'group-2',
    isActive: false,
    sessions: [
      createStorySession({
        activity: 'attention',
        alias: 'tooltip & show an indicator on the active card',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(3),
        sessionId: 'session-4',
        shortcutLabel: '⌘⌥4',
      }),
      createStorySession({
        alias: 'Indigo Grove',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(11),
        sessionId: 'session-5',
        shortcutLabel: '⌘⌥5',
      }),
    ],
    title: 'Group 2',
  },
  {
    groupId: 'group-4',
    isActive: true,
    sessions: [
      createStorySession({
        alias: 'Amber Lattice',
        detail: 'OpenAI Codex',
        isFocused: true,
        isVisible: true,
        sessionId: 'session-6',
        shortcutLabel: '⌘⌥6',
      }),
    ],
    title: 'Group 4',
  },
];

const COMMAND_INDICATOR_ACTIVE_GROUPS: SidebarStoryGroup[] = [
  {
    groupId: 'group-1',
    isActive: true,
    sessions: [
      createStorySession({
        alias: 'Dev server',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        isFocused: true,
        isVisible: true,
        lastInteractionAt: secondsAgo(12),
        sessionId: 'session-1',
        shortcutLabel: '⌘⌥1',
      }),
      createStorySession({
        alias: 'layout drift fix',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(7),
        sessionId: 'session-2',
        shortcutLabel: '⌘⌥2',
      }),
    ],
    title: 'Main',
  },
];

/*
 * CDXC:AgentDetection 2026-04-27-06:47
 * Keep a narrow Storybook fixture for verifying that sidebar cards render the
 * agent identity already present in session data before changing production CSS.
 */
const AGENT_ICON_RENDER_GROUPS: SidebarStoryGroup[] = [
  {
    groupId: 'agent-icon-main',
    isActive: true,
    sessions: [
      createStorySession({
        alias: 'Codex assigned',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        isFocused: true,
        isVisible: true,
        lastInteractionAt: secondsAgo(15),
        sessionId: 'agent-icon-codex',
        shortcutLabel: '⌘⌥1',
        terminalTitle: 'codex',
      }),
      createStorySession({
        alias: 'Claude assigned',
        agentIcon: 'claude',
        detail: 'Claude Code',
        isVisible: true,
        lastInteractionAt: secondsAgo(35),
        sessionId: 'agent-icon-claude',
        shortcutLabel: '⌘⌥2',
        terminalTitle: '✳ Claude Code',
      }),
    ],
    title: 'Main',
  },
];

const SELECTOR_STATE_GROUPS: SidebarStoryGroup[] = [
  {
    groupId: 'group-1',
    isActive: true,
    sessions: [
      createStorySession({
        activity: 'working',
        alias: 'active refactor',
        detail: 'Claude Code',
        isFocused: true,
        isVisible: true,
        lastInteractionAt: minutesAgo(2),
        sessionId: 'session-1',
        shortcutLabel: '⌘⌥1',
      }),
      createStorySession({
        alias: 'ui hover audit',
        detail: 'OpenAI Codex',
        isVisible: true,
        lastInteractionAt: minutesAgo(6),
        sessionId: 'session-2',
        shortcutLabel: '⌘⌥2',
      }),
      createStorySession({
        activity: 'attention',
        alias: 'terminal title indicator',
        detail: 'OpenAI Codex',
        isVisible: true,
        lastInteractionAt: minutesAgo(9),
        sessionId: 'session-3',
        shortcutLabel: '⌘⌥3',
      }),
      createStorySession({
        alias: 'workspace sync',
        detail: 'OpenAI Codex',
        isVisible: true,
        lastInteractionAt: minutesAgo(24),
        sessionId: 'session-4',
        shortcutLabel: '⌘⌥4',
      }),
    ],
    title: 'Main',
  },
  {
    groupId: 'group-2',
    isActive: false,
    sessions: [
      createStorySession({
        alias: 'fallback styling pass',
        detail: 'OpenAI Codex',
        isRunning: false,
        lastInteractionAt: minutesAgo(42),
        sessionId: 'session-5',
        shortcutLabel: '⌘⌥5',
      }),
    ],
    title: 'Review',
  },
];

const SORT_TOGGLE_DEMO_GROUPS: SidebarStoryGroup[] = [
  {
    groupId: 'group-1',
    isActive: true,
    sessions: [
      createStorySession({
        alias: 'older draft first',
        detail: 'OpenAI Codex',
        isFocused: true,
        isVisible: true,
        lastInteractionAt: minutesAgo(18),
        sessionId: 'session-1',
        shortcutLabel: '⌘⌥1',
      }),
      createStorySession({
        activity: 'working',
        alias: 'most recent follow-up',
        detail: 'OpenAI Codex',
        isVisible: true,
        lastInteractionAt: minutesAgo(2),
        sessionId: 'session-2',
        shortcutLabel: '⌘⌥2',
      }),
      createStorySession({
        alias: 'middle checkpoint',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(9),
        sessionId: 'session-3',
        shortcutLabel: '⌘⌥3',
      }),
    ],
    title: 'Main',
  },
  {
    groupId: 'group-2',
    isActive: false,
    sessions: [
      createStorySession({
        alias: 'stale notes',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(27),
        sessionId: 'session-4',
        shortcutLabel: '⌘⌥4',
      }),
      createStorySession({
        activity: 'attention',
        alias: 'recent interrupt',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(4),
        sessionId: 'session-5',
        shortcutLabel: '⌘⌥5',
      }),
    ],
    title: 'Review',
  },
];

const OVERFLOW_STRESS_GROUPS: SidebarStoryGroup[] = [
  {
    groupId: 'group-1',
    isActive: true,
    sessions: [
      createStorySession({
        activity: 'working',
        alias: 'extremely long alias for the primary debugging session that should truncate cleanly',
        detail: 'OpenAI Codex running a sidebar layout regression pass with long secondary text',
        isFocused: true,
        isVisible: true,
        lastInteractionAt: secondsAgo(45),
        sessionId: 'session-1',
        shortcutLabel: '⌘⌥1',
        terminalTitle: 'OpenAI Codex / terminal / feature/sidebar-storybook / very-long-branch-name',
      }),
      createStorySession({
        activity: 'attention',
        alias: 'hover tooltip verification for overflow and status chip alignment',
        detail: 'Claude Code with a surprisingly verbose secondary line to stress wrapping assumptions',
        isVisible: true,
        lastInteractionAt: minutesAgo(8),
        sessionId: 'session-2',
        shortcutLabel: '⌘⌥2',
        terminalTitle: 'Claude Code / visual diff / attention state',
      }),
      createStorySession({
        alias: 'inactive session with close button',
        detail: 'Gemini CLI',
        isRunning: false,
        lastInteractionAt: minutesAgo(15),
        sessionId: 'session-3',
        shortcutLabel: '⌘⌥3',
      }),
    ],
    title: 'Main workspace with a deliberately long group title',
  },
  {
    groupId: 'group-2',
    isActive: false,
    sessions: [
      createStorySession({
        alias: 'session card spacing audit across themes',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(4),
        sessionId: 'session-4',
        shortcutLabel: '⌘⌥4',
      }),
      createStorySession({
        alias: 'secondary label overflow with keyboard shortcut visible',
        detail: 'OpenAI Codex with another very long provider name for stress testing',
        lastInteractionAt: minutesAgo(12),
        sessionId: 'session-5',
        shortcutLabel: '⌘⌥5',
      }),
    ],
    title: 'Secondary investigations',
  },
  {
    groupId: 'group-3',
    isActive: false,
    sessions: [
      createStorySession({
        alias: 'one more card for density',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(26),
        sessionId: 'session-6',
        shortcutLabel: '⌘⌥6',
      }),
    ],
    title: 'QA',
  },
];

/*
 * CDXC:SidebarScroll 2026-05-08-10:53
 * Keep a long, stable sidebar fixture for bottom-scroll retention checks. The
 * session list must remain overflowed after the user reaches the end instead
 * of being reclassified as sparse content and snapped back to the top.
 */
const SCROLL_END_RETENTION_GROUPS: SidebarStoryGroup[] = Array.from({ length: 16 }, (_, groupIndex) => ({
  groupId: `scroll-retention-group-${groupIndex + 1}`,
  isActive: groupIndex === 0,
  sessions: [
    createStorySession({
      activity: groupIndex % 5 === 0 ? 'working' : undefined,
      alias: `scroll retention session ${groupIndex + 1}`,
      detail: groupIndex % 2 === 0 ? 'OpenAI Codex' : 'Claude Code',
      isFocused: groupIndex === 0,
      isVisible: groupIndex === 0,
      lastInteractionAt: minutesAgo(groupIndex + 1),
      sessionId: `scroll-retention-session-${groupIndex + 1}`,
      shortcutLabel: `⌘⌥${(groupIndex % 9) + 1}`,
    }),
  ],
  title: `Project ${groupIndex + 1}`,
}));

const EMPTY_GROUPS: SidebarStoryGroup[] = [
  {
    groupId: 'group-1',
    isActive: true,
    sessions: [
      createStorySession({
        alias: 'fresh workspace',
        detail: 'OpenAI Codex',
        isFocused: true,
        isVisible: true,
        lastInteractionAt: minutesAgo(1),
        sessionId: 'session-1',
        shortcutLabel: '⌘⌥1',
      }),
    ],
    title: 'Main',
  },
  {
    groupId: 'group-2',
    isActive: false,
    sessions: [],
    title: 'Design',
  },
  {
    groupId: 'group-3',
    isActive: false,
    sessions: [],
    title: 'Review',
  },
];

/*
 * CDXC:SidebarHover 2026-05-04-08:11
 * Reproduce the native Combined sidebar header-alignment surface. Project
 * groups carry projectContext, Actions are hidden by combined settings, and
 * the active project group exposes split/create controls on hover.
 */
const COMBINED_HEADER_ALIGNMENT_GROUPS: SidebarStoryGroup[] = [
  {
    groupId: 'combined-chats',
    isActive: true,
    isChatCollection: true,
    kind: 'workspace',
    sessions: [
      createStorySession({
        alias: 'Refactor pricing notes',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        isFocused: true,
        isVisible: true,
        lastInteractionAt: secondsAgo(22),
        sessionId: 'combined-chat-pricing',
        shortcutLabel: '⌘⌥1',
      }),
      createStorySession({
        alias: 'Trip brainstorm',
        agentIcon: 'claude',
        detail: 'Claude Code',
        lastInteractionAt: minutesAgo(4),
        sessionId: 'combined-chat-trip',
        shortcutLabel: '⌘⌥2',
      }),
    ],
    title: 'Chats',
  },
  {
    groupId: 'combined-project-root',
    isActive: false,
    kind: 'workspace',
    projectContext: createStoryProjectContext('combined-project-root'),
    sessions: [],
    title: '/',
  },
  {
    groupId: 'combined-project-ghostex',
    isActive: false,
    kind: 'workspace',
    projectContext: createStoryOpenProjectEditorContext('combined-project-ghostex'),
    sessions: [
      {
        ...createStorySession({
          alias: 'Browser Preview',
          detail: 'Browser',
          isVisible: true,
          lastInteractionAt: secondsAgo(25),
          sessionId: 'combined-ghostex-browser',
          shortcutLabel: '⌘⌥1',
        }),
        kind: 'browser',
        sessionKind: 'browser',
      },
      createStorySession({
        alias: 'Terminal Session',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        isFocused: true,
        isVisible: true,
        lastInteractionAt: secondsAgo(45),
        sessionId: 'combined-ghostex-terminal',
        shortcutLabel: '⌘⌥2',
      }),
    ],
    title: 'ghostex',
  },
  {
    groupId: 'combined-project-agent-manager',
    isActive: false,
    kind: 'workspace',
    projectContext: createStoryProjectContext('combined-project-agent-manager'),
    sessions: [],
    title: 'agent-manager-x',
  },
];

const COMBINED_SPARSE_REFERENCE_GROUPS: SidebarStoryGroup[] = [
  {
    groupId: 'combined-sparse-chats',
    isActive: true,
    isChatCollection: true,
    kind: 'workspace',
    sessions: [
      createStorySession({
        alias: 'Terminal Session',
        /**
         * CDXC:SidebarStories 2026-05-09-18:52
         * The sparse reference story is used to validate session title, Last
         * Active, close button, and agent-icon alignment. Keep this row on the
         * explicit agent-icon path instead of the agentless terminal fallback.
         */
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(59),
        sessionId: 'combined-sparse-terminal',
        shortcutLabel: '⌘⌥1',
      }),
    ],
    title: 'Chats',
  },
  {
    groupId: 'combined-sparse-project-ghostex',
    isActive: false,
    kind: 'workspace',
    projectContext: createStoryProjectContext('combined-sparse-project-ghostex'),
    sessions: [
      createStorySession({
        alias: 'Sidebar and settings integration',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        lastInteractionAt: new Date(Date.now() - 9 * 60 * 60 * 1000).toISOString(),
        sessionId: 'combined-sparse-ghostex-session',
        shortcutLabel: '⌘⌥2',
      }),
      /**
       * CDXC:SidebarSearch 2026-05-08-12:02
       * Combined-reference search stories need multiple matching project rows
       * before Previous Sessions so regressions in current-result height
       * measurement reproduce in Storybook.
       */
      createStorySession({
        alias: 'Disable Button Entry',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        lastInteractionAt: new Date(Date.now() - 60 * 60 * 1000).toISOString(),
        sessionId: 'combined-sparse-disable-button',
        shortcutLabel: '⌘⌥3',
      }),
      createStorySession({
        alias: 'Floating Indicator Button Text',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        lastInteractionAt: new Date(Date.now() - 5 * 60 * 60 * 1000).toISOString(),
        sessionId: 'combined-sparse-floating-button',
        shortcutLabel: '⌘⌥4',
      }),
    ],
    title: 'ghostex',
  },
  {
    groupId: 'combined-sparse-project-release-manager',
    isActive: false,
    kind: 'workspace',
    projectContext: createStoryProjectContext('combined-sparse-project-release-manager'),
    sessions: [
      createStorySession({
        alias: 'nn release manager',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        lastInteractionAt: new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString(),
        sessionId: 'combined-sparse-release-manager-session',
        shortcutLabel: '⌘⌥5',
      }),
    ],
    title: 'releases manager',
  },
  {
    groupId: 'combined-sparse-project-iscode-embed',
    isActive: false,
    kind: 'workspace',
    projectContext: createStoryProjectContext('combined-sparse-project-iscode-embed'),
    sessions: [
      createStorySession({
        alias: 'nn iscode embed',
        agentIcon: 'codex',
        detail: 'OpenAI Codex',
        lastInteractionAt: new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString(),
        sessionId: 'combined-sparse-iscode-embed-session',
        shortcutLabel: '⌘⌥6',
      }),
    ],
    title: 'iscode embed',
  },
];

const THREE_GROUPS_STRESS: SidebarStoryGroup[] = [
  {
    groupId: 'group-1',
    isActive: true,
    sessions: [
      createStorySession({
        alias: 'Atlas Forge',
        detail: 'OpenAI Codex',
        isFocused: true,
        isVisible: true,
        lastInteractionAt: secondsAgo(20),
        sessionId: 'session-1',
        shortcutLabel: '⌘⌥1',
      }),
      createStorySession({
        alias: 'Beryl Note',
        detail: 'OpenAI Codex',
        isVisible: true,
        lastInteractionAt: minutesAgo(6),
        sessionId: 'session-2',
        shortcutLabel: '⌘⌥2',
      }),
    ],
    title: 'Main',
  },
  {
    groupId: 'group-2',
    isActive: false,
    sessions: [
      createStorySession({
        alias: 'Cinder Path',
        detail: 'OpenAI Codex',
        lastInteractionAt: minutesAgo(13),
        sessionId: 'session-3',
        shortcutLabel: '⌘⌥3',
      }),
      createStorySession({
        alias: 'Dune Echo',
        detail: 'OpenAI Codex',
        sessionId: 'session-4',
        shortcutLabel: '⌘⌥4',
      }),
    ],
    title: 'Group 2',
  },
  {
    groupId: 'group-3',
    isActive: false,
    sessions: [
      createStorySession({
        alias: 'Elm Signal',
        detail: 'OpenAI Codex',
        sessionId: 'session-5',
        shortcutLabel: '⌘⌥5',
      }),
      createStorySession({
        alias: 'Fjord Thread',
        detail: 'OpenAI Codex',
        sessionId: 'session-6',
        shortcutLabel: '⌘⌥6',
      }),
    ],
    title: 'Group 3',
  },
];

export const GROUPS_BY_FIXTURE: Record<SidebarStoryFixture, SidebarStoryGroup[]> = {
  'agent-icon-render': AGENT_ICON_RENDER_GROUPS,
  'combined-header-alignment': COMBINED_HEADER_ALIGNMENT_GROUPS,
  'combined-recent-projects': COMBINED_HEADER_ALIGNMENT_GROUPS.filter(
    (group) => group.sessions.length > 0 || group.isChatCollection === true
  ),
  'combined-sparse-reference': COMBINED_SPARSE_REFERENCE_GROUPS,
  'command-indicator-active': COMMAND_INDICATOR_ACTIVE_GROUPS,
  default: DEFAULT_GROUPS,
  'empty-groups': EMPTY_GROUPS,
  'overflow-stress': OVERFLOW_STRESS_GROUPS,
  'scroll-end-retention': SCROLL_END_RETENTION_GROUPS,
  'selector-states': SELECTOR_STATE_GROUPS,
  'sort-toggle-demo': SORT_TOGGLE_DEMO_GROUPS,
  'three-groups-stress': THREE_GROUPS_STRESS,
};
