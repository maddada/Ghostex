import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ClipboardEvent as ReactClipboardEvent,
} from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { trimPromptEditorTrailingSpaces } from "../shared/prompt-editor-text";
import {
  createSidebarAgentSelectItems,
  type SidebarAgentButton,
} from "../shared/sidebar-agents";
import type { SidebarGitAction, SidebarGitChangedFile } from "../shared/sidebar-git";
import type { SidebarTheme } from "../shared/session-grid-contract";
import { ChangedFilesTree } from "./changed-files-tree";
import { summarizeChangedFiles } from "./changed-files-tree-utils";
import { ConfirmationModal } from "./confirmation-modal";
import {
  GitFileDiffControls,
  GitFileDiffPanel,
  type GitDiffViewMode,
  type GitFileDiffModalDraft,
} from "./git-file-diff-modal";

type GitCommitInlineDiffMode = "all" | "file";

type GitCommitDiffPreferences = {
  hideWhitespace: boolean;
  lineWrap: boolean;
  viewMode: GitDiffViewMode;
};

const GIT_COMMIT_DIFF_PREFERENCES_STORAGE_KEY =
  "ghostex.gitCommitModal.diffPreferences.v1";

const DEFAULT_GIT_COMMIT_DIFF_PREFERENCES: GitCommitDiffPreferences = {
  hideWhitespace: false,
  lineWrap: false,
  viewMode: "unified",
};

export type GitCommitModalDraft = {
  action?: SidebarGitAction;
  agentId?: string;
  branch?: string | null;
  changedFiles?: SidebarGitChangedFile[];
  confirmLabel: string;
  deleteWorktreeAfterDefault?: boolean;
  description: string;
  isWorktree?: boolean;
  isDefaultRef?: boolean;
  requestId: string;
  showCommitMessage?: boolean;
  suggestedBody?: string;
  suggestedSubject: string;
  worktreeName?: string;
};

export type GitCommitModalProps = {
  agents?: SidebarAgentButton[];
  draft: GitCommitModalDraft;
  isOpen: boolean;
  onCancel: (requestId: string) => void;
  onConfirm: (
    requestId: string,
    message: string,
    options: {
      agentId?: string;
      commitOnNewRef?: boolean;
      deleteWorktreeAfter: boolean;
      filePaths?: string[];
    },
  ) => void;
  onDirectMerge?: (
    requestId: string,
    message: string,
    options: {
      agentId?: string;
      deleteWorktreeAfter: boolean;
      filePaths?: string[];
    },
  ) => void;
  fileDiffDraft?: GitFileDiffModalDraft;
  onMultipleCommits: (requestId: string, agentId?: string) => void;
  onOpenFileDiff: (filePath: string, requestId: string) => void;
  onPromptAgentIdChange?: (agentId: string) => void;
  promptAgentId?: string;
  theme?: SidebarTheme;
};

export function GitCommitModal({
  agents = [],
  draft,
  isOpen,
  onCancel,
  onConfirm,
  onDirectMerge,
  fileDiffDraft,
  onMultipleCommits,
  onOpenFileDiff,
  onPromptAgentIdChange,
  promptAgentId,
  theme = "dark-1",
}: GitCommitModalProps) {
  const [message, setMessage] = useState(buildDraftMessage(draft));
  const [deleteWorktreeAfter, setDeleteWorktreeAfter] = useState(
    draft.deleteWorktreeAfterDefault === true,
  );
  const [excludedFiles, setExcludedFiles] = useState<Set<string>>(() => new Set());
  const [isEditingFiles, setIsEditingFiles] = useState(false);
  const [isDirectMergeConfirmOpen, setIsDirectMergeConfirmOpen] = useState(false);
  const [localPromptAgentId, setLocalPromptAgentId] = useState("");
  const [inlineDiffMode, setInlineDiffMode] = useState<GitCommitInlineDiffMode>("file");
  const [diffDraftCache, setDiffDraftCache] = useState<Record<string, GitFileDiffModalDraft>>(
    () => ({}),
  );
  const [allDiffLoadingFilePath, setAllDiffLoadingFilePath] = useState<string>();
  const [selectedDiffFilePath, setSelectedDiffFilePath] = useState<string>();
  const [loadingDiffFilePath, setLoadingDiffFilePath] = useState<string>();
  const [inlineDiffPreferences, setInlineDiffPreferences] =
    useState<GitCommitDiffPreferences>(readGitCommitDiffPreferences);
  const initializedDraftRequestRef = useRef<string | undefined>(undefined);
  const commandAgents = useMemo(
    () => agents.filter((agent) => agent.command?.trim()),
    [agents],
  );
  const promptAgents = useMemo(
    () => commandAgents,
    [commandAgents],
  );
  const promptAgentSelectItems = useMemo(
    () => createSidebarAgentSelectItems(promptAgents),
    [promptAgents],
  );
  const effectivePromptAgentId = promptAgentId ?? localPromptAgentId;
  const selectedPromptAgentId =
    promptAgents.find((agent) => agent.agentId === effectivePromptAgentId)?.agentId ??
    promptAgents[0]?.agentId ??
    "";
  const inlineDiffViewMode = inlineDiffPreferences.viewMode;
  const inlineDiffLineWrap = inlineDiffPreferences.lineWrap;
  const inlineDiffHideWhitespace = inlineDiffPreferences.hideWhitespace;
  const updateInlineDiffPreferences = (updates: Partial<GitCommitDiffPreferences>) => {
    setInlineDiffPreferences((currentPreferences) => {
      const nextPreferences = { ...currentPreferences, ...updates };
      writeGitCommitDiffPreferences(nextPreferences);
      return nextPreferences;
    });
  };
  const descriptionId = useId();
  const generateAgentId = useId();
  const titleId = useId();
  const isDarkTheme = getSidebarThemeVariant(theme) === "dark";
  const changedFiles = draft.changedFiles ?? [];
  const showCommitMessage = draft.showCommitMessage ?? true;
  const canDirectMerge = Boolean(draft.isWorktree && onDirectMerge);
  const selectedFiles = useMemo(
    () => changedFiles.filter((file) => !excludedFiles.has(file.path)),
    [changedFiles, excludedFiles],
  );
  const selectedStats = useMemo(() => summarizeChangedFiles(selectedFiles), [selectedFiles]);
  const allChangedStats = useMemo(() => summarizeChangedFiles(changedFiles), [changedFiles]);
  const allFilesDiffDraft = useMemo(
    () => buildAllFilesDiffDraft(changedFiles, diffDraftCache),
    [changedFiles, diffDraftCache],
  );
  const allSelected = changedFiles.length > 0 && selectedFiles.length === changedFiles.length;
  const noneSelected = changedFiles.length > 0 && selectedFiles.length === 0;

  useEffect(() => {
    if (!isOpen) {
      initializedDraftRequestRef.current = undefined;
      return;
    }
    if (initializedDraftRequestRef.current === draft.requestId) {
      return;
    }
    initializedDraftRequestRef.current = draft.requestId;

    setMessage(buildDraftMessage(draft));
    setDeleteWorktreeAfter(draft.deleteWorktreeAfterDefault === true);
    setExcludedFiles(new Set());
    setIsEditingFiles(false);
    setIsDirectMergeConfirmOpen(false);
    setInlineDiffMode("file");
    setDiffDraftCache({});
    setAllDiffLoadingFilePath(undefined);
    const initialDiffFilePath = draft.changedFiles?.[0]?.path;
    setSelectedDiffFilePath(initialDiffFilePath);
    setLoadingDiffFilePath(initialDiffFilePath);
    if (initialDiffFilePath) {
      onOpenFileDiff(initialDiffFilePath, draft.requestId);
    }
  }, [commandAgents, draft, isOpen]);

  useEffect(() => {
    if (!fileDiffDraft) {
      return;
    }
    setDiffDraftCache((currentCache) => ({
      ...currentCache,
      [fileDiffDraft.filePath]: fileDiffDraft,
    }));
    if (fileDiffDraft.filePath === loadingDiffFilePath) {
      setLoadingDiffFilePath(undefined);
    }
    if (fileDiffDraft.filePath === allDiffLoadingFilePath) {
      setAllDiffLoadingFilePath(undefined);
    }
  }, [allDiffLoadingFilePath, fileDiffDraft, loadingDiffFilePath]);

  useEffect(() => {
    if (!isOpen || inlineDiffMode !== "all" || allDiffLoadingFilePath) {
      return;
    }
    const nextMissingDiffPath = changedFiles.find(
      (file) => diffDraftCache[file.path] === undefined,
    )?.path;
    if (!nextMissingDiffPath) {
      return;
    }
    setAllDiffLoadingFilePath(nextMissingDiffPath);
    onOpenFileDiff(nextMissingDiffPath, draft.requestId);
  }, [
    allDiffLoadingFilePath,
    changedFiles,
    diffDraftCache,
    draft.requestId,
    inlineDiffMode,
    isOpen,
    onOpenFileDiff,
  ]);

  /**
   * CDXC:TitlebarGit 2026-05-28-07:47:
   * Commit messages should not carry trailing spaces at line ends. Normalize
   * pasted text in the textarea and normalize again before confirm, while a
   * fully blank message still reaches native as empty so auto-generation works.
   */
  const normalizedMessage = trimPromptEditorTrailingSpaces(message);
  const trimmedMessage = normalizedMessage.trim();
  const canConfirm = !showCommitMessage || !noneSelected;
  const canRunDirectMerge = canConfirm && selectedPromptAgentId.length > 0;
  const selectedFilePaths =
    changedFiles.length > 0 && selectedFiles.length !== changedFiles.length
      ? selectedFiles.map((file) => file.path)
      : undefined;
  const selectedDiffDraft =
    selectedDiffFilePath !== undefined
      ? diffDraftCache[selectedDiffFilePath] ??
        (fileDiffDraft && fileDiffDraft.filePath === selectedDiffFilePath ? fileDiffDraft : undefined)
      : undefined;
  const isSelectedDiffLoading =
    inlineDiffMode === "file" &&
    selectedDiffFilePath !== undefined &&
    loadingDiffFilePath === selectedDiffFilePath &&
    selectedDiffDraft === undefined;
  const activeDiffDraft = inlineDiffMode === "all" ? allFilesDiffDraft : selectedDiffDraft;
  const activeDiffFileLabel =
    inlineDiffMode === "all" ? "All files" : selectedDiffFilePath;
  const activeDiffStats =
    inlineDiffMode === "all"
      ? allChangedStats
      : {
          additions: selectedDiffDraft?.additions ?? 0,
          deletions: selectedDiffDraft?.deletions ?? 0,
        };

  const handleMessagePaste = (event: ReactClipboardEvent<HTMLTextAreaElement>) => {
    const pastedText = event.clipboardData.getData("text/plain");
    const trimmedText = trimPromptEditorTrailingSpaces(pastedText);
    if (!pastedText || trimmedText === pastedText) {
      return;
    }
    const textarea = event.currentTarget;
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    event.preventDefault();
    setMessage(`${message.slice(0, start)}${trimmedText}${message.slice(end)}`);
    window.requestAnimationFrame(() => {
      const nextSelection = start + trimmedText.length;
      textarea.setSelectionRange(nextSelection, nextSelection);
    });
  };

  const handlePromptAgentChange = (agentId: string) => {
    if (onPromptAgentIdChange) {
      onPromptAgentIdChange(agentId);
      return;
    }
    setLocalPromptAgentId(agentId);
  };

  const openInlineFileDiff = (filePath: string) => {
    setInlineDiffMode("file");
    setSelectedDiffFilePath(filePath);
    setLoadingDiffFilePath(filePath);
    onOpenFileDiff(filePath, draft.requestId);
  };

  const showAllInlineFileDiffs = () => {
    setInlineDiffMode("all");
    setLoadingDiffFilePath(undefined);
  };

  /*
   * CDXC:Worktrees 2026-05-18-23:07:
   * The git review modal must support three worktree-specific choices: file selection, skipping the commit-message field when only push/PR is needed, and deleting the temporary worktree after a successful action.
   *
   * CDXC:TitlebarGit 2026-05-24-17:41:
   * Titlebar-launched commits allow the message box to be left blank; confirmation then generates the commit subject/body from the staged selected files.
   *
   * CDXC:TitlebarGit 2026-05-25-07:40:
   * The commit review dialog should use the same shadcn Settings modal surface, typography scale, button style, checkbox treatment, and neutral dark background. User-facing copy must call the destination a branch instead of lower-level Git reference terminology.
   *
   * CDXC:TitlebarGit 2026-05-25-09:41:
   * Multiple Commits hands the current repository to an agent prompt that splits commits by file/topic. Keep it in the same footer row as the normal commit actions so the Settings-style modal never shows stacked button rows.
   *
   * CDXC:TitlebarGit 2026-05-25-10:16:
   * Changed-file rows in the commit review modal should preview the exact patch before users choose a commit action instead of jumping straight to the IDE.
   *
   * CDXC:WorktreeMerge 2026-05-27-06:25:
   * Worktree PR review keeps the commit/push/PR flow as the primary action, but the same review modal also offers an explicit merge-to-main action. Direct merge uses the same prompt-agent selector as PR creation so the modal has one clear agent choice.
   *
   * CDXC:PromptAgents 2026-05-29-10:53:
   * Commit review exposes a plain prompt-agent dropdown for generated commit
   * messages and Multiple Commits. The modal host remembers this modal-specific
   * selection until Settings -> Default Prompt Agent changes.
   *
   * CDXC:PromptAgents 2026-05-29-18:29:
   * The commit review prompt-agent selector should be a compact footer control
   * without a visible "Generate with" label, so the message editor keeps focus
   * on the commit text while generated actions still share the chosen agent.
   *
   * CDXC:AppModals 2026-05-29-19:44:
   * Session attention/activity can refresh app-modal props while commit review
   * is open. Commit drafts are reinitialized only for a new request id; later
   * agent-list updates may repair an invalid prompt agent without replacing the
   * user's edited commit message or file selection.
   *
   * CDXC:TitlebarGit 2026-06-05-20:59:
   * The commit review modal is a wider, taller two-pane workspace: the full
   * existing commit flow stays on the left, and selected file diffs render on
   * the right with display controls inside the diff overflow menu instead of a
   * second modal stacked above the review.
   *
   * CDXC:TitlebarGit 2026-06-08-04:07:
   * The commit review modal uses compact source-control review
   * controls: remove the Files and Diff headings, keep the branch summary
   * compact, place three icon-only tooltip diff controls in the diff header, and
   * use hover-only 5px transparent-gutter scrollbars on the file tree and diff
   * body without scroll-mask overflow fades.
   *
   * CDXC:TitlebarGit 2026-06-08-09:41:
   * Commit review should open directly into the review workspace without a
   * visible title/subtitle row. Let Show All concatenate every changed-file
   * patch in the diff pane, and persist diff display options globally across
   * projects and app restarts.
   *
   * CDXC:TitlebarGit 2026-06-19-15:43:
   * The commit review branch header should read as plain branch text without a
   * visible "Branch" label or pill background. Select and Show All belong below
   * the changed-files tree so they do not compete with the branch name.
   */
  return (
    <>
      <Dialog
        onOpenChange={(nextOpen) => {
          if (!nextOpen) {
            onCancel(draft.requestId);
          }
        }}
        open={isOpen}
      >
        <DialogContent
          aria-describedby={descriptionId}
          aria-labelledby={titleId}
          className={cn(
            "ghostex-settings-shadcn settings-modal-dialog command-config-modal-shadcn git-commit-modal-shadcn flex flex-col gap-0 overflow-hidden p-0 font-sans",
            isDarkTheme && "dark",
          )}
          data-sidebar-theme={theme}
        >
          <DialogTitle className="sr-only" id={titleId}>
            Commit changes
          </DialogTitle>
          <DialogDescription className="sr-only" id={descriptionId}>
            {draft.description ||
              "Review and confirm your commit. Leave the message blank to auto-generate one."}
          </DialogDescription>
          <div className="git-commit-modal-body">
            <div className="git-commit-modal-left" data-has-message={String(showCommitMessage)}>
              <div className="git-commit-files-panel">
                <div className="git-commit-files-header">
                  <div className="git-commit-files-heading">
                    {draft.branch !== undefined ? (
                      <div className="git-commit-branch-row">
                        <span aria-label="Branch" className="git-commit-branch-name">
                          {draft.branch ?? "(detached HEAD)"}
                        </span>
                      </div>
                    ) : null}
                    {isEditingFiles && changedFiles.length > 0 ? (
                      <span className="git-commit-files-selected">
                        {selectedFiles.length} of {changedFiles.length} selected
                      </span>
                    ) : null}
                  </div>
                </div>
                {changedFiles.length > 0 ? (
                  <>
                    {isEditingFiles ? (
                      <label className="git-commit-files-select-all">
                        <input
                          checked={allSelected}
                          className="changed-files-tree-checkbox"
                          onChange={() => {
                            setExcludedFiles(
                              allSelected
                                ? new Set(changedFiles.map((file) => file.path))
                                : new Set(),
                            );
                          }}
                          type="checkbox"
                        />
                        Include all files
                      </label>
                    ) : null}
                    <div className="git-commit-files-list">
                      <ChangedFilesTree
                        excludedPaths={excludedFiles}
                        files={changedFiles}
                        isEditing={isEditingFiles}
                        onToggleFile={(filePath) => {
                          setExcludedFiles((current) => {
                            const next = new Set(current);
                            if (next.has(filePath)) {
                              next.delete(filePath);
                            } else {
                              next.add(filePath);
                            }
                            return next;
                          });
                        }}
                        onOpenFile={openInlineFileDiff}
                        selectedPath={inlineDiffMode === "file" ? selectedDiffFilePath : undefined}
                      />
                    </div>
                    <div className="git-commit-files-footer">
                      <div className="git-commit-files-actions">
                        <button
                          className="git-commit-files-edit-button"
                          onClick={() => setIsEditingFiles((current) => !current)}
                          type="button"
                        >
                          {isEditingFiles ? "Done" : "Select"}
                        </button>
                        <button
                          className="git-commit-files-show-all-button"
                          data-active={inlineDiffMode === "all" ? "true" : "false"}
                          onClick={showAllInlineFileDiffs}
                          type="button"
                        >
                          Show All
                        </button>
                      </div>
                      <div className="git-commit-files-summary">
                        <span className="changed-files-tree-additions">+{selectedStats.additions}</span>
                        <span className="changed-files-tree-stat-divider">/</span>
                        <span className="changed-files-tree-deletions">-{selectedStats.deletions}</span>
                      </div>
                    </div>
                  </>
                ) : (
                  <div className="git-commit-files-empty">No changed files.</div>
                )}
              </div>
              {showCommitMessage ? (
                <label className="command-config-field">
                  <span className="command-config-label">Commit Message</span>
                  <Textarea
                    autoFocus
                    className="git-commit-modal-textarea"
                    onChange={(event) => setMessage(event.currentTarget.value)}
                    onPaste={handleMessagePaste}
                    placeholder="Leave empty to auto-generate"
                    rows={draft.suggestedBody ? 10 : 4}
                    value={message}
                    wrap="soft"
                  />
                </label>
              ) : null}
              {draft.isWorktree ? (
                <label className="command-config-toggle git-commit-delete-worktree-toggle">
                  <input
                    checked={deleteWorktreeAfter}
                    className="command-config-checkbox"
                    onChange={(event) => setDeleteWorktreeAfter(event.currentTarget.checked)}
                    type="checkbox"
                  />
                  <span className="command-config-toggle-copy">
                    Delete worktree project after this action finishes
                    {draft.worktreeName ? ` (${draft.worktreeName})` : ""}.
                  </span>
                </label>
              ) : null}
            </div>
            <section className="git-commit-inline-diff-panel" aria-label="Selected file diff">
              <div className="git-commit-inline-diff-header">
                {activeDiffFileLabel ? (
                  <div className="git-commit-inline-diff-file">
                    <span className="git-file-diff-modal-path">{activeDiffFileLabel}</span>
                    <span className="git-file-diff-modal-stats">
                      <span className="changed-files-tree-additions">
                        +{activeDiffStats.additions}
                      </span>
                      <span className="changed-files-tree-stat-divider">/</span>
                      <span className="changed-files-tree-deletions">
                        -{activeDiffStats.deletions}
                      </span>
                    </span>
                  </div>
                ) : null}
                <GitFileDiffControls
                  hideWhitespace={inlineDiffHideWhitespace}
                  lineWrap={inlineDiffLineWrap}
                  onHideWhitespaceChange={(hideWhitespace) =>
                    updateInlineDiffPreferences({ hideWhitespace })
                  }
                  onLineWrapChange={(lineWrap) => updateInlineDiffPreferences({ lineWrap })}
                  onViewModeChange={(viewMode) => updateInlineDiffPreferences({ viewMode })}
                  viewMode={inlineDiffViewMode}
                />
              </div>
              <div className="git-commit-inline-diff-body">
                <GitFileDiffPanel
                  draft={activeDiffDraft}
                  hideWhitespace={inlineDiffHideWhitespace}
                  isLoading={isSelectedDiffLoading}
                  lineWrap={inlineDiffLineWrap}
                  onHideWhitespaceChange={(hideWhitespace) =>
                    updateInlineDiffPreferences({ hideWhitespace })
                  }
                  onLineWrapChange={(lineWrap) => updateInlineDiffPreferences({ lineWrap })}
                  onViewModeChange={(viewMode) => updateInlineDiffPreferences({ viewMode })}
                  placeholder={
                    changedFiles.length > 0
                      ? "Select a file to preview its diff."
                      : "No changed files to preview."
                  }
                  showToolbar={false}
                  viewMode={inlineDiffViewMode}
                />
              </div>
            </section>
          </div>
          <DialogFooter className="git-commit-modal-actions">
            {promptAgents.length > 0 ? (
              <Select
                items={promptAgentSelectItems}
                onValueChange={handlePromptAgentChange}
                value={selectedPromptAgentId}
              >
                <SelectTrigger
                  aria-label="Generate commit agent"
                  className="git-commit-prompt-agent-select"
                  id={generateAgentId}
                >
                  <SelectValue placeholder="Select agent" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {promptAgents.map((agent) => (
                      <SelectItem key={agent.agentId} value={agent.agentId}>
                        {agent.name}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            ) : null}
            <Button
              className="git-commit-modal-button"
              onClick={() => onCancel(draft.requestId)}
              type="button"
              variant="outline"
            >
              Cancel
            </Button>
            {canDirectMerge ? (
              <Button
                className="git-commit-modal-button"
                disabled={!canRunDirectMerge}
                onClick={() => setIsDirectMergeConfirmOpen(true)}
                type="button"
                variant="outline"
              >
                Merge to main
              </Button>
            ) : null}
            {showCommitMessage ? (
              <Button
                className="git-commit-modal-button"
                disabled={!canConfirm}
                onClick={() =>
                  onConfirm(draft.requestId, trimmedMessage, {
                    agentId: selectedPromptAgentId || undefined,
                    commitOnNewRef: true,
                    deleteWorktreeAfter,
                    filePaths: selectedFilePaths,
                  })
                }
                type="button"
                variant="outline"
              >
                Commit on new branch
              </Button>
            ) : null}
            {showCommitMessage ? (
              <Button
                className="git-commit-modal-button"
                disabled={!canConfirm}
                onClick={() => onMultipleCommits(draft.requestId, selectedPromptAgentId || undefined)}
                type="button"
                variant="outline"
              >
                Multiple Commits
              </Button>
            ) : null}
            <Button
              className="git-commit-modal-button"
              disabled={!canConfirm}
              onClick={() =>
                onConfirm(draft.requestId, trimmedMessage, {
                  agentId: selectedPromptAgentId || undefined,
                  deleteWorktreeAfter,
                  filePaths: selectedFilePaths,
                })
              }
              type="button"
              variant="outline"
            >
              {draft.confirmLabel}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <ConfirmationModal
        confirmLabel="Merge to main"
        description={`This will merge ${draft.worktreeName ?? "this worktree"} directly into main without creating a PR.`}
        isOpen={isDirectMergeConfirmOpen}
        onCancel={() => setIsDirectMergeConfirmOpen(false)}
        onConfirm={() => {
          if (!onDirectMerge || !canRunDirectMerge) {
            return;
          }
          setIsDirectMergeConfirmOpen(false);
          onDirectMerge(draft.requestId, trimmedMessage, {
            agentId: selectedPromptAgentId || undefined,
            deleteWorktreeAfter,
            filePaths: selectedFilePaths,
          });
        }}
        title="Merge worktree into main?"
      />
    </>
  );
}

function buildDraftMessage(draft: GitCommitModalDraft): string {
  const subject = draft.suggestedSubject.trim();
  const body = draft.suggestedBody?.trim();
  return body ? `${subject}\n\n${body}` : subject;
}

function getSidebarThemeVariant(theme: SidebarTheme): "dark" | "light" {
  /**
   * CDXC:SidebarTheme 2026-06-15-01:43:
   * Commit review modals follow the app modal theme: Dark 1/Dark 2 keep dark
   * shadcn mode, while Light removes the dark class and uses light tokens.
   */
  return theme.startsWith("light-") || theme === "plain-light" ? "light" : "dark";
}

function buildAllFilesDiffDraft(
  files: ReadonlyArray<SidebarGitChangedFile>,
  diffDraftCache: Readonly<Record<string, GitFileDiffModalDraft>>,
): GitFileDiffModalDraft | undefined {
  if (files.length === 0) {
    return undefined;
  }
  const stats = summarizeChangedFiles(files);
  return {
    additions: stats.additions,
    deletions: stats.deletions,
    filePath: "All files",
    patch: files
      .map((file) => diffDraftCache[file.path]?.patch.trimEnd() || buildLoadingFileDiffPatch(file.path))
      .join("\n\n"),
  };
}

function buildLoadingFileDiffPatch(filePath: string): string {
  return [
    `diff --git a/${filePath} b/${filePath}`,
    `--- a/${filePath}`,
    `+++ b/${filePath}`,
    "@@ loading diff @@",
    " Loading diff...",
  ].join("\n");
}

function readGitCommitDiffPreferences(): GitCommitDiffPreferences {
  if (typeof window === "undefined") {
    return DEFAULT_GIT_COMMIT_DIFF_PREFERENCES;
  }
  try {
    const rawPreferences = window.localStorage.getItem(GIT_COMMIT_DIFF_PREFERENCES_STORAGE_KEY);
    if (!rawPreferences) {
      return DEFAULT_GIT_COMMIT_DIFF_PREFERENCES;
    }
    const parsedPreferences = JSON.parse(rawPreferences) as Partial<GitCommitDiffPreferences>;
    return {
      hideWhitespace: parsedPreferences.hideWhitespace === true,
      lineWrap: parsedPreferences.lineWrap === true,
      viewMode: parsedPreferences.viewMode === "split" ? "split" : "unified",
    };
  } catch {
    return DEFAULT_GIT_COMMIT_DIFF_PREFERENCES;
  }
}

/*
 * CDXC:TitlebarGit 2026-06-08-09:41:
 * Commit review display preferences are global UI preferences, not project
 * data. Store only the diff mode toggles in localStorage so unified/split,
 * wrapping, and whitespace visibility survive app restarts without logging or
 * persisting repository paths.
 */
function writeGitCommitDiffPreferences(preferences: GitCommitDiffPreferences): void {
  if (typeof window === "undefined") {
    return;
  }
  try {
    window.localStorage.setItem(
      GIT_COMMIT_DIFF_PREFERENCES_STORAGE_KEY,
      JSON.stringify(preferences),
    );
  } catch {
    // localStorage can be unavailable in isolated test/story contexts.
  }
}
