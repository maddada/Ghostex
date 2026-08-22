/*
 * Visual re-mock of the tips catalog. Titles/bodies are copied verbatim from
 * TITLEBAR_TIPS in native/sidebar/titlebar-host.tsx so the sandbox panel reads
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
    body: "Search for project actions, pane splits and moves, session controls, settings shortcuts, and other Ghostex actions.",
    id: "command-palette-all-actions",
    title: "Press Cmd Shift P anywhere to open Ghostex Quick Access",
  },
  {
    body: "Open Settings to customize sidebar presets, visible details, agents, actions, project tools, and workspace open targets.",
    id: "customize-sidebar-layout-and-tools",
    title: "Customize the sidebar",
  },
  {
    body: "The Resources menu can sleep inactive terminal sessions while keeping them restorable in the sidebar.",
    id: "sleep-idle-sessions-from-resources",
    title: "Sleep idle sessions from Resources",
  },
  {
    body: "Use browser panes beside agents when the task needs screenshots, DOM inspection, or logged-in product state.",
    id: "attach-browser-pane-to-task",
    title: "Attach a browser pane to a task",
  },
  {
    body: "Configure Ghostex Computer Use in Settings, then ask agents to use /ghostex-computer-use for native macOS app control.",
    id: "use-ghostex-computer-use-skill",
    title: "Use /ghostex-computer-use for desktop control",
  },
  {
    body: "Configure Ghostex Browser Use in Settings, then ask agents to use /ghostex-browser-use for page inspection, console logs, screenshots, and clicks.",
    id: "use-ghostex-browser-use-skill",
    title: "Use /ghostex-browser-use for browser panes",
  },
  {
    body: "Configure Ghostex Auto Rename Session in Settings, then ask agents to use $ghostex-auto-rename-session to auto rename the current session from the work they just did.",
    id: "use-ghostex-auto-rename-session-skill",
    title: "Use $ghostex-auto-rename-session to auto rename sessions",
  },
  {
    body: "Install Faster Chrome DevTools Skill when agents need fast CLI-backed access to your own Chrome profile, tabs, cookies, and extensions.",
    id: "recommend-faster-chrome-devtools-skill",
    title: "Give agents fast access to your personal Chrome",
  },
  {
    body: 'Open the sidebar Search row, click "Search by Text", then type any words you remember from the prompt.',
    id: "find-session-by-prompt-text",
    title: "Find any session from prompt text",
  },
  {
    body: "Pin a session in the sidebar when you need it to stay at the top.",
    id: "pin-important-workspaces",
    title: "Pin important sessions",
  },
  {
    body: 'Then you can easily ask agents to "work on beads with high priority from the kanban board"',
    id: "add-todos-to-kanban-page",
    title: "Add all your Todos in the Kanban page",
  },
  {
    body: "Quit and relaunch from the control panel to see how persisted onboarding flags change what the next launch shows.",
    id: "sandbox-relaunch-to-compare-runs",
    title: "Relaunch to compare onboarding runs",
  },
];
