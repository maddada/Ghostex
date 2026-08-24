import {
  IconChevronDown,
  IconChevronRight,
  IconDeviceFloppy,
  IconEdit,
  IconFile,
  IconFolderOpen,
  IconRefresh,
} from "@tabler/icons-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Button } from "@/packages/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/packages/components/ui/dialog";
import { Input } from "@/packages/components/ui/input";
import { ScrollArea } from "@/packages/components/ui/scroll-area";
import { Separator } from "@/packages/components/ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/packages/components/ui/tabs";
import { Textarea } from "@/packages/components/ui/textarea";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/packages/components/ui/tooltip";
import { cn } from "@/packages/components/utils";
import { getBrandAgentLogoStyle } from "./agent-logos";
import type { WebviewApi } from "./webview-api";
import { applySavedAgentsHubContents } from "../shared/agents-hub-catalog";
import type {
  AgentsHubCatalogMessage,
  AgentsHubFile,
  AgentsHubFileContentMessage,
  AgentsHubGroup,
  AgentsHubProfile,
  AgentsHubTab,
} from "../shared/session-grid-contract";

type MonacoAmdRequire = {
  (deps: string[], callback: () => void): void;
  config?: (config: Record<string, unknown>) => void;
};

declare global {
  interface Window {
    MonacoEnvironment?: unknown;
    monaco?: {
      editor: {
        create: (
          element: HTMLElement,
          options: Record<string, unknown>,
        ) => {
          dispose: () => void;
          getModel: () => unknown;
          getValue: () => string;
          layout: () => void;
          onDidChangeModelContent: (listener: () => void) => { dispose: () => void };
          setValue: (value: string) => void;
        };
        setModelLanguage: (model: unknown, language: string) => void;
      };
    };
  }
}

const tabLabels: Record<AgentsHubTab, string> = {
  configs: "Configs & MCPs",
  hooks: "Hooks",
  mds: "MDs",
  skills: "Skills",
};

const emptyGroupsByTab: Record<AgentsHubTab, AgentsHubGroup[]> = {
  configs: [],
  hooks: [],
  mds: [],
  skills: [],
};
const emptySavedContentsByPath: Record<string, string> = {};

type AgentsHubSavedContentOverlay = {
  catalogGeneratedAt?: string;
  contentsByPath: Record<string, string>;
};

type LoadedAgentsHubFile = AgentsHubFile & {
  content: string;
};

export function AgentsHubModal({
  catalog,
  fileContent,
  initialTab,
  isOpen,
  onClose,
  vscode,
}: {
  catalog?: AgentsHubCatalogMessage;
  fileContent?: AgentsHubFileContentMessage;
  initialTab?: AgentsHubTab;
  isOpen: boolean;
  onClose: () => void;
  vscode: WebviewApi;
}) {
  return (
    <TooltipProvider>
      <Dialog open={isOpen} onOpenChange={(open) => (!open ? onClose() : undefined)}>
        <DialogContent className="agents-hub-dialog ghostex-settings-shadcn" showCloseButton={false}>
          <DialogHeader className="ghostex-modal-heading-bar agents-hub-header">
            <DialogTitle className="ghostex-modal-heading-title agents-hub-title">
              {/*
               * CDXC:ExperimentalFeatures 2026-06-28-07:41:
               * Agents Hub is no longer gated by Enable Experimental Features,
               * so the modal title should not carry beta status after open.
               */}
              Agents Hub
            </DialogTitle>
          </DialogHeader>
          <AgentsHubSurface
            catalog={catalog}
            fileContent={fileContent}
            initialTab={initialTab}
            isOpen={isOpen}
            vscode={vscode}
          />
        </DialogContent>
      </Dialog>
    </TooltipProvider>
  );
}

function AgentsHubSurface({
  catalog,
  fileContent,
  initialTab = "mds",
  isOpen,
  vscode,
}: {
  catalog?: AgentsHubCatalogMessage;
  fileContent?: AgentsHubFileContentMessage;
  initialTab?: AgentsHubTab;
  isOpen: boolean;
  vscode: WebviewApi;
}) {
  const [fileContentsByPath, setFileContentsByPath] = useState<Record<string, string>>({});
  const [fileContentErrorsByPath, setFileContentErrorsByPath] = useState<Record<string, string>>(
    {},
  );
  const [pendingFileContentRequestsByPath, setPendingFileContentRequestsByPath] = useState<
    Record<string, string>
  >({});
  const [savedContentOverlay, setSavedContentOverlay] = useState<AgentsHubSavedContentOverlay>({
    contentsByPath: emptySavedContentsByPath,
  });
  const activeSavedContentsByPath =
    savedContentOverlay.catalogGeneratedAt === catalog?.generatedAt
      ? savedContentOverlay.contentsByPath
      : emptySavedContentsByPath;
  const groupsByTab = useMemo(
    () =>
      applySavedAgentsHubContents(catalog?.groupsByTab ?? emptyGroupsByTab, activeSavedContentsByPath),
    [activeSavedContentsByPath, catalog?.groupsByTab],
  );
  const [activeTab, setActiveTab] = useState<AgentsHubTab>(initialTab);
  const [query, setQuery] = useState("");
  const [selectedFileIds, setSelectedFileIds] = useState<Record<AgentsHubTab, string>>({
    configs: firstFileId(emptyGroupsByTab, "configs"),
    hooks: firstFileId(emptyGroupsByTab, "hooks"),
    mds: firstFileId(emptyGroupsByTab, "mds"),
    skills: firstFileId(emptyGroupsByTab, "skills"),
  });
  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => new Set());
  const didResetSkillExpansionForOpenRef = useRef(false);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    /**
     * CDXC:AgentsHub 2026-05-14-08:29:
     * The Hub catalog is filesystem-owned data. Request it from native on each open so profile-specific files, installed skills, and config files reflect the current machine without baking private file contents into the web bundle.
     */
    vscode.postMessage({ type: "requestAgentsHubCatalog" });
  }, [isOpen, vscode]);

  useEffect(() => {
    if (!isOpen) {
      didResetSkillExpansionForOpenRef.current = false;
      return;
    }
    if (didResetSkillExpansionForOpenRef.current || !catalog) {
      return;
    }

    didResetSkillExpansionForOpenRef.current = true;
    setExpandedIds((current) => {
      const skillGroupIds = new Set(catalog.groupsByTab.skills.map((group) => group.id));
      if (![...skillGroupIds].some((groupId) => current.has(groupId))) {
        return current;
      }
      const next = new Set(current);
      for (const groupId of skillGroupIds) {
        next.delete(groupId);
      }
      return next;
    });
  }, [catalog, isOpen]);

  useEffect(() => {
    /*
     * CDXC:AgentsHub 2026-06-12-02:53:
     * The Hub catalog is metadata-only so opening the modal does not bridge
     * every local agent file buffer. Clear per-file editor caches whenever
     * native returns a new catalog generation, then fetch only the selected
     * file content through a separate small request.
     */
    setFileContentsByPath({});
    setFileContentErrorsByPath({});
    setPendingFileContentRequestsByPath({});
  }, [catalog?.generatedAt]);

  useEffect(() => {
    setSelectedFileIds((current) => ({
      configs: findFile(groupsByTab, "configs", current.configs)
        ? current.configs
        : firstFileId(groupsByTab, "configs"),
      hooks: findFile(groupsByTab, "hooks", current.hooks)
        ? current.hooks
        : firstFileId(groupsByTab, "hooks"),
      mds: findFile(groupsByTab, "mds", current.mds)
        ? current.mds
        : firstFileId(groupsByTab, "mds"),
      skills: findFile(groupsByTab, "skills", current.skills)
        ? current.skills
        : firstFileId(groupsByTab, "skills"),
    }));
    setExpandedIds((current) => {
      const next = new Set(current);
      for (const group of [...groupsByTab.configs, ...groupsByTab.hooks]) {
        next.add(group.id);
      }
      return next;
    });
  }, [groupsByTab]);

  const activeFile = findFile(groupsByTab, activeTab, selectedFileIds[activeTab]);
  const activeFileContent =
    activeFile === undefined
      ? undefined
      : activeFile.content ?? fileContentsByPath[activeFile.path];
  const activeFileLoadError =
    activeFile === undefined ? undefined : fileContentErrorsByPath[activeFile.path];
  const isActiveFileContentLoading =
    activeFile !== undefined && activeFileContent === undefined && activeFileLoadError === undefined;

  useEffect(() => {
    if (!fileContent) {
      return;
    }

    setPendingFileContentRequestsByPath((current) => {
      if (current[fileContent.filePath] !== fileContent.requestId) {
        return current;
      }
      const next = { ...current };
      delete next[fileContent.filePath];
      return next;
    });

    if (typeof fileContent.content === "string") {
      setFileContentsByPath((current) => ({
        ...current,
        [fileContent.filePath]: fileContent.content!,
      }));
      setFileContentErrorsByPath((current) => {
        if (!(fileContent.filePath in current)) {
          return current;
        }
        const next = { ...current };
        delete next[fileContent.filePath];
        return next;
      });
      return;
    }

    setFileContentErrorsByPath((current) => ({
      ...current,
      [fileContent.filePath]: fileContent.errorMessage ?? "Unable to load file contents.",
    }));
  }, [fileContent]);

  useEffect(() => {
    if (
      !isOpen ||
      !activeFile ||
      activeFileContent !== undefined ||
      pendingFileContentRequestsByPath[activeFile.path]
    ) {
      return;
    }

    const requestId = `agents-hub-file-${Date.now().toString(36)}-${Math.random()
      .toString(36)
      .slice(2)}`;
    setPendingFileContentRequestsByPath((current) => ({
      ...current,
      [activeFile.path]: requestId,
    }));
    setFileContentErrorsByPath((current) => {
      if (!(activeFile.path in current)) {
        return current;
      }
      const next = { ...current };
      delete next[activeFile.path];
      return next;
    });
    vscode.postMessage({
      filePath: activeFile.path,
      requestId,
      type: "requestAgentsHubFileContent",
    });
  }, [
    activeFile,
    activeFileContent,
    isOpen,
    pendingFileContentRequestsByPath,
    vscode,
  ]);

  return (
    <Tabs
      className="agents-hub-tabs"
      onValueChange={(value) => setActiveTab(value as AgentsHubTab)}
      value={activeTab}
    >
      <TabsList className="agents-hub-tabs-list app-modal-tab-rail">
        {(Object.keys(tabLabels) as AgentsHubTab[]).map((tab) => (
          <TabsTrigger key={tab} value={tab}>
            {tabLabels[tab]}
          </TabsTrigger>
        ))}
      </TabsList>
      {(Object.keys(tabLabels) as AgentsHubTab[]).map((tab) => (
        <TabsContent className="agents-hub-tab-content" key={tab} value={tab}>
          <section className="agents-hub-layout">
            <aside className="agents-hub-list-pane">
              {/*
               * CDXC:AgentsHub 2026-06-12-02:53:
               * Agents Hub should not render search icons in the search field,
               * empty list, or editor loading states. Repeated magnifiers make
               * the modal look broken when the catalog is still loading.
               */}
              <div className="agents-hub-search">
                <Input
                  aria-label={`Search ${tabLabels[tab]}`}
                  className="h-8"
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={`Search ${tabLabels[tab]}`}
                  value={query}
                />
              </div>
              <ScrollArea className="agents-hub-scroll">
                <GroupList
                  activeFileId={selectedFileIds[tab]}
                  activeTab={tab}
                  expandedIds={expandedIds}
                  onSelectFile={(fileId) => {
                    setActiveTab(tab);
                    setSelectedFileIds((current) => ({ ...current, [tab]: fileId }));
                  }}
                  onToggleExpanded={(groupId) => {
                    setExpandedIds((current) => {
                      const next = new Set(current);
                      if (next.has(groupId)) {
                        next.delete(groupId);
                      } else {
                        next.add(groupId);
                      }
                      return next;
                    });
                  }}
                  groupsByTab={groupsByTab}
                  isCatalogLoading={!catalog}
                  query={query}
                  vscode={vscode}
                />
              </ScrollArea>
            </aside>
            {activeFile && activeFileContent !== undefined ? (
              <EditorPane
                file={{ ...activeFile, content: activeFileContent }}
                onRefreshCatalog={() => {
                  /**
                   * CDXC:AgentsHub 2026-06-04-20:08:
                   * External file edits need a user-triggered refresh inside the open editor because Agents Hub catalogs file contents on demand instead of watching every local profile folder.
                   * Clear the saved-content overlay before requesting a new native scan so stale in-modal buffers cannot mask the latest disk contents.
                   */
                  setSavedContentOverlay({ contentsByPath: emptySavedContentsByPath });
                  vscode.postMessage({ type: "requestAgentsHubCatalog" });
                }}
                onSaveContent={(filePath, content) => {
                  /**
                   * CDXC:AgentsHub 2026-05-16-07:19:
                   * Saving a Hub file must immediately update the open modal's file catalog because users can select another file and return before native sends a fresh filesystem scan.
                   * Keep the persisted editor text as the selected file content only for the catalog generation it was saved from, so reselecting a saved file cannot rehydrate the pre-save buffer and a later native scan stays authoritative.
                   */
                  setSavedContentOverlay((current) => ({
                    catalogGeneratedAt: catalog?.generatedAt,
                    contentsByPath: {
                      ...(current.catalogGeneratedAt === catalog?.generatedAt
                        ? current.contentsByPath
                        : emptySavedContentsByPath),
                      [filePath]: content,
                    },
                  }));
                }}
                vscode={vscode}
              />
            ) : activeFile ? (
              <FileContentStatePane
                errorMessage={activeFileLoadError}
                file={activeFile}
                isLoading={isActiveFileContentLoading}
              />
            ) : (
              <div className="agents-hub-editor-frame">
                <div className="agents-hub-empty">
                  <span>{catalog ? "No files found." : "Loading agent files..."}</span>
                </div>
              </div>
            )}
          </section>
        </TabsContent>
      ))}
    </Tabs>
  );
}

function GroupList({
  activeFileId,
  activeTab,
  expandedIds,
  groupsByTab,
  isCatalogLoading,
  onSelectFile,
  onToggleExpanded,
  query,
  vscode,
}: {
  activeFileId: string;
  activeTab: AgentsHubTab;
  expandedIds: Set<string>;
  groupsByTab: Record<AgentsHubTab, AgentsHubGroup[]>;
  isCatalogLoading: boolean;
  onSelectFile: (fileId: string) => void;
  onToggleExpanded: (groupId: string) => void;
  query: string;
  vscode: WebviewApi;
}) {
  const groups = useFilteredGroups(groupsByTab, activeTab, query);
  const expandable = activeTab !== "mds";

  if (groups.length === 0) {
    return (
      <div className="agents-hub-empty">
        <span>{isCatalogLoading ? "Loading agent files..." : "No matching files."}</span>
      </div>
    );
  }

  return (
    <div className="agents-hub-group-list">
      {groups.map((group) => {
        const isExpanded = expandedIds.has(group.id);
        const isActiveGroup = group.files.some((file) => file.id === activeFileId);
        const isCollapsedSkill = activeTab === "skills" && !isExpanded;
        const primaryFile = group.files[0]!;

        return (
          <section className={cn("agents-hub-group", isActiveGroup && "is-active")} key={group.id}>
            <button
              className="agents-hub-group-main"
              onClick={() => {
                if (expandable) {
                  onToggleExpanded(group.id);
                }
                onSelectFile(primaryFile.id);
              }}
              type="button"
            >
              <span className="agents-hub-group-title-row">
                {expandable ? (
                  isExpanded ? (
                    <IconChevronDown data-icon="inline-start" />
                  ) : (
                    <IconChevronRight data-icon="inline-start" />
                  )
                ) : (
                  <IconFile data-icon="inline-start" />
                )}
                <span className="agents-hub-group-title">{group.name}</span>
                {!isCollapsedSkill ? (
                  <span className="agents-hub-count">
                    {group.files.length} {group.files.length === 1 ? "file" : "files"}
                  </span>
                ) : null}
              </span>
              {!isCollapsedSkill ? (
                <>
                  <span className="agents-hub-path">{group.path}</span>
                  <span className="agents-hub-description">{group.description}</span>
                </>
              ) : null}
            </button>
            {!isCollapsedSkill ? <ProfileRow profiles={group.profiles} vscode={vscode} /> : null}
            {expandable && isExpanded ? (
              <div className="agents-hub-file-list">
                {group.files.map((file) => (
                  <button
                    className={cn("agents-hub-file-row", file.id === activeFileId && "is-active")}
                    key={file.id}
                    onClick={() => onSelectFile(file.id)}
                    type="button"
                  >
                    <IconFile data-icon="inline-start" />
                    <span>{file.name}</span>
                  </button>
                ))}
              </div>
            ) : null}
          </section>
        );
      })}
    </div>
  );
}

function ProfileRow({ profiles, vscode }: { profiles: AgentsHubProfile[]; vscode: WebviewApi }) {
  /**
   * CDXC:AgentsHub 2026-05-15-15:41:
   * Profile icon tooltips must keep the same profile label, instruction file path, optional resolved target path, and folder-opening action as the original tooltip, but render them as organized sections instead of a loose preformatted text block so dense path content remains scannable.
   *
   * CDXC:AgentsHub 2026-06-04-13:39:
   * Filesystem actions in Agents Hub should use OS-agnostic "Open Folder" language so the shared modal does not expose Finder-specific copy outside macOS implementation details.
   */
  return (
    <div className="agents-hub-profile-row" aria-label="Profiles using this item">
      {profiles.map((profile) => {
        const profileBadge = getAgentProfileBadge(profile.profilePath);

        return (
          <Tooltip key={`${profile.agentIcon}-${profile.profilePath}`}>
            <TooltipTrigger
              render={
                <button
                  aria-label={`Open ${profile.label} profile folder`}
                  className="agents-hub-agent-icon"
                  onClick={(event) => {
                    event.stopPropagation();
                    vscode.postMessage({
                      path: profile.profilePath,
                      type: "openAgentsHubPathInFinder",
                    });
                  }}
                  type="button"
                >
                  <span
                    aria-hidden="true"
                    className="agents-hub-agent-logo"
                    data-agent-icon={profile.agentIcon}
                    style={getBrandAgentLogoStyle(profile.agentIcon)}
                  />
                  {profileBadge ? (
                    <span className="agents-hub-agent-badge">{profileBadge}</span>
                  ) : null}
                </button>
              }
            />
            <TooltipContent align="start">
              <div className="agents-hub-profile-tooltip">
                <div className="agents-hub-profile-tooltip-main">
                  <div className="agents-hub-profile-tooltip-title">{profile.label}</div>
                  <div className="agents-hub-profile-tooltip-path">{profile.filePath}</div>
                </div>
                {profile.targetPath ? (
                  <div className="agents-hub-profile-tooltip-target">
                    <div aria-hidden="true" className="agents-hub-profile-tooltip-arrow">
                      -&gt;
                    </div>
                    <div className="agents-hub-profile-tooltip-path">{profile.targetPath}</div>
                  </div>
                ) : null}
                <div className="agents-hub-profile-tooltip-action">Click to open folder</div>
              </div>
            </TooltipContent>
          </Tooltip>
        );
      })}
    </div>
  );
}

function getAgentProfileBadge(profilePath: string): string | undefined {
  /**
   * CDXC:AgentsHub 2026-05-13-08:16
   * Main agent profiles should show only the agent logo. Non-main profile chips
   * derive their corner badge from the first alphanumeric character of the
   * profile folder name, so `personal` maps to P and `work` maps to W without
   * hard-coded per-agent badge labels.
   */
  if (!profilePath.includes("-profiles/")) {
    return undefined;
  }

  const profileFolder = profilePath.split("/").filter(Boolean).at(-1)?.toLowerCase();
  return profileFolder?.match(/[a-z0-9]/i)?.[0]?.toUpperCase();
}

function FileContentStatePane({
  errorMessage,
  file,
  isLoading,
}: {
  errorMessage?: string;
  file: AgentsHubFile;
  isLoading: boolean;
}) {
  return (
    <div className="agents-hub-editor-frame">
      <div className="agents-hub-editor-toolbar">
        <div className="agents-hub-editor-file">
          <span className="agents-hub-editor-name">{file.name}</span>
          <span className="agents-hub-path">{file.path}</span>
        </div>
      </div>
      <Separator />
      <div className="agents-hub-empty">
        <span>{errorMessage ?? (isLoading ? "Loading file..." : "Select a file.")}</span>
      </div>
    </div>
  );
}

function EditorPane({
  file,
  onRefreshCatalog,
  onSaveContent,
  vscode,
}: {
  file: LoadedAgentsHubFile;
  onRefreshCatalog: () => void;
  onSaveContent: (filePath: string, content: string) => void;
  vscode: WebviewApi;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<ReturnType<NonNullable<typeof window.monaco>["editor"]["create"]> | null>(
    null,
  );
  const latestFileRef = useRef(file);
  const [fallbackValue, setFallbackValue] = useState(file.content);
  const [savedValue, setSavedValue] = useState(file.content);
  const [isSaving, setIsSaving] = useState(false);
  const [monacoAvailable, setMonacoAvailable] = useState(true);
  const isDirty = fallbackValue !== savedValue;

  useEffect(() => {
    latestFileRef.current = file;
    setFallbackValue(file.content);
    setSavedValue(file.content);
    setIsSaving(false);
  }, [file]);

  useEffect(() => {
    let disposed = false;
    let contentDisposable: { dispose: () => void } | null = null;

    loadMonaco()
      .then(() => {
        if (disposed || !containerRef.current || !window.monaco || editorRef.current) {
          return;
        }

        const initialFile = latestFileRef.current;
        const editor = window.monaco.editor.create(containerRef.current, {
          automaticLayout: true,
          fontFamily: "var(--font-mono)",
          fontSize: 13,
          language: initialFile.language,
          minimap: { enabled: false },
          padding: { bottom: 16, top: 16 },
          scrollBeyondLastLine: false,
          /*
           * CDXC:AgentsHub 2026-06-04-19:29:
           * Agents Hub's inline Monaco editor must use the same thin rail as the Hub file-list sidebar so editor scrollbars do not look heavier than adjacent modal chrome.
           *
           * CDXC:AgentsHub 2026-06-04-19:48:
           * The inline editor scrollbar should be 7px wide so the code editor matches the requested lighter macOS treatment.
           */
          scrollbar: {
            horizontalScrollbarSize: 7,
            verticalScrollbarSize: 7,
          },
          theme: "vs-dark",
          value: initialFile.content,
        });
        contentDisposable = editor.onDidChangeModelContent(() => {
          setFallbackValue(editor.getValue());
        });
        editorRef.current = editor;
      })
      .catch(() => {
        if (!disposed) {
          setMonacoAvailable(false);
        }
      });

    return () => {
      disposed = true;
      contentDisposable?.dispose();
      editorRef.current?.dispose();
      editorRef.current = null;
    };
  }, []);

  useEffect(() => {
    const editor = editorRef.current;
    const monaco = window.monaco;
    if (!editor || !monaco) {
      return;
    }
    if (editor.getValue() !== file.content) {
      editor.setValue(file.content);
    }
    const model = editor.getModel();
    if (model) {
      // Keep Monaco's language mode aligned with the selected file extension.
      monaco.editor.setModelLanguage(model, file.language);
    }
    editor.layout();
  }, [file.content, file.id, file.language]);

  const handleSave = () => {
    const value = editorRef.current?.getValue() ?? fallbackValue;
    if (value === savedValue || isSaving) {
      return;
    }
    /**
     * CDXC:AgentsHub 2026-05-14-08:27:
     * Users edit agent instruction/config files directly in the Hub modal, so the top-right Save action should stay disabled until the current editor text differs from the last saved file contents.
     * Saving posts the active file path and editor value through the native sidebar bridge because the modal host cannot write local files itself.
     */
    setIsSaving(true);
    vscode.postMessage({
      content: value,
      filePath: file.path,
      type: "saveAgentsHubFile",
    });
    onSaveContent(file.path, value);
    setSavedValue(value);
    setFallbackValue(value);
    setIsSaving(false);
  };

  return (
    <div className="agents-hub-editor-frame">
      <div className="agents-hub-editor-toolbar">
        <div className="agents-hub-editor-file">
          <span className="agents-hub-editor-name">{file.name}</span>
          <span className="agents-hub-path">{file.path}</span>
        </div>
        <div className="agents-hub-editor-actions">
          {/*
           * CDXC:AgentsHub 2026-06-04-13:39:
           * The selected file header keeps Open Folder beside the built-in
           * Source action so users can choose filesystem or in-app navigation.
           *
           * CDXC:AgentsHub 2026-06-04-20:08:
           * Editor toolbar actions should be compact icon-only controls with descriptive hover tooltips, and Refresh should sit immediately before Save so disk changes can be reloaded without closing Agents Hub.
           */}
          <EditorToolbarButton
            label="Open containing folder"
            onClick={() =>
              vscode.postMessage({
                path: file.path,
                type: "openAgentsHubPathInFinder",
              })
            }
          >
            <IconFolderOpen aria-hidden="true" />
          </EditorToolbarButton>
          <EditorToolbarButton
            label="Open in built-in editor"
            onClick={() =>
              vscode.postMessage({
                filePath: file.path,
                type: "openAgentsHubFileInBuiltInEditor",
              })
            }
          >
            <IconEdit aria-hidden="true" />
          </EditorToolbarButton>
          <EditorToolbarButton
            label="Refresh contents from disk"
            onClick={onRefreshCatalog}
          >
            <IconRefresh aria-hidden="true" />
          </EditorToolbarButton>
          <EditorToolbarButton
            disabled={!isDirty || isSaving}
            label={isDirty ? "Save changes" : "No changes to save"}
            onClick={handleSave}
          >
            <IconDeviceFloppy aria-hidden="true" />
          </EditorToolbarButton>
        </div>
      </div>
      <Separator />
      <div className="agents-hub-editor-body">
        {monacoAvailable ? (
          <div className="agents-hub-monaco" ref={containerRef} />
        ) : (
          <Textarea
            aria-label={`${file.name} contents`}
            className="agents-hub-editor-fallback"
            onChange={(event) => setFallbackValue(event.target.value)}
            spellCheck={false}
            value={fallbackValue}
          />
        )}
      </div>
    </div>
  );
}

function EditorToolbarButton({
  children,
  disabled,
  label,
  onClick,
}: {
  children: ReactNode;
  disabled?: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            aria-disabled={disabled}
            aria-label={label}
            className={cn("agents-hub-editor-action-button", disabled && "is-disabled")}
            onClick={() => {
              if (!disabled) {
                onClick();
              }
            }}
            size="icon"
            type="button"
            variant="outline"
          >
            {children}
          </Button>
        }
      />
      <TooltipContent className="agents-hub-editor-action-tooltip" sideOffset={6}>
        {label}
      </TooltipContent>
    </Tooltip>
  );
}

function useFilteredGroups(
  groupsByTab: Record<AgentsHubTab, AgentsHubGroup[]>,
  tab: AgentsHubTab,
  query: string,
): AgentsHubGroup[] {
  return useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    if (!normalizedQuery) {
      return groupsByTab[tab];
    }
    return groupsByTab[tab].filter((group) => {
      const groupText = `${group.name} ${group.path} ${group.description}`.toLowerCase();
      const fileText = group.files
        .map((file) => `${file.name} ${file.path}`)
        .join(" ")
        .toLowerCase();
      return groupText.includes(normalizedQuery) || fileText.includes(normalizedQuery);
    });
  }, [groupsByTab, query, tab]);
}

function findFile(
  groupsByTab: Record<AgentsHubTab, AgentsHubGroup[]>,
  tab: AgentsHubTab,
  fileId: string,
): AgentsHubFile | undefined {
  for (const group of groupsByTab[tab]) {
    const file = group.files.find((candidate) => candidate.id === fileId);
    if (file) {
      return file;
    }
  }
  return groupsByTab[tab][0]?.files[0];
}

function firstFileId(groupsByTab: Record<AgentsHubTab, AgentsHubGroup[]>, tab: AgentsHubTab): string {
  return groupsByTab[tab][0]?.files[0]?.id ?? "";
}

function getMonacoRequire(): MonacoAmdRequire | undefined {
  return (window as unknown as { require?: MonacoAmdRequire }).require;
}

function loadMonaco(): Promise<void> {
  if (window.monaco) {
    return Promise.resolve();
  }

  return new Promise((resolve, reject) => {
    const configureLoader = (amdRequire: MonacoAmdRequire) => {
      window.MonacoEnvironment = {
        getWorkerUrl: () => "./monaco/vs/base/worker/workerMain.js",
      };
      amdRequire.config?.({ paths: { vs: "./monaco/vs" } });
      amdRequire(["vs/editor/editor.main"], resolve);
    };

    const existingRequire = getMonacoRequire();
    if (existingRequire) {
      configureLoader(existingRequire);
      return;
    }

    const script = document.createElement("script");
    script.src = "./monaco/vs/loader.js";
    script.onload = () => {
      const loadedRequire = getMonacoRequire();
      if (loadedRequire) {
        configureLoader(loadedRequire);
      } else {
        reject(new Error("Monaco loader did not expose require"));
      }
    };
    script.onerror = () => reject(new Error("Unable to load Monaco"));
    document.body.appendChild(script);
  });
}
