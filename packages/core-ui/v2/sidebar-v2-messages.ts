import type { SidebarSessionItem, SidebarSessionTag } from '../../shared/session-grid-contract';
import { openAppModal } from '../app-modal-host-bridge';
import type { WebviewApi } from '../webview-api';

/*
 * CDXC:SidebarV2 2026-07-29:
 * Sidebar V2 owns no host channel of its own. Every mutation it can perform is
 * declared here as a thin wrapper over the SAME `SidebarToExtensionMessage`
 * variants the classic sidebar posts, so switching sidebars can never change
 * what the host is asked to do. If a V2 affordance needs a new host behavior,
 * the message belongs on the shared contract first — never as a V2-only side
 * channel.
 */

/** Activate a session. Identical for terminal, agent, and browser rows:
    the host resolves the surface from the session id, exactly as V1 relies on. */
export function postSidebarV2FocusSession(vscode: WebviewApi, sessionId: string): void {
  vscode.postMessage({ sessionId, type: 'focusSession' });
}

/** Zoom the session's pane tab group (context-menu Focus in V1). */
export function postSidebarV2FocusSessionMode(vscode: WebviewApi, sessionId: string): void {
  vscode.postMessage({ sessionId, type: 'focusSessionMode' });
}

/**
 * Commit an inline rename. V1's row rename opens the full-window modal and the
 * modal posts this exact message, so V2's inline editor is the same command
 * with the dialog step removed.
 */
export function postSidebarV2RenameSession(vscode: WebviewApi, sessionId: string, title: string): void {
  vscode.postMessage({ sessionId, title, type: 'renameSession' });
}

/**
 * CDXC:SidebarV2ContextMenuParity 2026-07-30:
 * Generate Title is NOT a command of its own. V1 retitles through the ordinary
 * rename path with `shouldGenerateTitle`, because that host path already owns
 * the summarization, the agent-CLI `/name` sync, and the "Generating title…"
 * card state. V2 posts the identical payload — the captured 1st user message as
 * the rename input — so the host cannot tell the two sidebars apart.
 */
export function postSidebarV2GenerateSessionTitle(
  vscode: WebviewApi,
  sessionId: string,
  firstUserMessage: string
): void {
  vscode.postMessage({
    sessionId,
    shouldGenerateTitle: true,
    title: firstUserMessage,
    type: 'renameSession',
  });
}

export function postSidebarV2SetSessionSleeping(vscode: WebviewApi, sessionId: string, sleeping: boolean): void {
  vscode.postMessage({ sessionId, sleeping, type: 'setSessionSleeping' });
}

export function postSidebarV2SetSessionPinned(vscode: WebviewApi, sessionId: string, pinned: boolean): void {
  vscode.postMessage({ pinned, sessionId, type: 'setSessionPinned' });
}

export function postSidebarV2SetSessionParked(vscode: WebviewApi, sessionId: string, parked: boolean): void {
  vscode.postMessage({ parked, sessionId, type: 'setSessionParked' });
}

/*
 * CDXC:SidebarV2ContextMenuParity 2026-07-30:
 * The V1 session-menu commands the V2 row menu now also offers. Each one is the
 * SAME contract variant `sortable-session-card` posts, with the same payload
 * shape, so parity here is a wire-level fact rather than a resemblance. None of
 * them is optimistic: the host answers with a presentation delta (tag, fork,
 * reload) or with a clipboard/modal side effect it owns outright.
 */
export function postSidebarV2CopyResumeCommand(vscode: WebviewApi, sessionId: string): void {
  vscode.postMessage({ sessionId, type: 'copyResumeCommand' });
}

export function postSidebarV2CopyAttachCommand(vscode: WebviewApi, sessionId: string): void {
  vscode.postMessage({ sessionId, type: 'copyAttachCommand' });
}

/** The clipboard text is built in the sidebar (it is the rendered row's own
    facts) and only the finished string travels to the host, exactly as V1. */
export function postSidebarV2CopySessionDetails(vscode: WebviewApi, sessionId: string, detailsText: string): void {
  vscode.postMessage({ detailsText, sessionId, type: 'copySessionDetails' });
}

export function postSidebarV2ToggleCloseAfterDone(vscode: WebviewApi, sessionId: string): void {
  vscode.postMessage({ sessionId, type: 'toggleCloseAfterDone' });
}

export function postSidebarV2ForkSession(vscode: WebviewApi, sessionId: string): void {
  vscode.postMessage({ sessionId, type: 'forkSession' });
}

export function postSidebarV2FullReloadSession(vscode: WebviewApi, sessionId: string): void {
  vscode.postMessage({ sessionId, type: 'fullReloadSession' });
}

/*
 * CDXC:SessionAgentNotes 2026-08-24:
 * The note editor is a full-window app modal, not a host command, so this is
 * the one V2 "message" helper that goes through the app-modal bridge instead of
 * `vscode`. It exists here anyway for the same reason the rest of this file
 * does: V1 builds this payload inline in `sortable-session-card`, and the two
 * sidebars must open the SAME dialog with the SAME fields — the host cannot
 * tell which one asked.
 */
export function openSidebarV2SessionNoteModal(
  session: Pick<SidebarSessionItem, 'sessionId' | 'sessionNote'>,
  sessionTitle: string
): void {
  openAppModal({
    initialNote: session.sessionNote ?? '',
    modal: 'sessionNote',
    sessionId: session.sessionId,
    sessionTitle,
    type: 'open',
  });
}

/** `undefined` is the CLEAR: the contract's explicit "no tag" value is `null`,
    which is what re-picking the session's current tag sends. */
export function postSidebarV2SetSessionTag(
  vscode: WebviewApi,
  sessionId: string,
  tag: SidebarSessionTag | undefined
): void {
  vscode.postMessage({ sessionId, sessionTag: tag ?? null, type: 'setSessionTag' });
}

/*
 * CDXC:SidebarV2Lifecycle 2026-07-29:
 * Settle/snooze are the first commands V2 owns that V1 has no twin for, so they
 * are still declared here rather than inline in the components: the host
 * contract stays in one file, and the payload carries the sidebar session id
 * only — the host owns the machine/project resolution.
 *
 * None of these post an optimistic update. gxserver answers with a presentation
 * delta, and that delta is what moves the row between shelves; a local guess
 * would fight the server's guards (a working session cannot settle) and produce
 * a row that jumps out and back.
 */
export function postSidebarV2SettleSession(vscode: WebviewApi, sessionId: string): void {
  vscode.postMessage({ sessionId, type: 'settleSession' });
}

export function postSidebarV2UnsettleSession(vscode: WebviewApi, sessionId: string): void {
  vscode.postMessage({ sessionId, type: 'unsettleSession' });
}

export function postSidebarV2SnoozeSession(vscode: WebviewApi, sessionId: string, snoozedUntil: string): void {
  vscode.postMessage({ sessionId, snoozedUntil, type: 'snoozeSession' });
}

export function postSidebarV2UnsnoozeSession(vscode: WebviewApi, sessionId: string): void {
  vscode.postMessage({ sessionId, type: 'unsnoozeSession' });
}

/*
 * CDXC:SidebarV2Worktree 2026-07-29:
 * The worktree flow's two commands. They are declared here for the same reason
 * settle/snooze are: the host contract stays in one file, and the payload
 * carries sidebar ids only — the host owns project, machine, and path
 * resolution. Neither is optimistic: the created session arrives as a
 * presentation delta, and the request/result pair exists only to drive the
 * popover's pending state.
 */
export function postSidebarV2CreateWorktreeSession(
  vscode: WebviewApi,
  input: {
    agentId?: string;
    baseBranch?: string;
    existingWorktreePath?: string;
    firstPrompt?: string;
    projectId: string;
    requestId: string;
    startFromOrigin?: boolean;
  }
): void {
  vscode.postMessage({
    ...(input.agentId ? { agentId: input.agentId } : {}),
    ...(input.baseBranch ? { baseBranch: input.baseBranch } : {}),
    ...(input.existingWorktreePath ? { existingWorktreePath: input.existingWorktreePath } : {}),
    ...(input.firstPrompt ? { firstPrompt: input.firstPrompt } : {}),
    projectId: input.projectId,
    requestId: input.requestId,
    ...(input.startFromOrigin === true ? { startFromOrigin: true } : {}),
    type: 'createWorktreeSession',
  });
}

export function postSidebarV2RemoveSessionWorktree(
  vscode: WebviewApi,
  input: {
    force?: boolean;
    projectId: string;
    requestId: string;
    worktreePath: string;
  }
): void {
  vscode.postMessage({
    ...(input.force === true ? { force: true } : {}),
    projectId: input.projectId,
    requestId: input.requestId,
    type: 'removeSessionWorktree',
    worktreePath: input.worktreePath,
  });
}

/**
 * The branch/worktree list the worktree popover fills its pickers from. This is
 * the SAME request the classic Add Worktree modal sends; the host answers it
 * once and delivers the answer to both surfaces, so V2 introduced no second
 * server-side probe.
 */
export function postSidebarV2RequestProjectWorktrees(
  vscode: WebviewApi,
  input: { projectId?: string; requestId: string }
): void {
  vscode.postMessage({
    ...(input.projectId ? { projectId: input.projectId } : {}),
    requestId: input.requestId,
    type: 'requestProjectWorktrees',
  });
}

/**
 * Close deferred to a macrotask, mirroring V1's
 * `postSidebarSessionCloseInBackground`: native message delivery is
 * synchronous, so tearing the runtime down inside the click gesture makes the
 * sidebar feel stalled.
 */
export function postSidebarV2CloseSession(vscode: WebviewApi, sessionId: string): void {
  globalThis.setTimeout(() => {
    vscode.postMessage({ sessionId, type: 'closeSession' });
  }, 0);
}

/**
 * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
 * Park a project into Recent Projects — the same `closeWorkspaceProjectForGroup`
 * V1's project header menu posts, so the host's close/remove split (park to
 * Recent vs hard delete) is unchanged and remote group ids keep routing through
 * the host's remote scope resolver.
 *
 * It takes a LIST because a V2 grouped row is a logical project that can merge
 * several physical checkouts across machines. Closing only the representative
 * would leave the merged row alive on its other members, which is exactly the
 * "I closed it and it is still there" report this fixes.
 */
export function postSidebarV2CloseWorkspaceProjects(vscode: WebviewApi, groupIds: readonly string[]): void {
  for (const groupId of groupIds) {
    vscode.postMessage({ groupId, type: 'closeWorkspaceProjectForGroup' });
  }
}
