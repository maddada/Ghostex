import {
  getSidebarSessionLifecycleState,
  type SidebarSessionItem,
} from "../shared/session-grid-contract";

export type GroupSessionSummary = {
  attentionCount: number;
  indicatorActivity: "attention" | "working" | undefined;
  workingCount: number;
};

export function getAwakeTerminalAndBrowserCount(
  sessions: readonly SidebarSessionItem[],
): number {
  return sessions.filter(
    (session) =>
      getSidebarSessionLifecycleState(session) === "running" &&
      (session.sessionKind === "terminal" ||
        session.sessionKind === "browser" ||
        session.kind === "browser"),
  ).length;
}

export function getGroupSessionSummary(
  sessions: readonly SidebarSessionItem[],
): GroupSessionSummary {
  let hasWorking = false;
  let hasAttention = false;
  let attentionCount = 0;
  let workingCount = 0;

  for (const session of sessions) {
    if (session.activity === "working") {
      hasWorking = true;
      workingCount += 1;
      continue;
    }

    if (session.activity === "attention") {
      hasAttention = true;
      attentionCount += 1;
    }
  }

  return {
    attentionCount,
    indicatorActivity: hasAttention ? "attention" : hasWorking ? "working" : undefined,
    workingCount,
  };
}
