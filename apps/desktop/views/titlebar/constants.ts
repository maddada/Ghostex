import type { TitlebarDropdownPanelKind, TitlebarDropdownPanelSize } from './types';

/*
 * CDXC:TipsAndTricks 2026-06-16-19:42:
 * The Tips & Tricks header needs a Changelog action that opens the full Ghostex GitHub releases page as an in-project browser session, keeping release history inside the current workspace instead of the system browser.
 *
 * CDXC:TipsAndTricks 2026-06-18-04:53:
 * The tips panel header should not repeat the Tips & Tricks label in text. Expose Docs as a first-row action and keep documentation inside the current workspace browser session.
 */
export const GHOSTEX_CHANGELOG_URL = 'https://github.com/maddada/ghostex/releases';
export const GHOSTEX_DOCS_URL = 'https://ghostex.dev/docs';
export { GHOSTEX_DISCORD_URL } from '@/packages/shared/sidebar-commands';
export const TITLEBAR_GRADIENT_BLEND_START_PERCENT = 40;
export const DEFAULT_CODE_SERVER_RESOURCE_PORT = 3775;

export function codeServerResourcePort(): number {
  const port = window.__ghostex_NATIVE_HOST__?.codeServerRuntime?.port;
  return typeof port === 'number' && Number.isInteger(port) && port > 0 ? port : DEFAULT_CODE_SERVER_RESOURCE_PORT;
}

export const LAST_OPEN_TARGET_STORAGE_KEY = 'ghostex.titlebar.lastOpenTargetId';
export const LAST_ACTION_COMMAND_STORAGE_PREFIX = 'ghostex.titlebar.lastActionCommandByProject:';
export const KEEP_AWAKE_RUNTIME_STORAGE_KEY = 'ghostex.titlebar.keepAwakeRuntime';
export const KEEP_AWAKE_RUNTIME_SYNC_STORAGE_KEY = 'ghostex.titlebar.keepAwakeRuntimeSync';
export const KEEP_AWAKE_RUNTIME_CHANGED_EVENT = 'ghostex:titlebar-keep-awake-runtime-changed';
export const KEEP_AWAKE_LID_SLEEP_STORAGE_KEY = 'ghostex.titlebar.lidSleepPrevention';
export const TITLEBAR_GIT_STATE_CACHE_STORAGE_PREFIX = 'ghostex.titlebar.gitState.';
export const TITLEBAR_TIPS_READ_STORAGE_KEY = 'ghostex.titlebar.tips.readIds';
export const KEEP_AWAKE_POWER_CHECK_INTERVAL_MS = 30_000;
export const KEEP_AWAKE_WORKING_SESSION_GRACE_MS = 20 * 60_000;
export const KEEP_AWAKE_ADMIN_PROCESS_TIMEOUT_MS = 120_000;
export const RESOURCE_POLL_INTERVAL_MS = 5_000;
export const TITLEBAR_EVENT_LOOP_WATCHDOG_INTERVAL_MS = 2_000;
export const TITLEBAR_EVENT_LOOP_STALL_THRESHOLD_MS = 1_000;
export const TITLEBAR_EVENT_LOOP_STALL_LOG_THROTTLE_MS = 10_000;
/**
 * CDXC:ReactTitlebar 2026-06-11-13:22:
 * The titlebar document uses native child-window dropdown panels instead of
 * Radix portals in the full-window WKWebView, so the workspace never sits under
 * a titlebar-owned overlay during editor drag/drop.
 *
 * CDXC:ReactTitlebar 2026-06-11-15:58:
 * Native titlebar dropdown panels must load the real titlebar-host.html file URL
 * without query parameters. Swift injects the panel kind at document start so
 * WebKit does not treat a synthetic local-file URL as the document resource.
 */
export const TITLEBAR_PANEL_QUERY_PARAM = 'ghostexTitlebarPanel';
export const TITLEBAR_DROPDOWN_RESOURCES_PANEL_WIDTH = 656;
/**
 * CDXC:TipsAndTricks 2026-06-12-08:56:
 * The macOS Tips & Tricks child panel should be 100px narrower than the shared
 * Resources reading panel while preserving the always-expanded section layout.
 */
export const TITLEBAR_DROPDOWN_TIPS_PANEL_WIDTH = 556;
export const TITLEBAR_DROPDOWN_READING_PANEL_HEIGHT = 650;

export function readTitlebarDropdownPanelKind(): TitlebarDropdownPanelKind | undefined {
  const injectedKind =
    typeof window.__ghostex_TITLEBAR_PANEL_KIND__ === 'string' ? window.__ghostex_TITLEBAR_PANEL_KIND__ : undefined;
  const rawKind = injectedKind ?? new URLSearchParams(window.location.search).get(TITLEBAR_PANEL_QUERY_PARAM);
  if (rawKind === 'resources' || rawKind === 'tips') {
    return rawKind;
  }
  return undefined;
}

export function createTitlebarDropdownPanelPreferredSize(kind: TitlebarDropdownPanelKind): TitlebarDropdownPanelSize {
  /*
   * CDXC:ReactTitlebar 2026-06-12-02:50:
   * Compact native titlebar dropdown panels must be sized from the number and
   * type of rendered options before AppKit creates the child window. This keeps
   * short menus from clipping rows below the fold without reintroducing
   * post-open WebKit measurement feedback.
   */
  switch (kind) {
    case 'resources':
      return {
        height: TITLEBAR_DROPDOWN_READING_PANEL_HEIGHT,
        width: TITLEBAR_DROPDOWN_RESOURCES_PANEL_WIDTH,
      };
    case 'tips':
      return {
        height: TITLEBAR_DROPDOWN_READING_PANEL_HEIGHT,
        width: TITLEBAR_DROPDOWN_TIPS_PANEL_WIDTH,
      };
  }
}

/**
 * CDXC:ReactTitlebar 2026-06-11-17:16:
 * Native dropdown child windows reuse this titlebar bundle, but their document
 * must avoid inheriting the normal full-width titlebar viewport. Read the panel
 * kind once before React mounts so document, body, and root sizing can be set
 * before WebKit lays out content.
 *
 * CDXC:ReactTitlebar 2026-06-11-17:27:
 * Dynamic measurement still allowed WebKit/AppKit feedback to shrink panels
 * after opening. Native titlebar dropdowns now use fixed child-window sizes, so
 * panel documents fill the WebView and dropdown content scrolls internally.
 */
export const initialTitlebarDropdownPanelKind = readTitlebarDropdownPanelKind();
