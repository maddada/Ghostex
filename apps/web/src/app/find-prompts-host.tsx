/*
CDXC:AgentHistorySearch 2026-08-20:
Mounts the shared Find surface — the GUI for `gx f` — for a workspace session's
pane. The transport is memoized per machine so the hook's in-flight search is
not restarted by unrelated re-renders.

Opening a result that is already running dispatches the same
`ghostex-web:focusSession` event the sidebar fork uses, so Find reuses the one
focus path the web app already has instead of inventing another.
*/

import { useMemo } from "react";
import { FindPromptsView } from "@/packages/core-ui/find/find-prompts-view";
import "@/packages/core-ui/styles.css";
import { createFindPromptsTransport } from "../find/find-prompts-transport";
import type { WorkspaceSession } from "../workspace/workspace-model";

export function FindPromptsHost({
  onSwitchToTerminal,
  session,
}: {
  onSwitchToTerminal(): void;
  session: WorkspaceSession;
}) {
  const machineId = session.machineId;
  const transport = useMemo(
    () =>
      createFindPromptsTransport(machineId, {
        focusSession: ({ projectId, sessionId }) => {
          window.dispatchEvent(
            new CustomEvent("ghostex-web:focusSession", {
              detail: { machineId, projectId, sessionId },
            }),
          );
        },
        switchToTerminal: onSwitchToTerminal,
      }),
    // `onSwitchToTerminal` is re-created per render by the workspace; capturing
    // the first one is correct because it only ever flips this pane's mode.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [machineId],
  );
  return (
    <div className="ghostex-web-find-prompts-host">
      <FindPromptsView transport={transport} />
    </div>
  );
}
