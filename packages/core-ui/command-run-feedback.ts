import type { SidebarCommandButton, SidebarCommandRunMode } from "../shared/sidebar-commands";
import type {
  SidebarCommandRunState,
  SidebarCommandRunStateChangedMessage,
} from "../shared/session-grid-contract";

export const SIDEBAR_COMMAND_SUCCESS_FEEDBACK_DURATION_MS = 10_000;
export const SIDEBAR_COMMAND_ERROR_FEEDBACK_DURATION_MS = 10_000;

export type SidebarCommandRunFeedbackState = {
  activeRunIds: string[];
  status: SidebarCommandRunState;
};

export function applySidebarCommandRunStateChangedMessage(
  current: SidebarCommandRunFeedbackState | undefined,
  message: SidebarCommandRunStateChangedMessage,
): SidebarCommandRunFeedbackState {
  if (message.state === "running") {
    if (current?.status === "running" && current.activeRunIds.includes(message.runId)) {
      return current;
    }

    return {
      activeRunIds: current?.activeRunIds.includes(message.runId)
        ? current.activeRunIds
        : [...(current?.activeRunIds ?? []), message.runId],
      status: "running",
    };
  }

  const nextActiveRunIds = current?.activeRunIds.filter((runId) => runId !== message.runId) ?? [];
  const nextStatus = nextActiveRunIds.length > 0 ? "running" : message.state;

  if (
    current &&
    current.status === nextStatus &&
    haveSameRunIds(current.activeRunIds, nextActiveRunIds)
  ) {
    return current;
  }

  return {
    activeRunIds: nextActiveRunIds,
    status: nextStatus,
  };
}

export function getSidebarCommandRunFeedbackDuration(
  status: SidebarCommandRunState,
): number | undefined {
  switch (status) {
    case "success":
      return SIDEBAR_COMMAND_SUCCESS_FEEDBACK_DURATION_MS;
    case "error":
      return SIDEBAR_COMMAND_ERROR_FEEDBACK_DURATION_MS;
    default:
      return undefined;
  }
}

export function getSidebarCommandRunModeForClick(
  command: Pick<SidebarCommandButton, "actionType" | "closeTerminalOnExit">,
  runState: SidebarCommandRunFeedbackState | undefined,
): SidebarCommandRunMode {
  return command.actionType === "terminal" &&
    command.closeTerminalOnExit &&
    runState?.status === "error" &&
    runState.activeRunIds.length === 0
    ? "debug"
    : "default";
}

export function reconcileSidebarCommandRunFeedbackStates(
  commandRunStates: Record<string, SidebarCommandRunFeedbackState>,
  commandIds: readonly string[],
): Record<string, SidebarCommandRunFeedbackState> {
  const validCommandIds = new Set(commandIds);
  const nextEntries = Object.entries(commandRunStates).filter(([commandId]) =>
    validCommandIds.has(commandId),
  );

  if (nextEntries.length === Object.keys(commandRunStates).length) {
    return commandRunStates;
  }

  return Object.fromEntries(nextEntries);
}

function haveSameRunIds(left: readonly string[], right: readonly string[]): boolean {
  if (left.length !== right.length) {
    return false;
  }

  return left.every((runId, index) => runId === right[index]);
}
