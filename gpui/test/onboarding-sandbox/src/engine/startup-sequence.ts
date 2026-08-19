/*
 * Copy + code anchors for the startup sequence.
 *
 * Toast copy is verbatim from gpui/src/main.rs
 * start_gpui_local_gxserver_bootstrap (:37884) and
 * start_gpui_first_run_onboarding (:37475) so the sandbox shows the same strings
 * a user sees on a real launch.
 */
import type { GxserverHealthScenario } from "../state/types";

export const CODE_REFS = {
  bootstrap: "gpui/src/main.rs:37884 start_gpui_local_gxserver_bootstrap",
  cefReady: "gpui/src/main.rs:61678 initialize_cef → sidebar surface ready",
  firstRun: "gpui/src/main.rs:37475 start_gpui_first_run_onboarding",
  healthProbe: "gpui/src/main.rs gpui_probe_local_gxserver_health",
  modalSlot: "gpui/src/main.rs:31669 open_gpui_app_modal_window_inner (app_modal_window)",
  modalOpen: "gpui/src/main.rs:31595 open_gpui_app_modal_window",
  modalReady: "gpui/src/main.rs:79424 GpuiAppModalHost::receive_bridge_message",
  nonReactHost: "gpui/src/main.rs:3267 GpuiAppModalKind::uses_react_modal_host",
  tutorialVideoUrl: "gpui/src/main.rs:1130 GHOSTEX_TUTORIAL_VIDEO_URL (CDXC:GPUITutorialVideo)",
  persistState: "gpui/src/main.rs persist_gpui_first_run_onboarding_state",
  portlessCheck: "gpui/src/main.rs:39386 start_gpui_portless_setup_prompt_check",
  progressiveHooks: "gpui/src/main.rs:38485 run_gpui_progressive_agent_hook_status_task",
  cliSettingsAction: "gpui/src/main.rs:38548 run_gpui_ghostex_cli_settings_action",
  sidebarCommand: "gpui/src/main.rs:41345 handle_gpui_app_modal_sidebar_command",
  tipsRuntimeStatus: "gpui/src/main.rs:31568 request_gpui_titlebar_tips_runtime_status",
  toast: "gpui/src/main.rs:36970 show_gpui_gxserver_bootstrap_toast",
  firstLaunchSetup: "gpui/src/main.rs open_gpui_first_launch_setup_with_sidebar_state",
  addProject: "gpui/src/main.rs handle_gpui_add_project_dialog_request_message",
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
