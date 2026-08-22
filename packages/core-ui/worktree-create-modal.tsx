import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ClipboardEvent as ReactClipboardEvent,
  type FormEvent,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { IconFolderOpen, IconGitBranch, IconPhotoPlus, IconSearch } from "@tabler/icons-react";
import { Button } from "@/packages/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/packages/components/ui/dialog";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/packages/components/ui/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/packages/components/ui/select";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/packages/components/ui/input-group";
import { Textarea } from "@/packages/components/ui/textarea";
import { trimPromptEditorTrailingSpaces } from "../shared/prompt-editor-text";
import {
  createSidebarAgentSelectItems,
  type SidebarAgentButton,
} from "../shared/sidebar-agents";
import { postAppModalHostMessage } from "./app-modal-host-bridge";

export type ExistingWorktreeOption = {
  branch: string;
  isCurrentProject?: boolean;
  isRegistered?: boolean;
  name: string;
  path: string;
  worktreeKey?: string;
};

export type WorktreeBaseBranchOption = {
  current?: boolean;
  name: string;
  remote?: boolean;
};

export type WorktreeCreateModalDraft =
  | { agentId: string; baseBranch: string; mode: "create"; prompt: string }
  | {
      agentId?: string;
      existingWorktreeKey?: string;
      existingWorktreePath: string;
      mode: "openExisting";
      prompt?: string;
    };

export type WorktreeCreateModalProps = {
  agents: SidebarAgentButton[];
  defaultAgentId?: string;
  isOpen: boolean;
  onCancel: () => void;
  onConfirm: (draft: WorktreeCreateModalDraft) => void;
  onRequestExistingWorktrees?: (requestId: string) => void;
  projectName?: string;
};

type WorktreeCreateMode = WorktreeCreateModalDraft["mode"];

type ProjectWorktreesResultMessage = {
  branches?: unknown;
  error?: unknown;
  ok: boolean;
  requestId: string;
  type: "projectWorktreesResult";
  worktrees?: unknown;
};

export function WorktreeCreateModal({
  agents,
  defaultAgentId,
  isOpen,
  onCancel,
  onConfirm,
  onRequestExistingWorktrees,
}: WorktreeCreateModalProps) {
  const promptId = useId();
  const agentId = useId();
  const existingWorktreeId = useId();
  const baseBranchId = useId();
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const userInteractedAfterOpenRef = useRef(false);
  const worktreeListRequestIdRef = useRef<string | undefined>(undefined);
  const commandAgents = useMemo(
    () => agents.filter((agent) => agent.command?.trim()),
    [agents],
  );
  const agentSelectItems = useMemo(
    () => createSidebarAgentSelectItems(commandAgents),
    [commandAgents],
  );
  const [prompt, setPrompt] = useState("");
  const [selectedAgentId, setSelectedAgentId] = useState(commandAgents[0]?.agentId ?? "");
  const [mode, setMode] = useState<WorktreeCreateMode>("create");
  const [baseBranches, setBaseBranches] = useState<WorktreeBaseBranchOption[]>([]);
  const [selectedBaseBranch, setSelectedBaseBranch] = useState("");
  const [baseBranchFilter, setBaseBranchFilter] = useState("");
  const [existingWorktrees, setExistingWorktrees] = useState<ExistingWorktreeOption[]>([]);
  const [selectedExistingWorktreeValue, setSelectedExistingWorktreeValue] = useState("");
  const [existingWorktreeFilter, setExistingWorktreeFilter] = useState("");
  const [worktreeListError, setWorktreeListError] = useState<string | undefined>(undefined);
  const [isLoadingWorktrees, setIsLoadingWorktrees] = useState(false);
  const [imageCount, setImageCount] = useState(0);

  /**
   * CDXC:Worktrees 2026-05-18-23:07:
   * New worktrees need an agent plus a first prompt. Image paste and native file picking insert Markdown links into that prompt so visual context travels with the first agent instruction.
   *
   * CDXC:PromptAgents 2026-05-28-07:15:
   * The first selectable worktree agent should start from Settings' default
   * prompt agent when that agent is visible and configured, while preserving
   * any valid in-modal selection during the current modal session.
   *
   * CDXC:Worktrees 2026-05-28-07:47:
   * The first prompt is prompt-editor text. Plain-text paste and submit should
   * remove spaces at line ends before the prompt is sent to the selected agent,
   * while image paste keeps inserting durable Markdown links.
   *
   * CDXC:AppModals 2026-05-29-19:44:
   * Session attention/activity can refresh app-modal props while a user is
   * typing. The Add Worktree draft is initialized only when the modal opens;
   * later agent-list updates may repair an invalid selection but must not clear
   * the prompt, images, or a still-valid selected agent.
   *
   * CDXC:WorktreeProjectRegistration 2026-06-01-20:59:
   * Add Worktree supports creating a fresh checkout or opening an existing Git
   * worktree from the same machine. The existing-worktree list is requested when
   * the modal opens and submit carries the selected path so native registers and
   * opens it through gxserver.
   *
   * CDXC:GPUIWorktrees 2026-06-24-14:06:
   * Open Existing remains valid as a project-open-only flow when the first
   * prompt is blank. When the user enters a prompt, the reused modal must carry
   * that prompt plus the visible selected agent with the trusted worktree path
   * so macOS and GPUI can start the real agent session without inventing a
   * default prompt or accepting arbitrary renderer paths.
   *
   * CDXC:WorktreeModal 2026-06-13-18:39:
   * Add Worktree should close from the same top-right shadcn X used by Rename
   * Session. Do not keep a footer Cancel row; the bottom of the modal should
   * be reserved for the primary worktree action and shared prompt image picker.
   *
   * CDXC:WorktreeModal 2026-06-15-11:30:
   * Add Worktree opens in the same hidden native child-window host as Rename
   * Session, so the first-prompt textarea must retry focus across native
   * window activation and stop retrying after any user click or key input.
   *
   * CDXC:WorktreeBaseBranch 2026-06-24-11:32:
   * Creating a new Git worktree must expose the source branch explicitly. The
   * modal loads branch options with the existing worktree request, requires one
   * selected base branch for create mode, and sends that branch through submit
   * so backend creation does not silently default to HEAD.
   *
   * CDXC:RemoteWorktrees 2026-06-24-18:40:
   * Remote Open Existing selections may carry an opaque gxserver worktree key in
   * addition to the display path. Submit both fields so remote GPUI can send
   * only the list-derived key back to the owning daemon while local/native
   * receivers continue to use the selected path.
   *
   * CDXC:WorktreeModal 2026-06-30-16:05:
   * Add Worktree should use a title-only header in the macOS child-window modal.
   * Mode is the first explanatory row, so omit the project subtitle and preserve
   * vertical room for the image picker plus primary worktree action.
   *
   * CDXC:WorktreeModal 2026-06-30-23:58:
   * Open Existing already shows the selected worktree in the select trigger, so
   * helper text under the selector should be reserved for load, error, and empty
   * states instead of echoing the selected path.
   */
  const hasInitializedOpenDraftRef = useRef(false);
  useEffect(() => {
    if (!isOpen) {
      hasInitializedOpenDraftRef.current = false;
      userInteractedAfterOpenRef.current = false;
      return;
    }
    if (hasInitializedOpenDraftRef.current) {
      return;
    }
    hasInitializedOpenDraftRef.current = true;
    userInteractedAfterOpenRef.current = false;

    setPrompt("");
    setImageCount(0);
    setMode("create");
    setBaseBranches([]);
    setSelectedBaseBranch("");
    setBaseBranchFilter("");
    setExistingWorktrees([]);
    setSelectedExistingWorktreeValue("");
    setExistingWorktreeFilter("");
    setWorktreeListError(undefined);
    setSelectedAgentId(resolveInitialWorktreeAgentId(commandAgents, defaultAgentId));
    if (onRequestExistingWorktrees) {
      const requestId = `worktrees-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
      worktreeListRequestIdRef.current = requestId;
      setIsLoadingWorktrees(true);
      onRequestExistingWorktrees(requestId);
    }
  }, [commandAgents, defaultAgentId, isOpen]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    setSelectedAgentId((currentAgentId) =>
      commandAgents.some((agent) => agent.agentId === currentAgentId)
        ? currentAgentId
        : resolveInitialWorktreeAgentId(commandAgents, defaultAgentId),
    );
  }, [commandAgents, defaultAgentId, isOpen]);

  const focusInput = useCallback(() => {
    const input = inputRef.current;
    if (input) {
      input.focus({ preventScroll: true });
      const selectionIndex = input.value.length;
      input.setSelectionRange(selectionIndex, selectionIndex);
    }
    return false as const;
  }, []);

  const markUserInteractedAfterOpen = useCallback(() => {
    userInteractedAfterOpenRef.current = true;
  }, []);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const focusUnlessUserInteracted = () => {
      if (userInteractedAfterOpenRef.current) {
        return;
      }
      focusInput();
    };
    const retryDelaysMs = [0, 16, 50, 100, 250, 500, 1000, 1600, 2400];
    const timeoutIds = retryDelaysMs.map((delayMs) =>
      window.setTimeout(focusUnlessUserInteracted, delayMs),
    );
    const animationFrame = window.requestAnimationFrame(focusUnlessUserInteracted);
    const windowFocusTimeoutIds: number[] = [];
    const windowFocusAnimationFrames: number[] = [];
    const handleWindowFocus = () => {
      windowFocusTimeoutIds.push(window.setTimeout(focusUnlessUserInteracted, 0));
      windowFocusAnimationFrames.push(window.requestAnimationFrame(focusUnlessUserInteracted));
    };

    window.addEventListener("focus", handleWindowFocus);
    return () => {
      window.cancelAnimationFrame(animationFrame);
      timeoutIds.forEach((timeoutId) => window.clearTimeout(timeoutId));
      windowFocusTimeoutIds.forEach((timeoutId) => window.clearTimeout(timeoutId));
      windowFocusAnimationFrames.forEach((frameId) => window.cancelAnimationFrame(frameId));
      window.removeEventListener("focus", handleWindowFocus);
    };
  }, [focusInput, isOpen]);

  const normalizedPrompt = trimPromptEditorTrailingSpaces(prompt);
  const trimmedPrompt = normalizedPrompt.trim();
  const canCreate = Boolean(
    mode === "openExisting"
      ? selectedExistingWorktreeValue && (!trimmedPrompt || selectedAgentId)
      : trimmedPrompt && selectedAgentId && selectedBaseBranch,
  );
  const existingWorktreeStatusDescription =
    worktreeListError ??
    (isLoadingWorktrees
      ? "Loading existing worktrees."
      : existingWorktrees.length === 0
        ? "No existing worktrees found."
        : undefined);
  const filteredBaseBranches = useMemo(
    () => filterWorktreeBaseBranchOptions(baseBranches, baseBranchFilter),
    [baseBranches, baseBranchFilter],
  );
  const filteredExistingWorktrees = useMemo(
    () => filterExistingWorktreeOptions(existingWorktrees, existingWorktreeFilter),
    [existingWorktrees, existingWorktreeFilter],
  );

  const insertImageLinks = (files: readonly File[]) => {
    if (files.length === 0) {
      return;
    }

    const links = files.map((file, index) => {
      const path =
        (file as File & { path?: string }).path?.trim() ||
        file.webkitRelativePath?.trim() ||
        file.name;
      return `[Image #${imageCount + index + 1}](${path})`;
    });
    setImageCount((count) => count + files.length);
    insertPromptText(
      `${prompt.endsWith("\n") || prompt.length === 0 ? "" : "\n"}${links.join("\n")}\n`,
    );
  };

  const insertImagePaths = (paths: readonly string[]) => {
    if (paths.length === 0) {
      return;
    }
    const links = paths.map((path, index) => `[Image #${imageCount + index + 1}](${path})`);
    setImageCount((count) => count + paths.length);
    insertPromptText(
      `${prompt.endsWith("\n") || prompt.length === 0 ? "" : "\n"}${links.join("\n")}\n`,
    );
  };

  const insertPromptText = (text: string) => {
    const textarea = inputRef.current;
    if (!textarea) {
      setPrompt((current) => `${current}${text}`);
      return;
    }

    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    const nextPrompt = `${prompt.slice(0, start)}${text}${prompt.slice(end)}`;
    setPrompt(nextPrompt);
    window.requestAnimationFrame(() => {
      textarea.focus();
      const nextSelection = start + text.length;
      textarea.setSelectionRange(nextSelection, nextSelection);
    });
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canCreate) {
      return;
    }
    onConfirm(
      createDraft(
        mode,
        selectedAgentId,
        selectedBaseBranch,
        trimmedPrompt,
        selectedExistingWorktreeValue,
        existingWorktrees,
      ),
    );
  };

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (canCreate) {
      onConfirm(
        createDraft(
          mode,
          selectedAgentId,
          selectedBaseBranch,
          trimmedPrompt,
          selectedExistingWorktreeValue,
          existingWorktrees,
        ),
      );
    }
  };

  const handlePaste = (event: ReactClipboardEvent<HTMLTextAreaElement>) => {
    const imageFiles = Array.from(event.clipboardData.files).filter((file): file is File =>
      file.type.startsWith("image/"),
    );
    if (imageFiles.length > 0) {
      event.preventDefault();
      insertImageLinks(imageFiles);
      return;
    }

    const pastedText = event.clipboardData.getData("text/plain");
    const trimmedText = trimPromptEditorTrailingSpaces(pastedText);
    if (!pastedText || trimmedText === pastedText) {
      return;
    }
    event.preventDefault();
    insertPromptText(trimmedText);
  };

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const handlePickedImages = (event: Event) => {
      const message = (event as CustomEvent<unknown>).detail;
      if (!message || typeof message !== "object" || !("type" in message)) {
        return;
      }
      if (message.type !== "worktreeImageFilesPicked") {
        if (message.type === "projectWorktreesResult") {
          const result = message as ProjectWorktreesResultMessage;
          if (result.requestId !== worktreeListRequestIdRef.current) {
            return;
          }
          setIsLoadingWorktrees(false);
          if (!result.ok) {
            setWorktreeListError(
              typeof result.error === "string" ? result.error : "Could not load existing worktrees.",
            );
            setBaseBranches([]);
            setSelectedBaseBranch("");
            setExistingWorktrees([]);
            return;
          }
          const branches = normalizeWorktreeBaseBranchOptions(result.branches);
          setBaseBranches(branches);
          setSelectedBaseBranch((current) =>
            branches.some((branch) => branch.name === current)
              ? current
              : branches.find((branch) => branch.current)?.name ?? branches[0]?.name ?? "",
          );
          const worktrees = normalizeExistingWorktreeOptions(result.worktrees);
          setExistingWorktrees(worktrees);
          setSelectedExistingWorktreeValue((current) =>
            worktrees.some((worktree) => existingWorktreeOptionValue(worktree) === current)
              ? current
              : existingWorktreeOptionValue(worktrees.find((worktree) => !worktree.isRegistered) ?? worktrees[0]),
          );
          return;
        }
        return;
      }
      const candidate = message as { paths?: unknown };
      const paths = Array.isArray(candidate.paths)
        ? candidate.paths.filter(
            (path): path is string => typeof path === "string" && path.trim().length > 0,
          )
        : [];
      insertImagePaths(paths);
    };

    window.addEventListener("ghostex-app-modal-host-message", handlePickedImages);
    return () => {
      window.removeEventListener("ghostex-app-modal-host-message", handlePickedImages);
    };
  });

  if (!isOpen) {
    return null;
  }

  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onCancel();
        }
      }}
      open={isOpen}
    >
      <DialogContent
        className="command-config-modal-shadcn worktree-create-modal-shadcn font-sans"
        initialFocus={focusInput}
      >
        <form
          className="session-rename-form worktree-create-form"
          onKeyDownCapture={markUserInteractedAfterOpen}
          onPointerDownCapture={markUserInteractedAfterOpen}
          onSubmit={submit}
        >
          <DialogHeader>
            <DialogTitle className="text-xl">Add Worktree</DialogTitle>
          </DialogHeader>
          <FieldGroup className="session-rename-field-group">
            <Field>
              <FieldLabel>Mode</FieldLabel>
              <div className="worktree-create-mode-toggle" role="group" aria-label="Worktree mode">
                <Button
                  onClick={() => setMode("create")}
                  type="button"
                  variant={mode === "create" ? "default" : "outline"}
                >
                  <IconGitBranch aria-hidden="true" data-icon="inline-start" />
                  Create New
                </Button>
                <Button
                  onClick={() => setMode("openExisting")}
                  type="button"
                  variant={mode === "openExisting" ? "default" : "outline"}
                >
                  <IconFolderOpen aria-hidden="true" data-icon="inline-start" />
                  Open Existing
                </Button>
              </div>
            </Field>
            {mode === "openExisting" ? (
              <Field>
                <FieldLabel htmlFor={existingWorktreeId}>Existing worktree</FieldLabel>
                <Select
                  onValueChange={setSelectedExistingWorktreeValue}
                  value={selectedExistingWorktreeValue}
                >
                  <SelectTrigger
                    aria-label="Existing worktree"
                    className="worktree-create-existing-select"
                    id={existingWorktreeId}
                  >
                    <SelectValue placeholder={isLoadingWorktrees ? "Loading worktrees" : "Select worktree"} />
                  </SelectTrigger>
                  <SelectContent
                    header={
                      <WorktreeSelectFilterInput
                        ariaLabel="Filter existing worktrees"
                        onValueChange={setExistingWorktreeFilter}
                        placeholder="Filter worktrees"
                        value={existingWorktreeFilter}
                      />
                    }
                  >
                    <SelectGroup>
                      {filteredExistingWorktrees.length > 0 ? (
                        filteredExistingWorktrees.map((worktree) => (
                          <SelectItem
                            key={existingWorktreeOptionValue(worktree)}
                            value={existingWorktreeOptionValue(worktree)}
                          >
                            {worktree.name} {worktree.branch ? `(${worktree.branch})` : ""}
                          </SelectItem>
                        ))
                      ) : (
                        <SelectItem disabled value="__ghostex-no-existing-worktrees-match">
                          No worktrees match
                        </SelectItem>
                      )}
                    </SelectGroup>
                  </SelectContent>
                </Select>
                {existingWorktreeStatusDescription ? (
                  <FieldDescription>{existingWorktreeStatusDescription}</FieldDescription>
                ) : null}
              </Field>
            ) : null}
            {mode === "create" ? (
              <Field>
                <FieldLabel htmlFor={baseBranchId}>Base branch</FieldLabel>
                <Select
                  onValueChange={setSelectedBaseBranch}
                  value={selectedBaseBranch}
                >
                  <SelectTrigger
                    aria-label="Base branch"
                    className="worktree-create-base-branch-select"
                    id={baseBranchId}
                  >
                    <SelectValue placeholder={isLoadingWorktrees ? "Loading branches" : "Select branch"} />
                  </SelectTrigger>
                  <SelectContent
                    header={
                      <WorktreeSelectFilterInput
                        ariaLabel="Filter base branches"
                        onValueChange={setBaseBranchFilter}
                        placeholder="Filter branches"
                        value={baseBranchFilter}
                      />
                    }
                  >
                    <SelectGroup>
                      {filteredBaseBranches.length > 0 ? (
                        filteredBaseBranches.map((branch) => (
                          <SelectItem key={branch.name} value={branch.name}>
                            {branch.name}
                            {branch.current ? " (current)" : branch.remote ? " (remote)" : ""}
                          </SelectItem>
                        ))
                      ) : (
                        <SelectItem disabled value="__ghostex-no-base-branches-match">
                          No branches match
                        </SelectItem>
                      )}
                    </SelectGroup>
                  </SelectContent>
                </Select>
                {!selectedBaseBranch ? (
                  <FieldDescription>
                    {isLoadingWorktrees ? "Loading branches." : "No branches found."}
                  </FieldDescription>
                ) : null}
              </Field>
            ) : null}
            <Field>
              <FieldLabel htmlFor={agentId}>Agent</FieldLabel>
              <Select
                items={agentSelectItems}
                onValueChange={setSelectedAgentId}
                value={selectedAgentId}
              >
                <SelectTrigger
                  aria-label="Agent"
                  className="worktree-create-agent-select"
                  id={agentId}
                >
                  <SelectValue placeholder="Select agent" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {commandAgents.map((agent) => (
                      <SelectItem key={agent.agentId} value={agent.agentId}>
                        {agent.name}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
            <Field>
              <FieldLabel htmlFor={promptId}>
                {mode === "openExisting" ? "First prompt (optional)" : "First prompt"}
              </FieldLabel>
              <Textarea
                aria-label="First prompt"
                autoFocus
                id={promptId}
                onChange={(event) => setPrompt(event.currentTarget.value)}
                onKeyDown={handleKeyDown}
                onPaste={handlePaste}
                placeholder={
                  mode === "openExisting"
                    ? "Optional prompt after opening"
                    : "Describe the worktree task"
                }
                ref={inputRef}
                value={prompt}
              />
              <FieldDescription>
                Paste images or pick files to insert image links into the prompt.
              </FieldDescription>
            </Field>
          </FieldGroup>
          <DialogFooter>
            <Button
              onClick={() =>
                postAppModalHostMessage(
                  { type: "pickWorktreeImages" },
                  "AppModals:pickWorktreeImages",
                )
              }
              type="button"
              variant="secondary"
            >
              <IconPhotoPlus aria-hidden="true" data-icon="inline-start" />
              Add Images
            </Button>
            <Button disabled={!canCreate} type="submit">
              {mode === "openExisting" ? "Open Worktree" : "New Worktree"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function WorktreeSelectFilterInput({
  ariaLabel,
  onValueChange,
  placeholder,
  value,
}: {
  ariaLabel: string;
  onValueChange: (value: string) => void;
  placeholder: string;
  value: string;
}) {
  return (
    <div
      className="worktree-select-filter"
      onPointerDownCapture={(event) => event.stopPropagation()}
    >
      <InputGroup className="worktree-select-filter-input-group">
        <InputGroupInput
          aria-label={ariaLabel}
          autoComplete="off"
          className="worktree-select-filter-input"
          onChange={(event) => onValueChange(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape" && value) {
              event.preventDefault();
              event.stopPropagation();
              onValueChange("");
              return;
            }
            if (isWorktreeSelectFilterTextEditingKey(event.key)) {
              event.stopPropagation();
            }
          }}
          placeholder={placeholder}
          value={value}
        />
        <InputGroupAddon align="inline-end">
          <IconSearch aria-hidden="true" />
        </InputGroupAddon>
      </InputGroup>
    </div>
  );
}

function resolveInitialWorktreeAgentId(
  commandAgents: SidebarAgentButton[],
  defaultAgentId?: string,
): string {
  return (
    commandAgents.find((agent) => agent.agentId === defaultAgentId)?.agentId ??
    commandAgents[0]?.agentId ??
    ""
  );
}

function createDraft(
  mode: WorktreeCreateMode,
  agentId: string,
  baseBranch: string,
  prompt: string,
  existingWorktreeValue: string,
  existingWorktrees: readonly ExistingWorktreeOption[],
): WorktreeCreateModalDraft {
  if (mode === "openExisting") {
    const openExistingPrompt = prompt.trim();
    const selectedWorktree = existingWorktrees.find(
      (worktree) => existingWorktreeOptionValue(worktree) === existingWorktreeValue,
    );
    return {
      ...(openExistingPrompt ? { agentId, prompt: openExistingPrompt } : {}),
      ...(selectedWorktree?.worktreeKey ? { existingWorktreeKey: selectedWorktree.worktreeKey } : {}),
      existingWorktreePath: selectedWorktree?.path ?? existingWorktreeValue,
      mode,
    };
  }
  return { agentId, baseBranch, mode, prompt };
}

function existingWorktreeOptionValue(worktree: ExistingWorktreeOption | undefined): string {
  return worktree?.worktreeKey?.trim() || worktree?.path?.trim() || "";
}

function filterWorktreeBaseBranchOptions(
  branches: readonly WorktreeBaseBranchOption[],
  filter: string,
): WorktreeBaseBranchOption[] {
  const query = normalizeWorktreeSelectFilter(filter);
  if (!query) {
    return [...branches];
  }
  return branches.filter((branch) => normalizeWorktreeSelectFilter(branch.name).includes(query));
}

function filterExistingWorktreeOptions(
  worktrees: readonly ExistingWorktreeOption[],
  filter: string,
): ExistingWorktreeOption[] {
  const query = normalizeWorktreeSelectFilter(filter);
  if (!query) {
    return [...worktrees];
  }
  return worktrees.filter((worktree) =>
    [worktree.name, worktree.branch, worktree.path].some((value) =>
      normalizeWorktreeSelectFilter(value).includes(query),
    ),
  );
}

function normalizeWorktreeSelectFilter(value: string): string {
  return value.trim().toLocaleLowerCase();
}

function isWorktreeSelectFilterTextEditingKey(key: string): boolean {
  return (
    key.length === 1 ||
    key === "Backspace" ||
    key === "Delete" ||
    key === "End" ||
    key === "Home" ||
    key === "ArrowLeft" ||
    key === "ArrowRight"
  );
}

function normalizeWorktreeBaseBranchOptions(candidate: unknown): WorktreeBaseBranchOption[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  const seenBranches = new Set<string>();
  return candidate.flatMap((entry): WorktreeBaseBranchOption[] => {
    if (!entry || typeof entry !== "object") {
      return [];
    }
    const branch = entry as Partial<WorktreeBaseBranchOption>;
    const name = branch.name?.trim();
    if (!name || seenBranches.has(name)) {
      return [];
    }
    seenBranches.add(name);
    return [
      {
        current: branch.current === true,
        name,
        remote: branch.remote === true,
      },
    ];
  });
}

function normalizeExistingWorktreeOptions(candidate: unknown): ExistingWorktreeOption[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  return candidate.flatMap((entry): ExistingWorktreeOption[] => {
    if (!entry || typeof entry !== "object") {
      return [];
    }
    const worktree = entry as Partial<ExistingWorktreeOption>;
    const path = worktree.path?.trim();
    const name = worktree.name?.trim();
    if (!path || !name) {
      return [];
    }
    return [
      {
        branch: worktree.branch?.trim() ?? "",
        isCurrentProject: worktree.isCurrentProject === true,
        isRegistered: worktree.isRegistered === true,
        name,
        path,
        ...(worktree.worktreeKey?.trim() ? { worktreeKey: worktree.worktreeKey.trim() } : {}),
      },
    ];
  });
}
