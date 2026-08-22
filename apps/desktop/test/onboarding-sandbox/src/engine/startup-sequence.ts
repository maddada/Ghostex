/*
 * Copy + code anchors for the startup sequence.
 *
 * Toast copy is verbatim from apps/desktop/src/app/os_integration.rs
 * start_gpui_local_gxserver_bootstrap (:954) and
 * start_gpui_first_run_onboarding (:536) so the sandbox shows the same strings
 * a user sees on a real launch.
 */
import type { GxserverHealthScenario } from "../state/types";

export const CODE_REFS = {
  bootstrap: "apps/desktop/src/app/os_integration.rs:954 start_gpui_local_gxserver_bootstrap",
  cefReady: "apps/desktop/src/app/terminal_sync.rs:5426 initialize_cef → sidebar surface ready",
  firstRun: "apps/desktop/src/app/os_integration.rs:536 start_gpui_first_run_onboarding",
  healthProbe: "apps/desktop/src/app/helpers/board_gxserver.rs gpui_probe_local_gxserver_health",
  modalSlot: "apps/desktop/src/app/modals.rs:1040 open_gpui_app_modal_window_inner (app_modal_window)",
  modalOpen: "apps/desktop/src/app/modals.rs:966 open_gpui_app_modal_window",
  modalReady: "apps/desktop/src/app/window/modal_host.rs:159 GpuiAppModalHost::receive_bridge_message",
  nonReactHost: "apps/desktop/src/app/model/app_modal_kind.rs GpuiAppModalKind::uses_react_modal_host",
  tutorialVideoUrl: "apps/desktop/src/app/consts.rs:665 GHOSTEX_TUTORIAL_VIDEO_URL (CDXC:GPUITutorialVideo)",
  persistState: "apps/desktop/src/app/helpers/os_cli.rs:4729 persist_gpui_first_run_onboarding_state",
  portlessCheck: "apps/desktop/src/app/os_integration.rs:2477 start_gpui_portless_setup_prompt_check",
  progressiveHooks: "apps/desktop/src/app/os_integration.rs:1565 run_gpui_progressive_agent_hook_status_task",
  cliSettingsAction: "apps/desktop/src/app/os_integration.rs:1628 run_gpui_ghostex_cli_settings_action",
  sidebarCommand: "apps/desktop/src/app/delayed_send.rs:1820 handle_gpui_app_modal_sidebar_command",
  tipsRuntimeStatus: "apps/desktop/src/app/modals.rs:939 request_gpui_titlebar_tips_runtime_status",
  toast: "apps/desktop/src/app/modals.rs:1987 show_gpui_gxserver_bootstrap_toast",
  firstLaunchSetup: "apps/desktop/src/app/modals.rs:1361 open_gpui_first_launch_setup_with_sidebar_state",
  addProject: "apps/desktop/src/app/remote_conn.rs:1812 handle_gpui_add_project_dialog_request_message",
} as const;

export const GPUI_APP_TOAST_DEFAULT_DURATION_MS = 8_000;

export const OS_INTEGRATION_TOAST = {
  id: "gpui-os-integration-onboarding",
  title: "OS Integration available",
  message:
    "Open Settings > OS Integration to set Ghostex as your editor or terminal target.",
} as const;

export type GxserverBranchToast = {
  kind: "info" | "warning" | "error";
  title: string;
  message: string;
};

/** The toast each unhealthy probe result raises before anything else happens. */
export const GXSERVER_BRANCH_TOASTS: Record<GxserverHealthScenario, GxserverBranchToast | null> = {
  healthyToolsAvailable: null,
  healthyToolsUnavailable: {
    kind: "info",
    title: "Restarting gxserver",
    message:
      "The running gxserver does not match the tools bundled with this Ghostex build.",
  },
  buildMismatch: {
    kind: "info",
    title: "Updating gxserver",
    message:
      "The running gxserver belongs to a different Ghostex build. Ghostex is restarting it before loading migrated storage.",
  },
  protocolMismatch: {
    kind: "error",
    title: "gxserver protocol mismatch",
    message:
      "The running gxserver speaks a different protocol version than this Ghostex build. Update Ghostex or restart the daemon.",
  },
  spawnFailure: {
    kind: "error",
    title: "gxserver failed",
    message:
      "Bundled gxserver binary is missing. Run `bun run build` for development, or reinstall Ghostex so Web/gxserver is present.",
  },
};

export const GXSERVER_RESPAWN_FAILURE_TOAST: GxserverBranchToast = {
  kind: "error",
  title: "gxserver failed to start",
  message: "The daemon did not become healthy in time.",
};

export const GXSERVER_LOADING_TOAST: GxserverBranchToast = {
  kind: "info",
  title: "Loading sessions",
  message: "Starting gxserver and loading projects.",
};

/** Branches that restart the daemon instead of stopping at the toast. */
export function scenarioRestartsDaemon(scenario: GxserverHealthScenario): boolean {
  return scenario === "healthyToolsUnavailable" || scenario === "buildMismatch";
}
