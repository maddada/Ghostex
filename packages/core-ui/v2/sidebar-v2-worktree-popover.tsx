import { useEffect, useMemo, useRef, useState } from "react";
import type { SidebarAgentButton } from "../../shared/sidebar-agents";
import { formatWorktreePathForDisplay } from "../../shared/sidebar-v2-worktree-cleanup";
import { SidebarContextMenuPortal } from "../sidebar-context-menu-portal";
import type { WebviewApi } from "../webview-api";

/*
 * CDXC:SidebarV2Worktree 2026-07-29:
 * The compact worktree popover that replaces the full-window Add Worktree modal
 * for Sidebar V2.
 *
 * Why a popover and not the existing modal: the modal lives in a native
 * child-window host (`openAppModal`), which means it cannot render in the
 * sidebar document, cannot be driven in Storybook, and answers its branch
 * request through the app-modal event route. V2's creation flow starts from a
 * sidebar button and should stay in the sidebar, so it rides
 * `SidebarContextMenuPortal` — the same surface the snooze popover and the V2
 * context menu already use, which owns viewport clamping, Escape/click-away
 * dismissal, and the native menu-opened/closed notifications.
 *
 * The branch and existing-worktree lists come from the SAME host request the
 * old modal sends (`requestProjectWorktrees`); the host now delivers the answer
 * to both surfaces. Until that answer lands — and if it fails — the base branch
 * degrades to a plain text field, so the flow is never blocked on a probe.
 */

export type SidebarV2WorktreePopoverPosition = {
  clientX: number;
  clientY: number;
};

export type SidebarV2WorktreeDraft = {
  agentId?: string;
  baseBranch?: string;
  existingWorktreePath?: string;
  firstPrompt?: string;
  startFromOrigin?: boolean;
};

/** One selectable base branch, normalized from the host's answer. */
type WorktreeBranchOption = {
  isCurrent: boolean;
  name: string;
};

/** One existing checkout the "Open existing" section can spawn into. */
type WorktreeExistingOption = {
  branch?: string;
  path: string;
};

type ProjectWorktreesResult = {
  branches?: unknown;
  error?: unknown;
  ok?: unknown;
  requestId?: unknown;
  type?: unknown;
  worktrees?: unknown;
};

export type SidebarV2WorktreeEventSource = Pick<
  Window,
  "addEventListener" | "removeEventListener"
>;

export type SidebarV2WorktreePopoverProps = {
  agents: readonly SidebarAgentButton[];
  /** Agent pre-selected when the popover opens: last used, else the configured
      default, else the first agent with a command. */
  defaultAgentId?: string;
  /** Inline failure text from the host's answer to a submit. */
  errorMessage?: string;
  /** Submit is in flight; the form disables until the host answers. */
  isPending: boolean;
  /** Where `projectWorktreesResult` arrives. Defaults to `window`; gpui hands
      the sidebar its own message source, so the caller passes that through. */
  messageSource?: SidebarV2WorktreeEventSource;
  onDismiss: () => void;
  /** Ask the host for branches + existing worktrees for this project. */
  onRequestWorktrees: (requestId: string) => void;
  onSubmit: (draft: SidebarV2WorktreeDraft) => void;
  position: SidebarV2WorktreePopoverPosition;
  /** Project the worktree is cut from, for the popover heading. */
  projectLabel?: string;
  vscode: WebviewApi;
};

function normalizeBranchOptions(candidate: unknown): WorktreeBranchOption[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  const options: WorktreeBranchOption[] = [];
  const seen = new Set<string>();
  for (const entry of candidate) {
    const record =
      typeof entry === "string"
        ? { current: false, name: entry }
        : entry && typeof entry === "object"
          ? (entry as { current?: unknown; name?: unknown })
          : undefined;
    const name = typeof record?.name === "string" ? record.name.trim() : "";
    if (!name || seen.has(name)) {
      continue;
    }
    seen.add(name);
    options.push({ isCurrent: record?.current === true, name });
  }
  return options;
}

function normalizeExistingOptions(candidate: unknown): WorktreeExistingOption[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  const options: WorktreeExistingOption[] = [];
  const seen = new Set<string>();
  for (const entry of candidate) {
    if (!entry || typeof entry !== "object") {
      continue;
    }
    const record = entry as { branch?: unknown; path?: unknown };
    const path = typeof record.path === "string" ? record.path.trim() : "";
    if (!path || seen.has(path)) {
      continue;
    }
    seen.add(path);
    options.push({
      branch: typeof record.branch === "string" && record.branch.trim() ? record.branch.trim() : undefined,
      path,
    });
  }
  return options;
}

function resolveInitialAgentId(
  agents: readonly SidebarAgentButton[],
  defaultAgentId: string | undefined,
): string {
  const configured = agents.filter((agent) => agent.command?.trim());
  const preferred = configured.find((agent) => agent.agentId === defaultAgentId);
  return preferred?.agentId ?? configured[0]?.agentId ?? "";
}

export function SidebarV2WorktreePopover({
  agents,
  defaultAgentId,
  errorMessage,
  isPending,
  messageSource,
  onDismiss,
  onRequestWorktrees,
  onSubmit,
  position,
  projectLabel,
  vscode,
}: SidebarV2WorktreePopoverProps) {
  const configuredAgents = useMemo(
    () => agents.filter((agent) => agent.command?.trim()),
    [agents],
  );
  const [agentId, setAgentId] = useState(() =>
    resolveInitialAgentId(agents, defaultAgentId),
  );
  const [baseBranch, setBaseBranch] = useState("");
  const [startFromOrigin, setStartFromOrigin] = useState(false);
  const [firstPrompt, setFirstPrompt] = useState("");
  const [branches, setBranches] = useState<WorktreeBranchOption[]>([]);
  const [existingWorktrees, setExistingWorktrees] = useState<WorktreeExistingOption[]>([]);
  const [listError, setListError] = useState<string>();
  const requestIdRef = useRef<string | undefined>(undefined);

  /*
   * The list is requested exactly once, when the popover opens. Re-requesting
   * on every render would hammer git in a repo with many worktrees, and the
   * popover's lifetime is short enough that a stale-by-seconds branch list is
   * not a correctness problem — gxserver re-resolves the branch at create time.
   */
  useEffect(() => {
    const requestId = `sidebar-v2-worktrees-${Date.now().toString(36)}-${Math.random()
      .toString(36)
      .slice(2, 8)}`;
    requestIdRef.current = requestId;
    onRequestWorktrees(requestId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const source: SidebarV2WorktreeEventSource = messageSource ?? window;
    const handleMessage = (event: Event) => {
      const data = (event as MessageEvent<ProjectWorktreesResult | undefined>).data;
      if (!data || typeof data !== "object" || data.type !== "projectWorktreesResult") {
        return;
      }
      if (data.requestId !== requestIdRef.current) {
        return;
      }
      if (data.ok !== true) {
        setListError(
          typeof data.error === "string" && data.error.trim()
            ? data.error
            : "Could not load branches.",
        );
        return;
      }
      setListError(undefined);
      const branchOptions = normalizeBranchOptions(data.branches);
      setBranches(branchOptions);
      setBaseBranch((current) =>
        current || branchOptions.find((branch) => branch.isCurrent)?.name || branchOptions[0]?.name || "",
      );
      setExistingWorktrees(normalizeExistingOptions(data.worktrees));
    };
    source.addEventListener("message", handleMessage);
    return () => {
      source.removeEventListener("message", handleMessage);
    };
  }, [messageSource]);

  const submitNew = () => {
    if (isPending) {
      return;
    }
    onSubmit({
      agentId: agentId || undefined,
      baseBranch: baseBranch.trim() || undefined,
      firstPrompt: firstPrompt.trim() || undefined,
      startFromOrigin,
    });
  };

  return (
    <SidebarContextMenuPortal
      menuClassName="session-context-menu sidebar-v2-worktree-popover"
      menuStyle={{ left: `${position.clientX}px`, top: `${position.clientY}px` }}
      onDismiss={onDismiss}
      vscode={vscode}
    >
      <div className="sidebar-v2-worktree-form">
        <div className="sidebar-v2-worktree-heading">
          New worktree session
          {projectLabel ? (
            <span className="sidebar-v2-worktree-project">{projectLabel}</span>
          ) : null}
        </div>

        <label className="sidebar-v2-worktree-field">
          <span className="sidebar-v2-worktree-label">Agent</span>
          <select
            aria-label="Worktree agent"
            className="sidebar-v2-worktree-select"
            data-worktree-field="agent"
            disabled={isPending}
            onChange={(event) => setAgentId(event.target.value)}
            value={agentId}
          >
            {configuredAgents.length === 0 ? <option value="">No configured agents</option> : null}
            {configuredAgents.map((agent) => (
              <option key={agent.agentId} value={agent.agentId}>
                {agent.name}
              </option>
            ))}
          </select>
        </label>

        <label className="sidebar-v2-worktree-field">
          <span className="sidebar-v2-worktree-label">Base branch</span>
          {/*
           * The select appears only once the host answered. Before that (and
           * after a failed probe) the same field is a free text input, so a
           * repository whose branch list cannot be read still gets a worktree
           * — gxserver falls back to the default branch for an empty value.
           */}
          {branches.length > 0 ? (
            <select
              aria-label="Base branch"
              className="sidebar-v2-worktree-select"
              data-worktree-field="baseBranch"
              disabled={isPending}
              onChange={(event) => setBaseBranch(event.target.value)}
              value={baseBranch}
            >
              {branches.map((branch) => (
                <option key={branch.name} value={branch.name}>
                  {branch.name}
                </option>
              ))}
            </select>
          ) : (
            <input
              aria-label="Base branch"
              className="sidebar-v2-worktree-input"
              data-worktree-field="baseBranch"
              disabled={isPending}
              onChange={(event) => setBaseBranch(event.target.value)}
              placeholder="Default branch"
              type="text"
              value={baseBranch}
            />
          )}
        </label>

        <label className="sidebar-v2-worktree-toggle">
          <input
            aria-label="Start from origin"
            checked={startFromOrigin}
            data-worktree-field="startFromOrigin"
            disabled={isPending}
            onChange={(event) => setStartFromOrigin(event.target.checked)}
            type="checkbox"
          />
          <span>Start from origin</span>
        </label>

        <label className="sidebar-v2-worktree-field">
          <span className="sidebar-v2-worktree-label">First prompt (optional)</span>
          <textarea
            aria-label="First prompt"
            className="sidebar-v2-worktree-textarea"
            data-worktree-field="firstPrompt"
            disabled={isPending}
            onChange={(event) => setFirstPrompt(event.target.value)}
            rows={3}
            value={firstPrompt}
          />
        </label>

        {listError ? <p className="sidebar-v2-worktree-note">{listError}</p> : null}
        {errorMessage ? (
          <p className="sidebar-v2-worktree-error" role="alert">
            {errorMessage}
          </p>
        ) : null}

        <button
          className="sidebar-v2-worktree-submit"
          data-pending={String(isPending)}
          disabled={isPending}
          onClick={submitNew}
          type="button"
        >
          {isPending ? "Creating…" : "Create worktree session"}
        </button>

        {existingWorktrees.length > 0 ? (
          <div className="sidebar-v2-worktree-existing">
            <div className="sidebar-v2-worktree-label">Open existing worktree</div>
            {existingWorktrees.map((worktree) => (
              <button
                className="sidebar-v2-worktree-existing-item"
                data-worktree-path={worktree.path}
                disabled={isPending}
                key={worktree.path}
                onClick={() => {
                  if (isPending) {
                    return;
                  }
                  /*
                   * Open-existing skips creation entirely, so it carries no
                   * base branch and no origin flag — those describe a checkout
                   * that is about to be cut, and this one already exists.
                   */
                  onSubmit({
                    agentId: agentId || undefined,
                    existingWorktreePath: worktree.path,
                    firstPrompt: firstPrompt.trim() || undefined,
                  });
                }}
                type="button"
              >
                <span className="sidebar-v2-worktree-existing-name">
                  {formatWorktreePathForDisplay(worktree.path)}
                </span>
                {worktree.branch ? (
                  <span className="sidebar-v2-worktree-existing-branch">{worktree.branch}</span>
                ) : null}
              </button>
            ))}
          </div>
        ) : null}
      </div>
    </SidebarContextMenuPortal>
  );
}
