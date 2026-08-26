import {
  IconArrowRight,
  IconApps,
  IconArrowLeft,
  IconBellRinging,
  IconBolt,
  IconBrowser,
  IconBrandAndroid,
  IconBrandOpenai,
  IconCircleCheck,
  IconCircleCheckFilled,
  IconCode,
  IconDeviceMobile,
  IconDeviceFloppy,
  IconDownload,
  IconExternalLink,
  IconFileText,
  IconFolders,
  IconHistory,
  IconInfoCircle,
  IconLayoutDashboard,
  IconLayoutKanban,
  IconMessageCircle,
  IconMoon,
  IconPencil,
  IconPlayerPlay,
  IconRefresh,
  IconSettings,
  IconSparkles,
  IconStack,
  IconTerminal2,
  IconTools,
  IconWorld,
} from '@tabler/icons-react';
import { useEffect, useId, useRef, useState, type ComponentType } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/packages/components/ui/dialog';
import { Switch } from '@/packages/components/ui/switch';
import { cn } from '@/packages/components/utils';
import type { FirstLaunchSetupMainSettingKey } from '../shared/first-launch-setup-settings';
import type { SidebarTheme } from '../shared/session-grid-contract';
import type {
  SidebarAgentHookStatusItem,
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
} from '../shared/session-grid-contract';
import {
  DEFAULT_ghostex_SETTINGS,
  SESSION_TITLE_GENERATION_AGENT_OPTIONS,
  SIDEBAR_SETTINGS_PRESETS,
  applySidebarSettingsPreset,
  getSessionTitleGenerationCommandPreview,
  getSidebarSettingsPresetId,
  normalizeghostexSettings,
  type PreferredAgentInterface,
  type SessionTitleGenerationAgent,
  type SidebarSettingsPresetId,
  type ghostexSettings,
} from '../shared/ghostex-settings';
import {
  GHOSTEX_TRYCUA_PRODUCT_NAME,
  VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS,
  type BundledGhostexAgentSkill,
  type BundledGhostexAgentSkillId,
} from '../shared/ghostex-agent-skills';
import { DEFAULT_SIDEBAR_AGENTS } from '../shared/sidebar-agents';
import { getBrandAgentLogoStyle } from './agent-logos';
import type { WebviewApi } from './webview-api';

export type FirstLaunchSetupPage =
  | 'video'
  | 'welcome'
  | 'plugins'
  | 'agents'
  | 'preferences'
  | 'hooks'
  | 'cli'
  | 'skills'
  | 'ready'
  | 'project'
  | 'browserControl'
  | 'desktopCua'
  | 'agentsSessions'
  | 'remoteAccess';

export type FirstLaunchSetupModalProps = {
  agentHookStatus?: SidebarAgentHookStatusMessage;
  agentHookStatusLoading?: boolean;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading?: boolean;
  initialPage?: FirstLaunchSetupPage;
  isOpen: boolean;
  onClose: () => void;
  onChange: (settings: ghostexSettings) => void;
  onInstallAgentHooks?: (agentIds?: readonly string[]) => void;
  onInstallCliSkill?: () => void;
  onInstallBrowserControl?: () => void;
  onInstallBrowserUseSkill?: () => void;
  onInstallComputerUseSkill?: () => void;
  onInstallCuaDriver?: () => void;
  onInstallFable56OrchestrationSkill?: () => void;
  onInstallManageBeadsSkill?: () => void;
  onInstallGenerateTitleSkill?: () => void;
  onInstallGhostexCli?: () => void;
  onInstallMoveCodexSessionSkill?: () => void;
  onUninstallBundledAgentSkill?: (skillId: BundledGhostexAgentSkillId) => void;
  onOpenAccessibilityPreferences?: () => void;
  onOpenScreenRecordingPreferences?: () => void;
  /** Opens the native folder dialog; the picked path returns as a `firstLaunchProjectFolderPicked` host message. */
  onPickProjectFolder?: () => void;
  /** Registers the chosen folder as a project and starts the first session in it. `agentId` is a sidebar agent id or `'terminal'`. */
  onFinishFirstLaunch?: (options: { agentId: string; path: string }) => Promise<void> | void;
  onRequestAgentHookStatus?: (agentIds?: readonly string[]) => void;
  onRequestGhostexCliStatus?: () => void;
  settings?: ghostexSettings;
  theme?: SidebarTheme;
  vscode?: WebviewApi;
};

type FirstLaunchGuideAction = {
  description: string;
  eyebrow: string;
  examplesAtBottom?: boolean;
  snippet?: string[];
  subtitle?: string;
};

type FirstLaunchGuideItem = {
  icon: ComponentType<{ className?: string; size?: number; stroke?: number }>;
  text: string;
  title: string;
};

type FirstLaunchMobileBenefit = {
  icon: ComponentType<{ className?: string; size?: number; stroke?: number }>;
  text: string;
  title: string;
};

type FirstLaunchGuidePage = {
  action?: FirstLaunchGuideAction;
  icon: ComponentType<{ className?: string; size?: number; stroke?: number }>;
  imageAlt?: string;
  imageSrc?: string;
  items: FirstLaunchGuideItem[];
  kicker?: string;
  page: FirstLaunchSetupPage;
  title: string;
};

const FIRST_LAUNCH_HOOK_SUPPORTED_AGENTS = DEFAULT_SIDEBAR_AGENTS;
const FIRST_LAUNCH_PROMPT_AGENT_OPTIONS = DEFAULT_SIDEBAR_AGENTS.filter(
  (agent) => !('hiddenByDefault' in agent) || agent.hiddenByDefault !== true
).map((agent) => ({ label: agent.name, value: agent.agentId }));
const FIRST_LAUNCH_SIDEBAR_PRESET_ORDER: readonly SidebarSettingsPresetId[] = [
  'recommended',
  'minimal',
  'codex',
  'detailed',
];
const FIRST_LAUNCH_SIDEBAR_PRESETS = FIRST_LAUNCH_SIDEBAR_PRESET_ORDER.flatMap((presetId) => {
  const preset = SIDEBAR_SETTINGS_PRESETS.find((candidate) => candidate.id === presetId);
  return preset ? [preset] : [];
});

/*
 * CDXC:FirstLaunchSetup 2026-05-31-07:15:
 * ZMU-72: The mobile download button must match README.md's stable React Native
 * Android release URL without per-version README edits.
 *
 * CDXC:FirstLaunchSetup 2026-06-16-01:04:
 * Android download buttons must use GitHub's latest-release asset redirect so the first-launch setup never points at an older tagged APK after a new stable release ships.
 */
const FIRST_LAUNCH_ANDROID_APK_URL = 'https://github.com/maddada/Ghostex/releases/latest/download/ghostex-android.apk';
const FIRST_LAUNCH_DISCORD_URL = 'https://discord.gg/df7b3G92CS';
const FIRST_LAUNCH_TUTORIAL_VIDEO_WATCH_URL = 'https://www.youtube.com/watch?v=APdP-j5n4Mw';
const FIRST_LAUNCH_RELEASES_URL = 'https://github.com/maddada/ghostex/releases';

const FIRST_LAUNCH_CLI_MOBILE_BENEFITS: readonly FirstLaunchMobileBenefit[] = [
  {
    icon: IconDeviceMobile,
    text: 'Open the same agent sessions from the React Native Android app when you are away from the Mac.',
    title: 'Live Remote Sessions',
  },
  {
    icon: IconTerminal2,
    text: 'The Android app calls ghostex over SSH, so mobile actions attach to the right session.',
    title: 'CLI Bridge',
  },
  {
    icon: IconInfoCircle,
    text: "Agents can add ghostex browser mcp to inspect Browser's console logs, snapshots, screenshots, clicks, fills, and key presses.",
    title: 'Ghostex Embedded Browser Use',
  },
];
/*
 * CDXC:FirstLaunchSetup 2026-08-24:
 * The 2026-08-24 onboarding redesign replaced the video/announcement pages with
 * a six-step flow: Welcome (use cases) -> Plugins (title-bar view toggles) ->
 * Agents (CLI scan + install guides) -> Connect (hooks, only when an agent CLI
 * exists) -> Skills (checkbox picks) -> Get started (first project + default
 * agent + default view + first session). Dormant page components stay in this
 * file so they can be restored without losing the previous setup content.
 */
const FIRST_LAUNCH_SETUP_PAGES: readonly FirstLaunchSetupPage[] = [
  'welcome',
  'plugins',
  'agents',
  'hooks',
  'skills',
  'project',
];

const FIRST_LAUNCH_STEP_LABELS: Partial<Record<FirstLaunchSetupPage, string>> = {
  agents: 'Agents',
  hooks: 'Connect',
  plugins: 'Plugins',
  project: 'Get started',
  skills: 'Skills',
  welcome: 'Welcome',
};

function getFirstLaunchSetupPages(hasInstalledAgentClis: boolean): readonly FirstLaunchSetupPage[] {
  // The Connect (hooks) step can only connect agents that exist on the machine,
  // so it is skipped entirely until the scan finds at least one agent CLI.
  return hasInstalledAgentClis ? FIRST_LAUNCH_SETUP_PAGES : FIRST_LAUNCH_SETUP_PAGES.filter((page) => page !== 'hooks');
}

type FirstLaunchUseCase = { number: string; text: string; title: string; wide?: boolean };

const FIRST_LAUNCH_USE_CASES: readonly FirstLaunchUseCase[] = [
  {
    number: '01',
    text: 'Claude Code on a refactor, Codex on tests, OpenCode on docs. Each in its own session, not a pile of terminal tabs.',
    title: 'Run several agents at once',
  },
  {
    number: '02',
    text: 'Open the built-in browser and editor next to the agent. Inspect the UI, read the diff, send comments back.',
    title: 'Review work, not just the chat',
  },
  {
    number: '03',
    text: 'Check status, send a follow-up, or resume a session from your phone when you step away.',
    title: 'Keep agents moving on mobile',
  },
  {
    number: '04',
    text: 'Let Claude launch Codex, or drop tasks on the Kanban board so an orchestrator delegates them.',
    title: 'Hand work between models',
  },
  {
    number: '05',
    text: 'Sessions persist and name themselves. Search old prompts and resume any conversation.',
    title: 'Pick up where you left off',
  },
  {
    number: '06',
    text: 'Attach over SSH. Heavy agent work stays on the host; you stay connected.',
    title: 'Use a remote box as local',
  },
  {
    number: '07',
    text: 'Supported agents get a real chat view of the session, so you can read and reply like a conversation. Switch between chat and terminal with one click.',
    title: 'Chat view for your agents',
  },
  {
    number: '08',
    text: 'Drive terminal CLIs from a chat interface so you can preview images and edit text more easily than in a raw terminal.',
    title: 'Use terminal CLIs from chat',
  },
  {
    number: '09',
    text: 'Fork sessions, stash prompts, write notes or tags, park or pin sessions, and much more.',
    title: 'Advanced Chat Controls',
  },
];

type FirstLaunchPluginKey = 'browser' | 'docs' | 'code' | 'kanban' | 'automate';

const FIRST_LAUNCH_PLUGIN_ROWS: readonly {
  description: string;
  icon: ComponentType<{ className?: string; size?: number; stroke?: number }>;
  key: FirstLaunchPluginKey;
  title: string;
}[] = [
  {
    description: 'Built-in Chromium panes so you can preview the app your agent is building.',
    icon: IconWorld,
    key: 'browser',
    title: 'Browser',
  },
  {
    description: 'Annotate and edit markdown reports or plans, HTML mockups, and diagrams right next to your agents.',
    icon: IconFileText,
    key: 'docs',
    title: 'Docs',
  },
  {
    description: 'A VS Code-style editor next to your agent, for reviewing diffs and editing files.',
    icon: IconCode,
    key: 'code',
    title: 'Code',
  },
  {
    description: 'A project board where tickets can be handed straight to agents.',
    icon: IconLayoutKanban,
    key: 'kanban',
    title: 'Kanban',
  },
  {
    description:
      'Let agents schedule repeated or one-time work. Generate reports, check statuses, run nightly test sweeps.',
    icon: IconBolt,
    key: 'automate',
    title: 'Automate',
  },
];

const FIRST_LAUNCH_PLUGIN_HIDDEN_SETTING_KEYS = {
  automate: 'automateViewTabHidden',
  browser: 'browserViewTabHidden',
  code: 'codeViewTabHidden',
  docs: 'docsViewTabHidden',
  kanban: 'kanbanViewTabHidden',
} as const satisfies Record<FirstLaunchPluginKey, keyof ghostexSettings>;

/** Browser and Docs start on; Code, Kanban, and Automate wait until the user wants them. */
const FIRST_LAUNCH_RECOMMENDED_PLUGINS_VISIBLE: Record<FirstLaunchPluginKey, boolean> = {
  automate: false,
  browser: true,
  code: false,
  docs: true,
  kanban: false,
};

/** Friendlier install-card names for agents whose registry name is terse. */
const FIRST_LAUNCH_AGENT_DISPLAY_NAMES: Readonly<Record<string, string>> = {
  claude: 'Claude Code',
  codex: 'Codex CLI',
};

const FIRST_LAUNCH_AGENT_INSTALL_GUIDE_URLS: Readonly<Record<string, string>> = {
  claude: 'https://docs.anthropic.com/en/docs/claude-code/setup',
  codex: 'https://developers.openai.com/codex/cli',
  copilot: 'https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli',
  gemini: 'https://github.com/google-gemini/gemini-cli#installation',
};

const FIRST_LAUNCH_RECOMMENDED_INSTALL_AGENTS: readonly { agentId: string; subtitle: string }[] = [
  { agentId: 'claude', subtitle: "Anthropic's coding agent. Uses your Claude subscription." },
  { agentId: 'codex', subtitle: "OpenAI's coding agent. Uses your ChatGPT subscription." },
];

const FIRST_LAUNCH_CONNECT_BENEFITS: readonly {
  icon: ComponentType<{ className?: string; size?: number; stroke?: number }>;
  text: string;
  title: string;
}[] = [
  {
    icon: IconCircleCheck,
    text: 'Sessions show "Working" or "Waiting for you" so you never dig through terminals to find the stuck one.',
    title: 'Know who needs you',
  },
  {
    icon: IconPencil,
    text: 'Sessions name themselves from your first message: "Fix login bug", not "zsh (3)".',
    title: 'Names, not noise',
  },
  {
    icon: IconBellRinging,
    text: 'Ghostex can notify you when an agent finishes or asks a question, even on your phone.',
    title: 'Get pinged',
  },
];

const FIRST_LAUNCH_RECOMMENDED_SKILL_IDS: readonly BundledGhostexAgentSkillId[] =
  VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS.filter((skill) => skill.tier === 'recommended').map((skill) => skill.id);

const FIRST_LAUNCH_CHAT_SUPPORT_NOTE =
  'Chat is available for Claude, Codex, Pi, OMP, and Grok for now. You can flip any session between chat and terminal with one click.';

const FIRST_LAUNCH_GUIDE_PAGES: readonly FirstLaunchGuidePage[] = [
  {
    action: {
      description:
        'Ghostex installs the ghostex CLI with the app and can install a local $ghostex-embedded-browser-use skill that teaches agents how to attach to embedded CEF panes. Run ghostex browser install-skill or use the button below. After installation, agents add ghostex browser mcp to their MCP config so Ghostex can list pages, read console logs, take snapshots, click, fill, and capture screenshots while Ghostex is running.',
      eyebrow: 'Embedded Browser Use Installation Guide',
      examplesAtBottom: true,
      snippet: [
        'ghostex browser --help',
        'gx browser --help',
        'ghostex browser mcp',
        'ghostex browser open https://example.com',
      ],
      subtitle:
        'Make sure Ghostex Embedded Browser Use is installed on your Mac. If it is not ready yet, install the skill below.',
    },
    icon: IconBrowser,
    items: [
      {
        icon: IconBrowser,
        text: 'Run ghostex browser --help or gx browser --help to see the Ghostex Embedded Browser Use commands and MCP setup.',
        title: 'CLI help',
      },
      {
        icon: IconTerminal2,
        text: 'Use ghostex browser mcp when an agent needs DevTools access to Ghostex browser panes.',
        title: 'MCP attach',
      },
      {
        icon: IconInfoCircle,
        text: 'Ghostex Embedded Browser Use exposes page listing, target selection, navigation, console logs, snapshots, click/fill, key presses, evaluation, and screenshots.',
        title: 'Agent capabilities',
      },
      {
        icon: IconTools,
        text: 'The recommended debugging loop is: list pages, select the right page, read console logs, take a snapshot, interact with element refs, then capture a screenshot for proof.',
        title: 'Debugging loop',
      },
    ],
    kicker: 'Ghostex Embedded Browser Use',
    page: 'browserControl',
    title: 'Set up Ghostex Embedded Browser Use',
  },
  {
    action: {
      description:
        'Install Desktop Control so agents can operate native macOS apps through Ghostex Computer Use. Ghostex handles the installer; macOS may still ask you to grant permissions.',
      eyebrow: 'One-click setup',
    },
    icon: IconTools,
    items: [
      {
        icon: IconDownload,
        text: 'The installer adds Cua Driver and installs the $ghostex-computer-use wrapper skill agents use for desktop control.',
        title: 'One-click installer',
      },
      {
        icon: IconSettings,
        text: 'Grant Accessibility and Screen Recording when macOS asks; those permissions let the driver see and control desktop apps.',
        title: 'macOS permissions',
      },
      {
        icon: IconInfoCircle,
        text: 'You can skip this now. Desktop Control will not work until Cua Driver, the Ghostex Computer Use skill, and the macOS permissions are ready.',
        title: 'Optional for now',
      },
      {
        icon: IconBrowser,
        text: 'Use Ghostex Computer Use for native apps, Ghostex Browser Use for supported external browser pages, and Ghostex Embedded Browser Use for browser panes inside Ghostex.',
        title: 'Browser vs desktop',
      },
    ],
    kicker: 'Ghostex Computer Use',
    page: 'desktopCua',
    title: 'Set up Ghostex Computer Use',
  },
  {
    icon: IconApps,
    items: [
      {
        icon: IconTerminal2,
        text: 'Manage multiple CLI coding agent sessions from one native macOS workspace.',
        title: 'Parallel agents',
      },
      {
        icon: IconStack,
        text: 'Keep agents, browser pages, terminal work, prompts, and Git flow visible together.',
        title: 'Unified workspace',
      },
      {
        icon: IconFolders,
        text: 'Jump between sessions, project groups, and worktrees without losing the current context.',
        title: 'Project context',
      },
      {
        icon: IconSparkles,
        text: 'Use Ghostex as the always-on workspace for parallel agent work, not just a terminal list.',
        title: 'Always-on ADE',
      },
      {
        icon: IconHistory,
        text: 'Review setup tasks later from Settings > Integrations.',
        title: 'Review setup',
      },
      {
        icon: IconSettings,
        text: 'Add custom CLI agents from Settings, then launch them from the sidebar.',
        title: 'Custom agents',
      },
      {
        icon: IconMoon,
        text: 'Sleep sessions to keep them in the sidebar without keeping every terminal fully active.',
        title: 'Sleep sessions',
      },
      {
        icon: IconPencil,
        text: 'Paste long text into rename and Ghostex will turn it into a cleaner session name.',
        title: 'Smart rename',
      },
      {
        icon: IconDeviceFloppy,
        text: 'Agent hooks capture the native session id that Claude, Codex, Grok, OpenCode, Pi, Amp, Cursor CLI, Gemini, Antigravity, Rovo Dev, Hermes Agent, Copilot, CodeBuddy, Factory, and Qoder need for exact resume.',
        title: 'Native session ids',
      },
      {
        icon: IconSettings,
        text: 'Ghostex installs hooks into the agent config files it can find after the agent CLI exists on your PATH.',
        title: 'Automatic install',
      },
      {
        icon: IconTerminal2,
        text: 'Start agent sessions from Ghostex terminals so the hooks can attach the captured id to the correct session card.',
        title: 'Launch from Ghostex',
      },
      {
        icon: IconHistory,
        text: 'If an id was not captured, Ghostex still falls back to the existing title-based resume flow.',
        title: 'Title fallback',
      },
    ],
    page: 'agentsSessions',
    title: 'Agents & Sessions',
  },
  {
    action: {
      description:
        'After you SSH into the Mac that is running Ghostex, list sessions and attach by the alias shown in the table.',
      eyebrow: 'Remote session commands',
      snippet: [
        '# For CLI debugging, connect to your Mac over Tailscale',
        'ssh madda@my-mac',
        '',
        '# List Ghostex sessions and note the left-column alias',
        'gx sessions',
        '',
        '# Attach to session 1',
        'gx a 1',
        '',
        '# Wake, focus, or sleep sessions from the phone',
        'gx wake 1',
        'gx focus 1',
        'gx sleep 1',
        '',
        '# Use a title when the alias is not handy',
        'gx a "project:session title"',
      ],
    },
    icon: IconWorld,
    items: [
      {
        icon: IconWorld,
        text: 'Install Tailscale on the Mac and phone, sign into the same tailnet, then enable SSH into the Mac.',
        title: 'Tailscale SSH',
      },
      {
        icon: IconSettings,
        text: 'In Ghostex Settings, enable Session Persistence and choose zmx for the smoothest remote attach flow.',
        title: 'Session persistence',
      },
      {
        icon: IconTerminal2,
        text: "Install the Ghostex React Native APK, add the Mac's Tailscale name or IP, and connect with your SSH credentials.",
        title: 'Ghostex Android',
      },
      {
        icon: IconMoon,
        text: 'Keep the Mac awake while remote so your phone can reach it through Tailscale.',
        title: 'Keep Mac awake',
      },
      {
        icon: IconStack,
        text: 'Keep Ghostex open on the Mac so gx can list live sessions; zmx, tmux, or zellij keeps the terminal session itself durable.',
        title: 'Live session list',
      },
    ],
    kicker: 'Remote Access',
    page: 'remoteAccess',
    title: 'Connecting to Any Terminal Session Remotely',
  },
];
const FIRST_LAUNCH_GUIDE_PAGE_BY_ID = new Map(FIRST_LAUNCH_GUIDE_PAGES.map((page) => [page.page, page]));

function getVisibleFirstLaunchSetupPage(
  page: FirstLaunchSetupPage,
  pages: readonly FirstLaunchSetupPage[]
): FirstLaunchSetupPage {
  // No requested page (or a dormant one) starts the sequence at its first page.
  return pages.includes(page) ? page : pages[0];
}

/**
 * CDXC:FirstLaunchSetup 2026-05-26-06:23:
 * First launch setup is the production onboarding flow, and Storybook must
 * mount this same component with mocked native calls instead of maintaining a
 * separate prototype. The first page introduces Ghostex, uses generated
 * product artwork, and asks for agent hooks because those hooks power desktop
 * notifications for In Progress / Needs Attention states and automatic
 * first-message session titles.
 *
 * CDXC:FirstLaunchSetup 2026-05-26-07:14:
 * The intro page should read as an app setup screen, not a marketing landing
 * page. Use a two-column body with intro copy and benefits on the left and the
 * README-derived workspace screenshot on the right, then a bordered hook setup
 * panel below so install actions and agent readiness stay prominent without a
 * full-width tinted band or scattered chips.
 *
 * CDXC:FirstLaunchSetup 2026-05-26-07:22:
 * Hook setup actions belong inside the bordered agent-status panel so the
 * install action is visually tied to the exact agent cards it updates. Do not
 * show a separate readiness summary line; grouped agent headers already expose
 * the counts, and refresh should be an icon-only control.
 *
 * CDXC:FirstLaunchSetup 2026-05-26-07:27:
 * Remove the repeated Recommended Setup copy from the intro page and consolidate
 * Refresh, Install Hooks, Skip, and Continue in one footer action row. The agent
 * card panel should only show installation state while the footer owns decisions.
 *
 * CDXC:FirstLaunchSetup 2026-05-26-07:43:
 * The first page should open directly on the product promise without a redundant
 * "First launch" eyebrow below the modal title. The headline should frame setup
 * as integrating Ghostex with the user's agents.
 *
 * CDXC:FirstLaunchSetup 2026-05-26-07:46:
 * The intro description should make hook installation the immediate setup task
 * and introduce the feature list as the reason those hooks are required.
 *
 * CDXC:FirstLaunchSetup 2026-05-26-07:48:
 * The product preview image should align with the feature list, not the headline,
 * so the intro copy reads as one full-width setup prompt above the visual row.
 *
 * CDXC:FirstLaunchSetup 2026-05-26-15:53:
 * The second first-launch page explains Ghostex CLI setup for the React Native
 * Android client and links to its stable GitHub Releases APK.
 *
 * CDXC:FirstLaunchSetup 2026-05-26-17:12:
 * The CLI page renders native command status and describes `gx` as usable only
 * when Ghostex owns that alias.
 *
 * CDXC:CliInstall 2026-06-07-13:53:
 * The first-launch CLI page must not show Install CLI or Refresh controls.
 * Production startup auto-links the app-bundled CLI, so onboarding should teach
 * why ghostex/gx are useful instead of making CLI setup look manual.
 *
 * CDXC:BrowserAgentControl 2026-05-26-22:17:
 * The bundled skills page should expose the Ghostex Browser Use skill after CLI
 * setup, because agents need local instructions for `ghostex browser mcp`, CEF
 * control, console logs, snapshots, screenshots, and form interactions.
 *
 * CDXC:BrowserAgentControl 2026-05-27-01:59:
 * Browser control setup should teach the `ghostex browser ...` namespace
 * because "browser" is now the durable CLI keyword for agent-facing embedded
 * CEF control. The explicit skill install button therefore uses
 * `ghostex browser install-skill` instead of the older top-level alias.
 *
 * CDXC:FirstLaunchSetup 2026-05-27-02:41:
 * Tips & Tricks is no longer a separate modal. The first-launch modal is the
 * single app-teaching surface, so it includes every guide page from the old
 * Tips & Tricks flow after the required hooks and CLI/browser setup pages.
 *
 * CDXC:FirstLaunchSetup 2026-05-27-03:30:
 * The third page teaches Ghostex Browser Use as the agent-facing entry point for
 * embedded CEF panes. The fourth page teaches Cua Driver separately for native
 * desktop app control without exposing a scary shell-first setup.
 *
 * CDXC:IntegrationsSetup 2026-05-27-04:17:
 * Ghostex Browser Use, hooks, and Ghostex Computer Use are optional onboarding
 * integrations. If an integration is missing, Continue must show a warning
 * first and only advance after the user confirms they want to proceed without
 * it. Partial hook installs are acceptable; only zero installed hooks trigger
 * the hook warning. The Ghostex CLI itself is app-installed and should not be a
 * first-launch warning branch.
 *
 * CDXC:ComputerAgentControl 2026-05-27-06:58:
 * Desktop Control setup must install Cua Driver and the `$ghostex-computer-use`
 * wrapper skill. Treat Desktop Control as incomplete until both are present, so
 * users do not finish onboarding with native-app automation installed but
 * undiscoverable by agents.
 *
 * CDXC:SkillConsolidation 2026-08-24:
 * The bundled skills page installs `$ghostex-cli`, the help-first entry-point
 * skill. The old agent-orchestration, manage-automations, and find-prev-session
 * skills were folded into `ghostex --help`, so agents learn pane/session
 * commands, automations, and prompt history through CLI discovery instead of
 * per-domain skills.
 *
 * CDXC:GenerateTitleSkill 2026-05-27-07:28:
 * The bundled skills page installs `$ghostex-auto-rename-session` so every Ghostex
 * agent session can generate a title under 47 characters and submit
 * `/rename <title>` in its own prompt.
 *
 * CDXC:GenerateTitleSkill 2026-06-09-17:49:
 * Generated title skills should submit the staged rename through Ghostex's
 * native Enter bridge so macOS matches the Delayed Send key path.
 *
 * CDXC:CodexSessionMove 2026-06-26-13:24:
 * The first-launch bundled-skills page should install `$ghostex-move-codex-session`
 * with the other app-shipped skills so agents can explain the fork-into-folder
 * Codex workflow without users manually adding a local skill.
 *
 * CDXC:FirstLaunchWelcome 2026-05-27-05:04:
 * First launch should start with a candid product welcome before setup tasks.
 * The page explains Ghostex as an intuitive Agent Development Environment that
 * combines Ghostty-backed terminals with Codex-app UX, states why native
 * terminals and Chromium increase complexity, and invites users to Discord for
 * support, questions, and contributions.
 *
 * CDXC:FirstLaunchWelcome 2026-05-27-05:39:
 * The welcome page is now the first setup page, so the footer must not show a
 * self-targeting Back button or Skip button until the user advances into actual
 * setup tasks.
 *
 * CDXC:FirstLaunchWelcome 2026-05-27-05:55:
 * The first page's Discord CTA should be centered and use the official SVGL
 * Discord mark inline rather than as a bundled image URL so the native app shell
 * cannot render it as an empty image placeholder. The CTA sits centered in the
 * remaining body space between the note card and footer.
 *
 * CDXC:FirstLaunchWelcome 2026-05-27-07:42:
 * The first-page headline should be short enough to stay on one line at modal
 * width: "Fast Ghostty terminals, Codex inspired Features!" Keep the explanatory
 * subtitle at 18px medium weight, and put each card icon beside its heading to
 * reduce vertical height.
 *
 * CDXC:FirstLaunchPreferences 2026-05-29-15:31:
 * First launch should include a compact defaults page for the highest-impact
 * settings before optional integrations. The page writes to the same persisted
 * settings model as Settings: sidebar preset in Recommended / Minimal / Codex /
 * Detailed order, default prompt agent, lid-close keep-awake, Accept All, macOS
 * attention notifications, and completion sound.
 *
 * CDXC:FirstLaunchPreferences 2026-05-31-07:10:
 * ZMU-71: "Keep awake when lid is closed" maps to keepAwakePreventLidSleep on
 * this defaults page and must stay listed in FIRST_LAUNCH_PREFERENCES_MAIN_SETTING_KEYS.
 *
 * CDXC:FirstLaunchSetup 2026-05-31-07:15:
 * ZMU-72: First-launch external links must use openExternalUrl from the native
 * sidebar host because webview anchor clicks do not open the browser. The CLI page
 * mobile benefit rows use title plus subtitle layout, the README stable Android APK
 * URL, and the Browser Use page keeps install guidance at the top
 * with command examples in a bottom Examples card.
 *
 * CDXC:AgentSkills 2026-05-31-09:18:
 * CLI setup no longer silently installs bundled skills. First launch includes a
 * dedicated skills page so users explicitly choose Browser Use, Computer Use,
 * Agent Orchestration, and Generate Title with a short explanation for each one.
 *
 * CDXC:FirstLaunchPreferences 2026-06-04-21:02:
 * New users need the same first-prompt title-generation agent choice available
 * in Settings before their first automatic title job runs. Keep the first-time
 * modal wired to the shared Settings fields so Codex, Cursor, Claude, Grok
 * Build, and Custom stay consistent across onboarding and Settings.
 *
 * CDXC:FirstLaunchPreferences 2026-06-13-03:28:
 * The first-launch sidebar-style row must put Recommended on the left and use
 * the shared default settings so new installs open with Recommended selected.
 * Keep Minimal, Codex, and Detailed after it for the existing density ramp.
 */
export function FirstLaunchSetupModal({
  agentHookStatus,
  agentHookStatusLoading = false,
  ghostexCliStatus,
  ghostexCliStatusLoading = false,
  initialPage = FIRST_LAUNCH_SETUP_PAGES[0],
  isOpen,
  onClose,
  onFinishFirstLaunch,
  onInstallAgentHooks,
  onInstallCliSkill,
  onInstallBrowserControl,
  onInstallBrowserUseSkill,
  onInstallComputerUseSkill,
  onInstallCuaDriver,
  onInstallFable56OrchestrationSkill,
  onOpenAccessibilityPreferences,
  onOpenScreenRecordingPreferences,
  onPickProjectFolder,
  onRequestAgentHookStatus,
  onRequestGhostexCliStatus,
  onChange,
  settings = DEFAULT_ghostex_SETTINGS,
  theme = 'dark-blue',
  vscode,
}: FirstLaunchSetupModalProps) {
  const hookStatusByAgentId = new Map(agentHookStatus?.agents.map((status) => [status.agentId, status]) ?? []);
  const installedCliAgents = FIRST_LAUNCH_HOOK_SUPPORTED_AGENTS.filter(
    (agent) => hookStatusByAgentId.get(agent.agentId)?.cliInstalled === true
  );
  const visiblePages = getFirstLaunchSetupPages(installedCliAgents.length > 0);

  const [requestedPage, setRequestedPage] = useState<FirstLaunchSetupPage>(initialPage);
  const activePage = getVisibleFirstLaunchSetupPage(requestedPage, visiblePages);
  /*
   * CDXC:FirstLaunchSetup 2026-08-24:
   * Settings writes from this modal must compose. The `settings` prop only
   * updates after a native round trip, so two quick writes built from the prop
   * would silently drop the first one. Every write goes through this ref so
   * each patch layers on top of the previous one.
   */
  const latestSettingsRef = useRef(settings);
  useEffect(() => {
    latestSettingsRef.current = settings;
  }, [settings]);
  const updateSettings = (patch: Partial<ghostexSettings>) => {
    const next = normalizeghostexSettings({ ...latestSettingsRef.current, ...patch });
    latestSettingsRef.current = next;
    onChange(next);
  };

  const [pluginsDraft, setPluginsDraft] = useState<Record<FirstLaunchPluginKey, boolean>>(() =>
    createFirstLaunchPluginsDraft(settings)
  );
  const commitPluginsDraft = (draft: Record<FirstLaunchPluginKey, boolean>) => {
    updateSettings({
      automateViewTabHidden: !draft.automate,
      browserViewTabHidden: !draft.browser,
      codeViewTabHidden: !draft.code,
      docsViewTabHidden: !draft.docs,
      kanbanViewTabHidden: !draft.kanban,
    });
  };

  const [selectedSkillIds, setSelectedSkillIds] = useState<ReadonlySet<BundledGhostexAgentSkillId>>(
    () => new Set(FIRST_LAUNCH_RECOMMENDED_SKILL_IDS)
  );
  const [projectFolder, setProjectFolder] = useState('');
  const [projectAgentChoice, setProjectAgentChoice] = useState<string>();
  const [finishError, setFinishError] = useState<string>();
  const [isFinishing, setIsFinishing] = useState(false);
  const projectAgentId = projectAgentChoice ?? installedCliAgents[0]?.agentId ?? 'terminal';

  useEffect(() => {
    if (!isOpen || agentHookStatus || agentHookStatusLoading) {
      return;
    }
    onRequestAgentHookStatus?.();
  }, [agentHookStatus, agentHookStatusLoading, isOpen, onRequestAgentHookStatus]);

  useEffect(() => {
    if (isOpen) {
      setRequestedPage(initialPage);
      setSelectedSkillIds(new Set(FIRST_LAUNCH_RECOMMENDED_SKILL_IDS));
      setProjectFolder('');
      setProjectAgentChoice(undefined);
      setFinishError(undefined);
      setIsFinishing(false);
      setPluginsDraft(createFirstLaunchPluginsDraft(latestSettingsRef.current));
    }
  }, [initialPage, isOpen]);

  const activePageIndex = Math.max(0, visiblePages.indexOf(activePage));
  const isLastPage = activePageIndex === visiblePages.length - 1;
  const previousPage = visiblePages[Math.max(0, activePageIndex - 1)];
  const nextPage = visiblePages[Math.min(visiblePages.length - 1, activePageIndex + 1)];

  const navigateToPage = (page: FirstLaunchSetupPage) => {
    // What the plugins page shows is what the user gets, so the draft commits
    // whenever navigation leaves that page.
    if (activePage === 'plugins' && page !== 'plugins') {
      commitPluginsDraft(pluginsDraft);
    }
    setRequestedPage(getVisibleFirstLaunchSetupPage(page, visiblePages));
  };

  const finishFirstLaunchSetup = async () => {
    if (activePage === 'plugins') {
      commitPluginsDraft(pluginsDraft);
    }
    const path = projectFolder.trim();
    if (!path || !onFinishFirstLaunch) {
      onClose();
      return;
    }
    setFinishError(undefined);
    setIsFinishing(true);
    try {
      await onFinishFirstLaunch({ agentId: projectAgentId, path });
      onClose();
    } catch (error) {
      setFinishError(error instanceof Error ? error.message : 'Ghostex could not open the selected project.');
      setIsFinishing(false);
    }
  };

  const handleContinue = () => {
    if (isLastPage) {
      void finishFirstLaunchSetup();
      return;
    }
    navigateToPage(nextPage);
  };

  const hasProjectFolder = projectFolder.trim().length > 0;

  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onClose();
        }
      }}
      open={isOpen}
    >
      <DialogContent
        className={cn(
          'ghostex-settings-shadcn settings-modal-dialog first-launch-setup-modal-dialog flex flex-col gap-0 overflow-hidden p-0 font-sans',
          getSidebarThemeVariant(theme) === 'dark' && 'dark'
        )}
        data-sidebar-theme={theme}
      >
        <DialogHeader className='first-launch-setup-header'>
          <DialogTitle className='first-launch-setup-dialog-title'>Welcome to Ghostex</DialogTitle>
          <nav aria-label='Setup steps' className='first-launch-setup-stepper'>
            {visiblePages.map((page, index) => {
              const state = page === activePage ? 'active' : index < activePageIndex ? 'done' : 'todo';
              return (
                <button
                  className='first-launch-setup-step-chip'
                  data-state={state}
                  key={page}
                  onClick={() => navigateToPage(page)}
                  type='button'
                >
                  <span className='first-launch-setup-step-num'>{state === 'done' ? '✓' : index + 1}</span>
                  {FIRST_LAUNCH_STEP_LABELS[page] ?? page}
                </button>
              );
            })}
          </nav>
        </DialogHeader>

        <div className='first-launch-setup-body'>
          {activePage === 'welcome' ? (
            <FirstLaunchWelcomePage vscode={vscode} />
          ) : activePage === 'plugins' ? (
            <FirstLaunchPluginsPage
              draft={pluginsDraft}
              onToggle={(key, visible) => setPluginsDraft((draft) => ({ ...draft, [key]: visible }))}
            />
          ) : activePage === 'agents' ? (
            <FirstLaunchAgentsPage
              agentHookStatusLoading={agentHookStatusLoading}
              installedCliAgents={installedCliAgents}
              onRequestAgentHookStatus={onRequestAgentHookStatus}
              vscode={vscode}
            />
          ) : activePage === 'hooks' ? (
            <FirstLaunchConnectPage
              agentHookStatusLoading={agentHookStatusLoading}
              hookStatusByAgentId={hookStatusByAgentId}
              installedCliAgents={installedCliAgents}
              onInstallAgentHooks={onInstallAgentHooks}
              onSkip={() => navigateToPage(nextPage)}
            />
          ) : activePage === 'skills' ? (
            <FirstLaunchSkillsPage
              ghostexCliStatus={ghostexCliStatus}
              ghostexCliStatusLoading={ghostexCliStatusLoading}
              onInstallCuaDriver={onInstallCuaDriver}
              onInstallSkill={{
                cli: onInstallCliSkill,
                browserUse: onInstallBrowserUseSkill,
                computerUse: onInstallComputerUseSkill,
                embeddedBrowserUse: onInstallBrowserControl,
                fable56Orchestration: onInstallFable56OrchestrationSkill,
              }}
              onSkip={() => navigateToPage(nextPage)}
              onToggleSkill={(skillId, selected) =>
                setSelectedSkillIds((current) => {
                  const next = new Set(current);
                  if (selected) {
                    next.add(skillId);
                  } else {
                    next.delete(skillId);
                  }
                  return next;
                })
              }
              selectedSkillIds={selectedSkillIds}
            />
          ) : activePage === 'project' ? (
            <FirstLaunchProjectPage
              attentionNotificationsEnabled={settings.showMacOSAttentionNotifications}
              completionSoundEnabled={settings.completionBellEnabled}
              installedCliAgents={installedCliAgents}
              onChangePreferredInterface={(preferredAgentInterface) => updateSettings({ preferredAgentInterface })}
              onChangeProjectFolder={setProjectFolder}
              onPickProjectFolder={onPickProjectFolder}
              onSelectAgent={(agentId) => {
                setProjectAgentChoice(agentId);
                if (agentId !== 'terminal') {
                  updateSettings({ defaultPromptAgentId: agentId });
                }
              }}
              onToggleAttentionNotifications={(enabled) => updateSettings({ showMacOSAttentionNotifications: enabled })}
              onToggleCompletionSound={(enabled) => updateSettings({ completionBellEnabled: enabled })}
              preferredInterface={settings.preferredAgentInterface}
              projectAgentId={projectAgentId}
              projectFolder={projectFolder}
            />
          ) : activePage === 'preferences' ? (
            <FirstLaunchPreferencesPage onChange={onChange} settings={settings} />
          ) : activePage === 'cli' ? (
            <FirstLaunchCliPage
              ghostexCliStatus={ghostexCliStatus}
              ghostexCliStatusLoading={ghostexCliStatusLoading}
              vscode={vscode}
            />
          ) : (
            <FirstLaunchGuidePageView
              ghostexCliStatus={ghostexCliStatus}
              ghostexCliStatusLoading={ghostexCliStatusLoading}
              onInstallBrowserControl={onInstallBrowserControl}
              onInstallCuaDriver={onInstallCuaDriver}
              onOpenAccessibilityPreferences={onOpenAccessibilityPreferences}
              onOpenScreenRecordingPreferences={onOpenScreenRecordingPreferences}
              page={FIRST_LAUNCH_GUIDE_PAGE_BY_ID.get(activePage)}
            />
          )}
        </div>

        <div className='first-launch-setup-footer'>
          <span
            aria-live='polite'
            className={cn('first-launch-setup-footer-note', finishError && 'first-launch-setup-footer-error')}
            role={finishError ? 'alert' : undefined}
          >
            {finishError ?? 'You can re-run this tour anytime from Tips in the title bar.'}
          </span>
          <div className='first-launch-setup-footer-actions' role='group' aria-label='Setup actions'>
            {activePageIndex === 0 ? null : (
              <Button disabled={isFinishing} onClick={() => navigateToPage(previousPage)} type='button' variant='ghost'>
                <IconArrowLeft aria-hidden='true' data-icon='inline-start' />
                Back
              </Button>
            )}
            {activePage === 'welcome' ? (
              <Button onClick={() => openFirstLaunchExternalUrl(vscode, FIRST_LAUNCH_DISCORD_URL)} type='button'>
                <DiscordLogoIcon />
                Join our Discord!
              </Button>
            ) : null}
            {isLastPage ? (
              <Button disabled={isFinishing} onClick={handleContinue} type='button'>
                {isFinishing ? 'Opening Project…' : hasProjectFolder ? 'Finish and Open My Project' : 'Finish'}
                <IconArrowRight aria-hidden='true' data-icon='inline-end' />
              </Button>
            ) : (
              <Button onClick={handleContinue} type='button'>
                Continue
                <IconArrowRight aria-hidden='true' data-icon='inline-end' />
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function createFirstLaunchPluginsDraft(settings: ghostexSettings): Record<FirstLaunchPluginKey, boolean> {
  /*
   * CDXC:FirstLaunchSetup 2026-08-24:
   * A brand-new install has every view tab visible (all hidden flags false), so
   * the plugins step starts from the recommended set instead: Browser and Docs
   * on, Code, Kanban, and Automate off. An install where any tab was already
   * hidden has real user choices, and the step mirrors them instead.
   */
  const anyHidden = FIRST_LAUNCH_PLUGIN_ROWS.some((row) => {
    return settings[FIRST_LAUNCH_PLUGIN_HIDDEN_SETTING_KEYS[row.key]] === true;
  });
  if (!anyHidden) {
    return { ...FIRST_LAUNCH_RECOMMENDED_PLUGINS_VISIBLE };
  }
  return {
    automate: settings.automateViewTabHidden !== true,
    browser: settings.browserViewTabHidden !== true,
    code: settings.codeViewTabHidden !== true,
    docs: settings.docsViewTabHidden !== true,
    kanban: settings.kanbanViewTabHidden !== true,
  };
}

type FirstLaunchSidebarAgent = (typeof FIRST_LAUNCH_HOOK_SUPPORTED_AGENTS)[number];

function getFirstLaunchAgentDisplayName(agent: FirstLaunchSidebarAgent): string {
  return FIRST_LAUNCH_AGENT_DISPLAY_NAMES[agent.agentId] ?? agent.name;
}

function FirstLaunchAgentLogo({ agent }: { agent: FirstLaunchSidebarAgent }) {
  return <span aria-hidden='true' className='first-launch-onb-agent-logo' style={getBrandAgentLogoStyle(agent.icon)} />;
}

function FirstLaunchWelcomePage({ vscode }: { vscode?: WebviewApi }) {
  return (
    <section aria-labelledby='first-launch-welcome-title' className='first-launch-onb-page'>
      <h2 className='first-launch-onb-title' id='first-launch-welcome-title'>
        Your workspace for AI agents
      </h2>
      <p className='first-launch-onb-lede'>
        Ghostex is not a coding agent. It is the workspace you run your agents in, so more of them can work at once and
        you can actually keep up. Here is what people use it for:
      </p>
      <div className='first-launch-onb-grid3'>
        {FIRST_LAUNCH_USE_CASES.map((useCase) => (
          <article
            className={cn('first-launch-onb-card', useCase.wide && 'first-launch-onb-card-wide')}
            key={useCase.number}
          >
            <span className='first-launch-onb-card-number'>{useCase.number}</span>
            <h3>{useCase.title}</h3>
            <p>{useCase.text}</p>
          </article>
        ))}
      </div>
      <div className='first-launch-onb-row first-launch-onb-video-row'>
        <span className='first-launch-onb-video-thumb'>
          <IconPlayerPlay aria-hidden='true' size={18} />
        </span>
        <span className='first-launch-onb-row-main'>
          <strong>Prefer watching? Take the 6-minute Intro and Guide</strong>
          <span>A quick walkthrough of terminals, agents, and the workflows you just read about.</span>
        </span>
        <Button onClick={() => openFirstLaunchExternalUrl(vscode, FIRST_LAUNCH_TUTORIAL_VIDEO_WATCH_URL)} type='button'>
          Watch Intro and Guide
          <IconExternalLink aria-hidden='true' data-icon='inline-end' />
        </Button>
      </div>
    </section>
  );
}

function FirstLaunchPluginsPage({
  draft,
  onToggle,
}: {
  draft: Record<FirstLaunchPluginKey, boolean>;
  onToggle: (key: FirstLaunchPluginKey, visible: boolean) => void;
}) {
  return (
    <section aria-labelledby='first-launch-plugins-title' className='first-launch-onb-page'>
      <h2 className='first-launch-onb-title' id='first-launch-plugins-title'>
        Pick your plugins
      </h2>
      <p className='first-launch-onb-lede'>
        Ghostex has more than terminals. Turn on what you want in the title bar. You can change this anytime in
        Settings.
      </p>
      {FIRST_LAUNCH_PLUGIN_ROWS.map((row) => {
        const RowIcon = row.icon;
        const visible = draft[row.key];
        return (
          <div className='first-launch-onb-row' key={row.key}>
            <span className='first-launch-onb-row-icon'>
              <RowIcon aria-hidden='true' size={16} />
            </span>
            <span className='first-launch-onb-row-main'>
              <strong>{row.title}</strong>
              <span>{row.description}</span>
            </span>
            <Switch
              aria-label={`${row.title} plugin`}
              checked={visible}
              onCheckedChange={(checked) => onToggle(row.key, checked)}
            />
          </div>
        );
      })}
    </section>
  );
}

function FirstLaunchAgentsPage({
  agentHookStatusLoading,
  installedCliAgents,
  onRequestAgentHookStatus,
  vscode,
}: {
  agentHookStatusLoading: boolean;
  installedCliAgents: readonly FirstLaunchSidebarAgent[];
  onRequestAgentHookStatus?: (agentIds?: readonly string[]) => void;
  vscode?: WebviewApi;
}) {
  const hasAgents = installedCliAgents.length > 0;
  const installedAgentIds = new Set(installedCliAgents.map((agent) => agent.agentId));
  const notInstalledAgents = FIRST_LAUNCH_HOOK_SUPPORTED_AGENTS.filter(
    (agent) => !installedAgentIds.has(agent.agentId)
  );
  const notInstalledWithGuides = notInstalledAgents.filter(
    (agent) => FIRST_LAUNCH_AGENT_INSTALL_GUIDE_URLS[agent.agentId] !== undefined
  );
  const notInstalledWithoutGuides = notInstalledAgents.filter(
    (agent) => FIRST_LAUNCH_AGENT_INSTALL_GUIDE_URLS[agent.agentId] === undefined
  );

  return (
    <section aria-labelledby='first-launch-agents-title' className='first-launch-onb-page'>
      <h2 className='first-launch-onb-title' id='first-launch-agents-title'>
        Get an AI agent on your machine
      </h2>
      <p className='first-launch-onb-lede'>
        Ghostex drives command-line AI agents. We scanned your machine and{' '}
        <strong>
          {hasAgents
            ? `found ${installedCliAgents.length} installed agent ${installedCliAgents.length === 1 ? 'CLI' : 'CLIs'}.`
            : agentHookStatusLoading
              ? 'are still checking for installed agents.'
              : 'found no agents yet.'}
        </strong>{' '}
        {hasAgents
          ? 'You can start right away, or add more below.'
          : 'That is fine: pick one below, follow its install guide, then come back and re-scan.'}
      </p>

      {hasAgents ? (
        <>
          <div className='first-launch-onb-seclabel'>INSTALLED AGENT CLIS</div>
          {installedCliAgents.map((agent) => (
            <div className='first-launch-onb-row' key={agent.agentId}>
              <span className='first-launch-onb-row-icon'>
                <FirstLaunchAgentLogo agent={agent} />
              </span>
              <span className='first-launch-onb-row-main'>
                <strong>{getFirstLaunchAgentDisplayName(agent)}</strong>
                <span>Ready to use.</span>
              </span>
              <span className='first-launch-onb-pill' data-tone='ok'>
                Installed
              </span>
            </div>
          ))}
          <details className='first-launch-onb-flat-details'>
            <summary>Add more agents ({notInstalledAgents.length} supported, not installed)</summary>
            <div className='first-launch-onb-flat-details-body'>
              {notInstalledWithGuides.map((agent) => (
                <div className='first-launch-onb-row' key={agent.agentId}>
                  <span className='first-launch-onb-row-icon'>
                    <FirstLaunchAgentLogo agent={agent} />
                  </span>
                  <span className='first-launch-onb-row-main'>
                    <strong>{getFirstLaunchAgentDisplayName(agent)}</strong>
                  </span>
                  <Button
                    onClick={() =>
                      openFirstLaunchExternalUrl(vscode, FIRST_LAUNCH_AGENT_INSTALL_GUIDE_URLS[agent.agentId] ?? '')
                    }
                    type='button'
                    variant='outline'
                  >
                    Install Guide
                    <IconExternalLink aria-hidden='true' data-icon='inline-end' />
                  </Button>
                </div>
              ))}
              <div className='first-launch-onb-pill-wrap'>
                {notInstalledWithoutGuides.map((agent) => (
                  <span className='first-launch-onb-pill' data-tone='dim' key={agent.agentId}>
                    {agent.name}
                  </span>
                ))}
              </div>
            </div>
          </details>
        </>
      ) : (
        <>
          <div className='first-launch-onb-seclabel'>RECOMMENDED: INSTALL THE ONE YOU ALREADY SUBSCRIBE TO</div>
          <div className='first-launch-onb-grid2'>
            {FIRST_LAUNCH_RECOMMENDED_INSTALL_AGENTS.map(({ agentId, subtitle }) => {
              const agent = FIRST_LAUNCH_HOOK_SUPPORTED_AGENTS.find((candidate) => candidate.agentId === agentId);
              if (!agent) {
                return null;
              }
              return (
                <div className='first-launch-onb-row' key={agentId}>
                  <span className='first-launch-onb-row-icon'>
                    <FirstLaunchAgentLogo agent={agent} />
                  </span>
                  <span className='first-launch-onb-row-main'>
                    <strong>{getFirstLaunchAgentDisplayName(agent)}</strong>
                    <span>{subtitle}</span>
                  </span>
                  <Button
                    onClick={() =>
                      openFirstLaunchExternalUrl(vscode, FIRST_LAUNCH_AGENT_INSTALL_GUIDE_URLS[agentId] ?? '')
                    }
                    type='button'
                  >
                    Install Guide
                    <IconExternalLink aria-hidden='true' data-icon='inline-end' />
                  </Button>
                </div>
              );
            })}
          </div>
          <p className='first-launch-onb-hint'>
            These agents need a paid account, so pick the one you already have a subscription for. You will see its
            sign-in screen the first time Ghostex launches it.
          </p>
          <details className='first-launch-onb-flat-details'>
            <summary>{notInstalledAgents.length - 2} other supported agents</summary>
            <div className='first-launch-onb-flat-details-body'>
              <div className='first-launch-onb-pill-wrap'>
                {notInstalledAgents
                  .filter((agent) => agent.agentId !== 'claude' && agent.agentId !== 'codex')
                  .map((agent) => (
                    <span className='first-launch-onb-pill' data-tone='dim' key={agent.agentId}>
                      {agent.name}
                    </span>
                  ))}
              </div>
            </div>
          </details>
        </>
      )}

      <div className='first-launch-onb-rescan'>
        <span>Already installed one?</span>
        <Button
          disabled={agentHookStatusLoading || !onRequestAgentHookStatus}
          onClick={() => onRequestAgentHookStatus?.()}
          type='button'
          variant='outline'
        >
          <IconRefresh aria-hidden='true' data-icon='inline-start' />
          {agentHookStatusLoading ? 'Scanning' : 'Re-scan'}
        </Button>
      </div>
    </section>
  );
}

function FirstLaunchConnectPage({
  agentHookStatusLoading,
  hookStatusByAgentId,
  installedCliAgents,
  onInstallAgentHooks,
  onSkip,
}: {
  agentHookStatusLoading: boolean;
  hookStatusByAgentId: ReadonlyMap<string, SidebarAgentHookStatusItem>;
  installedCliAgents: readonly FirstLaunchSidebarAgent[];
  onInstallAgentHooks?: (agentIds?: readonly string[]) => void;
  onSkip: () => void;
}) {
  const isAgentConnected = (agentId: string) => {
    const status = hookStatusByAgentId.get(agentId)?.status;
    return status === 'installed' || status === 'notRequired';
  };
  const allConnected = installedCliAgents.every((agent) => isAgentConnected(agent.agentId));
  const anyUpdateRequired = installedCliAgents.some(
    (agent) => hookStatusByAgentId.get(agent.agentId)?.status === 'updateRequired'
  );

  return (
    <section aria-labelledby='first-launch-connect-title' className='first-launch-onb-page'>
      <h2 className='first-launch-onb-title' id='first-launch-connect-title'>
        Let Ghostex see what your agents are doing
      </h2>
      <p className='first-launch-onb-lede'>
        One click installs a small helper into each agent so Ghostex can show live status. This is what makes the
        sidebar useful, and you will see it working the moment an agent starts.
      </p>

      <div className='first-launch-onb-grid3'>
        {FIRST_LAUNCH_CONNECT_BENEFITS.map((benefit) => {
          const BenefitIcon = benefit.icon;
          return (
            <article className='first-launch-onb-card' key={benefit.title}>
              <h3>
                <BenefitIcon aria-hidden='true' size={15} /> {benefit.title}
              </h3>
              <p>{benefit.text}</p>
            </article>
          );
        })}
      </div>

      <div className='first-launch-onb-seclabel'>WILL BE CONNECTED</div>
      {installedCliAgents.map((agent) => {
        const status = hookStatusByAgentId.get(agent.agentId)?.status;
        const connected = isAgentConnected(agent.agentId);
        return (
          <div className='first-launch-onb-row' key={agent.agentId}>
            <span className='first-launch-onb-row-icon'>
              <FirstLaunchAgentLogo agent={agent} />
            </span>
            <span className='first-launch-onb-row-main'>
              <strong>{getFirstLaunchAgentDisplayName(agent)}</strong>
              <span>
                Adds a hooks entry to {getFirstLaunchAgentDisplayName(agent)}. Removable anytime from Settings,
                Integrations.
              </span>
            </span>
            <span className='first-launch-onb-pill' data-tone={connected ? 'ok' : 'dim'}>
              {connected ? 'Connected' : status === 'updateRequired' ? 'Needs update' : 'Not connected'}
            </span>
          </div>
        );
      })}

      <div className='first-launch-onb-actions'>
        <Button
          disabled={agentHookStatusLoading || allConnected || !onInstallAgentHooks}
          onClick={() => onInstallAgentHooks?.(installedCliAgents.map((agent) => agent.agentId))}
          type='button'
        >
          <IconDownload aria-hidden='true' data-icon='inline-start' />
          {allConnected ? 'Agents Connected' : anyUpdateRequired ? 'Update Connections' : 'Connect My Agents'}
        </Button>
        <Button onClick={onSkip} type='button' variant='ghost'>
          Skip for Now
        </Button>
      </div>
      <details className='first-launch-onb-flat-details'>
        <summary>What exactly gets installed?</summary>
        <div className='first-launch-onb-flat-details-body'>
          <p className='first-launch-onb-hint'>
            A per-agent hook script that reports session lifecycle events (started, waiting, done) to Ghostex on this
            machine. Nothing leaves your machine. Agents you install later get offered the same hook automatically.
          </p>
        </div>
      </details>
    </section>
  );
}

function FirstLaunchProjectPage({
  attentionNotificationsEnabled,
  completionSoundEnabled,
  installedCliAgents,
  onChangePreferredInterface,
  onChangeProjectFolder,
  onPickProjectFolder,
  onSelectAgent,
  onToggleAttentionNotifications,
  onToggleCompletionSound,
  preferredInterface,
  projectAgentId,
  projectFolder,
}: {
  attentionNotificationsEnabled: boolean;
  completionSoundEnabled: boolean;
  installedCliAgents: readonly FirstLaunchSidebarAgent[];
  onChangePreferredInterface: (preferredInterface: PreferredAgentInterface) => void;
  onChangeProjectFolder: (path: string) => void;
  onPickProjectFolder?: () => void;
  onSelectAgent: (agentId: string) => void;
  onToggleAttentionNotifications: (enabled: boolean) => void;
  onToggleCompletionSound: (enabled: boolean) => void;
  preferredInterface: PreferredAgentInterface;
  projectAgentId: string;
  projectFolder: string;
}) {
  /*
   * CDXC:FirstLaunchSetup 2026-08-24:
   * Browse opens the native folder dialog host-side (same round trip as the
   * terminal background image picker); the picked absolute path returns as a
   * firstLaunchProjectFolderPicked host message and lands in the input like a
   * typed path.
   */
  useEffect(() => {
    const handlePickedFolder = (event: Event) => {
      const message = (event as CustomEvent<unknown>).detail;
      if (
        !message ||
        typeof message !== 'object' ||
        !('type' in message) ||
        message.type !== 'firstLaunchProjectFolderPicked'
      ) {
        return;
      }
      const path = 'path' in message && typeof message.path === 'string' ? message.path.trim() : '';
      if (!path) {
        return;
      }
      onChangeProjectFolder(path);
    };
    window.addEventListener('ghostex-app-modal-host-message', handlePickedFolder);
    return () => {
      window.removeEventListener('ghostex-app-modal-host-message', handlePickedFolder);
    };
  }, [onChangeProjectFolder]);

  const selectedAgent = installedCliAgents.find((agent) => agent.agentId === projectAgentId);
  const finishSummary = projectFolder.trim()
    ? `"Finish" adds the project, opens it, and starts a ${
        selectedAgent ? getFirstLaunchAgentDisplayName(selectedAgent) : 'terminal'
      } session in your preferred view.`
    : 'Pick a folder above and "Finish" will open it with your first session running. You can also finish without one and add projects later.';

  return (
    <section aria-labelledby='first-launch-project-title' className='first-launch-onb-page'>
      <h2 className='first-launch-onb-title' id='first-launch-project-title'>
        Let's get started!
      </h2>
      <p className='first-launch-onb-lede'>
        Point Ghostex at a folder you are working on. We will start your first agent session in it so you land in a
        working workspace, not an empty window.
      </p>

      <div className='first-launch-onb-seclabel'>PROJECT FOLDER</div>
      <div className='first-launch-onb-folder-row'>
        <input
          className='first-launch-onb-input'
          onChange={(event) => onChangeProjectFolder(event.currentTarget.value)}
          placeholder='~/dev/my-project'
          type='text'
          value={projectFolder}
        />
        {onPickProjectFolder ? (
          <Button onClick={onPickProjectFolder} type='button' variant='outline'>
            Browse
          </Button>
        ) : null}
      </div>

      <div className='first-launch-onb-seclabel'>DEFAULT AGENT (used when you hit "New session")</div>
      <div className='first-launch-onb-chip-options' role='radiogroup' aria-label='Default agent'>
        {installedCliAgents.map((agent) => (
          <button
            aria-checked={projectAgentId === agent.agentId}
            className='first-launch-onb-chip-option'
            data-selected={projectAgentId === agent.agentId}
            key={agent.agentId}
            onClick={() => onSelectAgent(agent.agentId)}
            role='radio'
            type='button'
          >
            <FirstLaunchAgentLogo agent={agent} />
            {getFirstLaunchAgentDisplayName(agent)}
          </button>
        ))}
        <button
          aria-checked={projectAgentId === 'terminal'}
          className='first-launch-onb-chip-option'
          data-selected={projectAgentId === 'terminal'}
          onClick={() => onSelectAgent('terminal')}
          role='radio'
          type='button'
        >
          <IconTerminal2 aria-hidden='true' size={15} />
          Plain terminal
        </button>
      </div>

      <div className='first-launch-onb-seclabel'>DEFAULT SESSION VIEW</div>
      <div className='first-launch-onb-chip-options' role='radiogroup' aria-label='Default session view'>
        <button
          aria-checked={preferredInterface !== 'chat'}
          className='first-launch-onb-chip-option'
          data-selected={preferredInterface !== 'chat'}
          onClick={() => onChangePreferredInterface('terminal')}
          role='radio'
          type='button'
        >
          <IconTerminal2 aria-hidden='true' size={15} />
          Terminal
        </button>
        <button
          aria-checked={preferredInterface === 'chat'}
          className='first-launch-onb-chip-option'
          data-selected={preferredInterface === 'chat'}
          onClick={() => onChangePreferredInterface('chat')}
          role='radio'
          type='button'
        >
          <IconMessageCircle aria-hidden='true' size={15} />
          Chat
        </button>
      </div>
      <p className='first-launch-onb-hint'>{FIRST_LAUNCH_CHAT_SUPPORT_NOTE}</p>

      <div className='first-launch-onb-seclabel'>A FEW DEFAULTS</div>
      <div className='first-launch-onb-row'>
        <span className='first-launch-onb-row-icon'>
          <IconBellRinging aria-hidden='true' size={16} />
        </span>
        <span className='first-launch-onb-row-main'>
          <strong>Attention notifications</strong>
          <span>Show a notification when an agent finishes or needs you.</span>
        </span>
        <Switch
          aria-label='Attention notifications'
          checked={attentionNotificationsEnabled}
          onCheckedChange={onToggleAttentionNotifications}
        />
      </div>
      <div className='first-launch-onb-row'>
        <span className='first-launch-onb-row-icon'>
          <IconCircleCheck aria-hidden='true' size={16} />
        </span>
        <span className='first-launch-onb-row-main'>
          <strong>Completion sound</strong>
          <span>Play a short sound when long-running work finishes.</span>
        </span>
        <Switch
          aria-label='Completion sound'
          checked={completionSoundEnabled}
          onCheckedChange={onToggleCompletionSound}
        />
      </div>

      <div className='first-launch-onb-check-hero'>
        <IconCircleCheckFilled aria-hidden='true' className='first-launch-onb-check-icon' size={40} />
        <div className='first-launch-onb-check-title'>That's everything</div>
        <p className='first-launch-onb-hint'>{finishSummary}</p>
      </div>
    </section>
  );
}

function DiscordLogoIcon() {
  return (
    <svg
      aria-hidden='true'
      className='first-launch-setup-discord-logo'
      viewBox='0 0 256 199'
      xmlns='http://www.w3.org/2000/svg'
    >
      <path
        d='M216.856 16.597A208.502 208.502 0 0 0 164.042 0c-2.275 4.113-4.933 9.645-6.766 14.046-19.692-2.961-39.203-2.961-58.533 0-1.832-4.4-4.55-9.933-6.846-14.046a207.809 207.809 0 0 0-52.855 16.638C5.618 67.147-3.443 116.4 1.087 164.956c22.169 16.555 43.653 26.612 64.775 33.193A161.094 161.094 0 0 0 79.735 175.3a136.413 136.413 0 0 1-21.846-10.632 108.636 108.636 0 0 0 5.356-4.237c42.122 19.702 87.89 19.702 129.51 0a131.66 131.66 0 0 0 5.355 4.237 136.07 136.07 0 0 1-21.886 10.653c4.006 8.02 8.638 15.67 13.873 22.848 21.142-6.58 42.646-16.637 64.815-33.213 5.316-56.288-9.08-105.09-38.056-148.36ZM85.474 135.095c-12.645 0-23.015-11.805-23.015-26.18s10.149-26.2 23.015-26.2c12.867 0 23.236 11.804 23.015 26.2.02 14.375-10.148 26.18-23.015 26.18Zm85.051 0c-12.645 0-23.014-11.805-23.014-26.18s10.148-26.2 23.014-26.2c12.867 0 23.236 11.804 23.015 26.2 0 14.375-10.148 26.18-23.015 26.18Z'
        fill='#5865F2'
      />
    </svg>
  );
}

function FirstLaunchPreferencesPage({
  onChange,
  settings,
}: {
  onChange: (settings: ghostexSettings) => void;
  settings: ghostexSettings;
}) {
  const activePresetId = getSidebarSettingsPresetId(settings);
  const normalizedDefaultPromptAgentId =
    settings.defaultPromptAgentId.trim() || DEFAULT_ghostex_SETTINGS.defaultPromptAgentId;
  const firstLaunchPromptAgentHasSavedDefault = FIRST_LAUNCH_PROMPT_AGENT_OPTIONS.some(
    (option) => option.value === normalizedDefaultPromptAgentId
  );
  const firstLaunchPromptAgentOptions = firstLaunchPromptAgentHasSavedDefault
    ? FIRST_LAUNCH_PROMPT_AGENT_OPTIONS
    : [
        /*
         * CDXC:GxserverAgentSettings 2026-06-19-08:58:
         * First-launch preferences must display a gxserver-owned custom or
         * currently unavailable Default Prompt Agent as unavailable instead of
         * visually falling back to Codex. Other preference saves should preserve
         * that canonical agent id until the user explicitly changes it.
         */
        {
          label: `Unavailable (${normalizedDefaultPromptAgentId})`,
          value: normalizedDefaultPromptAgentId,
        },
        ...FIRST_LAUNCH_PROMPT_AGENT_OPTIONS,
      ];
  const selectedDefaultPromptAgentId = normalizedDefaultPromptAgentId;

  const updateSetting = <Key extends keyof ghostexSettings>(key: Key, value: ghostexSettings[Key]) => {
    onChange(normalizeghostexSettings({ ...settings, [key]: value }));
  };

  const applySidebarPreset = (presetId: SidebarSettingsPresetId) => {
    onChange(applySidebarSettingsPreset(settings, presetId));
  };

  return (
    <section aria-labelledby='first-launch-preferences-title' className='first-launch-setup-preferences'>
      <div className='first-launch-setup-preferences-hero'>
        <span className='first-launch-setup-guide-icon-shell'>
          <IconSettings aria-hidden='true' className='first-launch-setup-guide-icon' size={26} />
        </span>
        <div className='first-launch-setup-guide-copy'>
          <h2 className='first-launch-setup-title' id='first-launch-preferences-title'>
            Choose the defaults that shape Ghostex.
          </h2>
          <p className='first-launch-setup-description'>
            These are the settings most likely to affect how Ghostex feels day to day. You can change all of them later
            from Settings.
          </p>
        </div>
      </div>

      <div className='first-launch-setup-preferences-grid'>
        <article className='first-launch-setup-preference-card first-launch-setup-preference-card-wide'>
          <div className='first-launch-setup-preference-copy'>
            <div className='first-launch-setup-preference-heading'>
              <span className='first-launch-setup-preference-icon'>
                <IconLayoutDashboard aria-hidden='true' size={16} />
              </span>
              <div>
                <h3>Sidebar style</h3>
                <p>Pick how much detail session cards and sidebar chrome should show.</p>
              </div>
            </div>
          </div>
          <div className='first-launch-setup-preset-options' role='group' aria-label='Sidebar style'>
            {FIRST_LAUNCH_SIDEBAR_PRESETS.map((preset) => (
              <button
                aria-pressed={activePresetId === preset.id}
                className='first-launch-setup-preset-button'
                data-active={activePresetId === preset.id}
                key={preset.id}
                onClick={() => applySidebarPreset(preset.id)}
                type='button'
              >
                {preset.label}
              </button>
            ))}
            {activePresetId ? null : <span className='first-launch-setup-preset-custom'>Custom</span>}
          </div>
        </article>

        <article className='first-launch-setup-preference-card'>
          <label className='first-launch-setup-preference-select-label'>
            <span className='first-launch-setup-preference-heading'>
              <span className='first-launch-setup-preference-icon'>
                <IconBrandOpenai aria-hidden='true' size={16} />
              </span>
              <span>
                <span className='first-launch-setup-preference-title'>Default agent</span>
                <span className='first-launch-setup-preference-description'>
                  Used by helper prompts and new project-board agent starts.
                </span>
              </span>
            </span>
            <select
              className='first-launch-setup-preference-select'
              onChange={(event) => updateSetting('defaultPromptAgentId', event.currentTarget.value)}
              value={selectedDefaultPromptAgentId}
            >
              {firstLaunchPromptAgentOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <div className='first-launch-setup-preference-usage'>
            <span className='first-launch-setup-preference-usage-title'>This agent powers</span>
            <ul className='first-launch-setup-preference-usage-list'>
              <li>Starting work from a card on the Kanban board</li>
              <li>Writing commit messages in the Git commit dialog</li>
              <li>The pre-selected agent when you create a new worktree</li>
            </ul>
          </div>
        </article>

        <article className='first-launch-setup-preference-card'>
          <label className='first-launch-setup-preference-select-label'>
            <span className='first-launch-setup-preference-heading'>
              <span className='first-launch-setup-preference-icon'>
                <IconSparkles aria-hidden='true' size={16} />
              </span>
              <span>
                <span className='first-launch-setup-preference-title'>Title generation agent</span>
                <span className='first-launch-setup-preference-description'>
                  Used for automatic first-prompt session names.
                </span>
              </span>
            </span>
            <select
              className='first-launch-setup-preference-select'
              onChange={(event) =>
                updateSetting('sessionTitleGenerationAgent', event.currentTarget.value as SessionTitleGenerationAgent)
              }
              value={settings.sessionTitleGenerationAgent}
            >
              {SESSION_TITLE_GENERATION_AGENT_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label className='first-launch-setup-preference-select-label'>
            <span className='first-launch-setup-preference-description'>
              Command Ghostex sends for automatic first-prompt session titles.
            </span>
            <textarea
              className='first-launch-setup-preference-command-preview'
              disabled
              readOnly
              value={getSessionTitleGenerationCommandPreview(settings.sessionTitleGenerationAgent, {
                command:
                  settings.sessionTitleGenerationAgent === 'custom'
                    ? settings.customSessionTitleGenerationCommand
                    : undefined,
              })}
            />
          </label>
          {settings.sessionTitleGenerationAgent === 'custom' ? (
            <label className='first-launch-setup-preference-select-label'>
              <span className='first-launch-setup-preference-description'>
                Command that reads the title prompt on stdin and prints only the title.
              </span>
              <input
                className='first-launch-setup-preference-input'
                onChange={(event) => updateSetting('customSessionTitleGenerationCommand', event.currentTarget.value)}
                placeholder='title-generator'
                value={settings.customSessionTitleGenerationCommand}
              />
            </label>
          ) : null}
        </article>

        {/*
         * CDXC:FirstLaunchPreferences 2026-05-31-07:10:
         * ZMU-71: expose lid-close keep-awake on the first-time defaults page so
         * installs can opt in before enabling Keep Awake from the titlebar.
         */}
        <FirstLaunchCheckboxSetting
          checked={settings.keepAwakePreventLidSleep}
          description='When Keep Awake is on, keep the Mac reachable after closing the lid.'
          icon={IconMoon}
          label='Keep awake when lid is closed'
          onChange={(checked) => updateSetting('keepAwakePreventLidSleep', checked)}
        />
        <FirstLaunchCheckboxSetting
          checked={settings.agentAcceptAllEnabled}
          description='Launch supported agents with their permission-bypass mode by default.'
          icon={IconBolt}
          label='Accept All for new agent sessions'
          onChange={(checked) => updateSetting('agentAcceptAllEnabled', checked)}
        />
        <FirstLaunchCheckboxSetting
          checked={settings.showMacOSAttentionNotifications}
          description='Show a macOS banner when an agent needs attention.'
          icon={IconBellRinging}
          label='macOS attention notifications'
          onChange={(checked) => updateSetting('showMacOSAttentionNotifications', checked)}
        />
        <FirstLaunchCheckboxSetting
          checked={settings.completionBellEnabled}
          description='Play a completion sound when long-running work finishes.'
          icon={IconCircleCheck}
          label='Completion sound'
          onChange={(checked) => updateSetting('completionBellEnabled', checked)}
        />
      </div>
    </section>
  );
}

function FirstLaunchCheckboxSetting({
  checked,
  description,
  icon: SettingIcon,
  label,
  onChange,
}: {
  checked: boolean;
  description: string;
  icon: ComponentType<{ className?: string; size?: number; stroke?: number }>;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  const id = useId();

  return (
    <article className='first-launch-setup-preference-card'>
      <label className='first-launch-setup-checkbox-setting' htmlFor={id}>
        <span className='first-launch-setup-preference-heading'>
          <span className='first-launch-setup-preference-icon'>
            <SettingIcon aria-hidden='true' size={16} />
          </span>
          <span>
            <span className='first-launch-setup-preference-title'>{label}</span>
            <span className='first-launch-setup-preference-description'>{description}</span>
          </span>
        </span>
        <input
          checked={checked}
          className='first-launch-setup-checkbox'
          id={id}
          onChange={(event) => onChange(event.currentTarget.checked)}
          type='checkbox'
        />
      </label>
    </article>
  );
}

function isFirstLaunchSkillInstalled(
  skillId: BundledGhostexAgentSkillId,
  status: SidebarGhostexCliStatusMessage | undefined
): boolean {
  switch (skillId) {
    case 'browserUse':
      return status?.browserSkillInstalled === true;
    case 'embeddedBrowserUse':
      return status?.embeddedBrowserSkillInstalled === true;
    case 'computerUse':
      return status?.computerUseSkillInstalled === true;
    case 'cli':
      return status?.cliSkillInstalled === true;
    case 'fable56Orchestration':
      return status?.fable56OrchestrationSkillInstalled === true;
    case 'manageBeads':
      return status?.manageBeadsSkillInstalled === true;
    case 'generateTitle':
      return status?.generateTitleSkillInstalled === true;
    case 'moveCodexSession':
      return status?.moveCodexSessionSkillInstalled === true;
  }
}

function FirstLaunchSkillRow({
  ghostexCliStatus,
  onToggleSkill,
  selected,
  skill,
}: {
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  onToggleSkill: (skillId: BundledGhostexAgentSkillId, selected: boolean) => void;
  selected: boolean;
  skill: BundledGhostexAgentSkill;
}) {
  const installed = isFirstLaunchSkillInstalled(skill.id, ghostexCliStatus);
  // "Ghostex CLI" keeps its product prefix; the other rows read cleaner
  // without repeating "Ghostex" five times in one list.
  const shortName = skill.id === 'cli' ? skill.name : skill.name.replace(/^Ghostex /u, '');
  return (
    <label className='first-launch-onb-row first-launch-onb-skill-row' data-installed={installed}>
      <input
        checked={installed || selected}
        className='first-launch-onb-checkbox'
        disabled={installed}
        onChange={(event) => onToggleSkill(skill.id, event.currentTarget.checked)}
        type='checkbox'
      />
      <span className='first-launch-onb-row-main'>
        <strong>{shortName}</strong>
        <span>{skill.description}</span>
      </span>
      {installed ? (
        <span className='first-launch-onb-pill' data-tone='ok'>
          Installed
        </span>
      ) : skill.requiresCuaDriver ? (
        <span className='first-launch-onb-pill' data-tone='dim'>
          Uses {GHOSTEX_TRYCUA_PRODUCT_NAME}
        </span>
      ) : null}
    </label>
  );
}

function FirstLaunchSkillsPage({
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onInstallCuaDriver,
  onInstallSkill,
  onSkip,
  onToggleSkill,
  selectedSkillIds,
}: {
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onInstallCuaDriver?: () => void;
  onInstallSkill: Partial<Record<BundledGhostexAgentSkillId, () => void>>;
  onSkip: () => void;
  onToggleSkill: (skillId: BundledGhostexAgentSkillId, selected: boolean) => void;
  selectedSkillIds: ReadonlySet<BundledGhostexAgentSkillId>;
}) {
  const recommendedSkills = VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS.filter((skill) => skill.tier === 'recommended');
  const optionalSkills = VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS.filter((skill) => skill.tier === 'optional');
  const cliReady = ghostexCliStatus?.installed === true;
  const installableSelection = VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS.filter(
    (skill) => selectedSkillIds.has(skill.id) && !isFirstLaunchSkillInstalled(skill.id, ghostexCliStatus)
  );
  const needsCuaDriver =
    installableSelection.some((skill) => skill.requiresCuaDriver === true) &&
    ghostexCliStatus?.cuaDriverInstalled !== true;

  const installSelectedSkills = () => {
    if (needsCuaDriver) {
      // Computer Use and Browser Use run through Trycua, so one install step
      // covers the shared driver before the skills that need it.
      onInstallCuaDriver?.();
    }
    for (const skill of installableSelection) {
      onInstallSkill[skill.id]?.();
    }
  };

  return (
    <section aria-labelledby='first-launch-skills-title' className='first-launch-onb-page'>
      <h2 className='first-launch-onb-title' id='first-launch-skills-title'>
        Optional superpowers
      </h2>
      <p className='first-launch-onb-lede'>
        Skills teach your agents Ghostex tricks, like driving your browser or controlling your machine. Pick what sounds
        useful; everything here is optional and can be added later from Settings.
      </p>

      <div className='first-launch-onb-seclabel'>RECOMMENDED</div>
      {recommendedSkills.map((skill) => (
        <FirstLaunchSkillRow
          ghostexCliStatus={ghostexCliStatus}
          key={skill.id}
          onToggleSkill={onToggleSkill}
          selected={selectedSkillIds.has(skill.id)}
          skill={skill}
        />
      ))}

      {optionalSkills.length > 0 ? (
        <details className='first-launch-onb-flat-details'>
          <summary>
            {optionalSkills.length === 1
              ? '1 more skill for power users'
              : `${optionalSkills.length} more skills for power users`}
          </summary>
          <div className='first-launch-onb-flat-details-body'>
            {optionalSkills.map((skill) => (
              <FirstLaunchSkillRow
                ghostexCliStatus={ghostexCliStatus}
                key={skill.id}
                onToggleSkill={onToggleSkill}
                selected={selectedSkillIds.has(skill.id)}
                skill={skill}
              />
            ))}
          </div>
        </details>
      ) : null}

      <div className='first-launch-onb-actions'>
        <Button
          disabled={ghostexCliStatusLoading || !cliReady || installableSelection.length === 0}
          onClick={installSelectedSkills}
          type='button'
        >
          <IconDownload aria-hidden='true' data-icon='inline-start' />
          Install Selected ({installableSelection.length})
        </Button>
        <Button onClick={onSkip} type='button' variant='ghost'>
          Skip, Install None
        </Button>
      </div>
    </section>
  );
}

function FirstLaunchCliPage({
  ghostexCliStatus,
  ghostexCliStatusLoading,
  vscode,
}: {
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  vscode?: WebviewApi;
}) {
  const isInstalled = ghostexCliStatus?.installed === true;
  const isChecking = ghostexCliStatusLoading && !ghostexCliStatus;
  /**
   * CDXC:CliInstall 2026-06-07-13:53:
   * First launch should present the Ghostex CLI as included with the macOS app.
   * Production startup auto-repairs ghostex/gx commands to the bundled
   * Resources/CLI runtime, so onboarding must explain what the commands unlock
   * instead of showing install or refresh controls.
   *
   * CDXC:CliInstall 2026-06-12-09:31:
   * Startup repair now installs wrapper commands instead of app-bundled
   * symlinks, but onboarding copy stays focused on the user-facing command
   * names rather than the implementation detail.
   */
  const commandLabel = isChecking
    ? 'checking command links'
    : isInstalled
      ? 'available command'
      : 'command link needs attention';
  const commandText = isChecking ? 'Checking gx --help...' : 'gx --help';

  return (
    <div className='first-launch-setup-cli-page'>
      <section aria-labelledby='first-launch-cli-title' className='first-launch-setup-cli-copy'>
        <h2 className='first-launch-setup-title' id='first-launch-cli-title'>
          Ghostex CLI is installed with the app.
        </h2>
        <p className='first-launch-setup-description'>
          The app keeps the ghostex command pointed at the current app build automatically. Use it to list sessions,
          attach from another terminal, connect mobile clients, and install Ghostex agent skills when you choose them.
        </p>

        <div className='first-launch-setup-command-card' data-installed={isInstalled}>
          <div className='first-launch-setup-command-label'>
            {isInstalled ? (
              <IconCircleCheckFilled aria-hidden='true' size={16} />
            ) : (
              <IconTerminal2 aria-hidden='true' size={16} />
            )}
            {commandLabel}
          </div>
          <code>{commandText}</code>
          {ghostexCliStatus?.detail ? (
            <p className='first-launch-setup-cli-status-detail'>{ghostexCliStatus.detail}</p>
          ) : null}
        </div>

        <ul className='first-launch-setup-mobile-benefits' aria-label='CLI and browser agent features'>
          {FIRST_LAUNCH_CLI_MOBILE_BENEFITS.map((benefit) => {
            const BenefitIcon = benefit.icon;
            return (
              <li key={benefit.title}>
                <BenefitIcon aria-hidden='true' size={18} />
                <span className='first-launch-setup-mobile-benefit-copy'>
                  <span className='first-launch-setup-mobile-benefit-title'>{benefit.title}</span>
                  <span className='first-launch-setup-mobile-benefit-text'>{benefit.text}</span>
                </span>
              </li>
            );
          })}
        </ul>

        <div className='first-launch-setup-app-links' aria-label='Android app download'>
          <Button
            className='first-launch-setup-app-link-button'
            onClick={() => openFirstLaunchExternalUrl(vscode, FIRST_LAUNCH_ANDROID_APK_URL)}
            type='button'
            variant='outline'
          >
            <IconBrandAndroid aria-hidden='true' size={16} />
            React Native Android APK
          </Button>
        </div>
      </section>
    </div>
  );
}

function FirstLaunchGuidePageView({
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onInstallBrowserControl,
  onInstallCuaDriver,
  onOpenAccessibilityPreferences,
  onOpenScreenRecordingPreferences,
  page,
}: {
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onInstallBrowserControl?: () => void;
  onInstallCuaDriver?: () => void;
  onOpenAccessibilityPreferences?: () => void;
  onOpenScreenRecordingPreferences?: () => void;
  page?: FirstLaunchGuidePage;
}) {
  if (!page) {
    return null;
  }

  const PageIcon = page.icon;
  const snippetText = page.action?.snippet?.join('\n');
  const examplesAtBottom = page.action?.examplesAtBottom === true && Boolean(snippetText);
  const browserControlInstalled = ghostexCliStatus?.embeddedBrowserSkillInstalled === true;
  const desktopControlInstalled =
    ghostexCliStatus?.cuaDriverInstalled === true && ghostexCliStatus?.computerUseSkillInstalled === true;

  return (
    <section className='first-launch-setup-guide-page' aria-labelledby={`first-launch-${page.page}-title`}>
      <div className='first-launch-setup-guide-hero'>
        <span className='first-launch-setup-guide-icon-shell'>
          <PageIcon aria-hidden='true' className='first-launch-setup-guide-icon' size={26} />
        </span>
        <div className='first-launch-setup-guide-copy'>
          {page.kicker ? <div className='first-launch-setup-kicker'>{page.kicker}</div> : null}
          <h2 className='first-launch-setup-title' id={`first-launch-${page.page}-title`}>
            {page.title}
          </h2>
        </div>
      </div>

      {page.imageSrc ? (
        <div className='first-launch-setup-guide-visual-shell'>
          <img alt={page.imageAlt ?? ''} className='first-launch-setup-guide-visual' src={page.imageSrc} />
        </div>
      ) : null}

      <div className='first-launch-setup-guide-content'>
        {page.action ? (
          <div className='first-launch-setup-guide-callout'>
            <div className='first-launch-setup-guide-callout-heading'>
              <h3 className='first-launch-setup-guide-callout-title'>{page.action.eyebrow}</h3>
              {page.action.subtitle ? (
                <p className='first-launch-setup-guide-callout-subtitle'>{page.action.subtitle}</p>
              ) : null}
            </div>
            <p>{page.action.description}</p>
            {snippetText && !examplesAtBottom ? (
              <pre className='first-launch-setup-guide-snippet'>
                <code>{snippetText}</code>
              </pre>
            ) : null}
            {page.page === 'browserControl' ? (
              <div className='first-launch-setup-command-actions'>
                <Button
                  disabled={
                    ghostexCliStatusLoading ||
                    browserControlInstalled ||
                    !ghostexCliStatus?.installed ||
                    !onInstallBrowserControl
                  }
                  onClick={onInstallBrowserControl}
                  type='button'
                  variant={browserControlInstalled ? 'outline' : 'default'}
                >
                  {browserControlInstalled ? (
                    <IconCircleCheckFilled aria-hidden='true' data-icon='inline-start' />
                  ) : (
                    <IconDownload aria-hidden='true' data-icon='inline-start' />
                  )}
                  {browserControlInstalled ? 'Ghostex Browser Use Installed' : 'Install Ghostex Browser Use'}
                </Button>
              </div>
            ) : null}
            {page.page === 'desktopCua' ? (
              <div className='first-launch-setup-command-actions'>
                <Button
                  disabled={ghostexCliStatusLoading || desktopControlInstalled || !onInstallCuaDriver}
                  onClick={onInstallCuaDriver}
                  type='button'
                  variant={desktopControlInstalled ? 'outline' : 'default'}
                >
                  {desktopControlInstalled ? (
                    <IconCircleCheckFilled aria-hidden='true' data-icon='inline-start' />
                  ) : (
                    <IconDownload aria-hidden='true' data-icon='inline-start' />
                  )}
                  {desktopControlInstalled ? 'Desktop Control Installed' : 'Install Desktop Control'}
                </Button>
                <Button
                  disabled={!onOpenAccessibilityPreferences}
                  onClick={onOpenAccessibilityPreferences}
                  type='button'
                  variant='outline'
                >
                  <IconSettings aria-hidden='true' data-icon='inline-start' />
                  Accessibility
                </Button>
                <Button
                  disabled={!onOpenScreenRecordingPreferences}
                  onClick={onOpenScreenRecordingPreferences}
                  type='button'
                  variant='outline'
                >
                  <IconSettings aria-hidden='true' data-icon='inline-start' />
                  Screen Recording
                </Button>
              </div>
            ) : null}
          </div>
        ) : null}

        <ul className='first-launch-setup-guide-list'>
          {page.items.map((item) => {
            const ItemIcon = item.icon;
            return (
              <li className='first-launch-setup-guide-list-item' key={item.title}>
                <span className='first-launch-setup-guide-list-icon'>
                  <ItemIcon aria-hidden='true' size={14} />
                </span>
                <span className='first-launch-setup-guide-list-copy'>
                  <span className='first-launch-setup-guide-list-title'>{item.title}</span>
                  <span className='first-launch-setup-guide-list-text'>{item.text}</span>
                </span>
              </li>
            );
          })}
        </ul>

        {examplesAtBottom ? (
          <section
            aria-labelledby={`first-launch-${page.page}-examples-title`}
            className='first-launch-setup-guide-examples'
          >
            <h3 className='first-launch-setup-guide-examples-title' id={`first-launch-${page.page}-examples-title`}>
              Examples
            </h3>
            <pre className='first-launch-setup-guide-snippet'>
              <code>{snippetText}</code>
            </pre>
          </section>
        ) : null}
      </div>
    </section>
  );
}

function openFirstLaunchExternalUrl(vscode: WebviewApi | undefined, url: string) {
  if (!vscode) {
    return;
  }
  vscode.postMessage({ type: 'openExternalUrl', url });
}

function getSidebarThemeVariant(theme: SidebarTheme): 'dark' | 'light' {
  return theme.startsWith('light-') || theme === 'plain-light' ? 'light' : 'dark';
}

export type { FirstLaunchSetupMainSettingKey };
