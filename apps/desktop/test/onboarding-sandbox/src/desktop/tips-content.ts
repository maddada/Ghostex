/*
 * Visual re-mock of the tips catalog. Titles/bodies are copied verbatim from
 * TITLEBAR_TIPS in apps/desktop/views/titlebar/tips-data.ts so the sandbox panel reads
 * like the real Tips & Tricks dropdown. The real component is not mounted here
 * (see tips-panel.tsx for why), and notices come from the engine via
 * store.tipsNotices.
 */
export interface SandboxTip {
  body: string;
  id: string;
  title: string;
}

export const SANDBOX_TIPS: SandboxTip[] = [
  {
    body: 'Search for project actions, pane splits and moves, session controls, settings shortcuts, and other Ghostex actions.',
    id: 'command-palette-all-actions',
    title: 'Press Cmd Shift P anywhere to open Ghostex Quick Access',
  },
  {
    body: 'Open Settings to customize sidebar presets, visible details, agents, actions, project tools, and workspace open targets.',
    id: 'customize-sidebar-layout-and-tools',
    title: 'Customize the sidebar',
  },
  {
    body: 'The Resources menu can sleep inactive terminal sessions while keeping them restorable in the sidebar.',
    id: 'sleep-idle-sessions-from-resources',
    title: 'Sleep idle sessions from Resources',
  },
  {
    body: 'Click Add Worktree on a project header so a second agent can work on a branch without touching the main checkout.',
    id: 'run-same-project-in-a-worktree',
    title: 'Run the same project in a worktree',
  },
  {
    body: 'Configure Ghostex Computer Use in Settings, then ask agents to use /ghostex-computer-use for native macOS app control.',
    id: 'use-ghostex-computer-use-skill',
    title: 'Use /ghostex-computer-use for desktop control',
  },
  {
    body: 'Configure Ghostex Browser Use in Settings, then ask agents to use /ghostex-browser-use for page inspection, console logs, screenshots, and clicks.',
    id: 'use-ghostex-browser-use-skill',
    title: 'Use /ghostex-browser-use for browser panes',
  },
  {
    body: 'Configure Ghostex Embedded Browser Use in Settings, then ask agents to use /ghostex-embedded-browser-use for page inspection, console logs, screenshots, and clicks in Ghostex panes.',
    id: 'use-ghostex-embedded-browser-use-skill',
    title: 'Use /ghostex-embedded-browser-use for Ghostex panes',
  },
  {
    body: 'Open the Automate tab to run agents on a schedule without sitting in the session.',
    id: 'schedule-recurring-agent-work',
    title: 'Schedule recurring agent work',
  },
  {
    body: 'Open More Options in the top right of the sidebar, click "Mobile", then attach the Mobile app to a running agent session.',
    id: 'continue-session-from-mobile-app',
    title: 'Continue a session from the Mobile app',
  },
  {
    body: 'Open More Options in the top right of the sidebar, click "Search by Prompt", then type any words you remember from the prompt.',
    id: 'find-session-by-prompt-text',
    title: 'Find any session from prompt text',
  },
  {
    body: 'In Search by Prompt, favorite a prompt so it stays at the top the next time you search.',
    id: 'star-prompts-you-want-again',
    title: 'Star prompts you want again',
  },
  {
    body: 'Then you can easily ask agents to "work on beads with high priority from the kanban board"',
    id: 'add-todos-to-kanban-page',
    title: 'Add all your Todos in the Kanban page',
  },
  {
    body: 'Quit and relaunch from the control panel to see how persisted onboarding flags change what the next launch shows.',
    id: 'sandbox-relaunch-to-compare-runs',
    title: 'Relaunch to compare onboarding runs',
  },
];
