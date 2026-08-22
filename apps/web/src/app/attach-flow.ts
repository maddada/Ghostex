import type {
  GxserverAttachSessionMetadataResult,
  GxserverSessionLifecycleResult,
  GxserverStartupTextDisposition,
} from "@/shared/gxserver-protocol";
import { rpcForMachine } from "../connections/connection-registry";
import type { SessionReference } from "./session-mapping";

export type AttachIntent = "attach" | "wake";

export interface PreparedSessionAttach {
  attach: GxserverAttachSessionMetadataResult;
  persistenceSessionCreated?: boolean;
  startupText?: string;
  startupTextDisposition: GxserverStartupTextDisposition;
}

export class RestoreBlockedError extends Error {
  constructor(readonly reason: string) {
    super(
      reason === "missingCwd"
        ? "Session restore is blocked because its working directory is unavailable."
        : `Session restore is blocked: ${reason}.`,
    );
    this.name = "RestoreBlockedError";
  }
}

export async function prepareSessionAttach(
  reference: SessionReference,
  intent: AttachIntent,
  startupText?: string,
): Promise<PreparedSessionAttach> {
  const params = lifecycleParams(reference, startupText);
  let attach = await requestAttach(reference.machineId, intent, params);
  validateNotRestoreBlocked(attach);
  const firstStartupText = trimmed(attach.startupText);
  const firstDisposition = attach.startupTextDisposition;

  if (attach.providerState.lifecycleState === "missing") {
    await rpcForMachine(reference.machineId, "/api/startSessionProvider", lifecycleParams(
      reference,
      firstStartupText,
    ));
    attach = await requestAttach(reference.machineId, "attach", params);
    validateNotRestoreBlocked(attach);
  }

  if (attach.providerState.lifecycleState !== "exists" || !trimmed(attach.attachCommand)) {
    throw new Error("Session provider did not become ready for terminal attach.");
  }

  const resolvedStartupText = firstStartupText ?? trimmed(attach.startupText);
  const startupTextDisposition = firstDisposition ?? attach.startupTextDisposition;
  const persistenceSessionCreated = attach.persistenceSessionCreated;
  if (
    startupTextDisposition === "queueAfterTerminalReady"
    && resolvedStartupText
    && persistenceSessionCreated === false
  ) {
    throw new Error("gxserver did not confirm the session provider started before terminal attach.");
  }

  return {
    attach,
    persistenceSessionCreated,
    ...(resolvedStartupText ? { startupText: resolvedStartupText } : {}),
    startupTextDisposition,
  };
}

async function requestAttach(
  machineId: string,
  intent: AttachIntent,
  params: Record<string, unknown>,
): Promise<GxserverAttachSessionMetadataResult> {
  const result = await rpcForMachine<
    { attach?: GxserverAttachSessionMetadataResult } | GxserverSessionLifecycleResult
  >(
    machineId,
    intent === "wake" ? "/api/wakeSession" : "/api/attachSessionMetadata",
    params,
  );
  if (!result.attach) {
    throw new Error("gxserver did not return session attach metadata.");
  }
  return result.attach;
}

function validateNotRestoreBlocked(attach: GxserverAttachSessionMetadataResult): void {
  if (attach.restoreBlocked) {
    throw new RestoreBlockedError(attach.restoreBlocked.reason);
  }
}

function lifecycleParams(
  reference: SessionReference,
  startupText?: string,
): Record<string, unknown> {
  return {
    projectId: reference.projectId,
    reason: "ghostex-web-workspace",
    sessionId: reference.sessionId,
    ...(startupText ? { startupText } : {}),
  };
}

function trimmed(value: string | undefined): string | undefined {
  const result = value?.trim();
  return result ? result : undefined;
}
