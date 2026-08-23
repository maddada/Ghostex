import {
  IconAlertTriangle,
  IconArrowLeft,
  IconBrandAzure,
  IconBrandBitbucket,
  IconBrandGithub,
  IconBrandGitlab,
  IconCornerLeftUp,
  IconDeviceDesktop,
  IconFolder,
  IconFolderCheck,
  IconFolderPlus,
  IconFolderRoot,
  IconGitBranch,
  IconLink,
  IconSearch,
  IconServer,
} from "@tabler/icons-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { Button } from "@/packages/components/ui/button";
import { Checkbox } from "@/packages/components/ui/checkbox";
import { CommandDialog } from "@/packages/components/ui/command";
import { Input } from "@/packages/components/ui/input";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/packages/components/ui/input-group";
import { cn } from "@/packages/components/utils";
import { AppTooltip } from "../app-tooltip";
import {
  appendBrowsePathSegment,
  canNavigateUp,
  ensureBrowseDirectoryPath,
  getBrowseDirectoryPath,
  getBrowseLeafPathSegment,
  getBrowseParentPath,
  hasTrailingPathSeparator,
  isExplicitRelativeProjectPath,
  isFilesystemBrowseQuery,
  isUnsupportedWindowsProjectPath,
  resolveProjectPathForDispatch,
} from "../remote-project-picker/remote-project-paths";
import { filterBrowseEntries } from "../remote-project-picker/remote-command-palette-logic";
import { isRepositoryCloneBranchNameInputValid } from "../../shared/repository-clone";
import {
  ADD_PROJECT_ROOT_BROWSE_PATH,
  addProjectEmptyStateMessage,
  addProjectInitialBrowseQuery,
  addProjectModifierLabel,
  addProjectNewFolderMessage,
  addProjectPathPlaceholder,
  addProjectRepositoryActionLabel,
  addProjectRepositoryPlaceholder,
  addProjectSourceLabel,
  addProjectSourceRowDescription,
  addProjectSourceRowTitle,
  buildAddProjectSourceReadiness,
  isPrimaryModifierPlatform,
  matchesAddProjectFilter,
  orderedAddProjectSources,
} from "./add-project-modal-logic";
import type {
  AddProjectBrowseEntry,
  AddProjectBrowseResult,
  AddProjectCloneJob,
  AddProjectClonePreview,
  AddProjectMachineOption,
  AddProjectModalProps,
  AddProjectProviderId,
  AddProjectRepositoryInfo,
  AddProjectSourceControlDiscovery,
  AddProjectSourceId,
} from "./types";

/*
 * CDXC:AddProject 2026-07-30:
 * Ghostex's add-project flow uses a command-palette model. The dialog is
 * transport-free: gpui, ghostex-web, and Storybook all supply the same callback
 * props, so the exact same keyboard model and copy ship on every surface.
 *
 * Two load-bearing details from the spec:
 *
 * 1. There is NO auto-highlight in path modes. cmdk re-selects the first item on
 *    every search change (cmdk/dist/index.mjs `setState("search") -> selectFirstItem`),
 *    which is precisely the Enter trap the current remote picker suffers from:
 *    plain Enter navigates into a suggestion instead of adding the typed path.
 *    So this dialog owns its highlight state and its own arrow-key handling and
 *    does not mount a cmdk <Command>. It still uses the house dialog / input-group
 *    primitives and the shared remote path helpers.
 * 2. Errors are a persistent inline region, never a transient list line. A slow
 *    server call surfaces a "still working" notice instead of silently dying;
 *    the host owns the hard timeout.
 */

const EMPTY_BROWSE_ENTRIES: readonly AddProjectBrowseEntry[] = [];
const BROWSE_UP_VALUE = "browse:up";
const ADD_PROJECT_ROW_ICON_CLASS = "size-4 text-muted-foreground/80";

/*
 * CDXC:AddProjectChrome 2026-08-18:
 * The dialog's action buttons are titlebar chrome, not inset controls: each one
 * fills its container's full height, sits flush against the container edge, and
 * carries a single side border as the only separator. A row of them therefore
 * reads as one connected strip the way the titlebar Tips actions do, instead of
 * as floating pills with gaps around them.
 */
const ADD_PROJECT_ACTION_ADDON_CLASS =
  "h-full gap-0 self-stretch p-0 has-[>button]:ml-0 has-[>button]:mr-0";
const ADD_PROJECT_ACTION_BUTTON_CLASS =
  "h-full self-stretch border-y-0 border-r-0 border-l border-l-border/70 px-3 text-sm";

/*
 * CDXC:AddProjectChrome 2026-08-19:
 * The path bar is the dialog's only text field and it is autofocused, so the
 * shared InputGroup focus treatment (ring + accent border) would draw a
 * permanent highlight frame around the whole strip. Keep the resting border and
 * drop the focus ring instead of hiding focus outright: the caret already shows
 * where typing lands.
 */
const ADD_PROJECT_PATH_BAR_CLASS =
  "h-10 bg-input/30 has-[[data-slot=input-group-control]:focus-visible]:border-input has-[[data-slot=input-group-control]:focus-visible]:ring-0";

type AddProjectBusyKind = "add" | "clone" | "createFolder" | "lookup" | "preview";

interface AddProjectCloneFlow {
  readonly remoteUrl: string;
  readonly repository: AddProjectRepositoryInfo | null;
  readonly repositoryInput: string;
  readonly source: AddProjectSourceId;
  readonly step: "destination" | "repository" | "review";
}

interface AddProjectCloneOptions {
  readonly branchName: string;
  readonly cloneMainOnly: boolean;
  readonly shallowClone: boolean;
}

const DEFAULT_CLONE_OPTIONS: AddProjectCloneOptions = {
  branchName: "",
  cloneMainOnly: false,
  shallowClone: false,
};

type AddProjectView =
  | { readonly kind: "machines" }
  | { readonly kind: "sources"; readonly machineId: string }
  | { readonly initialQuery: string; readonly kind: "browse"; readonly machineId: string }
  | { readonly kind: "clone"; readonly machineId: string };

interface AddProjectRow {
  readonly dataAttributes?: Readonly<Record<string, string>>;
  readonly description?: string;
  readonly disabled?: boolean;
  readonly field: string;
  readonly icon: ReactNode;
  readonly onSelect: () => void;
  readonly searchTerms?: readonly string[];
  readonly submenu?: boolean;
  readonly title: string;
  readonly trailing?: ReactNode;
  readonly value: string;
}

export function AddProjectModal(props: AddProjectModalProps) {
  if (!props.isOpen) {
    return null;
  }
  /*
   * The body is keyed on nothing and mounted only while open, so dismissing the
   * dialog destroys every step of the flow
   * (spec §8 gotcha 8: state reset is by unmount, there is no reset()).
   */
  return (
    /*
     * CDXC:AddProjectChrome 2026-07-31:
     * Floating surfaces (web, Storybook) hug their content between a min and a
     * max so a six-row Sources step never renders a half-empty frame and a long
     * browse listing still gets a tall scroller. The gpui child window is the
     * dialog, so its stylesheet pins this to the full window instead; either way
     * the body below is a flex column, which keeps the input at the top and the
     * shortcut footer at the bottom edge rather than floating mid-frame.
     */
    <CommandDialog
      className="add-project-modal top-1/2 max-h-[min(32rem,calc(100vh-6rem))] min-h-[22rem] max-w-xl -translate-y-1/2 sm:max-w-xl"
      description="Browse a folder or clone a repository, then add it as a project."
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          props.onClose();
        }
      }}
      open
      title="Add project"
    >
      <AddProjectModalBody {...props} />
    </CommandDialog>
  );
}

function AddProjectModalBody(props: AddProjectModalProps) {
  const propsRef = useRef(props);
  propsRef.current = props;

  const {
    activeProjectCwd = null,
    cloneJobPollIntervalMs = 900,
    initialMachineId,
    platform: platformProp,
    slowOperationNoticeMs = 8000,
  } = props;

  const [machines, setMachines] = useState<readonly AddProjectMachineOption[]>([]);
  const [isLoadingMachines, setIsLoadingMachines] = useState(true);
  const [viewStack, setViewStack] = useState<readonly AddProjectView[]>([]);
  const [cloneFlow, setCloneFlow] = useState<AddProjectCloneFlow | null>(null);
  const [cloneOptions, setCloneOptions] = useState<AddProjectCloneOptions>(DEFAULT_CLONE_OPTIONS);
  const [clonePreview, setClonePreview] = useState<AddProjectClonePreview | null>(null);
  const [cloneDestinationPath, setCloneDestinationPath] = useState("");
  const [query, setQuery] = useState("");
  const [highlightedItemValue, setHighlightedItemValue] = useState<string | null>(null);
  const [browseGeneration, setBrowseGeneration] = useState(0);
  const [browseResult, setBrowseResult] = useState<AddProjectBrowseResult | null>(null);
  const [isBrowsePending, setIsBrowsePending] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState<AddProjectBusyKind | null>(null);
  const [isSlow, setIsSlow] = useState(false);
  const [discoveryByMachineId, setDiscoveryByMachineId] = useState<
    Readonly<Record<string, AddProjectSourceControlDiscovery | null>>
  >({});
  const [pendingDiscoveryMachineId, setPendingDiscoveryMachineId] = useState<string | null>(null);
  /*
   * `null` means "not naming a folder". The browse query is deliberately left
   * untouched while this step is open, so the listing behind it keeps showing
   * the directory the folder is about to be created in.
   */
  const [newFolderName, setNewFolderName] = useState<string | null>(null);

  const browseRequestRef = useRef(0);
  const cloneJobIdRef = useRef<string | null>(null);
  const isMountedRef = useRef(true);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    setIsLoadingMachines(true);
    void (async () => {
      try {
        const options = await propsRef.current.listMachineOptions();
        if (cancelled) {
          return;
        }
        setMachines(options);
        setViewStack(buildInitialViewStack(options, initialMachineId));
        if (options.length === 0) {
          setErrorMessage("No machine is available.");
        }
      } catch (error) {
        if (!cancelled) {
          setErrorMessage(describeError(error, "Unable to list machines."));
        }
      } finally {
        if (!cancelled) {
          setIsLoadingMachines(false);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [initialMachineId]);

  const currentView = viewStack.at(-1) ?? null;
  const machineId = currentView && "machineId" in currentView ? currentView.machineId : null;
  const machine = useMemo(
    () => machines.find((option) => option.machineId === machineId) ?? null,
    [machineId, machines],
  );
  const platform =
    machine?.platform ??
    platformProp ??
    (typeof navigator === "undefined" ? "" : navigator.platform);
  const canPopView = viewStack.length > 1;
  const isRepositoryStep = cloneFlow?.step === "repository";
  const isCloneDestinationStep = cloneFlow?.step === "destination";
  const isCloneReviewStep = cloneFlow?.step === "review";
  const isBrowsing =
    !isRepositoryStep && !isCloneReviewStep && isFilesystemBrowseQuery(query, platform);
  const isNewFolderStep = newFolderName !== null;

  const browseDirectoryPath = isBrowsing ? getBrowseDirectoryPath(query) : "";
  const browseFilterQuery =
    isBrowsing && !hasTrailingPathSeparator(query) ? getBrowseLeafPathSegment(query) : "";
  const unsupportedWindowsPath = isUnsupportedWindowsProjectPath(query.trim(), platform);
  const relativePathNeedsActiveProject =
    isExplicitRelativeProjectPath(query.trim()) && !activeProjectCwd;

  /* Source-control readiness is probed once per machine when its Sources step opens. */
  useEffect(() => {
    if (!machineId || currentView?.kind !== "sources") {
      return;
    }
    if (machineId in discoveryByMachineId) {
      return;
    }
    let cancelled = false;
    setPendingDiscoveryMachineId(machineId);
    void (async () => {
      try {
        const discovery = await propsRef.current.discoverSourceControl({ machineId });
        if (cancelled) {
          return;
        }
        setDiscoveryByMachineId((current) => ({ ...current, [machineId]: discovery ?? null }));
      } catch {
        if (!cancelled) {
          setDiscoveryByMachineId((current) => ({ ...current, [machineId]: null }));
        }
      } finally {
        if (!cancelled) {
          setPendingDiscoveryMachineId(null);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [currentView?.kind, discoveryByMachineId, machineId]);

  /*
   * No debounce: the request key is the DIRECTORY portion
   * of the query, so typing a leaf filter never refetches and crossing a `/`
   * always does.
   */
  useEffect(() => {
    if (
      !machineId ||
      !isBrowsing ||
      browseDirectoryPath.length === 0 ||
      unsupportedWindowsPath ||
      relativePathNeedsActiveProject
    ) {
      return;
    }
    const requestId = browseRequestRef.current + 1;
    browseRequestRef.current = requestId;
    setIsBrowsePending(true);
    void propsRef.current
      .browse({
        machineId,
        partialPath: browseDirectoryPath,
        ...(activeProjectCwd ? { cwd: activeProjectCwd } : {}),
      })
      .then((result) => {
        if (browseRequestRef.current !== requestId || !isMountedRef.current) {
          return;
        }
        setBrowseResult(result);
      })
      .catch((error: unknown) => {
        if (browseRequestRef.current !== requestId || !isMountedRef.current) {
          return;
        }
        setBrowseResult(null);
        setErrorMessage(describeError(error, "Unable to browse that directory."));
      })
      .finally(() => {
        if (browseRequestRef.current === requestId && isMountedRef.current) {
          setIsBrowsePending(false);
        }
      });
  }, [
    activeProjectCwd,
    browseDirectoryPath,
    browseGeneration,
    isBrowsing,
    machineId,
    relativePathNeedsActiveProject,
    unsupportedWindowsPath,
  ]);

  useEffect(() => {
    if (!busy) {
      setIsSlow(false);
      return;
    }
    const timer = window.setTimeout(() => setIsSlow(true), slowOperationNoticeMs);
    return () => {
      window.clearTimeout(timer);
    };
  }, [busy, slowOperationNoticeMs]);

  const browseEntries = browseResult?.entries ?? EMPTY_BROWSE_ENTRIES;
  const { exactEntry, filteredEntries, highlightedEntry } = useMemo(
    () => filterBrowseEntries({ browseEntries, browseFilterQuery, highlightedItemValue }),
    [browseEntries, browseFilterQuery, highlightedItemValue],
  );

  const hasHighlightedBrowseItem =
    highlightedEntry !== null || highlightedItemValue === BROWSE_UP_VALUE;
  const resolvedAddProjectPath = hasTrailingPathSeparator(query)
    ? (browseResult?.parentPath ?? query.trim())
    : (exactEntry?.fullPath ?? query.trim());
  const canSubmitBrowsePath =
    isBrowsing && !relativePathNeedsActiveProject && !unsupportedWindowsPath;
  const willCreateProjectPath =
    canSubmitBrowsePath &&
    !isBrowsePending &&
    query.trim().length > 0 &&
    !hasHighlightedBrowseItem &&
    (hasTrailingPathSeparator(query) ? !browseResult : exactEntry === null);

  /*
   * CDXC:AddProjectNewFolder 2026-08-18:
   * The folder is created inside the directory whose entries are on screen,
   * which is the server-resolved parent of the current query. A typed leaf
   * filter narrows that listing but never changes which directory it belongs
   * to, so the affordance stays available while the user is filtering.
   */
  const newFolderParentPath = browseResult?.parentPath ?? "";
  const canCreateNewFolder =
    isBrowsing &&
    machineId !== null &&
    !isBrowsePending &&
    newFolderParentPath.length > 0 &&
    !unsupportedWindowsPath &&
    !relativePathNeedsActiveProject;

  const submitActionLabel = isCloneDestinationStep
    ? "Continue"
    : willCreateProjectPath
      ? "Create & Add"
      : "Add";
  const submitModifierLabel = addProjectModifierLabel(platform);
  const addShortcutLabel = hasHighlightedBrowseItem ? `${submitModifierLabel} Enter` : "Enter";
  const readiness = useMemo(
    () => buildAddProjectSourceReadiness(machineId ? discoveryByMachineId[machineId] : null),
    [discoveryByMachineId, machineId],
  );

  const pushView = useCallback((view: AddProjectView) => {
    setViewStack((stack) => [...stack, view]);
    setHighlightedItemValue(null);
    setQuery("initialQuery" in view ? view.initialQuery : "");
    setBrowseResult(null);
    setErrorMessage(null);
    setBrowseGeneration((generation) => generation + 1);
  }, []);

  const popView = useCallback(() => {
    setCloneFlow(null);
    setCloneOptions(DEFAULT_CLONE_OPTIONS);
    setClonePreview(null);
    setCloneDestinationPath("");
    setViewStack((stack) => (stack.length <= 1 ? stack : stack.slice(0, -1)));
    setHighlightedItemValue(null);
    setQuery("");
    setBrowseResult(null);
    setErrorMessage(null);
    setBrowseGeneration((generation) => generation + 1);
  }, []);

  function handleQueryChange(nextQuery: string): void {
    setHighlightedItemValue(null);
    setQuery(nextQuery);
    setErrorMessage(null);
    if (
      nextQuery === "" &&
      canPopView &&
      currentView &&
      "initialQuery" in currentView &&
      currentView.initialQuery.length > 0
    ) {
      popView();
    }
  }

  function browseTo(name: string): void {
    setHighlightedItemValue(null);
    setQuery(appendBrowsePathSegment(query, name));
    setBrowseGeneration((generation) => generation + 1);
  }

  function browseUp(): void {
    const parentPath = getBrowseParentPath(query);
    if (parentPath === null) {
      return;
    }
    setHighlightedItemValue(null);
    setQuery(parentPath);
    setBrowseGeneration((generation) => generation + 1);
  }

  function startLocalBrowse(targetMachineId: string, startDirectory?: string): void {
    const targetMachine =
      machines.find((option) => option.machineId === targetMachineId) ?? null;
    setCloneFlow(null);
    setCloneOptions(DEFAULT_CLONE_OPTIONS);
    setClonePreview(null);
    setCloneDestinationPath("");
    pushView({
      initialQuery: ensureBrowseDirectoryPath(
        startDirectory ?? addProjectInitialBrowseQuery(targetMachine),
      ),
      kind: "browse",
      machineId: targetMachineId,
    });
  }

  function startCloneFlow(targetMachineId: string, source: AddProjectSourceId): void {
    setCloneOptions(DEFAULT_CLONE_OPTIONS);
    setClonePreview(null);
    setCloneDestinationPath("");
    setCloneFlow({
      remoteUrl: "",
      repository: null,
      repositoryInput: "",
      source,
      step: "repository",
    });
    pushView({ kind: "clone", machineId: targetMachineId });
  }

  function enterCloneDestinationStep(next: {
    readonly remoteUrl: string;
    readonly repository: AddProjectRepositoryInfo | null;
    readonly repositoryInput: string;
    readonly source: AddProjectSourceId;
  }): void {
    setCloneFlow({ ...next, step: "destination" });
    setHighlightedItemValue(null);
    setBrowseResult(null);
    setQuery(ensureBrowseDirectoryPath(addProjectInitialBrowseQuery(machine)));
    setBrowseGeneration((generation) => generation + 1);
  }

  async function submitRepositoryStep(): Promise<void> {
    const repositoryInput = query.trim();
    if (!cloneFlow || cloneFlow.step !== "repository" || !machineId) {
      return;
    }
    if (repositoryInput.length === 0 || busy) {
      return;
    }
    if (cloneFlow.source === "url") {
      enterCloneDestinationStep({
        remoteUrl: repositoryInput,
        repository: null,
        repositoryInput,
        source: cloneFlow.source,
      });
      return;
    }
    setBusy("lookup");
    setErrorMessage(null);
    try {
      const repository = await propsRef.current.lookupRepository({
        machineId,
        provider: cloneFlow.source,
        repository: repositoryInput,
      });
      if (!isMountedRef.current) {
        return;
      }
      enterCloneDestinationStep({
        remoteUrl: repository.url,
        repository,
        repositoryInput,
        source: cloneFlow.source,
      });
    } catch (error) {
      if (isMountedRef.current) {
        setErrorMessage(describeError(error, "Repository lookup failed."));
      }
    } finally {
      if (isMountedRef.current) {
        setBusy(null);
      }
    }
  }

  function validateProjectPath(rawPath: string): string | null {
    const trimmed = rawPath.trim();
    if (trimmed.length === 0) {
      return null;
    }
    if (isUnsupportedWindowsProjectPath(trimmed, platform)) {
      setErrorMessage("Windows-style paths are only supported on Windows machines.");
      return null;
    }
    if (isExplicitRelativeProjectPath(trimmed) && !activeProjectCwd) {
      setErrorMessage("Relative paths require an active project.");
      return null;
    }
    const resolvedPath = resolveProjectPathForDispatch(trimmed, activeProjectCwd);
    return resolvedPath.length === 0 ? null : resolvedPath;
  }

  async function registerProject(path: string, createIfMissing: boolean): Promise<void> {
    if (!machineId) {
      return;
    }
    const result = await propsRef.current.addProject({ createIfMissing, machineId, path });
    propsRef.current.onProjectAdded?.(result);
    propsRef.current.onClose();
  }

  async function submitAddProject(rawPath: string): Promise<void> {
    if (busy || !machineId) {
      return;
    }
    const path = validateProjectPath(rawPath);
    if (!path) {
      return;
    }
    setBusy("add");
    setErrorMessage(null);
    try {
      await registerProject(path, willCreateProjectPath);
    } catch (error) {
      if (isMountedRef.current) {
        setErrorMessage(describeError(error, "Failed to add project."));
      }
    } finally {
      if (isMountedRef.current) {
        setBusy(null);
      }
    }
  }

  async function pollCloneJob(jobId: string): Promise<AddProjectCloneJob> {
    for (;;) {
      const job = await propsRef.current.readCloneJob({ jobId, machineId: machineId ?? "" });
      if (job.state !== "running") {
        return job;
      }
      if (!isMountedRef.current) {
        return job;
      }
      await delay(cloneJobPollIntervalMs);
    }
  }

  async function openCloneReview(rawPath: string): Promise<void> {
    if (busy || !cloneFlow || cloneFlow.step !== "destination" || !machineId) {
      return;
    }
    const destinationPath = validateProjectPath(rawPath);
    if (!destinationPath) {
      return;
    }
    setBusy("preview");
    setErrorMessage(null);
    try {
      const preview = await propsRef.current.previewClone({
        ...cloneOptions,
        destinationPath,
        machineId,
        remoteUrl: cloneFlow.remoteUrl,
      });
      if (!isMountedRef.current) {
        return;
      }
      setCloneDestinationPath(destinationPath);
      setClonePreview(preview);
      setCloneFlow({ ...cloneFlow, step: "review" });
    } catch (error) {
      if (isMountedRef.current) {
        setErrorMessage(describeError(error, "Unable to review the clone destination."));
      }
    } finally {
      if (isMountedRef.current) {
        setBusy(null);
      }
    }
  }

  async function submitClone(): Promise<void> {
    if (
      busy ||
      !cloneFlow ||
      cloneFlow.step !== "review" ||
      !machineId ||
      !clonePreview ||
      clonePreview.destinationBlocked ||
      !isRepositoryCloneBranchNameInputValid(cloneOptions.branchName)
    ) {
      return;
    }
    setBusy("clone");
    setErrorMessage(null);
    try {
      const handle = await propsRef.current.startClone({
        ...cloneOptions,
        destinationPath: cloneDestinationPath,
        machineId,
        remoteUrl: cloneFlow.remoteUrl,
      });
      cloneJobIdRef.current = handle.jobId;
      const job = await pollCloneJob(handle.jobId);
      if (!isMountedRef.current) {
        return;
      }
      if (job.state === "canceled") {
        setErrorMessage("Clone canceled.");
        return;
      }
      if (job.state !== "completed") {
        throw new Error(job.error?.trim() || job.message?.trim() || "Repository clone failed.");
      }
      const clonedCwd = job.projectPath?.trim() ?? "";
      if (clonedCwd.length === 0) {
        throw new Error("Clone finished without a project path.");
      }
      await registerProject(clonedCwd, false);
    } catch (error) {
      if (isMountedRef.current) {
        setErrorMessage(describeError(error, "Clone failed."));
      }
    } finally {
      cloneJobIdRef.current = null;
      if (isMountedRef.current) {
        setBusy(null);
      }
    }
  }

  function startNewFolder(): void {
    if (!canCreateNewFolder || busy) {
      return;
    }
    setErrorMessage(null);
    setHighlightedItemValue(null);
    setNewFolderName("");
  }

  function cancelNewFolder(): void {
    setNewFolderName(null);
    setErrorMessage(null);
  }

  async function submitNewFolder(): Promise<void> {
    const name = (newFolderName ?? "").trim();
    if (busy || !machineId || name.length === 0 || newFolderParentPath.length === 0) {
      return;
    }
    setBusy("createFolder");
    setErrorMessage(null);
    try {
      const created = await propsRef.current.createDirectory({
        machineId,
        name,
        parentPath: newFolderParentPath,
      });
      if (!isMountedRef.current) {
        return;
      }
      /*
       * The new folder becomes the browse location, so the very next Enter adds
       * or clones into it. The query keeps whatever prefix the user typed
       * (`~/dev/`), because the created path is only ever a child of it.
       */
      setNewFolderName(null);
      setHighlightedItemValue(null);
      setQuery(appendBrowsePathSegment(query, created.name));
      setBrowseResult(null);
      setBrowseGeneration((generation) => generation + 1);
    } catch (error) {
      if (isMountedRef.current) {
        setErrorMessage(describeError(error, "Failed to create the folder."));
      }
    } finally {
      if (isMountedRef.current) {
        setBusy(null);
      }
    }
  }

  function submitResolvedPath(): void {
    if (isCloneDestinationStep) {
      void openCloneReview(resolvedAddProjectPath);
      return;
    }
    void submitAddProject(resolvedAddProjectPath);
  }

  function returnToCloneDestination(): void {
    if (!cloneFlow || cloneFlow.step !== "review" || busy) {
      return;
    }
    setCloneFlow({ ...cloneFlow, step: "destination" });
    setClonePreview(null);
    setCloneDestinationPath("");
    setErrorMessage(null);
    setHighlightedItemValue(null);
    setBrowseGeneration((generation) => generation + 1);
  }

  function cancelClone(): void {
    const jobId = cloneJobIdRef.current;
    const cancelCloneJob = propsRef.current.cancelCloneJob;
    if (!jobId || !machineId || !cancelCloneJob) {
      return;
    }
    void cancelCloneJob({ jobId, machineId }).catch((error: unknown) => {
      if (isMountedRef.current) {
        setErrorMessage(describeError(error, "Unable to cancel the clone."));
      }
    });
  }

  const rows = useMemo<readonly AddProjectRow[]>(() => {
    if (isRepositoryStep || isNewFolderStep) {
      return [];
    }
    if (isBrowsing) {
      if (unsupportedWindowsPath || relativePathNeedsActiveProject) {
        return [];
      }
      const browseRows: AddProjectRow[] = [];
      if (canNavigateUp(browseDirectoryPath)) {
        browseRows.push({
          field: "directoryUp",
          icon: <IconCornerLeftUp className={ADD_PROJECT_ROW_ICON_CLASS} />,
          onSelect: browseUp,
          title: "..",
          value: BROWSE_UP_VALUE,
        });
      }
      for (const entry of filteredEntries) {
        browseRows.push({
          dataAttributes: { "data-add-project-path": entry.fullPath },
          field: "directoryEntry",
          icon: <IconFolder className={ADD_PROJECT_ROW_ICON_CLASS} />,
          onSelect: () => browseTo(entry.name),
          title: entry.name,
          value: `browse:${entry.fullPath}`,
        });
      }
      return browseRows;
    }
    if (currentView?.kind === "machines") {
      return machines
        .filter((option) =>
          matchesAddProjectFilter(query, option.label, [option.description ?? "", option.machineId]),
        )
        .map((option) => ({
          dataAttributes: { "data-add-project-machine-id": option.machineId },
          description: option.description,
          field: "machineOption",
          icon: machineIcon(option),
          onSelect: () => {
            setCloneFlow(null);
            pushView({ kind: "sources", machineId: option.machineId });
          },
          submenu: true,
          title: option.label,
          value: `machine:${option.machineId}`,
        }));
    }
    if (currentView?.kind === "sources" && machineId) {
      const sourceRows: AddProjectRow[] = [];
      if (matchesAddProjectFilter(query, "Local folder", ["browse", "directory", "disk"])) {
        sourceRows.push({
          dataAttributes: { "data-add-project-source": "local" },
          description: "Browse a folder on disk",
          field: "sourceOption",
          icon: <IconFolder className={ADD_PROJECT_ROW_ICON_CLASS} />,
          onSelect: () => startLocalBrowse(machineId),
          submenu: true,
          title: "Local folder",
          value: "source:local",
        });
      }
      if (
        matchesAddProjectFilter(query, "External drives and other folders", [
          "root",
          "volumes",
          "external",
          "drive",
          "disk",
          "usb",
          "/",
        ])
      ) {
        sourceRows.push({
          dataAttributes: { "data-add-project-source": "root" },
          description: "Browse from the root of the filesystem",
          field: "sourceOption",
          icon: <IconFolderRoot className={ADD_PROJECT_ROW_ICON_CLASS} />,
          onSelect: () => startLocalBrowse(machineId, ADD_PROJECT_ROOT_BROWSE_PATH),
          submenu: true,
          title: "External drives and other folders",
          value: "source:root",
        });
      }
      for (const source of orderedAddProjectSources(readiness)) {
        const title = addProjectSourceRowTitle(source);
        if (!matchesAddProjectFilter(query, title, [source, "clone", "repository", "git"])) {
          continue;
        }
        const sourceReadiness = readiness[source];
        sourceRows.push({
          dataAttributes: { "data-add-project-source": source },
          description: sourceReadiness.ready
            ? addProjectSourceRowDescription(source)
            : (sourceReadiness.hint ?? addProjectSourceRowDescription(source)),
          disabled: !sourceReadiness.ready,
          field: "sourceOption",
          icon: sourceIcon(source),
          onSelect: () => startCloneFlow(machineId, source),
          submenu: sourceReadiness.ready,
          title,
          trailing: sourceReadiness.ready ? undefined : (
            <AppTooltip content={sourceReadiness.hint}>
              <Button
                aria-label={`${addProjectSourceLabel(source)} setup required`}
                className="ml-auto h-6 rounded-none px-2 text-sm"
                data-add-project-field="setupRequired"
                data-add-project-source={source}
                onClick={(event) => {
                  event.stopPropagation();
                  propsRef.current.onOpenSourceControlSettings?.(source as AddProjectProviderId);
                }}
                onMouseDown={(event) => {
                  event.preventDefault();
                }}
                size="xs"
                type="button"
                variant="outline"
              >
                Setup Required
              </Button>
            </AppTooltip>
          ),
          value: `source:${source}`,
        });
      }
      return sourceRows;
    }
    return [];
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    browseDirectoryPath,
    currentView?.kind,
    filteredEntries,
    isBrowsing,
    isNewFolderStep,
    isRepositoryStep,
    machineId,
    machines,
    query,
    readiness,
    relativePathNeedsActiveProject,
    unsupportedWindowsPath,
  ]);

  const selectableRows = useMemo(() => rows.filter((row) => !row.disabled), [rows]);
  const highlightedRow =
    selectableRows.find((row) => row.value === highlightedItemValue) ?? null;

  useEffect(() => {
    if (!highlightedItemValue || !listRef.current) {
      return;
    }
    const element = listRef.current.querySelector(
      `[data-add-project-value="${cssEscape(highlightedItemValue)}"]`,
    );
    if (element instanceof HTMLElement) {
      element.scrollIntoView({ block: "nearest" });
    }
  }, [highlightedItemValue]);

  function moveHighlight(direction: 1 | -1): void {
    if (selectableRows.length === 0) {
      return;
    }
    const currentIndex = selectableRows.findIndex((row) => row.value === highlightedItemValue);
    if (currentIndex === -1) {
      setHighlightedItemValue(
        (direction === 1 ? selectableRows[0] : selectableRows[selectableRows.length - 1]).value,
      );
      return;
    }
    const nextIndex = currentIndex + direction;
    if (nextIndex < 0 || nextIndex >= selectableRows.length) {
      setHighlightedItemValue(null);
      return;
    }
    setHighlightedItemValue(selectableRows[nextIndex].value);
  }

  function isPrimaryModifierPressed(event: ReactKeyboardEvent<HTMLInputElement>): boolean {
    return isPrimaryModifierPlatform(platform)
      ? event.metaKey && !event.ctrlKey
      : event.ctrlKey && !event.metaKey;
  }

  function handleKeyDown(event: ReactKeyboardEvent<HTMLInputElement>): void {
    if (isNewFolderStep) {
      if (event.key === "Enter") {
        event.preventDefault();
        void submitNewFolder();
        return;
      }
      if (event.key === "Backspace" && (newFolderName ?? "").length === 0) {
        event.preventDefault();
        cancelNewFolder();
      }
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      moveHighlight(1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      moveHighlight(-1);
      return;
    }
    if (event.key === "Enter") {
      if (isRepositoryStep) {
        event.preventDefault();
        void submitRepositoryStep();
        return;
      }
      const shouldSubmitBrowsePath =
        canSubmitBrowsePath && (!hasHighlightedBrowseItem || isPrimaryModifierPressed(event));
      if (shouldSubmitBrowsePath) {
        event.preventDefault();
        submitResolvedPath();
        return;
      }
      if (highlightedRow) {
        event.preventDefault();
        highlightedRow.onSelect();
      }
      return;
    }
    if (event.key === "Backspace" && query === "" && canPopView) {
      event.preventDefault();
      popView();
    }
  }

  const emptyStateMessage = isNewFolderStep
    ? addProjectNewFolderMessage({
        name: newFolderName ?? "",
        parentPath: newFolderParentPath,
      })
    : addProjectEmptyStateMessage({
        cloneSource: cloneFlow?.source ?? null,
        cloneStep:
          cloneFlow?.step === "review" ? "destination" : (cloneFlow?.step ?? null),
        hasMachines: machines.length > 0,
        isLoadingMachines,
        relativePathNeedsActiveProject,
        unsupportedWindowsPath,
        willCreateProjectPath,
      });
  const groupLabel = isBrowsing
    ? isCloneDestinationStep
      ? "Select where to clone"
      : "Directories"
    : currentView?.kind === "machines"
      ? "Machines"
      : "Sources";
  const placeholder = isNewFolderStep
    ? "New folder name"
    : isRepositoryStep
      ? addProjectRepositoryPlaceholder(cloneFlow.source)
      : addProjectPathPlaceholder(canPopView);
  const repositoryActionLabel = cloneFlow ? addProjectRepositoryActionLabel(cloneFlow.source) : "";
  const isCloning = busy === "clone";
  const busyLabel =
    busy === "add"
      ? "Adding"
      : busy === "clone"
        ? "Cloning"
        : busy === "preview"
          ? "Reviewing"
          : busy === "createFolder"
            ? "Creating"
            : busy === "lookup"
              ? "Working"
              : null;

  if (isCloneReviewStep && cloneFlow && clonePreview) {
    const hasInvalidBranchName = !isRepositoryCloneBranchNameInputValid(
      cloneOptions.branchName,
    );
    const destinationDescription = clonePreview.destinationBlocked
      ? (clonePreview.warning ?? "Choose a different destination before cloning.")
      : clonePreview.destinationExists && clonePreview.destinationIsEmpty
        ? "Existing empty folder. The repository will be cloned directly into it."
        : "This folder will be created for the repository.";
    const canClone =
      !hasInvalidBranchName && !clonePreview.destinationBlocked && busy === null;

    return (
      <div
        className="flex h-full min-h-0 w-full min-w-0 flex-col"
        data-add-project-clone-step="review"
        data-add-project-modal=""
      >
        <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-4 py-4 sm:px-5">
          <div className="mx-auto flex w-full max-w-lg flex-col gap-4">
            <div className="flex min-w-0 items-start gap-3">
              <Button
                aria-label="Back to clone destination"
                className="mt-0.5 size-8 shrink-0 rounded-none"
                disabled={busy !== null}
                onClick={returnToCloneDestination}
                size="icon-sm"
                type="button"
                variant="ghost"
              >
                <IconArrowLeft aria-hidden="true" className="size-4" />
              </Button>
              <div className="min-w-0">
                <h2 className="text-sm font-semibold text-foreground">Review clone</h2>
                <p className="mt-0.5 text-sm leading-relaxed text-muted-foreground">
                  Confirm the destination and adjust optional Git settings.
                </p>
              </div>
              {machine ? (
                <span className="ml-auto inline-flex min-w-0 shrink-0 items-center gap-1.5 pt-1 text-sm text-muted-foreground">
                  <IconFolderPlus aria-hidden="true" className="size-3 shrink-0" />
                  <span className="max-w-32 truncate">{machine.label}</span>
                </span>
              ) : null}
            </div>

            <div className="grid min-w-0 gap-2 sm:grid-cols-2">
              <div
                className="flex min-w-0 items-start gap-2.5 border border-border/60 bg-muted/15 px-3 py-2.5"
                data-add-project-field="reviewRepository"
              >
                <span className="mt-0.5 shrink-0 text-muted-foreground/80">
                  {sourceIcon(cloneFlow.source)}
                </span>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="text-sm font-medium text-muted-foreground">Repository</span>
                  <span className="mt-1 truncate text-sm font-medium text-foreground">
                    {cloneFlow.repository?.nameWithOwner ?? cloneFlow.repositoryInput}
                  </span>
                  <span className="mt-0.5 truncate text-sm text-muted-foreground">
                    {cloneFlow.repository?.url ?? cloneFlow.remoteUrl}
                  </span>
                </span>
              </div>

              <div
                className={cn(
                  "flex min-w-0 items-start gap-2.5 border bg-muted/15 px-3 py-2.5",
                  clonePreview.destinationBlocked
                    ? "border-destructive/50"
                    : "border-border/60",
                )}
                data-add-project-field="reviewDestination"
              >
                <IconFolderCheck
                  aria-hidden="true"
                  className="mt-0.5 size-4 shrink-0 text-muted-foreground/80"
                />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="text-sm font-medium text-muted-foreground">Destination</span>
                  <span
                    className="mt-1 break-all text-sm font-medium text-foreground"
                    title={clonePreview.destinationPath}
                  >
                    {clonePreview.destinationPath}
                  </span>
                  <span
                    className={cn(
                      "mt-0.5 text-sm leading-relaxed",
                      clonePreview.destinationBlocked
                        ? "text-destructive"
                        : "text-muted-foreground",
                    )}
                  >
                    {destinationDescription}
                  </span>
                </span>
              </div>
            </div>

            <div className="border border-border/60 bg-muted/10 px-3 py-3">
              <div className="mb-3 flex items-center justify-between gap-3">
                <span className="text-sm font-semibold text-foreground">Clone options</span>
                <span className="text-sm font-medium text-muted-foreground">Optional</span>
              </div>

              <label className="block min-w-0">
                <span className="mb-1.5 flex items-center gap-1.5 text-sm font-medium text-muted-foreground">
                  <IconGitBranch aria-hidden="true" className="size-3.5" />
                  Branch
                </span>
                <Input
                  aria-invalid={hasInvalidBranchName || undefined}
                  autoComplete="off"
                  className="h-9 text-sm"
                  disabled={busy !== null}
                  onChange={(event) => {
                    setCloneOptions((current) => ({
                      ...current,
                      branchName: event.currentTarget.value,
                    }));
                    setErrorMessage(null);
                  }}
                  placeholder="Default branch"
                  spellCheck={false}
                  value={cloneOptions.branchName}
                />
                <span
                  className={cn(
                    "mt-1.5 block text-sm",
                    hasInvalidBranchName ? "text-destructive" : "text-muted-foreground",
                  )}
                >
                  {hasInvalidBranchName
                    ? "Enter a valid Git branch name."
                    : "Leave empty to use the repository default branch."}
                </span>
              </label>

              <div className="mt-3 grid gap-2 sm:grid-cols-2">
                <label className="flex min-w-0 items-start gap-2.5 border border-border/50 px-3 py-2.5 hover:bg-muted/30">
                  <Checkbox
                    checked={cloneOptions.cloneMainOnly}
                    className="mt-0.5 rounded-none"
                    disabled={busy !== null}
                    onCheckedChange={(checked) =>
                      setCloneOptions((current) => ({
                        ...current,
                        cloneMainOnly: checked === true,
                      }))
                    }
                  />
                  <span className="min-w-0">
                    <span className="block text-sm font-medium text-foreground">
                      Clone branch only
                    </span>
                    <span className="mt-0.5 block text-sm leading-relaxed text-muted-foreground">
                      Fetch only the selected or default branch.
                    </span>
                  </span>
                </label>

                <label className="flex min-w-0 items-start gap-2.5 border border-border/50 px-3 py-2.5 hover:bg-muted/30">
                  <Checkbox
                    checked={cloneOptions.shallowClone}
                    className="mt-0.5 rounded-none"
                    disabled={busy !== null}
                    onCheckedChange={(checked) =>
                      setCloneOptions((current) => ({
                        ...current,
                        shallowClone: checked === true,
                      }))
                    }
                  />
                  <span className="min-w-0">
                    <span className="block text-sm font-medium text-foreground">
                      Shallow clone
                    </span>
                    <span className="mt-0.5 block text-sm leading-relaxed text-muted-foreground">
                      Fetch only the latest commit history.
                    </span>
                  </span>
                </label>
              </div>
            </div>

            {errorMessage ? (
              <div
                className="flex items-start gap-2 border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
                data-add-project-field="error"
                role="alert"
              >
                <IconAlertTriangle aria-hidden="true" className="mt-px size-3.5 shrink-0" />
                <span className="min-w-0 break-words">{errorMessage}</span>
              </div>
            ) : null}

            {isSlow && isCloning ? (
              <div
                className="flex items-center gap-2 border border-border/60 px-3 py-2 text-sm text-muted-foreground"
                data-add-project-field="notice"
                role="status"
              >
                <span>Still cloning. The machine may be reconnecting.</span>
                {props.cancelCloneJob ? (
                  <button
                    className="underline underline-offset-2 hover:text-foreground"
                    data-add-project-field="cloneCancel"
                    onClick={cancelClone}
                    type="button"
                  >
                    Cancel clone
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>
        </div>

        <div
          className="flex shrink-0 items-center gap-3 border-t border-border/70 px-4 py-2.5 text-sm text-muted-foreground"
          data-add-project-field="footer"
        >
          <AddProjectFooterHint keys="Esc" label="Close" />
          <div className="ml-auto flex items-center gap-2">
            <Button
              disabled={busy !== null}
              onClick={returnToCloneDestination}
              type="button"
              variant="outline"
            >
              Back
            </Button>
            <Button disabled={!canClone} onClick={() => void submitClone()} type="button">
              {isCloning ? "Cloning..." : "Clone & Add"}
            </Button>
          </div>
        </div>
      </div>
    );
  }

  return (
    /*
     * `min-w-0` matters: DialogContent is a grid, and a grid item's automatic
     * minimum size would otherwise let a long row description push the whole
     * dialog body past the popup's clipped edge.
     */
    <div className="flex h-full min-h-0 w-full min-w-0 flex-col" data-add-project-modal="">
      <div className="shrink-0 px-3 pt-3">
        <InputGroup className={ADD_PROJECT_PATH_BAR_CLASS}>
          {isNewFolderStep || canPopView ? (
            <InputGroupAddon align="inline-start" className={ADD_PROJECT_ACTION_ADDON_CLASS}>
              <button
                aria-label={isNewFolderStep ? "Cancel new folder" : "Back"}
                className="flex h-full w-10 items-center justify-center self-stretch rounded-none border-r border-border/70 text-muted-foreground hover:bg-muted hover:text-foreground"
                data-add-project-field="back"
                onClick={isNewFolderStep ? cancelNewFolder : popView}
                onMouseDown={(event) => {
                  event.preventDefault();
                }}
                type="button"
              >
                <IconArrowLeft aria-hidden="true" className="size-4" />
              </button>
            </InputGroupAddon>
          ) : (
            <InputGroupAddon align="inline-start">
              {isBrowsing ? (
                <IconFolderPlus aria-hidden="true" className="size-4" />
              ) : (
                <IconSearch aria-hidden="true" className="size-4 opacity-50" />
              )}
            </InputGroupAddon>
          )}
          <InputGroupInput
            aria-label={
              isNewFolderStep ? "New folder name" : isRepositoryStep ? "Repository" : "Project path"
            }
            autoComplete="off"
            autoFocus
            className="text-sm"
            data-add-project-field={isNewFolderStep ? "newFolderInput" : "pathInput"}
            onChange={(event) => {
              if (isNewFolderStep) {
                setErrorMessage(null);
                setNewFolderName(event.currentTarget.value);
                return;
              }
              handleQueryChange(event.currentTarget.value);
            }}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            spellCheck={false}
            value={isNewFolderStep ? (newFolderName ?? "") : query}
          />
          {isNewFolderStep ? (
            <InputGroupAddon align="inline-end" className={ADD_PROJECT_ACTION_ADDON_CLASS}>
              <Button
                aria-label="Create folder (Enter)"
                className={ADD_PROJECT_ACTION_BUTTON_CLASS}
                data-add-project-field="newFolderSubmit"
                disabled={(newFolderName ?? "").trim().length === 0 || busy !== null}
                onClick={() => {
                  void submitNewFolder();
                }}
                onMouseDown={(event) => {
                  event.preventDefault();
                }}
                size="xs"
                tabIndex={-1}
                type="button"
                variant="ghost"
              >
                {busy === "createFolder" ? "Creating" : "Create Folder"}
              </Button>
            </InputGroupAddon>
          ) : isRepositoryStep ? (
            <InputGroupAddon align="inline-end" className={ADD_PROJECT_ACTION_ADDON_CLASS}>
              <Button
                aria-label={`${repositoryActionLabel} (Enter)`}
                className={cn(ADD_PROJECT_ACTION_BUTTON_CLASS, "gap-1.5")}
                data-add-project-field="repositoryAction"
                disabled={query.trim().length === 0 || busy !== null}
                onClick={() => {
                  void submitRepositoryStep();
                }}
                onMouseDown={(event) => {
                  event.preventDefault();
                }}
                size="xs"
                tabIndex={-1}
                type="button"
                variant="ghost"
              >
                {busy === "lookup" ? "Working" : repositoryActionLabel}
                <kbd className="rounded-none bg-muted-foreground/10 px-1.5 text-sm">Enter</kbd>
              </Button>
            </InputGroupAddon>
          ) : isBrowsing ? (
            <InputGroupAddon align="inline-end" className={ADD_PROJECT_ACTION_ADDON_CLASS}>
              <Button
                aria-label="New folder"
                className={cn(ADD_PROJECT_ACTION_BUTTON_CLASS, "gap-1.5")}
                data-add-project-field="newFolder"
                disabled={!canCreateNewFolder || busy !== null}
                onClick={startNewFolder}
                onMouseDown={(event) => {
                  event.preventDefault();
                }}
                size="xs"
                tabIndex={-1}
                type="button"
                variant="ghost"
              >
                <IconFolderPlus aria-hidden="true" className="size-3.5" />
                New Folder
              </Button>
              <Button
                aria-label={`${submitActionLabel} (${addShortcutLabel})`}
                className={ADD_PROJECT_ACTION_BUTTON_CLASS}
                data-add-project-field="submit"
                disabled={!canSubmitBrowsePath || busy !== null}
                onClick={() => {
                  submitResolvedPath();
                }}
                onMouseDown={(event) => {
                  event.preventDefault();
                }}
                size="xs"
                tabIndex={-1}
                type="button"
                variant="ghost"
              >
                {busyLabel ?? submitActionLabel}
              </Button>
            </InputGroupAddon>
          ) : null}
        </InputGroup>
      </div>

      {isCloneDestinationStep && cloneFlow ? (
        <div
          className="mx-3 mt-2 flex min-w-0 shrink-0 flex-col gap-1 border border-border/60 px-3 py-2"
          data-add-project-field="repositoryCard"
        >
          <span className="text-sm font-medium text-muted-foreground">Repository</span>
          <span className="flex min-w-0 items-center gap-2">
            <span className="text-muted-foreground/80">{sourceIcon(cloneFlow.source)}</span>
            <span className="flex min-w-0 flex-col">
              <span className="truncate text-sm font-medium text-foreground">
                {cloneFlow.repository?.nameWithOwner ?? cloneFlow.repositoryInput}
              </span>
              <span className="truncate text-sm text-muted-foreground/85">
                {cloneFlow.repository?.url ?? cloneFlow.remoteUrl}
              </span>
            </span>
          </span>
        </div>
      ) : null}

      {errorMessage ? (
        <div
          className="mx-3 mt-2 flex shrink-0 items-start gap-2 border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
          data-add-project-field="error"
          role="alert"
        >
          <IconAlertTriangle aria-hidden="true" className="mt-px size-3.5 shrink-0" />
          <span className="min-w-0 break-words">{errorMessage}</span>
        </div>
      ) : null}

      {isSlow && busy ? (
        <div
          className="mx-3 mt-2 flex shrink-0 items-center gap-2 border border-border/60 px-3 py-2 text-sm text-muted-foreground"
          data-add-project-field="notice"
          role="status"
        >
          <span>Still working. The machine may be reconnecting.</span>
          {isCloning && props.cancelCloneJob ? (
            <button
              className="underline underline-offset-2 hover:text-foreground"
              data-add-project-field="cloneCancel"
              onClick={cancelClone}
              type="button"
            >
              Cancel clone
            </button>
          ) : null}
        </div>
      ) : null}

      <div
        className="vertical-scroll-fade-mask no-scrollbar min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-2 pb-2"
        data-add-project-field="list"
        ref={listRef}
        role="listbox"
      >
        {rows.length === 0 ? (
          <div
            className="flex h-full min-h-24 items-center justify-center px-6 text-center text-sm text-balance text-muted-foreground"
            data-add-project-field="emptyState"
          >
            {emptyStateMessage}
          </div>
        ) : (
          <div>
            <div className="px-2 pt-3 pb-1.5 text-sm font-medium text-muted-foreground">
              {groupLabel}
            </div>
            {rows.map((row) => (
              <div
                aria-disabled={row.disabled || undefined}
                aria-selected={row.value === highlightedItemValue}
                className={cn(
                  "relative flex min-h-9 cursor-default items-center gap-2.5 rounded-none px-2 py-1.5 text-sm outline-hidden select-none",
                  row.disabled ? "opacity-60" : undefined,
                  row.value === highlightedItemValue ? "bg-muted text-foreground" : undefined,
                )}
                data-add-project-field={row.field}
                data-add-project-value={row.value}
                key={row.value}
                onClick={() => {
                  if (!row.disabled) {
                    row.onSelect();
                  }
                }}
                onMouseDown={(event) => {
                  event.preventDefault();
                }}
                onMouseMove={() => {
                  if (!row.disabled) {
                    setHighlightedItemValue(row.value);
                  }
                }}
                role="option"
                {...(row.dataAttributes ?? {})}
              >
                {row.icon}
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-sm text-foreground">{row.title}</span>
                  {row.description ? (
                    <span className="truncate text-sm text-muted-foreground/85">
                      {row.description}
                    </span>
                  ) : null}
                </span>
                {row.trailing}
              </div>
            ))}
          </div>
        )}
        {pendingDiscoveryMachineId && currentView?.kind === "sources" ? (
          <div
            className="px-2 py-2 text-sm text-muted-foreground"
            data-add-project-field="discoveryPending"
          >
            Checking source control providers...
          </div>
        ) : null}
      </div>

      <div
        className="flex shrink-0 items-center gap-4 border-t border-border/70 px-4 py-2.5 text-sm text-muted-foreground"
        data-add-project-field="footer"
      >
        {isNewFolderStep ? null : <AddProjectFooterHint keys="↑ ↓" label="Navigate" />}
        {isNewFolderStep ? (
          <AddProjectFooterHint keys="Enter" label="Create folder" />
        ) : isRepositoryStep ? (
          <AddProjectFooterHint keys="Enter" label={repositoryActionLabel} />
        ) : isBrowsing ? (
          <AddProjectFooterHint keys={addShortcutLabel} label={submitActionLabel} />
        ) : (
          <AddProjectFooterHint keys="Enter" label="Select" />
        )}
        {isNewFolderStep ? (
          <AddProjectFooterHint keys="Backspace" label="Cancel" />
        ) : canPopView ? (
          <AddProjectFooterHint keys="Backspace" label="Back" />
        ) : null}
        <AddProjectFooterHint keys="Esc" label="Close" />
        {machine ? (
          <span
            className="ml-auto inline-flex min-w-0 items-center gap-1.5"
            data-add-project-field="machineLabel"
          >
            <IconFolderPlus aria-hidden="true" className="size-3 shrink-0" />
            <span className="truncate">{machine.label}</span>
          </span>
        ) : null}
      </div>
    </div>
  );
}

/*
 * CDXC:AddProjectChrome 2026-07-31:
 * Footer hints render their keys as key caps so the shortcut row reads as
 * chrome instead of a run-on sentence ("Enter Select Backspace Back Esc Close").
 */
function AddProjectFooterHint({
  keys,
  label,
}: {
  readonly keys: string;
  readonly label: string;
}): ReactNode {
  return (
    <span className="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap">
      <span className="inline-flex items-center gap-1">
        {keys.split(" ").map((key) => (
          <kbd
            className="inline-flex h-5 min-w-5 items-center justify-center rounded-none border border-border/70 bg-muted/60 px-1.5 font-sans text-sm leading-none text-muted-foreground"
            key={key}
          >
            {key}
          </kbd>
        ))}
      </span>
      {label}
    </span>
  );
}

function buildInitialViewStack(
  options: readonly AddProjectMachineOption[],
  initialMachineId: string | undefined,
): AddProjectView[] {
  if (options.length === 0) {
    return [];
  }
  const preselected = initialMachineId
    ? options.find((option) => option.machineId === initialMachineId)
    : undefined;
  if (preselected) {
    return [{ kind: "sources", machineId: preselected.machineId }];
  }
  if (options.length === 1) {
    return [{ kind: "sources", machineId: options[0].machineId }];
  }
  return [{ kind: "machines" }];
}

function machineIcon(option: AddProjectMachineOption): ReactNode {
  const isLocal = option.machineId === "local";
  return isLocal ? (
    <IconDeviceDesktop className={ADD_PROJECT_ROW_ICON_CLASS} />
  ) : (
    <IconServer className={ADD_PROJECT_ROW_ICON_CLASS} />
  );
}

function sourceIcon(source: AddProjectSourceId): ReactNode {
  switch (source) {
    case "azure-devops":
      return <IconBrandAzure className={ADD_PROJECT_ROW_ICON_CLASS} />;
    case "bitbucket":
      return <IconBrandBitbucket className={ADD_PROJECT_ROW_ICON_CLASS} />;
    case "github":
      return <IconBrandGithub className={ADD_PROJECT_ROW_ICON_CLASS} />;
    case "gitlab":
      return <IconBrandGitlab className={ADD_PROJECT_ROW_ICON_CLASS} />;
    default:
      return <IconLink className={ADD_PROJECT_ROW_ICON_CLASS} />;
  }
}

function describeError(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message.trim();
  }
  if (typeof error === "string" && error.trim().length > 0) {
    return error.trim();
  }
  return fallback;
}

function cssEscape(value: string): string {
  return value.replace(/["\\]/gu, "\\$&");
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, milliseconds);
  });
}
