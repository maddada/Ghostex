/*
 * Window chrome per modal kind.
 *
 * Titles and sizes mirror `GpuiAppModalKind::window_title` (apps/desktop/src/app/model/app_modal_kind.rs)
 * and `GpuiAppModalKind::window_size` (apps/desktop/src/app/model/app_modal_kind.rs) including the raw
 * pixel constants alongside it.
 *
 * `height: "fit"` marks the one-shot fit-height modals: the React host measures
 * its rendered dialog once and posts `contentHeightMeasured`, which the real app
 * clamps to 200..850 before resizing the child window
 * (apps/desktop/src/app/window/modal_host.rs:159 GpuiAppModalHost::receive_bridge_message). The table of
 * fit-height kinds is apps/desktop/views/modal-host.tsx:152
 * ONE_SHOT_NATIVE_FIT_HEIGHT_MODAL_SELECTORS.
 */
import type { SandboxModalKind } from '../state/types';

export interface SandboxModalChrome {
  title: string;
  width: number;
  height: number | 'fit';
  /** Fallback height used by the frame until `contentHeightMeasured` arrives. */
  initialHeight: number;
  /** Mirrors `GpuiAppModalKind::requires_sidebar_state` (apps/desktop/src/app/model/app_modal_kind.rs). */
  requiresSidebarState: boolean;
  /**
   * Set when `GpuiAppModalKind::uses_react_modal_host` is false
   * (apps/desktop/src/app/model/app_modal_kind.rs — only `WatchGhostexVideo`): the native child
   * window loads this URL as its top-level document instead of the modal-host
   * bundle, so there is no bridge, no hydrate, and no ready/presented
   * handshake (`is_ready: !uses_react_modal_host`, apps/desktop/src/app/window/modal_host.rs:159).
   */
  nonReactHostUrl?: string;
}

export const APP_MODAL_HOST_FIT_CONTENT_MIN_WINDOW_HEIGHT = 200;
export const APP_MODAL_HOST_FIT_CONTENT_MAX_WINDOW_HEIGHT = 850;

const QUICK_ACCESS_WIDTH = 654 + 19 + 19;
const QUICK_ACCESS_HEIGHT = 680;
const SETTINGS_WIDTH = 1080;
const SETTINGS_HEIGHT = 760;
const COMPACT_WIDTH = 760;

/** Kinds that hydrate the sidebar store before opening. */
const REQUIRES_SIDEBAR_STATE: ReadonlySet<SandboxModalKind> = new Set<SandboxModalKind>([
  'settings',
  'hotkeys',
  'configureAgents',
  'configureActions',
  'openTargets',
  'firstLaunchSetup',
  'agentsHub',
  'delayedSend',
  'renameSession',
  'worktree',
  'deleteWorktree',
  'gitCommit',
  'gitFileDiff',
  'portlessSetup',
  'discoverGhostex',
  /*
   * `tipsAndTricks` has no GpuiAppModalKind counterpart (gpui never opens it),
   * but the React host gates it behind the same hydrated store as
   * firstLaunchSetup, so the gallery must hydrate it to see anything.
   */
  'tipsAndTricks',
]);

/**
 * `GHOSTEX_TUTORIAL_VIDEO_URL` (apps/desktop/src/app/consts.rs:665) is
 * `https://www.youtube.com/watch?v=APdP-j5n4Mw` — the real watch page, NOT the
 * embed player: YouTube refuses the embed when it is framed from the file://
 * modal-host document (CDXC:Onboarding). The sandbox serves that exact
 * page back through the dev server's `/yt` reverse proxy (yt-proxy.ts), which
 * strips `x-frame-options`/CSP so the same document can render inside the fake
 * native window and stay same-origin for the simulated `f` key press.
 */
export const SANDBOX_TUTORIAL_VIDEO_URL = '/yt/watch?v=APdP-j5n4Mw';

type ChromeSpec = {
  title: string;
  width: number;
  height: number | 'fit';
  initialHeight?: number;
  nonReactHostUrl?: string;
};

const CHROME: Record<SandboxModalKind, ChromeSpec> = {
  addProject: { title: 'Ghostex Add Project', width: 640, height: 460 },
  addRepository: { title: 'Ghostex Clone Repository', width: 640, height: 'fit', initialHeight: 660 },
  agentConfig: { title: 'Ghostex Agent', width: COMPACT_WIDTH, height: 'fit', initialHeight: 600 },
  agentsHub: { title: 'Ghostex Agents Hub', width: SETTINGS_WIDTH, height: SETTINGS_HEIGHT },
  commandPalette: { title: 'Ghostex Quick Access', width: QUICK_ACCESS_WIDTH, height: QUICK_ACCESS_HEIGHT },
  configureActions: { title: 'Ghostex Actions', width: SETTINGS_WIDTH, height: SETTINGS_HEIGHT },
  configureAgents: { title: 'Ghostex Configure Agents', width: SETTINGS_WIDTH, height: SETTINGS_HEIGHT },
  delayedSend: { title: 'Ghostex Session Automations', width: 470, height: 'fit', initialHeight: 565 },
  deleteWorktree: { title: 'Ghostex Delete Worktree', width: COMPACT_WIDTH, height: 'fit', initialHeight: 600 },
  discoverGhostex: { title: 'Discover Ghostex', width: 1120, height: 850 },
  firstLaunchSetup: { title: 'Welcome to Ghostex', width: 1120, height: 850 },
  firstUserMessage: { title: 'Ghostex First Message', width: COMPACT_WIDTH, height: 'fit', initialHeight: 520 },
  gitCommit: { title: 'Ghostex Commit Changes', width: 1078, height: 758 },
  gitFileDiff: { title: 'Ghostex File Diff', width: SETTINGS_WIDTH, height: SETTINGS_HEIGHT },
  hotkeys: { title: 'Ghostex Hotkeys', width: SETTINGS_WIDTH, height: SETTINGS_HEIGHT },
  missingProjectFolder: {
    title: 'Ghostex Project Folder Missing',
    width: 560,
    height: 'fit',
    initialHeight: 360,
  },
  openTargets: { title: 'Ghostex Open Targets', width: SETTINGS_WIDTH, height: SETTINGS_HEIGHT },
  portlessSetup: { title: 'Ghostex Portless Setup', width: 640, height: 'fit', initialHeight: 340 },
  previousSessions: {
    title: 'Ghostex Quick Access',
    width: QUICK_ACCESS_WIDTH,
    height: QUICK_ACCESS_HEIGHT,
  },
  recentProjects: {
    title: 'Ghostex Quick Access',
    width: QUICK_ACCESS_WIDTH,
    height: QUICK_ACCESS_HEIGHT,
  },
  remoteGxserverInstall: {
    title: 'Ghostex Remote Setup',
    width: 560,
    height: 'fit',
    initialHeight: 380,
  },
  remoteProjectPicker: {
    title: 'Ghostex Remote Project',
    width: 720,
    height: 'fit',
    initialHeight: 640,
  },
  renameSession: { title: 'Ghostex Rename Session', width: 570, height: 'fit', initialHeight: 440 },
  settings: { title: 'Ghostex Settings', width: SETTINGS_WIDTH, height: SETTINGS_HEIGHT },
  stashedPrompts: {
    title: 'Ghostex Quick Access',
    width: QUICK_ACCESS_WIDTH,
    height: QUICK_ACCESS_HEIGHT,
  },
  tipsAndTricks: { title: 'Welcome to Ghostex', width: 1120, height: 850 },
  updateAvailable: { title: 'Ghostex Update', width: 640, height: 'fit', initialHeight: 560 },
  watchGhostexVideo: {
    title: 'Ghostex Tutorial Video',
    width: 1120,
    height: 750,
    nonReactHostUrl: SANDBOX_TUTORIAL_VIDEO_URL,
  },
  worktree: { title: 'Ghostex Add Worktree', width: 640, height: 'fit', initialHeight: 640 },
};

export function modalChrome(modal: SandboxModalKind): SandboxModalChrome {
  const spec = CHROME[modal];
  return {
    title: spec.title,
    width: spec.width,
    height: spec.height,
    initialHeight: spec.initialHeight ?? (spec.height === 'fit' ? 480 : spec.height),
    requiresSidebarState: REQUIRES_SIDEBAR_STATE.has(modal),
    ...(spec.nonReactHostUrl ? { nonReactHostUrl: spec.nonReactHostUrl } : {}),
  };
}

/**
 * `GpuiAppModalKind::open_message` (apps/desktop/src/app/model/app_modal_kind.rs). Most kinds carry only
 * `{modal, type}`; the few that need extra scaffolding get it here so the
 * gallery can force-open them into a renderable state.
 */
/*
 * The app serves this player page from its own synthetic https origin because
 * the modal host document is a file:// URL, where YouTube's embed player
 * answers "Error 153 - Video player configuration error". The sandbox is
 * already on a real origin, but it serves the same wrapper page so the modal
 * exercises the identical host page -> player page -> YouTube chain.
 */
export const SANDBOX_TUTORIAL_VIDEO_EMBED_URL = '/tutorial-video-player.html';

export function modalOpenPayload(modal: SandboxModalKind): Record<string, unknown> {
  switch (modal) {
    case 'commandPalette':
      return { initialQuery: '' };
    case 'firstLaunchSetup':
      return { tutorialVideoEmbedUrl: SANDBOX_TUTORIAL_VIDEO_EMBED_URL };
    case 'remoteGxserverInstall':
    case 'remoteProjectPicker':
      return { remoteMachineId: '', remoteMachineName: 'Remote' };
    case 'portlessSetup':
      return { mode: 'install', protocol: 'https' };
    case 'renameSession':
      return { initialTitle: '', sessionId: 'sandbox-session-1' };
    case 'updateAvailable':
      return { state: 'available', version: '7.11.1' };
    case 'missingProjectFolder':
      return {
        projectId: 'sandbox-project-1',
        projectName: 'ghostex',
        projectPath: '/Users/story/dev/ghostex',
      };
    default:
      return {};
  }
}

/** Gallery grouping for the control panel's force-open buttons. */
export const MODAL_GALLERY_GROUPS: ReadonlyArray<{
  id: string;
  label: string;
  modals: readonly SandboxModalKind[];
}> = [
  {
    id: 'onboarding',
    label: 'Onboarding',
    modals: ['watchGhostexVideo', 'firstLaunchSetup', 'discoverGhostex', 'tipsAndTricks'],
  },
  {
    id: 'project',
    label: 'Projects',
    modals: [
      'addProject',
      'addRepository',
      'missingProjectFolder',
      'remoteProjectPicker',
      'remoteGxserverInstall',
      'worktree',
      'deleteWorktree',
      'gitCommit',
      'gitFileDiff',
    ],
  },
  {
    id: 'settings',
    label: 'Settings',
    modals: ['settings', 'hotkeys', 'configureAgents', 'configureActions', 'openTargets', 'agentsHub', 'portlessSetup'],
  },
  {
    id: 'misc',
    label: 'Quick access & misc',
    modals: [
      'commandPalette',
      'previousSessions',
      'recentProjects',
      'stashedPrompts',
      'delayedSend',
      'renameSession',
      'agentConfig',
      'firstUserMessage',
      'updateAvailable',
    ],
  },
];
