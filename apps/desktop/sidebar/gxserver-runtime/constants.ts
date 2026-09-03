/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import type { GxserverGitAction } from '@/packages/shared/gxserver-protocol';
import type { SidebarRemoteMachineStatusMessage, SidebarTheme } from '@/packages/shared/session-grid-contract';
import type { SidebarGitAction } from '@/packages/shared/sidebar-git';

export const GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS = 20;
export const GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS = 250;
/*
CDXC:StateSync 2026-09-01:
Presentation-stream recovery used to run with no delay at all, so a daemon that
was restarting, or a socket the OS kept refusing, turned every `onClose` /
`onError` into an immediate full snapshot fetch plus three more RPCs — a tight
loop against a server that was already struggling. These are the desktop
counterpart of the web client's `RECONNECT_DELAYS_MS`, with a deliberately fast
first step so the ordinary single drop still reconnects in a quarter second.
*/
export const GPUI_PRESENTATION_STREAM_RECOVERY_DELAYS_MS = [250, 1_000, 2_000, 4_000, 8_000, 16_000] as const;
/*
How long a stream has to stay acknowledged before its next drop counts as a new
incident rather than a continuation of the current one. Resetting on the socket
merely opening would defeat the escalation, because a flapping socket does open
every time; the daemon's subscribe acknowledgement holding for this long is the
signal that the connection was actually usable.
*/
export const GPUI_PRESENTATION_STREAM_HEALTHY_MS = 30 * 1000;
/*
CDXC:StateSync 2026-09-01:
How long one stale-revision snapshot refetch suppresses the next for the same
remote machine. Stale deltas arrive in bursts — one per changed row — and each
refetch is a full presentation read across the SSH tunnel.
*/
export const GPUI_STALE_REMOTE_PRESENTATION_REFRESH_COOLDOWN_MS = 3 * 1000;
export const GPUI_AUTO_SLEEP_MONITOR_INTERVAL_MS = 60 * 1000;
export const GPUI_PROJECT_DIFF_STATS_BACKGROUND_INTERVAL_MS = 15 * 1000;
/*
CDXC:Git 2026-08-16:
Large sidebars (100+ project rows across local and remote machines) previously
kept the 15s cycle fixed, which meant the stagger compressed to ~100ms and the
runtime shelled out Git probes 8-16 times per second forever. Each background
probe is at least one subprocess-spawning gxserver RPC (a network round trip
for remote machines), so cap the global probe rate instead: the polling cycle
stretches so consecutive probes are never closer than this spacing.
*/
export const GPUI_PROJECT_DIFF_STATS_MIN_PROBE_SPACING_MS = 1000;
/*
CDXC:Git 2026-08-16:
`countFileLines` sums every requested path server-side in one RPC. Batch the
untracked-file fan-out instead of issuing one RPC per file, chunked only to
keep a single request body bounded for repos with thousands of untracked files.
*/
export const GPUI_UNTRACKED_LINE_COUNT_BATCH_SIZE = 200;
/*
CDXC:Git 2026-07-29:
GitHub CLI probes (`gh --version`, `gh pr view`) are the only networked calls in
the sidebar Git fan-out, and `gh pr view` can hold a gxserver worker for many
seconds. Background/switch-driven Git refreshes therefore publish local Git
state first and run the GitHub probe on this delay, so the RPC burst at the
switch instant never competes with terminal attach traffic.
*/
export const GPUI_SIDEBAR_GIT_HUB_DEFERRED_PROBE_DELAY_MS = 1500;
/*
CDXC:Git 2026-07-29:
`GxserverGitAction` members that change the working tree, the index, or a ref.
Running any of them invalidates that project's memoized Git state, so the memo
can only ever serve a repository the sidebar itself has not touched since.
*/
export const GPUI_MUTATING_GIT_ACTIONS: ReadonlySet<string> = new Set<GxserverGitAction>([
  'addAll',
  'checkout',
  'checkoutNewBranch',
  'commit',
  'deleteLocalBranch',
  'deleteRemoteBranch',
  'merge',
  'pullFastForward',
  'push',
  'pushSetUpstream',
  'pushSetUpstreamCurrent',
]);
export const GPUI_AUTO_SLEEP_MINUTE_MS = 60 * 1000;
export const GPUI_WORKSPACE_TERMINAL_LIFECYCLE_BRIDGE_RETRY_DELAY_MS = 25;
export const GPUI_WORKSPACE_GROUPS_SERVER_SYNC_DELAY_MS = 400;
export const GPUI_WORKSPACE_GROUPS_SERVER_SYNC_RETRY_DELAY_MS = 5000;
export const GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_DELAY_MS = 400;
export const GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_RETRY_DELAY_MS = 5000;
export const GPUI_SIDEBAR_SPACES_SERVER_SYNC_DELAY_MS = 400;
export const GPUI_SIDEBAR_SPACES_SERVER_SYNC_RETRY_DELAY_MS = 5000;
export const GPUI_ACTIVE_WORKSPACE_TAB_SESSION_TITLE_MAX_CHARS = 512;
export const GPUI_SIDEBAR_DEFAULT_CLIENT_ID = 'ghostex-gpui-sidebar';
export const GPUI_GXSERVER_UNAVAILABLE_GROUP_ID = 'gxserver-unavailable';
export const GPUI_GXSERVER_CHATS_GROUP_ID = 'combined-chats';
export const GPUI_DEFAULT_VISIBLE_COUNT = 1;
/*
CDXC:StateSync 2026-07-29:
Toast titles for a refused settle/snooze. Named per endpoint so the user learns
which action failed without the toast ever repeating a session title, project
path, or the daemon's response body.
*/
export const SESSION_LIFECYCLE_FAILURE_TITLES: Record<string, string> = {
  '/api/settleSession': 'Settle failed',
  '/api/snoozeSession': 'Snooze failed',
  '/api/unsettleSession': 'Un-settle failed',
  '/api/unsnoozeSession': 'Wake failed',
};
export const GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE = 'ghostex.gpui.sidebar.nativeProjectPathAction';
export const GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE = 'ghostex.gpui.sidebar.commandAction';
export const GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_TYPE = 'ghostex.gpui.sidebar.commandRunEnd';
export const GPUI_SIDEBAR_COMMAND_SELECTOR_MESSAGE_KEYS = new Set(['commandId', 'groupId', 'runMode', 'scope', 'type']);
export const GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_TYPE = 'ghostex.gpui.sidebar.gxserverPresentationFocusState';
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_TYPE = 'ghostex.gpui.sidebar.workspaceTerminalFocus';
export const GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE = 'ghostex.gpui.sidebar.openBrowserUrl';
export const GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS = 16 * 1024;
export const GPUI_SIDEBAR_BROWSER_FAVICON_URL_MAX_CHARS = 2048;
export const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.projectBoardConversationRequest';
export const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.projectBoardConversationResponse';
export const GPUI_QUICK_AUTOMATIONS_PROJECT_ID = 'quick-automations';
export const GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE = 'Automations Overview';
export const GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID = '__quick-automations__';
export const GPUI_AGENT_PROMPT_READY_DELAY_MS = 4_000;
export const GPUI_AGENT_PROMPT_STEP_DELAY_MS = 1_000;
export const GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_TTL_MS = 60_000;
export const GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_CACHE_MAX = 512;
export const GPUI_PROJECT_BOARD_LINK_AVAILABILITY_CONCURRENCY = 4;
/*
CDXC:ProjectBoard 2026-08-07:
Resuming a bead's closed conversation runs through the daemon's fork plan,
which only knows how to continue Codex, Claude, and Pi conversations. gxserver
stays the authority and rejects anything else, so this set exists to keep the
board from offering a Resume the daemon would refuse.
*/
export const GPUI_PROJECT_BOARD_RESUMABLE_AGENT_IDS = new Set(['claude', 'codex', 'pi']);
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.workspaceTerminalRenameCommand';
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.workspaceTerminalLifecycleRequest';
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.workspaceTerminalLifecycleResult';
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_TYPE = 'ghostex.gpui.sidebar.workspaceTerminalBell';
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.workspaceTerminalTitleChanged';
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_MAX_CHARS = 512;
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_SETTLE_MS = 1_500;
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.workspaceTerminalEscapePressed';
export const GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.workspaceFirstPromptTitleGenerationCancel';
export const GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.workspaceSessionAttentionAcknowledge';
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.workspaceTerminalRuntimeAction';
export const GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_TYPE = 'ghostex.gpui.sidebar.sessionCompletionSound';
export const GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_TYPE = 'ghostex.gpui.sidebar.globalActions';
/*
 * CDXC:AgentLauncher 2026-08-01:
 * The tab strip is gpui-drawn, so it cannot read the HUD store the React
 * surfaces use. Cap what crosses the bridge at the number of buttons the strip
 * will actually draw; gpui rejects a longer list outright rather than
 * truncating it, so the two caps must agree.
 */
export const GPUI_TAB_STRIP_MAX_GLOBAL_ACTIONS = 8;
export const GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_TYPE = 'ghostex.gpui.sidebar.sessionStatusIndicators';
export const GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_TYPE = 'ghostex.gpui.sidebar.petOverlayState';
export const GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_TYPE = 'ghostex.gpui.sidebar.statusPetActivation';
export const GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_TYPE = 'ghostex.gpui.sidebar.menuBarProjectActivation';
export const GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_TYPE = 'ghostex.gpui.sidebar.menuBarSessionActivation';
export const GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.workspaceTabSessionSelected';
export const GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.commandPaletteSessionFocus';
export const GPUI_SIDEBAR_STASHED_PROMPT_SESSION_JUMP_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_STASHED_PROMPT_SESSION_JUMP_MESSAGE_TYPE = 'ghostex.gpui.sidebar.stashedPromptSessionJump';
export const GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_TYPE =
  'ghostex.gpui.sidebar.commandPaletteRunSidebarCommand';
export const GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_TYPE = 'ghostex.gpui.sidebar.nativeAppShotCaptured';
export const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_TYPE = 'ghostex.gpui.sidebar.nativeAppShotPrompt';
export const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_VERSION = 1;
export const GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_TYPE = 'ghostex.gpui.sidebar.nativeAppShotPromptResult';
export const GPUI_SIDEBAR_REMOTE_EVENT_NAME = 'ghostex-gpui-sidebar-remote-event';
/*
CDXC:Navigation 2026-08-19:
The native titlebar owns the Back/Forward buttons but not the trail: Rust
dispatches the click here and this runtime performs the same gxserver walk and
sidebar activation the web app does, so both apps share one implementation.
*/
export const GPUI_SIDEBAR_NAVIGATION_HISTORY_COMMAND_EVENT_NAME = 'ghostex-gpui-sidebar-navigation-history-command';
export const APP_SHOT_RECENT_TARGET_MS = 60_000;
export const APP_SHOT_PROMPT_INSERT_RESULT_TIMEOUT_MS = 2_000;
export const GPUI_STATUS_INDICATOR_MAX_CANDIDATES = 96;
export const GPUI_STATUS_INDICATOR_MAX_PROJECTS = 32;
export const GPUI_STATUS_INDICATOR_MAX_SESSIONS_PER_PROJECT = 16;
export const GPUI_STATUS_INDICATOR_ID_MAX_CHARS = 256;
export const GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS = 120;
export const GPUI_RENDERER_COMMAND_RENAME_TITLE_MAX_CHARS = 120;
export const GPUI_RENDERER_COMMAND_RENAME_TITLE_CONTROL_PATTERN = /[\u0000-\u001f\u007f-\u009f]/u;
export const DEFAULT_GPUI_PROMPT_AGENT_ID = 'codex';

export const GPUI_BACKGROUND_COMMIT_MESSAGE_DEFAULT_AGENT_IDS = new Set(['claude', 'codex', 'cursor', 'gemini']);

export const GPUI_GIT_MULTIPLE_COMMITS_PROMPT = `Please review my current changes and commit them as multiple focused commits.

Commit-splitting rules:
- Group changes by related feature, fix, or topic.
- Do not combine unrelated work in the same commit.
- Use file-based splitting only; do not split individual hunks.
- Make each commit easy to revert or cherry-pick later.
- Use clear, concise commit messages.`;

export const GPUI_REMOTE_MERGE_CONFLICT_PROMPT =
  'A direct merge into main has conflicts in this remote project. Inspect the repository state, resolve the conflicts, and commit the merge when it is correct.';

export const GPUI_GIT_RELEASE_STEPS_PROMPT = `1. Push any local commits to remote.
2. Review the commits since the last released version.
3. Update CHANGELOG.md to mention the new changes.
4. Publish the next minor version to the usual places we publish this app.`;

export const GPUI_GIT_MULTICOMMIT_RELEASE_PROMPT = `${GPUI_GIT_MULTIPLE_COMMITS_PROMPT}

After all focused commits are created:
${GPUI_GIT_RELEASE_STEPS_PROMPT}`;

export const GPUI_GIT_RELEASE_ONLY_PROMPT = `Please release this app using the usual release workflow.

${GPUI_GIT_RELEASE_STEPS_PROMPT}`;

export const GPUI_REMOTE_RECENT_PROJECTS_STORAGE_KEY = 'ghostex-gpui-remote-recent-projects';
export const GPUI_REMOTE_GROUP_ORDER_STORAGE_KEY = 'ghostex-gpui-remote-group-order';
export const GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_STORAGE_KEY = 'ghostex-gpui-remote-last-seen-presentations';
export const GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_PERSIST_DELAY_MS = 2_000;

export const GPUI_COMMAND_PANE_SESSION_SUMMARY_LIMIT = 128;
export const GPUI_COMMAND_PANE_SESSION_STRING_MAX_LENGTH = 512;
export const GPUI_COMMAND_PANE_TIMER_DEADLINE_MAX_LENGTH = 64;
export const GPUI_COMMAND_PANE_TIMER_LABEL_MAX_LENGTH = 32;
export const GPUI_COMMAND_PANE_TIMER_REMAINING_MS_MAX = 2_147_483_647;
export const GPUI_GXSERVER_LOCAL_COMMAND_PANE_SESSION_ID_PATTERN = /^G[0-9][0-9A-Za-z_-]*$/u;

export const GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_TYPE = 'ghostex.gpui.sidebar.titlebarGitMenuState';
export const GPUI_TITLEBAR_GIT_MENU_STATE_MESSAGE_VERSION = 1;
export const GPUI_TITLEBAR_GIT_ACTION_MESSAGE_TYPE = 'ghostex.gpui.sidebar.titlebarGitAction';
export const GPUI_TITLEBAR_GIT_ACTION_MESSAGE_VERSION = 1;
export const GPUI_TITLEBAR_GIT_ACTIONS: ReadonlySet<SidebarGitAction> = new Set([
  'commit',
  'push',
  'pr',
  'syncMain',
  'syncRemote',
  'multiRelease',
  'release',
]);

export const GPUI_PROJECT_BOARD_CONVERSATION_ACTIONS = new Set<string>([
  'appendDebugLog',
  'associateFocusedSession',
  'getState',
  'jumpToConversation',
  'showToast',
  'startWork',
  'unlinkConversation',
]);

export const GPUI_SIDEBAR_THEME_VALUES = new Set<SidebarTheme>([
  'dark-1',
  'dark-2',
  'plain-dark',
  'plain-light',
  'dark-green',
  'dark-blue',
  'dark-red',
  'dark-pink',
  'dark-orange',
  'light-blue',
  'light-green',
  'light-pink',
  'light-orange',
]);

export const GPUI_MIN_ATTENTION_VISIBLE_MS = 1_500;
export const GPUI_ESCAPE_DONE_SUPPRESSION_MS = 5_000;
export const GPUI_ATTENTION_COMPLETION_SOUND_EVENT_CACHE_LIMIT = 2_048;
export const GPUI_LOCALLY_ACKNOWLEDGED_ATTENTION_EVENT_CACHE_LIMIT = 2_048;

export const GPUI_CLOSE_AFTER_DONE_DELAY_MS = 3 * 60_000;
export const GPUI_CLOSE_AFTER_DONE_STORAGE_KEY = 'ghostex-gpui-close-after-done-session-ids';
export const GPUI_DELAYED_SEND_MIN_DELAY_MS = 60_000;
export const GPUI_DELAYED_SEND_MAX_DELAY_MS = 2_147_483_647;

export const GPUI_REMOTE_MACHINE_STATUS_MESSAGE_MAX_CHARS = 300;
export const GPUI_REMOTE_MACHINE_RECONNECT_DELAYS_MS = [2_000, 5_000, 15_000, 30_000, 60_000] as const;

export const GPUI_REMOTE_MACHINE_RETRY_STATES = new Set<SidebarRemoteMachineStatusMessage['state']>([
  'disconnected',
  'failed',
  'keychainFailed',
  'presentationStreamFailed',
  'presentationSubscribeFailed',
  'sshFailed',
  'tokenUnavailable',
  'tunnelFailed',
]);

export const GPUI_REMOTE_MACHINE_RECONNECT_PROGRESS_STATES = new Set<SidebarRemoteMachineStatusMessage['state']>([
  'connecting',
  'downloadingRemoteServerPackage',
  'installing',
]);

export const GPUI_REMOTE_MACHINE_RECONNECT_STOP_STATES = new Set<SidebarRemoteMachineStatusMessage['state']>([
  'installApprovalRequired',
  'installFailed',
  'invalid',
  'unsupported',
  'unsupportedRemotePlatform',
]);

export const GPUI_REMOTE_MACHINE_STATUS_STATES = new Set([
  'connecting',
  'connected',
  'disconnected',
  'downloadingRemoteServerPackage',
  'installFailed',
  'installApprovalRequired',
  'installing',
  'invalid',
  'keychainFailed',
  'presentationStreamFailed',
  'presentationSubscribeFailed',
  'sshFailed',
  'tokenUnavailable',
  'tunnelFailed',
  'unsupported',
  'unsupportedRemotePlatform',
  'failed',
]);

export const GPUI_REMOTE_MACHINE_PRESENTATION_CLEAR_STATES = new Set([
  'disconnected',
  'failed',
  'installApprovalRequired',
  'installFailed',
  'invalid',
  'keychainFailed',
  'presentationStreamFailed',
  'presentationSubscribeFailed',
  'sshFailed',
  'tokenUnavailable',
  'tunnelFailed',
  'unsupported',
  'unsupportedRemotePlatform',
]);
