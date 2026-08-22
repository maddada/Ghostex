/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import type { GpuiSidebarRuntime } from "./core";
import { createGpuiSidebarSettings } from "./helpers/bootstrap";
import {
  comparePreviousSessionItemsByClosedTime,
  gxserverSearchResultToPreviousSessionItem,
  parseGpuiGxserverPreviousSessionHistoryId,
  parseGpuiRemotePreviousSessionHistoryId,
  previousSessionTitle,
} from "./helpers/previous-sessions";
import { normalizeNonEmptyString } from "./helpers/records";
import type { GpuiGxserverCreatedSessionResult } from "./types-and-protocol";
import type { GxserverPresentationSearchResponse } from "@/packages/shared/gxserver-protocol";
import type {
  SidebarPreviousSessionItem,
  SidebarToExtensionMessage,
} from "@/packages/shared/session-grid-contract";

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimePreviousSessionMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimePreviousSessionMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimePreviousSessionMethods {
  requestPreviousSessions(message: Extract<SidebarToExtensionMessage, { type: "requestPreviousSessions" }>): Promise<void>;
  restorePreviousSession(historyId: string): Promise<void>;
  restoreRemotePreviousSession(reference: { machineId: string; projectId: string; sessionId: string }, historyId: string): Promise<void>;
  deletePreviousSession(historyId: string): Promise<void>;
  connectedRemotePreviousSessionMachines(): Array<{
    machineId: string;
    machineName: string;
  }>;
  postPreviousSessionsResult(requestId: string, query: string | undefined, previousSessions: SidebarPreviousSessionItem[], cursor?: string): void;
  removePreviousSessionFromCurrentResult(historyId: string): void;
}

export const gpuiSidebarRuntimePreviousSessionMethods = {

  async requestPreviousSessions(this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: "requestPreviousSessions" }>,
  ): Promise<void> {
    const limit = message.limit ?? 80;
    const sessionTags = message.sessionTags;
    const remoteMachines = this.connectedRemotePreviousSessionMachines();
    try {
      const [localResponse, ...remoteResponses] = await Promise.all([
        this.client
          ? this.client
              .rpc<GxserverPresentationSearchResponse>("/api/listPreviousSessions", {
                cursor: message.cursor,
                includeActive: false,
                includePrevious: true,
                limit,
                query: message.query,
                sessionTags,
              })
              .catch((): GxserverPresentationSearchResponse => ({ results: [] }))
          : Promise.resolve<GxserverPresentationSearchResponse>({ results: [] }),
        ...remoteMachines.map((machine) =>
          this.requestRemoteGxserver<GxserverPresentationSearchResponse>(
            machine.machineId,
            "/api/listPreviousSessions",
            {
              cursor: message.cursor,
              includeActive: false,
              includePrevious: true,
              limit,
              query: message.query,
              sessionTags,
            },
          ).catch((): GxserverPresentationSearchResponse => ({ results: [] })),
        ),
      ]);
      /*
      CDXC:GPUIRemotePreviousSessions 2026-06-24-17:19:
      Previous-session list/search combines local gxserver rows with connected remote gxserver rows, but remote history ids are machine-prefixed so restore/delete can route back through Rust's tunnel owner. Keep only the current result page in memory and do not persist remote metadata in GPUI.
      */
      const remoteItems = remoteResponses.flatMap((response, index) =>
        response.results.map((result) =>
          gxserverSearchResultToPreviousSessionItem(result, {
            historyIdPrefix: `remote-gxserver:${remoteMachines[index]?.machineId ?? ""}`,
            projectNamePrefix: remoteMachines[index]?.machineName,
          }),
        ),
      );
      this.postPreviousSessionsResult(
        message.requestId,
        message.query,
        [
          ...localResponse.results.map((result) =>
            gxserverSearchResultToPreviousSessionItem(result),
          ),
          ...remoteItems,
        ]
          .sort(comparePreviousSessionItemsByClosedTime),
        localResponse.cursor ?? remoteResponses.find((response) => response.cursor)?.cursor,
      );
    } catch {
      this.postPreviousSessionsResult(message.requestId, message.query, []);
    }
  },

  async restorePreviousSession(this: GpuiSidebarRuntime, historyId: string): Promise<void> {
    const remoteReference = parseGpuiRemotePreviousSessionHistoryId(historyId);
    if (remoteReference) {
      await this.restoreRemotePreviousSession(remoteReference, historyId);
      return;
    }
    const reference = parseGpuiGxserverPreviousSessionHistoryId(historyId);
    if (!reference || !this.client) {
      return;
    }
    const previousSession = this.previousSessionsByHistoryId.get(historyId);
    if (previousSession && previousSession.isRestorable !== true) {
      return;
    }
    try {
      const response = await this.client.rpc<GpuiGxserverCreatedSessionResult>(
        "/api/createSession",
        {
          kind: "terminal",
          lifecycleState: "running",
          projectId: reference.projectId,
          restoredFromSessionId: reference.sessionId,
          ...(previousSession?.sessionTag ? { sessionTag: previousSession.sessionTag } : {}),
          ...(previousSession?.sidebarOrder !== undefined
            ? { sidebarOrder: previousSession.sidebarOrder }
            : {}),
          surface: "workspace",
          title: previousSessionTitle(previousSession),
        },
      );
      const restoredSessionId = normalizeNonEmptyString(response.session?.sessionId);
      if (restoredSessionId) {
        this.focusLocalWorkspaceSession(
          normalizeNonEmptyString(response.session?.projectId) ?? reference.projectId,
          restoredSessionId,
        );
      }
      await this.client
        .rpc("/api/removeSession", {
          projectId: reference.projectId,
          reason: "restorePreviousSession",
          sessionId: reference.sessionId,
        })
        .catch(() => undefined);
      this.removePreviousSessionFromCurrentResult(historyId);
    } catch {
      this.postRemoteToast("warning", "Previous session restore failed", {
        description: "gxserver could not restore that previous session.",
      });
    }
  },

  async restoreRemotePreviousSession(this: GpuiSidebarRuntime,
    reference: { machineId: string; projectId: string; sessionId: string },
    historyId: string,
  ): Promise<void> {
    const previousSession = this.previousSessionsByHistoryId.get(historyId);
    if (previousSession && previousSession.isRestorable !== true) {
      return;
    }
    /*
    CDXC:GPUIRemotePreviousSessions 2026-06-24-17:19:
    Restoring remote history recreates a real workspace session on the owning remote gxserver and then removes the stopped history row from that same machine. GPUI does not create a local terminal, synthesize resume commands, or trust visible previous-session labels as operation ids.

    CDXC:GPUIRemoteAttach 2026-06-24-19:06:
    When remote previous-session restore returns a new gxserver session id, GPUI may immediately ask Rust to attach that exact restored id through the same native remote terminal action as a direct session click. If gxserver does not return the new id, the restore remains server-only instead of guessing from labels or the old history id.
    */
    try {
      const response = await this.requestRemoteGxserver<{
        session?: { projectId?: string; sessionId?: string };
      }>(reference.machineId, "/api/createSession", {
        kind: "terminal",
        lifecycleState: "running",
        projectId: reference.projectId,
        restoredFromSessionId: reference.sessionId,
        ...(previousSession?.sessionTag ? { sessionTag: previousSession.sessionTag } : {}),
        ...(previousSession?.sidebarOrder !== undefined
          ? { sidebarOrder: previousSession.sidebarOrder }
          : {}),
        surface: "workspace",
        title: previousSessionTitle(previousSession),
      });
      await this.requestRemoteGxserver(reference.machineId, "/api/removeSession", {
        projectId: reference.projectId,
        reason: "restorePreviousSession",
        sessionId: reference.sessionId,
      }).catch(() => undefined);
      this.removePreviousSessionFromCurrentResult(historyId);
      const restoredSessionId = response.session?.sessionId;
      if (restoredSessionId) {
        const restoredReference = {
          machineId: reference.machineId,
          projectId: response.session?.projectId ?? reference.projectId,
          sessionId: restoredSessionId,
        };
        this.setRemotePresentationSessionFocus(restoredReference);
        this.postRemoteSessionNativeAction("openRemoteSessionTerminal", restoredReference, {
          historyId,
          type: "restorePreviousSession",
        });
      }
    } catch {
      this.postRemoteToast("warning", "Remote restore failed", {
        description: "The remote gxserver could not restore that previous session.",
      });
    }
  },

  async deletePreviousSession(this: GpuiSidebarRuntime, historyId: string): Promise<void> {
    const remoteReference = parseGpuiRemotePreviousSessionHistoryId(historyId);
    if (remoteReference) {
      await this.requestRemoteGxserver(remoteReference.machineId, "/api/removeSession", {
        projectId: remoteReference.projectId,
        reason: "deletePreviousSession",
        sessionId: remoteReference.sessionId,
      }).catch(() => undefined);
      this.removePreviousSessionFromCurrentResult(historyId);
      return;
    }
    const reference = parseGpuiGxserverPreviousSessionHistoryId(historyId);
    if (!reference || !this.client) {
      return;
    }
    await this.client
      .rpc("/api/removeSession", {
        projectId: reference.projectId,
        reason: "deletePreviousSession",
        sessionId: reference.sessionId,
      })
      .catch(() => undefined);
    this.removePreviousSessionFromCurrentResult(historyId);
  },

  connectedRemotePreviousSessionMachines(this: GpuiSidebarRuntime): Array<{
    machineId: string;
    machineName: string;
  }> {
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    return settings.remoteMachines.flatMap((machine) =>
      this.remotePresentations.has(machine.id)
        ? [{ machineId: machine.id, machineName: machine.name }]
        : [],
    );
  },

  postPreviousSessionsResult(this: GpuiSidebarRuntime,
    requestId: string,
    query: string | undefined,
    previousSessions: SidebarPreviousSessionItem[],
    cursor?: string,
  ): void {
    this.previousSessionsResult = {
      cursor,
      previousSessions,
      query,
      requestId,
    };
    for (const session of previousSessions) {
      this.previousSessionsByHistoryId.set(session.historyId, session);
    }
    this.messageSource.postMessage({
      cursor,
      previousSessions,
      query,
      requestId,
      type: "previousSessionsResult",
    });
  },

  removePreviousSessionFromCurrentResult(this: GpuiSidebarRuntime, historyId: string): void {
    this.previousSessionsByHistoryId.delete(historyId);
    const previousResult = this.previousSessionsResult;
    if (!previousResult) {
      return;
    }
    this.postPreviousSessionsResult(
      previousResult.requestId,
      previousResult.query,
      previousResult.previousSessions.filter((session) => session.historyId !== historyId),
      previousResult.cursor,
    );
  },
};

const gpuiSidebarRuntimePreviousSessionMethodsShapeCheck: GpuiSidebarRuntimePreviousSessionMethods = gpuiSidebarRuntimePreviousSessionMethods;
void gpuiSidebarRuntimePreviousSessionMethodsShapeCheck;
