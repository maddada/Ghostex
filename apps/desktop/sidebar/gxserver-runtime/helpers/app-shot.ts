/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_TYPE,
  GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_VERSION,
  GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_TYPE,
  GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_VERSION,
} from "../constants";
import type { GpuiNativeAppShotCapture } from "../types-and-protocol";
import { normalizeNonEmptyString } from "./records";
import {
  createGpuiRemotePresentationSessionId,
  parseGpuiRemotePresentationSessionId,
} from "./remote-presentation";
import {
  parseGxserverPresentationProjectSessionId,
} from "@/packages/shared/gxserver-presentation-sidebar-projection";
import type { GxserverPresentationSnapshot } from "@/packages/shared/gxserver-protocol";
import type { SidebarSessionItem } from "@/packages/shared/session-grid-contract";

export function normalizeGpuiNativeAppShotPromptResult(
  value: unknown,
): { ok: boolean; sessionId: string } | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    record.type !== GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_VERSION ||
    typeof record.ok !== "boolean"
  ) {
    return undefined;
  }
  const sessionId = normalizeNonEmptyString(record.sessionId);
  return sessionId ? { ok: record.ok, sessionId } : undefined;
}

export function nativeAppShotPromptSessionIdForSidebarSession(
  session: SidebarSessionItem | undefined,
): string | undefined {
  if (!session) {
    return undefined;
  }
  const remoteSession = parseGpuiRemotePresentationSessionId(session.sessionId);
  if (remoteSession) {
    return createGpuiRemotePresentationSessionId(
      remoteSession.machineId,
      remoteSession.projectId,
      remoteSession.sessionId,
    );
  }
  return localGxserverSessionIdForSidebarSession(session);
}

export function localGxserverSessionIdForSidebarSession(
  session: SidebarSessionItem | undefined,
): string | undefined {
  if (!session || parseGpuiRemotePresentationSessionId(session.sessionId)) {
    return undefined;
  }
  return (
    parseGxserverPresentationProjectSessionId(session.sessionId)?.sessionId ??
    normalizeNonEmptyString(session.sessionId)
  );
}

export function localGxserverProjectIdForSidebarSession(
  session: SidebarSessionItem,
  presentation: GxserverPresentationSnapshot | undefined,
): string | undefined {
  const scopedSession = parseGxserverPresentationProjectSessionId(session.sessionId);
  if (scopedSession?.projectId) {
    return scopedSession.projectId;
  }
  const sessionId = localGxserverSessionIdForSidebarSession(session);
  return sessionId
    ? presentation?.sessions.find((candidate) => candidate.sessionId === sessionId)?.projectId
    : undefined;
}

export function isNativeAppShotAgentSession(
  session: SidebarSessionItem | undefined,
): session is SidebarSessionItem {
  if (!session) {
    return false;
  }
  if (session.sessionKind !== "terminal" || session.isSleeping === true) {
    return false;
  }
  if (session.lifecycleState === "sleeping" || session.isLive !== true) {
    return false;
  }
  return Boolean(session.agentIcon);
}

export function normalizeGpuiNativeAppShotCapture(value: unknown): GpuiNativeAppShotCapture | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    record.type !== GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const appName = normalizeGpuiNativeAppShotString(record.appName, 256);
  const imagePath = normalizeGpuiNativeAppShotImagePath(record.imagePath);
  if (!appName || !imagePath) {
    return undefined;
  }
  const bundleIdentifier = normalizeGpuiNativeAppShotString(record.bundleIdentifier, 256);
  const windowTitle = normalizeGpuiNativeAppShotString(record.windowTitle, 512);
  const windowWidth = normalizeGpuiNativeAppShotDimension(record.windowWidth);
  const windowHeight = normalizeGpuiNativeAppShotDimension(record.windowHeight);
  const trigger = normalizeGpuiNativeAppShotTrigger(record.trigger);
  const appShot: GpuiNativeAppShotCapture = {
    appName,
    imagePath,
  };
  if (bundleIdentifier) {
    appShot.bundleIdentifier = bundleIdentifier;
  }
  if (windowTitle) {
    appShot.windowTitle = windowTitle;
  }
  if (windowWidth) {
    appShot.windowWidth = windowWidth;
  }
  if (windowHeight) {
    appShot.windowHeight = windowHeight;
  }
  if (trigger) {
    appShot.trigger = trigger;
  }
  return appShot;
}

export function normalizeGpuiNativeAppShotImagePath(value: unknown): string | undefined {
  const path = normalizeGpuiNativeAppShotString(value, 4096);
  if (!path || (!path.startsWith("~/") && !path.startsWith("/"))) {
    return undefined;
  }
  return path;
}

export function normalizeGpuiNativeAppShotString(value: unknown, maxLength: number): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const text = value.trim();
  if (!text || text.length > maxLength || /[\u0000-\u001f\u007f]/u.test(text)) {
    return undefined;
  }
  return text;
}

export function normalizeGpuiNativeAppShotDimension(value: unknown): number | undefined {
  if (typeof value !== "number" || !Number.isInteger(value) || value <= 0 || value > 100_000) {
    return undefined;
  }
  return value;
}

export function normalizeGpuiNativeAppShotTrigger(value: unknown): string | undefined {
  const trigger = normalizeGpuiNativeAppShotString(value, 80);
  return trigger === "both-command" ||
    trigger === "both-shift" ||
    trigger === "both-option" ||
    trigger === "double-left-shift" ||
    trigger === "double-left-option"
    ? trigger
    : undefined;
}

export function formatGpuiNativeAppShotPrompt(
  appShot: GpuiNativeAppShotCapture,
  includeMetadata: boolean,
): string {
  const metadataLines = [`App: ${appShot.appName}`];
  if (appShot.bundleIdentifier) {
    metadataLines.push(`Bundle ID: ${appShot.bundleIdentifier}`);
  }
  if (appShot.windowTitle) {
    metadataLines.push(`Window title: ${appShot.windowTitle}`);
  }
  if (appShot.windowWidth && appShot.windowHeight) {
    metadataLines.push(`Window size: ${appShot.windowWidth} x ${appShot.windowHeight} px`);
  }
  /*
  CDXC:GPUIAppShots 2026-06-25-23:07:
  GPUI formats App Shot prompts using only native-supplied app/window metadata and the resolved Ghostex image-directory display path. The prompt must not include OCR, Accessibility text, DOM text, terminal content, stdout/stderr, commands, URLs, or renderer-supplied file paths.

  CDXC:GPUIAppShots 2026-06-29-01:29:
  Superseded by 2026-06-29-02:59.

  CDXC:GPUIAppShots 2026-06-29-02:59:
  App Shot prompt text should paste only the image link by default, with no intro sentence, no closing instruction, no blank spacer lines, and one newline of padding before and after. Add WindowServer metadata only when the Settings App Shots metadata toggle is enabled.
  */
  const promptLines = [`[Image #1](${appShot.imagePath})`];
  if (includeMetadata) {
    promptLines.push("Metadata:", ...metadataLines);
  }
  return `\n${promptLines.join("\n")}\n`;
}