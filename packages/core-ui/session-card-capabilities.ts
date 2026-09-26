import { getSidebarSessionLifecycleState, type SidebarSessionItem } from '@/packages/shared/session-grid-contract';
import { resolveSessionChatTranscriptAgent } from '@/packages/shared/session-chat';

export type SidebarSessionContextMenuEligibilityInput = {
  isProjectSessionListMoreRow: boolean;
  isRemoteSession: boolean;
  session: SidebarSessionItem | undefined;
  debuggingMode: boolean;
};

export type SidebarSessionContextMenuEligibility = {
  canCloseAfterDone: boolean;
  canCopyAttachCommand: boolean;
  canCopyResumeCommand: boolean;
  canCopySessionDetails: boolean;
  canDelayedSend: boolean;
  canExportTranscript: boolean;
  canForkSession: boolean;
  canFullReloadSession: boolean;
  canGenerateSessionTitle: boolean;
  /**
   * CDXC:SessionNotes 2026-08-24:
   * Notes are keyed by the session's provider conversation id, so a row that
   * has not captured one yet has nothing to file a note against.
   */
  canOpenSessionNote: boolean;
  canPinSession: boolean;
  canPopOutPane: boolean;
  canRenameSession: boolean;
  canSleepSession: boolean;
  /**
   * CDXC:Workarea 2026-09-04 DECISION:
   * User: Advanced > Split Right opens the session in a pane to the right of
   * the focused agents pane, for local and remote machine rows alike. The Rust
   * workspace owns pane topology, so the item needs the GPUI bridge and is
   * hidden in the web app.
   */
  canSplitSessionRight: boolean;
  canTagSession: boolean;
  isBrowserSession: boolean;
};

export function getSidebarSessionContextMenuEligibility({
  isProjectSessionListMoreRow,
  isRemoteSession,
  session,
  debuggingMode,
}: SidebarSessionContextMenuEligibilityInput): SidebarSessionContextMenuEligibility {
  const isBrowserSession = isSidebarBrowserSession(session);
  const hasSession = session !== undefined;
  const isDraftSession = session?.isDraft === true;
  const isConcreteSessionRow = hasSession && !isProjectSessionListMoreRow;
  const canUseTerminalAgentMenuAction = isConcreteSessionRow && !isBrowserSession;

  /*
   * CDXC:RemoteMachines 2026-06-30-15:22:
   * Remote session rows share the local context-menu renderer, but local AppKit and host-timer actions must opt in through explicit row capabilities. Keep ordinary gxserver-backed actions visible from the remote group signal while avoiding frontend guesses for Pop Out Pane, Delayed Send, and Close After Done.
   */
  return {
    canCloseAfterDone:
      canUseTerminalAgentMenuAction && hasSession && supportsCloseAfterDoneMenuAction(session, isRemoteSession),
    canCopyAttachCommand:
      debuggingMode &&
      canUseTerminalAgentMenuAction &&
      Boolean(session?.sessionPersistenceProvider && session.sessionPersistenceName),
    canCopyResumeCommand:
      debuggingMode && canUseTerminalAgentMenuAction && hasSession && supportsResumeCommandCopy(session),
    canCopySessionDetails: isConcreteSessionRow,
    canDelayedSend:
      canUseTerminalAgentMenuAction && hasSession && supportsDelayedSendMenuAction(session, isRemoteSession),
    canExportTranscript:
      canUseTerminalAgentMenuAction && hasSession && !isDraftSession && supportsTranscriptExport(session),
    /*
     * CDXC:Drafts 2026-08-28:
     * A draft has no conversation and no prompt yet, so Fork has nothing to
     * fork from and Full reload has nothing to reload into: both would only
     * ever produce an empty agent. Hide them here — the ONE resolver both the
     * V1 card menu and the V2 row menu read — so the two menus cannot disagree.
     * Rename, Sleep, Pin, Tag, and Close stay available on drafts.
     */
    canForkSession: canUseTerminalAgentMenuAction && hasSession && !isDraftSession && supportsFork(session),
    canSplitSessionRight:
      canUseTerminalAgentMenuAction && hasSession && !isDraftSession && gpuiWorkspaceTerminalFocusBridgeAvailable(),
    canFullReloadSession:
      canUseTerminalAgentMenuAction &&
      hasSession &&
      !isDraftSession &&
      supportsFullReloadMenuAction(session, isRemoteSession),
    canGenerateSessionTitle:
      canUseTerminalAgentMenuAction &&
      hasSession &&
      supportsGeneratedName(session) &&
      Boolean(session.firstUserMessage?.trim()),
    canOpenSessionNote: canUseTerminalAgentMenuAction && Boolean(session?.agentSessionId?.trim()),
    canPinSession: isConcreteSessionRow,
    canPopOutPane:
      isConcreteSessionRow &&
      hasSession &&
      supportsPopOutPaneMenuAction(session, {
        isBrowserSession,
        isRemoteSession,
      }),
    canRenameSession: canUseTerminalAgentMenuAction,
    canSleepSession: isConcreteSessionRow && (canSleepSidebarSession(session) || canWakeSidebarSession(session)),
    canTagSession: canUseTerminalAgentMenuAction,
    isBrowserSession,
  };
}

export function isSidebarBrowserSession(session: SidebarSessionItem | undefined): boolean {
  return session?.sessionKind === 'browser' || session?.kind === 'browser';
}

export function canSleepSidebarSession(session: SidebarSessionItem | undefined): boolean {
  /*
  CDXC:ContextMenus 2026-06-07-13:34:
  Sleep below targets every running session, including browser panes. Stopped
  history can remain visible when pinned, tagged, or favorited, but sleeping it
  would reactivate that history as a sleeping sidebar row.
  */
  return session !== undefined && getSidebarSessionLifecycleState(session) === 'running';
}

export function canWakeSidebarSession(session: SidebarSessionItem | undefined): boolean {
  /*
   * CDXC:Sessions 2026-07-01-18:33:
   * Wake selected mirrors Sleep selected and targets only rows that are
   * actually parked or sleeping, avoiding no-op wake messages for active
   * terminal, agent, and browser sessions.
   */
  return session !== undefined && getSidebarSessionLifecycleState(session) === 'sleeping';
}

export function supportsResumeCommandCopy(session: SidebarSessionItem): boolean {
  /**
   * CDXC:SessionSleep 2026-04-27-08:04
   * Match agent-tiler context-menu visibility: Copy resume is only shown for
   * built-in agents with known resume or resume-selection CLI behavior.
   *
   * CDXC:AgentProviders 2026-05-20-08:20:
   * Cursor resume uses stored chat UUIDs or a local title lookup fallback, so
   * Cursor CLI cards expose the same copy-resume affordance as Codex and Pi.
   */
  return (
    session.agentIcon === 'codex' ||
    session.agentIcon === 'claude' ||
    session.agentIcon === 'copilot' ||
    session.agentIcon === 'gemini' ||
    session.agentIcon === 'opencode' ||
    session.agentIcon === 'pi' ||
    (session.agentName === 'zcode' && Boolean(session.agentSessionId)) ||
    session.agentIcon === 'cursor-cli' ||
    session.agentIcon === 'antigravity-cli'
  );
}

export function gpuiWorkspaceTerminalFocusBridgeAvailable(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }
  const bridge = (window as { ghostexGpui?: { postWorkspaceTerminalFocus?: unknown } }).ghostexGpui;
  return typeof bridge?.postWorkspaceTerminalFocus === 'function';
}

export function supportsFork(session: SidebarSessionItem): boolean {
  /**
   * CDXC:AgentProviders 2026-05-08-09:42
   * Pi exposes a real `--fork <session>` CLI path once ghostex has captured the
   * Pi session id/path, so Pi cards should show the same one-click Fork action
   * as Codex in the session context menu.
   */
  return session.agentIcon === 'codex' || session.agentIcon === 'claude' || session.agentIcon === 'pi';
}

export function supportsTranscriptExport(session: SidebarSessionItem): boolean {
  return resolveSessionChatTranscriptAgent(session.agentName, session.agentIcon) !== null;
}

export function supportsGeneratedName(session: SidebarSessionItem): boolean {
  /**
   * CDXC:AgentProviders 2026-05-08-16:18
   * Pi cards should expose the same right-click Generate Title action as Codex
   * once the first user message has been captured. The native rename path
   * already switches Pi to `/name <title>`, so the menu gate should include Pi
   * instead of creating a Pi-only title-generation command.
   *
   * Antigravity takes `/rename <title>`, the default rename command.
   */
  return (
    session.agentIcon === 'codex' ||
    session.agentIcon === 'claude' ||
    session.agentIcon === 'pi' ||
    session.agentIcon === 'antigravity-cli'
  );
}

export function supportsDelayedSendMenuAction(session: SidebarSessionItem, isRemoteSession: boolean): boolean {
  if (isRemoteSession) {
    return session.canScheduleDelayedSend === true;
  }

  return true;
}

export function supportsCloseAfterDoneMenuAction(session: SidebarSessionItem, isRemoteSession: boolean): boolean {
  if (isRemoteSession) {
    return session.canToggleCloseAfterDone === true;
  }

  return true;
}

export function supportsFullReloadMenuAction(session: SidebarSessionItem, isRemoteSession: boolean): boolean {
  if (isRemoteSession) {
    return session.sessionKind === 'terminal';
  }

  return supportsFullReload(session);
}

export function supportsPopOutPaneMenuAction(
  session: SidebarSessionItem,
  {
    isBrowserSession,
    isRemoteSession,
  }: {
    isBrowserSession: boolean;
    isRemoteSession: boolean;
  }
): boolean {
  if (isRemoteSession) {
    return session.canPopOutPane === true && getSidebarSessionLifecycleState(session) === 'running';
  }

  return supportsPopOutPane(session, isBrowserSession);
}

export function supportsPopOutPane(session: SidebarSessionItem, isBrowserSession: boolean): boolean {
  /**
   * CDXC:Workarea 2026-05-19-10:15:
   * Sidebar context menus expose pop-out for browser panes and agent terminal
   * sessions. Sleeping sessions dispose their native surface and cannot remain
   * in a detached window.
   */
  if (getSidebarSessionLifecycleState(session) !== 'running') {
    return false;
  }

  if (isBrowserSession) {
    return true;
  }

  return session.sessionKind === 'terminal' && Boolean(session.agentIcon);
}

export function supportsFullReload(session: SidebarSessionItem): boolean {
  /**
   * CDXC:SessionSleep 2026-04-27-08:04
   * Match agent-tiler context-menu visibility: Full reload is only shown for
   * agent sessions that can be recreated and resumed programmatically.
   *
   * CDXC:AgentProviders 2026-05-08-16:18
   * Pi has a restorable CLI identity through its captured session id/path, so
   * right-click Full reload should be visible on Pi cards like it is for Codex.
   *
   * CDXC:AgentProviders 2026-05-20-08:20:
   * Cursor cards can full-reload through stored chat UUIDs or trusted titles
   * resolved from the local Cursor chat store for the active project.
   *
   * CDXC:AgentProviders 2026-09-03:
   * Antigravity resumes only by conversation id (`agy --conversation <id>`),
   * which its hooks report; without one a reload could only start a fresh
   * conversation, so the card shows Full reload once the id is captured.
   */
  if (session.agentIcon === 'antigravity-cli') {
    return Boolean(session.agentSessionId?.trim());
  }
  return (
    session.agentIcon === 'codex' ||
    session.agentIcon === 'claude' ||
    session.agentIcon === 'opencode' ||
    session.agentIcon === 'pi' ||
    session.agentIcon === 'cursor-cli'
  );
}

export type SidebarBulkSessionContextMenuAvailability = {
  closableSessionIds: string[];
  fullReloadableSessionIds: string[];
  parkableSessionIds: string[];
  pinnableSessionIds: string[];
  sleepableSessionIds: string[];
  taggableSessionIds: string[];
  unparkableSessionIds: string[];
  unpinnableSessionIds: string[];
  wakeableSessionIds: string[];
};

export function getSidebarBulkSessionContextMenuAvailability({
  enableSessionParking,
  sessionIds,
  sessionsById,
}: {
  enableSessionParking: boolean;
  sessionIds: readonly string[];
  sessionsById: Record<string, SidebarSessionItem | undefined>;
}): SidebarBulkSessionContextMenuAvailability {
  /*
   * CDXC:Sessions 2026-07-01-18:33:
   * Bulk session context menus should show only actions that can run over the
   * current selected rows without guessing. Filter each action to eligible
   * concrete sessions and let the action handler target exactly that subset.
   */
  const concreteSessionIds: string[] = [];
  const seenSessionIds = new Set<string>();
  for (const sessionId of sessionIds) {
    if (seenSessionIds.has(sessionId) || !sessionsById[sessionId]) {
      continue;
    }
    seenSessionIds.add(sessionId);
    concreteSessionIds.push(sessionId);
  }

  const sessionForId = (sessionId: string) => sessionsById[sessionId];
  const parkingSessionIds = enableSessionParking
    ? concreteSessionIds.filter((sessionId) => !isSidebarBrowserSession(sessionsById[sessionId]!))
    : [];
  return {
    closableSessionIds: concreteSessionIds,
    fullReloadableSessionIds: concreteSessionIds.filter((sessionId) =>
      supportsSelectedSessionFullReload(sessionForId(sessionId), sessionId)
    ),
    parkableSessionIds: parkingSessionIds.filter((sessionId) => sessionForId(sessionId)?.isParked !== true),
    pinnableSessionIds: concreteSessionIds.filter((sessionId) => sessionForId(sessionId)?.isPinned !== true),
    sleepableSessionIds: concreteSessionIds.filter((sessionId) => canSleepSidebarSession(sessionForId(sessionId))),
    taggableSessionIds: concreteSessionIds.filter((sessionId) => canTagSelectedSidebarSession(sessionForId(sessionId))),
    unparkableSessionIds: parkingSessionIds.filter((sessionId) => sessionForId(sessionId)?.isParked === true),
    unpinnableSessionIds: concreteSessionIds.filter((sessionId) => sessionForId(sessionId)?.isPinned === true),
    wakeableSessionIds: concreteSessionIds.filter((sessionId) => canWakeSidebarSession(sessionForId(sessionId))),
  };
}

function canTagSelectedSidebarSession(session: SidebarSessionItem | undefined): boolean {
  return Boolean(session) && !isSidebarBrowserSession(session);
}

function supportsSelectedSessionFullReload(session: SidebarSessionItem | undefined, sessionId: string): boolean {
  if (!session || isSidebarBrowserSession(session)) {
    return false;
  }

  if (isRemotePresentationSidebarSessionId(sessionId)) {
    return session.sessionKind === 'terminal';
  }

  return supportsFullReload(session);
}

function isRemotePresentationSidebarSessionId(sessionId: string): boolean {
  return /^remote:[^:]+:session:[^:]+:.+$/u.test(sessionId);
}
