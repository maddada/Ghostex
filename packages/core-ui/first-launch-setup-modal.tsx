import {
  IconArrowRight,
  IconApps,
  IconAlertTriangle,
  IconArrowLeft,
  IconBellRinging,
  IconBolt,
  IconBrowser,
  IconBrandAndroid,
  IconBrandOpenai,
  IconBrandYoutube,
  IconCircleCheck,
  IconCircleCheckFilled,
  IconCircleX,
  IconCode,
  IconDeviceMobile,
  IconDeviceFloppy,
  IconDownload,
  IconFolders,
  IconHistory,
  IconInfoCircle,
  IconLayoutDashboard,
  IconMoon,
  IconPencil,
  IconRefresh,
  IconSettings,
  IconSparkles,
  IconStack,
  IconTerminal2,
  IconTools,
  IconUsersGroup,
  IconWorld,
} from '@tabler/icons-react';
import { useEffect, useId, useState, type ComponentType } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/packages/components/ui/dialog';
import { cn } from '@/packages/components/utils';
import type { FirstLaunchSetupMainSettingKey } from '../shared/first-launch-setup-settings';
import type { SidebarTheme } from '../shared/session-grid-contract';
import type {
  SidebarAgentHookStatus,
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
  type SessionTitleGenerationAgent,
  type SidebarSettingsPresetId,
  type ghostexSettings,
} from '../shared/ghostex-settings';
import type { BundledGhostexAgentSkillId } from '../shared/ghostex-agent-skills';
import { DEFAULT_SIDEBAR_AGENTS } from '../shared/sidebar-agents';
import { BundledAgentSkillsPanel } from './bundled-agent-skills-panel';
import type { WebviewApi } from './webview-api';
import ghostexIntroImage from './assets/first-launch/ghostex-intro.png';

export type FirstLaunchSetupPage =
  | 'video'
  | 'welcome'
  | 'preferences'
  | 'hooks'
  | 'cli'
  | 'skills'
  | 'ready'
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
  /** Player page URL for hosts that cannot embed YouTube from their own origin. */
  tutorialVideoEmbedUrl?: string;
  isOpen: boolean;
  onClose: () => void;
  onChange: (settings: ghostexSettings) => void;
  onInstallAgentHooks?: (agentIds?: readonly string[]) => void;
  onInstallAgentOrchestrationSkill?: () => void;
  onInstallBrowserControl?: () => void;
  onInstallBrowserUseSkill?: () => void;
  onInstallComputerUseSkill?: () => void;
  onInstallCuaDriver?: () => void;
  onInstallFable56OrchestrationSkill?: () => void;
  onInstallFindPrevSessionSkill?: () => void;
  onInstallGenerateTitleSkill?: () => void;
  onInstallGhostexCli?: () => void;
  onInstallMoveCodexSessionSkill?: () => void;
  onUninstallBundledAgentSkill?: (skillId: BundledGhostexAgentSkillId) => void;
  onOpenAccessibilityPreferences?: () => void;
  onOpenScreenRecordingPreferences?: () => void;
  onRequestAgentHookStatus?: (agentIds?: readonly string[]) => void;
  onRequestGhostexCliStatus?: () => void;
  settings?: ghostexSettings;
  theme?: SidebarTheme;
  vscode?: WebviewApi;
};

type FirstLaunchBenefit = {
  icon: ComponentType<{ className?: string; size?: number; stroke?: number }>;
  text: string;
  title: string;
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

type FirstLaunchContinueWarning = 'hooks' | 'skills' | 'browserControl' | 'desktopCua';

const FIRST_LAUNCH_INTRO_BENEFITS: readonly FirstLaunchBenefit[] = [
  {
    icon: IconSparkles,
    text: 'Keep parallel agent sessions, terminals, browsers, and project work together in one fast, native macOS workspace instead of scattered apps.',
    title: 'Agent workspace',
  },
  {
    icon: IconBellRinging,
    text: 'See In Progress and Needs Attention states across all your sessions at a glance, so you always know which agents are working and which are waiting on you, without hunting through every terminal one by one.',
    title: 'Status awareness',
  },
  {
    icon: IconCircleCheck,
    text: 'Name and tag sessions automatically from your first message, turning cryptic shell titles into clear, scannable labels and tags so the sidebar stays organized and readable even with dozens of sessions open at once.',
    title: 'More Organized Sessions',
  },
];

const FIRST_LAUNCH_HOOK_SUPPORTED_AGENTS = DEFAULT_SIDEBAR_AGENTS;
const FIRST_LAUNCH_HOOK_SKIP_WARNING_AGENT_IDS = ['claude', 'codex', 'opencode', 'pi'] as const;
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
/*
 * CDXC:FirstLaunchTutorialVideo 2026-08-19:
 * The tutorial used to open as its own window on startup, so a new user got two
 * modals in a row. It is the first page of this one modal instead. Hosts whose
 * document has a real http(s) origin can embed YouTube directly; the GPUI app
 * serves the modal host from file://, where the embed player answers "Error 153
 * - Video player configuration error", so it passes its own same-origin player
 * page through `tutorialVideoEmbedUrl`.
 */
const FIRST_LAUNCH_TUTORIAL_VIDEO_EMBED_URL = 'https://www.youtube.com/embed/APdP-j5n4Mw?rel=0&modestbranding=1';
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
 * CDXC:FirstLaunchSetup 2026-06-18-02:29:
 * The first-time launch modal should keep only Welcome, Agent Hooks, and
 * Bundled Agent Skills in the visible sequence. Leave the other page components
 * and guide data in this file so they can be restored or reused without losing
 * the previous setup content.
 *
 * CDXC:FirstLaunchSetup 2026-06-18-02:54:
 * First-launch hook setup should show every supported agent CLI. Native status
 * checks still prioritize Codex, Claude, and Pi, then continue through the
 * secondary providers one at a time so the page fills progressively.
 */
const FIRST_LAUNCH_SETUP_PAGES: readonly FirstLaunchSetupPage[] = ['video', 'welcome', 'hooks', 'skills', 'ready'];

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

function getVisibleFirstLaunchSetupPage(page: FirstLaunchSetupPage): FirstLaunchSetupPage {
  // No requested page (or a dormant one) starts the sequence at its first page.
  return FIRST_LAUNCH_SETUP_PAGES.includes(page) ? page : FIRST_LAUNCH_SETUP_PAGES[0];
}

const FIRST_LAUNCH_CONTINUE_WARNINGS: Record<
  FirstLaunchContinueWarning,
  {
    actionLabel: string;
    description: string;
    installLabel: string;
    title: string;
  }
> = {
  /*
   * CDXC:FirstLaunchSetup 2026-06-18-02:54:
   * First-launch warnings should tell users that Settings can remove all Ghostex hooks or bundled skills later.
   *
   * CDXC:IntegrationsSetup 2026-06-21-02:54:
   * Hook and bundled-skill uninstall controls moved to the bottom of Settings > Integrations, so skipped-setup warnings should point at that tab instead of the old General Settings advanced search path.
   */
  hooks: {
    actionLabel: 'Continue without hooks',
    description:
      'Ghostex will not notify you when agents enter In Progress or Needs Attention, and it will not automatically name agent sessions from the first message until hooks are installed. You can install them later from Settings > Integrations, or uninstall all Ghostex hooks from the bottom of Settings > Integrations.',
    installLabel: 'Install Hooks',
    title: 'Continue without agent hooks?',
  },
  skills: {
    actionLabel: 'Continue without skills',
    description:
      'Agents will not discover Ghostex Browser Use, Ghostex Embedded Browser Use, Ghostex Computer Use, Agent Orchestration, or Auto Rename Session until the bundled skills are installed. You can install them later from Settings > Integrations, or uninstall all bundled Ghostex skills from the bottom of Settings > Integrations.',
    installLabel: 'Install Missing Skills',
    title: 'Continue without bundled agent skills?',
  },
  browserControl: {
    actionLabel: 'Continue without Ghostex Embedded Browser Use',
    description:
      'Agents will not be able to inspect or operate Ghostex browser panes through Ghostex Embedded Browser Use until the skill is installed. You can install it later from Settings > Integrations.',
    installLabel: 'Install Ghostex Embedded Browser Use',
    title: 'Continue without Ghostex Embedded Browser Use?',
  },
  desktopCua: {
    actionLabel: 'Continue without Ghostex Computer Use',
    description:
      'Agents will not be able to control native macOS desktop apps until Ghostex Computer Use, Cua Driver, Accessibility, and Screen Recording are ready. You can finish this later from Settings > Integrations.',
    installLabel: 'Install Ghostex Computer Use',
    title: 'Continue without Ghostex Computer Use?',
  },
};

type FirstLaunchHookStatusGroupId = 'installed' | 'updateRequired' | 'missing' | 'cliMissing' | 'unknown';

type FirstLaunchHookStatusGroup = {
  agents: Array<(typeof FIRST_LAUNCH_HOOK_SUPPORTED_AGENTS)[number]>;
  id: FirstLaunchHookStatusGroupId;
  title: string;
};

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
 * CDXC:AgentOrchestration 2026-05-27-07:15:
 * The bundled skills page installs `$ghostex-agent-orchestration`, because
 * agents should learn Ghostex's supported pane/session commands for cross-agent
 * messaging, status checks, and terminal reads through `ghostex --help` instead
 * of raw zmx.
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
  onInstallAgentHooks,
  onInstallAgentOrchestrationSkill,
  onInstallBrowserControl,
  onInstallBrowserUseSkill,
  onInstallComputerUseSkill,
  onInstallCuaDriver,
  onInstallFable56OrchestrationSkill,
  onInstallFindPrevSessionSkill,
  onInstallGenerateTitleSkill,
  onInstallMoveCodexSessionSkill,
  onUninstallBundledAgentSkill,
  onOpenAccessibilityPreferences,
  onOpenScreenRecordingPreferences,
  onRequestAgentHookStatus,
  onRequestGhostexCliStatus,
  onChange,
  settings = DEFAULT_ghostex_SETTINGS,
  theme = 'dark-blue',
  tutorialVideoEmbedUrl,
  vscode,
}: FirstLaunchSetupModalProps) {
  const [activePage, setActivePage] = useState<FirstLaunchSetupPage>(getVisibleFirstLaunchSetupPage(initialPage));
  const [continueWarning, setContinueWarning] = useState<FirstLaunchContinueWarning>();

  useEffect(() => {
    if (!isOpen || agentHookStatus || agentHookStatusLoading) {
      return;
    }
    onRequestAgentHookStatus?.();
  }, [agentHookStatus, agentHookStatusLoading, isOpen, onRequestAgentHookStatus]);

  useEffect(() => {
    if (isOpen) {
      setActivePage(getVisibleFirstLaunchSetupPage(initialPage));
      setContinueWarning(undefined);
    }
  }, [initialPage, isOpen]);

  const hookTone = getFirstLaunchHookTone(agentHookStatus, agentHookStatusLoading);
  const hookStatusByAgentId = new Map(agentHookStatus?.agents.map((status) => [status.agentId, status]) ?? []);
  const firstLaunchHooksReady = areFirstLaunchAgentHooksReady(agentHookStatus);
  const firstLaunchBundledSkillsReady = isAnyFirstLaunchBundledSkillInstalled(ghostexCliStatus);
  const activePageIndex = Math.max(0, FIRST_LAUNCH_SETUP_PAGES.indexOf(activePage));
  const isLastPage = activePageIndex === FIRST_LAUNCH_SETUP_PAGES.length - 1;
  const previousPage = FIRST_LAUNCH_SETUP_PAGES[Math.max(0, activePageIndex - 1)];
  const nextPage = FIRST_LAUNCH_SETUP_PAGES[Math.min(FIRST_LAUNCH_SETUP_PAGES.length - 1, activePageIndex + 1)];
  const installFirstLaunchAgentHooks = () => onInstallAgentHooks?.();
  const installMissingBundledSkills =
    ghostexCliStatus?.installed === true
      ? () => {
          if (ghostexCliStatus.browserSkillInstalled !== true) {
            onInstallBrowserUseSkill?.();
          }
          if (ghostexCliStatus.embeddedBrowserSkillInstalled !== true) {
            onInstallBrowserControl?.();
          }
          if (ghostexCliStatus.computerUseSkillInstalled !== true) {
            onInstallComputerUseSkill?.();
          }
          if (ghostexCliStatus.agentOrchestrationSkillInstalled !== true) {
            onInstallAgentOrchestrationSkill?.();
          }
          if (ghostexCliStatus.fable56OrchestrationSkillInstalled !== true) {
            onInstallFable56OrchestrationSkill?.();
          }
          if (ghostexCliStatus.findPrevSessionSkillInstalled !== true) {
            onInstallFindPrevSessionSkill?.();
          }
          if (ghostexCliStatus.generateTitleSkillInstalled !== true) {
            onInstallGenerateTitleSkill?.();
          }
          if (ghostexCliStatus.moveCodexSessionSkillInstalled !== true) {
            onInstallMoveCodexSessionSkill?.();
          }
        }
      : undefined;
  const activeContinueWarning = getFirstLaunchContinueWarning({
    activePage,
    firstLaunchBundledSkillsReady,
    firstLaunchHooksReady,
  });

  const navigateToPage = (page: FirstLaunchSetupPage) => {
    setContinueWarning(undefined);
    setActivePage(getVisibleFirstLaunchSetupPage(page));
  };

  const advance = () => {
    if (isLastPage) {
      onClose();
      return;
    }
    navigateToPage(nextPage);
  };

  const handleContinue = () => {
    if (activeContinueWarning && continueWarning !== activeContinueWarning) {
      setContinueWarning(activeContinueWarning);
      return;
    }
    advance();
  };

  useEffect(() => {
    if (continueWarning && !activeContinueWarning) {
      setContinueWarning(undefined);
    }
  }, [activeContinueWarning, continueWarning]);

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
          <div className='first-launch-setup-progress' aria-hidden='true'>
            {FIRST_LAUNCH_SETUP_PAGES.map((page) => (
              <span className='first-launch-setup-progress-dot' data-active={page === activePage} key={page} />
            ))}
          </div>
        </DialogHeader>

        <div className='first-launch-setup-body'>
          {activePage === 'video' ? (
            <FirstLaunchVideoPage
              embedUrl={tutorialVideoEmbedUrl ?? FIRST_LAUNCH_TUTORIAL_VIDEO_EMBED_URL}
              vscode={vscode}
            />
          ) : activePage === 'welcome' ? (
            <FirstLaunchWelcomePage vscode={vscode} />
          ) : activePage === 'preferences' ? (
            <FirstLaunchPreferencesPage onChange={onChange} settings={settings} />
          ) : activePage === 'hooks' ? (
            <FirstLaunchHooksPage
              agentHookStatusLoading={agentHookStatusLoading}
              hookStatusByAgentId={hookStatusByAgentId}
              hookTone={hookTone}
              onInstallAgentHooks={installFirstLaunchAgentHooks}
            />
          ) : activePage === 'cli' ? (
            <FirstLaunchCliPage
              ghostexCliStatus={ghostexCliStatus}
              ghostexCliStatusLoading={ghostexCliStatusLoading}
              vscode={vscode}
            />
          ) : activePage === 'skills' ? (
            <FirstLaunchSkillsPage
              ghostexCliStatus={ghostexCliStatus}
              ghostexCliStatusLoading={ghostexCliStatusLoading}
              onInstallAgentOrchestrationSkill={onInstallAgentOrchestrationSkill}
              onInstallCuaDriver={onInstallCuaDriver}
              onOpenExternalUrl={(url) => openFirstLaunchExternalUrl(vscode, url)}
              onInstallBrowserControl={onInstallBrowserControl}
              onInstallBrowserUseSkill={onInstallBrowserUseSkill}
              onInstallComputerUseSkill={onInstallComputerUseSkill}
              onInstallFable56OrchestrationSkill={onInstallFable56OrchestrationSkill}
              onInstallFindPrevSessionSkill={onInstallFindPrevSessionSkill}
              onInstallGenerateTitleSkill={onInstallGenerateTitleSkill}
              onInstallMoveCodexSessionSkill={onInstallMoveCodexSessionSkill}
              onUninstallBundledAgentSkill={onUninstallBundledAgentSkill}
            />
          ) : activePage === 'ready' ? (
            <FirstLaunchReadyPage />
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
          {continueWarning ? (
            <FirstLaunchContinueWarningView
              kind={continueWarning}
              onContinue={advance}
              onInstallAgentHooks={installFirstLaunchAgentHooks}
              onInstallBrowserControl={onInstallBrowserControl}
              onInstallCuaDriver={onInstallCuaDriver}
              onInstallMissingSkills={installMissingBundledSkills}
            />
          ) : null}
        </div>

        <div className='first-launch-setup-footer'>
          <div className='first-launch-setup-footer-actions' role='group' aria-label='Setup actions'>
            {activePageIndex === 0 ? null : (
              <Button onClick={() => navigateToPage(previousPage)} type='button'>
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
              <Button onClick={handleContinue} type='button'>
                Open Ghostex
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

function FirstLaunchReadyPage() {
  return (
    <section aria-labelledby='first-launch-ready-title' className='flex min-h-0 flex-1 items-center justify-center'>
      <div className='flex max-w-xl flex-col items-center gap-4 text-center'>
        <IconCircleCheckFilled aria-hidden='true' className='size-12 text-emerald-500' />
        <h2 className='first-launch-setup-title' id='first-launch-ready-title'>
          Ghostex is ready
        </h2>
        <p className='first-launch-setup-description'>
          Your terminal engine is connected, and you’ve reviewed agent hooks and bundled skills. You can change any of
          these choices later in Settings.
        </p>
      </div>
    </section>
  );
}

function FirstLaunchWelcomePage({ vscode }: { vscode?: WebviewApi }) {
  return (
    <section className='first-launch-setup-welcome' aria-labelledby='first-launch-welcome-title'>
      <div className='first-launch-setup-welcome-hero'>
        <h2 className='first-launch-setup-welcome-title' id='first-launch-welcome-title'>
          Fast Ghostty terminals, Codex inspired Features!
        </h2>
        <p className='first-launch-setup-welcome-lede'>
          Ghostex gives you real Ghostty terminals in a clean, Codex-inspired interface, so your terminals stay fast and
          reliable while the project, agent, browser, and session workflows you rely on are right there with them.
        </p>
      </div>

      <div className='first-launch-setup-welcome-grid'>
        <article className='first-launch-setup-welcome-card first-launch-setup-welcome-card-wide'>
          <div className='first-launch-setup-welcome-card-heading'>
            <span className='first-launch-setup-welcome-card-icon'>
              <IconBellRinging aria-hidden='true' size={18} />
            </span>
            <div className='first-launch-setup-welcome-card-heading-text'>
              <h3>Latest Announcement</h3>
              <span className='first-launch-setup-welcome-card-date'>2026-06-13</span>
            </div>
          </div>
          <p>
            Thanks to everyone trying Ghostex! For the next period we&apos;re focusing entirely on polish, performance,
            and bug fixes, pausing new features until the app is reliable and enjoyable.
          </p>
          <p>
            So please keep the bug reports and UX feedback coming. v4.10.0 completes the move to the gxserver engine, so
            you can now run Ghostex on another machine and manage your machines from your Mac or Android phone, and
            testing help to harden this is very welcome.
          </p>
          <p>
            To see everything that changed in each release, check out{' '}
            <a
              className='first-launch-setup-welcome-link'
              href={FIRST_LAUNCH_RELEASES_URL}
              onClick={(event) => {
                event.preventDefault();
                openFirstLaunchExternalUrl(vscode, FIRST_LAUNCH_RELEASES_URL);
              }}
              rel='noreferrer'
              target='_blank'
            >
              our changelog on GitHub
            </a>
            .
          </p>
        </article>
        <article className='first-launch-setup-welcome-card first-launch-setup-welcome-card-wide'>
          <div className='first-launch-setup-welcome-card-heading'>
            <span className='first-launch-setup-welcome-card-icon'>
              <IconUsersGroup aria-hidden='true' size={18} />
            </span>
            <h3>A note from the developer</h3>
          </div>
          <p>
            Ghostex is built by one developer. I&apos;d be really grateful if you joined Discord to chit chat, support,
            ask questions, report rough edges, or contribute.
            <br />
            Please cut me a little slack if you hit any issues 😅. I&apos;m doing my very best to make this app super
            stable for all of us to depend on it. <br />
            The app is mostly stable as I use nothing but Ghostex to build Ghostex.
          </p>
        </article>
        <article className='first-launch-setup-welcome-card'>
          <div className='first-launch-setup-welcome-card-heading'>
            <span className='first-launch-setup-welcome-card-icon'>
              <IconTerminal2 aria-hidden='true' size={18} />
            </span>
            <h3>Why Ghostty terminals?</h3>
          </div>
          <p>
            Ghostty terminals are much lighter on RAM than web-based terminals and render agent CLIs cleanly. Ghostex is
            harder to build due to this requirement, but it's a must to get the best experience. Don't settle for
            mediocrity.
          </p>
        </article>
        <article className='first-launch-setup-welcome-card'>
          <div className='first-launch-setup-welcome-card-heading'>
            <span className='first-launch-setup-welcome-card-icon'>
              <IconWorld aria-hidden='true' size={18} />
            </span>
            <h3>Why Chromium browser panes?</h3>
          </div>
          <p>
            Ghostex uses Chromium instead of Safari&apos;s engine because Chrome DevTools are better for agent
            debugging, and Chromium gives you a closer preview of what most web users will see in Chrome-family
            browsers.
          </p>
        </article>
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

function FirstLaunchHooksPage({
  agentHookStatusLoading,
  hookStatusByAgentId,
  hookTone,
  onInstallAgentHooks,
}: {
  agentHookStatusLoading: boolean;
  hookStatusByAgentId: ReadonlyMap<string, SidebarAgentHookStatusItem>;
  hookTone: SidebarAgentHookStatus | 'checking' | 'unknown';
  onInstallAgentHooks?: () => void;
}) {
  const hasUpdateRequiredHooks = [...hookStatusByAgentId.values()].some((status) => status.status === 'updateRequired');
  return (
    <>
      <div className='first-launch-setup-main'>
        <section aria-labelledby='first-launch-intro-title' className='first-launch-setup-intro'>
          <h2 className='first-launch-setup-title' id='first-launch-intro-title'>
            Let's get Ghostex integrated with your agents!
          </h2>
          <p className='first-launch-setup-description'>Install the required hooks so that these features work:</p>
        </section>

        <div className='first-launch-setup-primary'>
          <ul className='first-launch-setup-benefits' aria-label='Ghostex highlights'>
            {FIRST_LAUNCH_INTRO_BENEFITS.map((benefit) => {
              const BenefitIcon = benefit.icon;
              return (
                <li className='first-launch-setup-benefit' key={benefit.title}>
                  <span className='first-launch-setup-benefit-icon'>
                    <BenefitIcon aria-hidden='true' size={16} />
                  </span>
                  <span className='first-launch-setup-benefit-copy'>
                    <span className='first-launch-setup-benefit-title'>{benefit.title}</span>
                    <span className='first-launch-setup-benefit-text'>{benefit.text}</span>
                  </span>
                </li>
              );
            })}
          </ul>
        </div>

        <aside className='first-launch-setup-visual'>
          <div className='first-launch-setup-art-shell'>
            <img
              alt='Ghostex workspace preview with agent session cards, terminal panes, and status indicators'
              className='first-launch-setup-art'
              src={ghostexIntroImage}
            />
          </div>
        </aside>
      </div>

      <section aria-label='Agent hook installation status' className='first-launch-setup-hooks' data-tone={hookTone}>
        <div className='first-launch-setup-hooks-panel'>
          {/*
           * CDXC:FirstLaunchSetup 2026-05-26-06:46
           * First launch hook setup must show the real supported agent names,
           * not only a readiness count, because users need to understand which
           * CLI configs Ghostex will inspect or install before accepting setup.
           * The supported set matches native hook installation for all default agents.
           *
           * CDXC:FirstLaunchSetup 2026-05-26-07:14:
           * Group agents under Installed / Needs update / Not installed / CLI missing headers so
           * status words live in section titles instead of repeating inside each chip.
           *
           * CDXC:AgentHooks 2026-06-07-11:05:
           * Old Ghostex hooks are update-required, not absent. First launch should
           * show the migration state and let Install Hooks act as an idempotent
           * update action, because gxserver is the source of truth for hook status.
           *
           * CDXC:FirstLaunchSetup 2026-05-26-07:22:
           * The grouped agent headers are the only visible readiness count on this
           * page, keeping the hook panel from repeating a separate "4/15 ready"
           * summary above the cards.
           */}
          <div className='first-launch-setup-hook-groups' aria-label='Agent hook status'>
            {getFirstLaunchHookStatusGroups(hookStatusByAgentId).map((group) => (
              <section className='first-launch-setup-hook-group' key={group.id}>
                <div className='first-launch-setup-hook-group-title'>{group.title}</div>
                <ul className='first-launch-setup-hook-grid'>
                  {group.agents.map((agent) => (
                    <li key={agent.agentId}>
                      <FirstLaunchHookAgentStatus
                        agentName={agent.name}
                        groupId={group.id}
                        isLoading={agentHookStatusLoading && hookStatusByAgentId.size === 0}
                        status={hookStatusByAgentId.get(agent.agentId)}
                      />
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </div>
          <div className='first-launch-setup-hooks-actions'>
            <Button
              disabled={!onInstallAgentHooks || agentHookStatusLoading}
              onClick={onInstallAgentHooks}
              type='button'
              variant='outline'
            >
              <IconDownload aria-hidden='true' data-icon='inline-start' />
              {hasUpdateRequiredHooks ? 'Update Hooks' : 'Install Hooks'}
            </Button>
          </div>
        </div>
      </section>
    </>
  );
}

function FirstLaunchContinueWarningView({
  kind,
  onContinue,
  onInstallAgentHooks,
  onInstallBrowserControl,
  onInstallCuaDriver,
  onInstallMissingSkills,
}: {
  kind: FirstLaunchContinueWarning;
  onContinue: () => void;
  onInstallAgentHooks?: () => void;
  onInstallBrowserControl?: () => void;
  onInstallCuaDriver?: () => void;
  onInstallMissingSkills?: () => void;
}) {
  const warning = FIRST_LAUNCH_CONTINUE_WARNINGS[kind];
  const installAction =
    kind === 'hooks'
      ? onInstallAgentHooks
      : kind === 'skills'
        ? onInstallMissingSkills
        : kind === 'browserControl'
          ? onInstallBrowserControl
          : onInstallCuaDriver;

  return (
    <div className='first-launch-setup-warning-backdrop'>
      <section aria-modal='true' className='first-launch-setup-warning' role='alertdialog'>
        <div className='first-launch-setup-warning-icon'>
          <IconAlertTriangle aria-hidden='true' size={18} />
        </div>
        <div className='first-launch-setup-warning-copy'>
          <h3>{warning.title}</h3>
          <p>{warning.description}</p>
          <div className='first-launch-setup-warning-actions'>
            <Button onClick={onContinue} type='button' variant='ghost'>
              {warning.actionLabel}
            </Button>
            <Button disabled={!installAction} onClick={installAction} type='button'>
              <IconDownload aria-hidden='true' data-icon='inline-start' />
              {warning.installLabel}
            </Button>
          </div>
        </div>
      </section>
    </div>
  );
}

/*
 * CDXC:FirstLaunchTutorialVideo 2026-08-19:
 * First page of the one first-run modal. The player is a plain iframe so this
 * page owns no playback state: whatever the host hands us in `embedUrl` is a
 * document that is allowed to embed YouTube from its own origin.
 */
function FirstLaunchVideoPage({ embedUrl, vscode }: { embedUrl: string; vscode?: WebviewApi }) {
  return (
    <section className='first-launch-setup-guide-page' aria-labelledby='first-launch-video-title'>
      <div className='first-launch-setup-guide-hero'>
        <span className='first-launch-setup-guide-icon-shell'>
          <IconBrandYoutube aria-hidden='true' className='first-launch-setup-guide-icon' size={26} />
        </span>
        <div className='first-launch-setup-guide-copy'>
          <div className='first-launch-setup-kicker'>Two minute tour</div>
          <h2 className='first-launch-setup-title' id='first-launch-video-title'>
            See what Ghostex can do.
          </h2>
          <p className='first-launch-setup-description'>
            A quick look at terminals, agents, and the workflows the next few pages set up for you.
          </p>
        </div>
      </div>
      <div className='first-launch-setup-video-shell'>
        <iframe
          allow='accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share'
          allowFullScreen
          className='first-launch-setup-video'
          src={embedUrl}
          title='Ghostex introduction and highlighted features'
        />
      </div>
      <p className='first-launch-setup-description'>
        Prefer YouTube?{' '}
        <a
          href={FIRST_LAUNCH_TUTORIAL_VIDEO_WATCH_URL}
          onClick={(event) => {
            if (!vscode) {
              return;
            }
            event.preventDefault();
            openFirstLaunchExternalUrl(vscode, FIRST_LAUNCH_TUTORIAL_VIDEO_WATCH_URL);
          }}
          rel='noreferrer'
          target='_blank'
        >
          Open the video in your browser
        </a>
        .
      </p>
    </section>
  );
}

function FirstLaunchSkillsPage({
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onInstallAgentOrchestrationSkill,
  onInstallBrowserControl,
  onInstallBrowserUseSkill,
  onInstallComputerUseSkill,
  onInstallCuaDriver,
  onInstallFable56OrchestrationSkill,
  onInstallFindPrevSessionSkill,
  onInstallGenerateTitleSkill,
  onInstallMoveCodexSessionSkill,
  onOpenExternalUrl,
  onUninstallBundledAgentSkill,
}: {
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onInstallAgentOrchestrationSkill?: () => void;
  onInstallCuaDriver?: () => void;
  onOpenExternalUrl?: (url: string) => void;
  onInstallBrowserControl?: () => void;
  onInstallBrowserUseSkill?: () => void;
  onInstallComputerUseSkill?: () => void;
  onInstallFable56OrchestrationSkill?: () => void;
  onInstallFindPrevSessionSkill?: () => void;
  onInstallGenerateTitleSkill?: () => void;
  onInstallMoveCodexSessionSkill?: () => void;
  onUninstallBundledAgentSkill?: (skillId: BundledGhostexAgentSkillId) => void;
}) {
  return (
    <section className='first-launch-setup-guide-page' aria-labelledby='first-launch-skills-title'>
      <div className='first-launch-setup-guide-hero'>
        <span className='first-launch-setup-guide-icon-shell'>
          <IconSparkles aria-hidden='true' className='first-launch-setup-guide-icon' size={26} />
        </span>
        <div className='first-launch-setup-guide-copy'>
          <div className='first-launch-setup-kicker'>Bundled Agent Skills</div>
          <h2 className='first-launch-setup-title' id='first-launch-skills-title'>
            Install the skills you want agents to use.
          </h2>
          <p className='first-launch-setup-description'>
            Ghostex bundles these skills with the app, but each one is installed into your shared agent skills folder
            only after you choose it here.
          </p>
        </div>
      </div>
      <BundledAgentSkillsPanel
        ghostexCliStatus={ghostexCliStatus}
        ghostexCliStatusLoading={ghostexCliStatusLoading}
        onInstallCuaDriver={onInstallCuaDriver}
        onOpenExternalUrl={onOpenExternalUrl}
        onInstallSkill={{
          agentOrchestration: onInstallAgentOrchestrationSkill,
          browserUse: onInstallBrowserUseSkill,
          computerUse: onInstallComputerUseSkill,
          embeddedBrowserUse: onInstallBrowserControl,
          fable56Orchestration: onInstallFable56OrchestrationSkill,
          findPrevSession: onInstallFindPrevSessionSkill,
          generateTitle: onInstallGenerateTitleSkill,
          moveCodexSession: onInstallMoveCodexSessionSkill,
        }}
        onUninstallSkill={onUninstallBundledAgentSkill}
        showHeader={false}
      />
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

function FirstLaunchHookAgentStatus({
  agentName,
  groupId,
  isLoading,
  status,
}: {
  agentName: string;
  groupId: FirstLaunchHookStatusGroupId;
  isLoading: boolean;
  status?: SidebarAgentHookStatusItem;
}) {
  return (
    <div
      className={cn(
        'first-launch-setup-hook-agent',
        getFirstLaunchAgentHookStatusClassName(groupId, status, isLoading)
      )}
    >
      {getFirstLaunchAgentHookStatusIcon(groupId, status, isLoading)}
      <span className='first-launch-setup-hook-agent-name'>{agentName}</span>
    </div>
  );
}

function getFirstLaunchHookStatusGroups(
  hookStatusByAgentId: ReadonlyMap<string, SidebarAgentHookStatusItem>
): FirstLaunchHookStatusGroup[] {
  const groups: FirstLaunchHookStatusGroup[] = [
    { agents: [], id: 'installed', title: 'Agent Hooks Installed' },
    { agents: [], id: 'updateRequired', title: 'Agent Hooks Need Update' },
    { agents: [], id: 'missing', title: 'Agent Hooks Not Installed' },
    { agents: [], id: 'cliMissing', title: 'Agent CLI Missing' },
    { agents: [], id: 'unknown', title: 'Agent Hooks Not Checked' },
  ];
  const groupById = new Map(groups.map((group) => [group.id, group]));

  for (const agent of FIRST_LAUNCH_HOOK_SUPPORTED_AGENTS) {
    const status = hookStatusByAgentId.get(agent.agentId);
    const groupId =
      status?.status === 'installed' || status?.status === 'notRequired'
        ? 'installed'
        : status?.status === 'updateRequired'
          ? 'updateRequired'
          : status?.status === 'missing'
            ? 'missing'
            : status?.status === 'cliMissing'
              ? 'cliMissing'
              : 'unknown';
    groupById.get(groupId)?.agents.push(agent);
  }

  return groups.filter((group) => group.agents.length > 0);
}

function getFirstLaunchContinueWarning({
  activePage,
  firstLaunchBundledSkillsReady,
  firstLaunchHooksReady,
}: {
  activePage: FirstLaunchSetupPage;
  firstLaunchBundledSkillsReady: boolean;
  firstLaunchHooksReady: boolean;
}): FirstLaunchContinueWarning | undefined {
  if (activePage === 'hooks' && !firstLaunchHooksReady) {
    return 'hooks';
  }
  if (activePage === 'skills' && !firstLaunchBundledSkillsReady) {
    return 'skills';
  }
  return undefined;
}

function areFirstLaunchAgentHooksReady(agentHookStatus: SidebarAgentHookStatusMessage | undefined): boolean {
  if (!agentHookStatus || agentHookStatus.errorMessage) {
    return false;
  }
  const statusByAgentId = new Map(agentHookStatus.agents.map((status) => [status.agentId, status]));
  /*
   * CDXC:FirstLaunchSetup 2026-06-19-08:42:
   * The continue warning should only protect users who have no usable
   * first-launch hook coverage. If Claude, Codex, OpenCode, or Pi already has a
   * current Ghostex hook, continuing should not show the warning just because
   * secondary providers are missing.
   */
  return FIRST_LAUNCH_HOOK_SKIP_WARNING_AGENT_IDS.some((agentId) =>
    isFirstLaunchAgentHookReadyStatus(statusByAgentId.get(agentId)?.status)
  );
}

/*
 * CDXC:FirstLaunchSetup 2026-08-19:
 * The skip warning used to demand all eight bundled skills, so anyone who
 * deliberately installed just the ones they wanted still got a full-page
 * warning overlay on the way out. It now only protects users who installed
 * nothing at all, matching how the hooks page treats partial coverage.
 */
function isAnyFirstLaunchBundledSkillInstalled(ghostexCliStatus: SidebarGhostexCliStatusMessage | undefined): boolean {
  return (
    ghostexCliStatus?.browserSkillInstalled === true ||
    ghostexCliStatus?.embeddedBrowserSkillInstalled === true ||
    ghostexCliStatus?.computerUseSkillInstalled === true ||
    ghostexCliStatus?.agentOrchestrationSkillInstalled === true ||
    ghostexCliStatus?.fable56OrchestrationSkillInstalled === true ||
    ghostexCliStatus?.findPrevSessionSkillInstalled === true ||
    ghostexCliStatus?.generateTitleSkillInstalled === true ||
    ghostexCliStatus?.moveCodexSessionSkillInstalled === true
  );
}

function isFirstLaunchAgentHookReadyStatus(status: SidebarAgentHookStatus | undefined): boolean {
  return status === 'installed' || status === 'notRequired';
}

function getFirstLaunchHookTone(
  agentHookStatus: SidebarAgentHookStatusMessage | undefined,
  isLoading: boolean
): SidebarAgentHookStatus | 'checking' | 'unknown' {
  if (agentHookStatus?.errorMessage) {
    return 'missing';
  }
  if (isLoading) {
    return 'checking';
  }
  if (!agentHookStatus) {
    return 'unknown';
  }
  return agentHookStatus.agents.every((agent) => agent.status === 'installed' || agent.status === 'notRequired')
    ? 'installed'
    : agentHookStatus.agents.some((agent) => agent.status === 'updateRequired')
      ? 'updateRequired'
      : 'missing';
}

function getSidebarThemeVariant(theme: SidebarTheme): 'dark' | 'light' {
  return theme.startsWith('light-') || theme === 'plain-light' ? 'light' : 'dark';
}

function getFirstLaunchAgentHookStatusIcon(
  groupId: FirstLaunchHookStatusGroupId,
  status: SidebarAgentHookStatusItem | undefined,
  isLoading: boolean
) {
  if (isLoading) {
    return <IconRefresh aria-hidden='true' className='first-launch-setup-hook-agent-icon' />;
  }
  if (!status) {
    return <IconInfoCircle aria-hidden='true' className='first-launch-setup-hook-agent-icon' />;
  }
  switch (groupId) {
    case 'installed':
      return <IconCircleCheckFilled aria-hidden='true' className='first-launch-setup-hook-agent-icon' />;
    case 'updateRequired':
      return <IconAlertTriangle aria-hidden='true' className='first-launch-setup-hook-agent-icon' />;
    case 'cliMissing':
      return <IconAlertTriangle aria-hidden='true' className='first-launch-setup-hook-agent-icon' />;
    case 'missing':
      return <IconCircleX aria-hidden='true' className='first-launch-setup-hook-agent-icon' />;
    case 'unknown':
      return <IconInfoCircle aria-hidden='true' className='first-launch-setup-hook-agent-icon' />;
  }
}

function getFirstLaunchAgentHookStatusClassName(
  groupId: FirstLaunchHookStatusGroupId,
  status: SidebarAgentHookStatusItem | undefined,
  isLoading: boolean
): string {
  if (isLoading || !status) {
    return 'first-launch-setup-hook-agent-unknown';
  }
  switch (groupId) {
    case 'installed':
      return 'first-launch-setup-hook-agent-installed';
    case 'updateRequired':
      return 'first-launch-setup-hook-agent-update-required';
    case 'cliMissing':
      return 'first-launch-setup-hook-agent-cli-missing';
    case 'missing':
      return 'first-launch-setup-hook-agent-missing';
    case 'unknown':
      return 'first-launch-setup-hook-agent-unknown';
  }
}

export type { FirstLaunchSetupMainSettingKey };
