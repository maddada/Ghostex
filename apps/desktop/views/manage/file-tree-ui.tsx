import {
  type CSSProperties,
  type DragEvent as ReactDragEvent,
  type FormEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import { Excalidraw } from "@excalidraw/excalidraw";
import {
  IconArrowsDiagonal2,
  IconArrowsDiagonalMinimize,
  IconChevronRight,
  IconCopy,
  IconCopyPlus,
  IconEdit,
  IconFile,
  IconFileTypeHtml,
  IconFolder,
  IconFolderOpen,
  IconFolderPlus,
  IconLayoutSidebarLeftCollapse,
  IconLayoutSidebarRightCollapse,
  IconMarkdown,
  IconMenu2,
  IconMessagePlus,
  IconPlus,
  IconRefresh,
  IconSettings,
  IconTrash,
  IconX,
} from "@tabler/icons-react";
import { type ProjectDocsFileEntry as ManageFileEntry } from "@/packages/shared/project-docs";
import { SidebarContextMenuPortal } from "@/packages/core-ui/sidebar-context-menu-portal";
import { createPortal } from "react-dom";
import { ManageArtifactKind, ManageFileContextMenuState, ManageFileOperationState, ManageSidebarSide } from "./types";
import { ManageTooltipButton } from "./manage-tooltip-button";
import { fileIconForPath } from "./file-tree-utils";
import "@/packages/core-ui/styles/session-overlays.css";

export function ManageSidebarActions({
  creatingKind,
  isRefreshing,
  isCreatingFolder,
  hasExpandableDirectories,
  hasExpandedDirectories,
  onCreate,
  onCreateFolder,
  onHideSidebar,
  onOpenDocsFoldersSettings,
  onRefresh,
  onSwitchSide,
  onToggleAllDirectories,
  sidebarSide,
}: {
  creatingKind?: ManageArtifactKind;
  isRefreshing: boolean;
  isCreatingFolder: boolean;
  hasExpandableDirectories: boolean;
  hasExpandedDirectories: boolean;
  onCreate: (kind: ManageArtifactKind) => void;
  onCreateFolder: () => void;
  onHideSidebar: () => void;
  onOpenDocsFoldersSettings: () => void;
  onRefresh: () => void;
  onSwitchSide: () => void;
  onToggleAllDirectories: () => void;
  sidebarSide: ManageSidebarSide;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [createMenuOpen, setCreateMenuOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const HideSidebarIcon = sidebarSide === "right" ? IconLayoutSidebarRightCollapse : IconLayoutSidebarLeftCollapse;
  const BulkDirectoryIcon = hasExpandedDirectories ? IconArrowsDiagonalMinimize : IconArrowsDiagonal2;
  const bulkDirectoryActionLabel = hasExpandedDirectories ? "Collapse All" : "Expand All";
  const isCreating = Boolean(creatingKind) || isCreatingFolder;

  useEffect(() => {
    if (!menuOpen && !createMenuOpen) {
      return;
    }
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node && wrapperRef.current?.contains(target)) {
        return;
      }
      setMenuOpen(false);
      setCreateMenuOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setMenuOpen(false);
        setCreateMenuOpen(false);
      }
    };
    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [createMenuOpen, menuOpen]);

  const runMenuAction = (action: () => void) => {
    setMenuOpen(false);
    action();
  };

  const runCreateAction = (action: () => void) => {
    setCreateMenuOpen(false);
    action();
  };

  return (
    <div className="manage-sidebar-actions" ref={wrapperRef}>
      <ManageTooltipButton
        aria-label={bulkDirectoryActionLabel}
        className="manage-icon-button manage-sidebar-tree-toggle"
        disabled={!hasExpandableDirectories}
        onClick={() => {
          setCreateMenuOpen(false);
          setMenuOpen(false);
          onToggleAllDirectories();
        }}
        tooltip={bulkDirectoryActionLabel}
        type="button"
      >
        <BulkDirectoryIcon aria-hidden="true" size={14} stroke={1.9} />
      </ManageTooltipButton>
      <ManageTooltipButton
        aria-expanded={createMenuOpen}
        aria-haspopup="menu"
        aria-label="Create docs item"
        className="manage-icon-button"
        disabled={isCreating}
        onClick={() => {
          setCreateMenuOpen((current) => !current);
          setMenuOpen(false);
        }}
        tooltip="Create docs item"
        type="button"
      >
        <IconPlus aria-hidden="true" size={15} stroke={1.9} />
      </ManageTooltipButton>
      {/*
        CDXC:DocsSidebar 2026-06-30-21:26:
        The Docs sidebar header should place the overflow menu before the Hide sidebar control so the two rightmost buttons match the requested visual order while keeping their existing actions unchanged.
      */}
      <button
        aria-expanded={menuOpen}
        aria-haspopup="menu"
        aria-label="Docs sidebar menu"
        className="manage-icon-button"
        onClick={() => {
          setMenuOpen((current) => !current);
          setCreateMenuOpen(false);
        }}
        type="button"
      >
        <IconMenu2 aria-hidden="true" size={15} stroke={1.8} />
      </button>
      <button
        aria-label="Hide file sidebar"
        className="manage-icon-button"
        onClick={onHideSidebar}
        type="button"
      >
        <HideSidebarIcon aria-hidden="true" size={15} stroke={1.8} />
      </button>
      {createMenuOpen ? (
        <div className="manage-sidebar-menu manage-create-menu" role="menu">
          <button
            className="manage-sidebar-menu-item"
            disabled={isCreating}
            onClick={() => runCreateAction(onCreateFolder)}
            role="menuitem"
            type="button"
          >
            <IconFolderPlus aria-hidden="true" size={14} stroke={1.8} />
            {isCreatingFolder ? "Creating folder" : "New folder"}
          </button>
          <button
            className="manage-sidebar-menu-item"
            disabled={isCreating}
            onClick={() => runCreateAction(() => onCreate("markdown"))}
            role="menuitem"
            type="button"
          >
            <IconMarkdown aria-hidden="true" size={14} stroke={1.8} />
            {creatingKind === "markdown" ? "Creating Markdown" : "New Markdown"}
          </button>
          <button
            className="manage-sidebar-menu-item"
            disabled={isCreating}
            onClick={() => runCreateAction(() => onCreate("html"))}
            role="menuitem"
            type="button"
          >
            <IconFileTypeHtml aria-hidden="true" size={14} stroke={1.8} />
            {creatingKind === "html" ? "Creating HTML" : "New HTML"}
          </button>
          <button
            className="manage-sidebar-menu-item"
            disabled={isCreating}
            onClick={() => runCreateAction(() => onCreate("excalidraw"))}
            role="menuitem"
            type="button"
          >
            <IconEdit aria-hidden="true" size={14} stroke={1.8} />
            {creatingKind === "excalidraw" ? "Creating drawing" : "New drawing"}
          </button>
        </div>
      ) : null}
      {menuOpen ? (
        <div className="manage-sidebar-menu" role="menu">
          <button
            className="manage-sidebar-menu-item"
            disabled={isRefreshing}
            onClick={() => runMenuAction(onRefresh)}
            role="menuitem"
            type="button"
          >
            <IconRefresh aria-hidden="true" size={14} stroke={1.8} />
            Refresh
          </button>
          <button
            className="manage-sidebar-menu-item"
            onClick={() => runMenuAction(onSwitchSide)}
            role="menuitem"
            type="button"
          >
            {sidebarSide === "right" ? (
              <IconLayoutSidebarLeftCollapse aria-hidden="true" size={14} stroke={1.8} />
            ) : (
              <IconLayoutSidebarRightCollapse aria-hidden="true" size={14} stroke={1.8} />
            )}
            Switch sidebar side
          </button>
          {/*
            CDXC:DocsSidebarSettings 2026-06-30-11:42:
            The Docs overflow menu should deep-link to Settings -> Projects -> Global Settings so users can configure the project-relative folders that Docs scans for files without leaving the Docs context.
          */}
          <button
            className="manage-sidebar-menu-item"
            onClick={() => runMenuAction(onOpenDocsFoldersSettings)}
            role="menuitem"
            type="button"
          >
            <IconSettings aria-hidden="true" size={14} stroke={1.8} />
            Configure docs folders
          </button>
        </div>
      ) : null}
    </div>
  );
}

export function ManageFileRow({
  annotationCount,
  canOpenContextMenu,
  entry,
  hasActiveFileDescendant,
  hasChildren,
  isContextMenuOpen,
  isDragging,
  isDropTarget,
  isExpanded,
  isSelected,
  onEntryDragOver,
  onEntryDrop,
  onDragEnd,
  onDragStart,
  onOpenContextMenu,
  onSelect,
}: {
  annotationCount: number;
  canOpenContextMenu: boolean;
  entry: ManageFileEntry;
  hasActiveFileDescendant: boolean;
  hasChildren: boolean;
  isContextMenuOpen: boolean;
  isDragging: boolean;
  isDropTarget: boolean;
  isExpanded: boolean;
  isSelected: boolean;
  onEntryDragOver: (entry: ManageFileEntry, event: ReactDragEvent<HTMLButtonElement>) => void;
  onEntryDrop: (entry: ManageFileEntry, event: ReactDragEvent<HTMLButtonElement>) => void;
  onDragEnd: () => void;
  onDragStart: (entry: ManageFileEntry, event: ReactDragEvent<HTMLButtonElement>) => void;
  onOpenContextMenu: (entry: ManageFileEntry, point: { x: number; y: number }) => void;
  onSelect: () => void;
}) {
  const Icon = entry.kind === "directory" ? (isExpanded ? IconFolderOpen : IconFolder) : fileIconForPath(entry.path);
  return (
    <button
      aria-expanded={entry.kind === "directory" && hasChildren ? isExpanded : undefined}
      aria-haspopup={canOpenContextMenu ? "menu" : undefined}
      aria-selected={entry.kind === "file" ? isSelected : undefined}
      className="manage-file-row"
      data-active-descendant={String(hasActiveFileDescendant)}
      data-context-menu-open={String(isContextMenuOpen)}
      data-dragging={String(isDragging)}
      data-drop-target={String(isDropTarget)}
      data-kind={entry.kind}
      data-selected={String(isSelected)}
      draggable={entry.kind === "file" || entry.kind === "directory"}
      onClick={onSelect}
      onContextMenu={(event: ReactMouseEvent<HTMLButtonElement>) => {
        event.preventDefault();
        event.stopPropagation();
        if (!canOpenContextMenu) {
          return;
        }
        onOpenContextMenu(entry, { x: event.clientX, y: event.clientY });
      }}
      onKeyDown={(event: ReactKeyboardEvent<HTMLButtonElement>) => {
        if (!canOpenContextMenu) {
          return;
        }
        if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) {
          return;
        }
        event.preventDefault();
        const bounds = event.currentTarget.getBoundingClientRect();
        onOpenContextMenu(entry, {
          x: bounds.left + 28,
          y: bounds.top + Math.min(22, bounds.height),
        });
      }}
      onDragEnd={onDragEnd}
      onDragOver={(event) => onEntryDragOver(entry, event)}
      onDragStart={(event) => onDragStart(entry, event)}
      onDrop={(event) => onEntryDrop(entry, event)}
      role="treeitem"
      style={{ "--depth": entry.depth } as CSSProperties}
      type="button"
    >
      <span
        aria-hidden="true"
        className="manage-file-disclosure"
        data-visible={String(entry.kind === "directory" && hasChildren)}
      >
        <IconChevronRight size={14} stroke={1.9} />
      </span>
      <Icon aria-hidden="true" className="manage-file-icon" size={15} stroke={1.75} />
      <span className="manage-file-name">{entry.name}</span>
      <span className="manage-file-badges">
        {annotationCount > 0 ? <span className="manage-count-badge">{annotationCount}</span> : null}
      </span>
    </button>
  );
}

export function ManageFileContextMenu({
  canAddToSessionContext,
  canCreateHere,
  canDuplicate,
  canRenameOrDelete,
  confirmingDelete,
  creatingKind,
  isCreatingFolder,
  onAddToSessionContext,
  onCopyFullPath,
  onCopyPath,
  onCreateFileHere,
  onCreateFolderHere,
  onDuplicate,
  onDelete,
  onDismiss,
  onRename,
  onRevealInFinder,
  pendingAction,
  position,
}: {
  canAddToSessionContext: boolean;
  canCreateHere: boolean;
  canDuplicate: boolean;
  canRenameOrDelete: boolean;
  confirmingDelete: boolean;
  creatingKind?: ManageArtifactKind;
  isCreatingFolder: boolean;
  onAddToSessionContext: () => void;
  onCopyFullPath: () => void;
  onCopyPath: () => void;
  onCreateFileHere: (kind: ManageArtifactKind) => void;
  onCreateFolderHere: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onDismiss: () => void;
  onRename: () => void;
  onRevealInFinder: () => void;
  pendingAction?: ManageFileOperationState["action"];
  position: Pick<ManageFileContextMenuState, "x" | "y">;
}) {
  const [createFileMenuOpen, setCreateFileMenuOpen] = useState(false);
  const isBusy = Boolean(pendingAction);
  return (
    <SidebarContextMenuPortal
      menuClassName="session-context-menu manage-file-context-menu"
      menuStyle={{
        left: `${position.x}px`,
        position: "fixed",
        top: `${position.y}px`,
      }}
      onDismiss={onDismiss}
    >
      <button
        className="session-context-menu-item manage-file-context-menu-item"
        disabled={isBusy}
        onClick={onRevealInFinder}
        role="menuitem"
        type="button"
      >
        <IconFolderOpen aria-hidden="true" className="session-context-menu-icon" size={14} stroke={1.8} />
        {pendingAction === "revealInFinder" ? "Revealing" : "Reveal in Finder"}
      </button>
      <button
        className="session-context-menu-item manage-file-context-menu-item"
        onClick={onCopyPath}
        role="menuitem"
        type="button"
      >
        <IconCopy aria-hidden="true" className="session-context-menu-icon" size={14} stroke={1.8} />
        Copy Relative Path
      </button>
      <button
        className="session-context-menu-item manage-file-context-menu-item"
        disabled={isBusy}
        onClick={onCopyFullPath}
        role="menuitem"
        type="button"
      >
        <IconCopy aria-hidden="true" className="session-context-menu-icon" size={14} stroke={1.8} />
        {pendingAction === "copyFullPath" ? "Copying Full Path" : "Copy Full Path"}
      </button>
      {canAddToSessionContext ? (
        <button
          className="session-context-menu-item manage-file-context-menu-item"
          disabled={isBusy}
          onClick={onAddToSessionContext}
          role="menuitem"
          type="button"
        >
          <IconMessagePlus aria-hidden="true" className="session-context-menu-icon" size={14} stroke={1.8} />
          {pendingAction === "addToSessionContext" ? "Adding context" : "Add to Session Context"}
        </button>
      ) : null}
      {canCreateHere ? (
        <>
          <div className="session-context-menu-divider manage-file-context-menu-divider" role="separator" />
          <button
            aria-expanded={createFileMenuOpen}
            className="session-context-menu-item manage-file-context-menu-item"
            disabled={isBusy}
            onClick={() => setCreateFileMenuOpen((current) => !current)}
            role="menuitem"
            type="button"
          >
            <IconFile aria-hidden="true" className="session-context-menu-icon" size={14} stroke={1.8} />
            <span>New File Here</span>
            <span className="manage-file-context-menu-spacer" />
            <IconChevronRight
              aria-hidden="true"
              className="manage-file-context-menu-chevron"
              data-open={String(createFileMenuOpen)}
              size={14}
              stroke={1.8}
            />
          </button>
          {createFileMenuOpen ? (
            <div className="manage-file-context-menu-nested" role="group">
              <button
                className="session-context-menu-item manage-file-context-menu-item manage-file-context-menu-subitem"
                disabled={isBusy}
                onClick={() => onCreateFileHere("markdown")}
                role="menuitem"
                type="button"
              >
                <IconMarkdown aria-hidden="true" size={14} stroke={1.8} />
                {creatingKind === "markdown" ? "Creating Markdown" : "Markdown"}
              </button>
              <button
                className="session-context-menu-item manage-file-context-menu-item manage-file-context-menu-subitem"
                disabled={isBusy}
                onClick={() => onCreateFileHere("html")}
                role="menuitem"
                type="button"
              >
                <IconFileTypeHtml aria-hidden="true" size={14} stroke={1.8} />
                {creatingKind === "html" ? "Creating HTML" : "HTML"}
              </button>
              <button
                className="session-context-menu-item manage-file-context-menu-item manage-file-context-menu-subitem"
                disabled={isBusy}
                onClick={() => onCreateFileHere("excalidraw")}
                role="menuitem"
                type="button"
              >
                <IconEdit aria-hidden="true" size={14} stroke={1.8} />
                {creatingKind === "excalidraw" ? "Creating Excalidraw" : "Excalidraw"}
              </button>
            </div>
          ) : null}
          <button
            className="session-context-menu-item manage-file-context-menu-item"
            disabled={isBusy}
            onClick={onCreateFolderHere}
            role="menuitem"
            type="button"
          >
            <IconFolderPlus aria-hidden="true" size={14} stroke={1.8} />
            {isCreatingFolder ? "Creating Folder" : "New Folder Here"}
          </button>
        </>
      ) : null}
      {canDuplicate ? (
        <>
          <div className="session-context-menu-divider manage-file-context-menu-divider" role="separator" />
          <button
            className="session-context-menu-item manage-file-context-menu-item"
            disabled={isBusy}
            onClick={onDuplicate}
            role="menuitem"
            type="button"
          >
            <IconCopyPlus aria-hidden="true" size={14} stroke={1.8} />
            {pendingAction === "duplicate" ? "Duplicating" : "Duplicate"}
          </button>
        </>
      ) : null}
      {canRenameOrDelete ? (
        <>
          {!canDuplicate ? <div className="session-context-menu-divider manage-file-context-menu-divider" role="separator" /> : null}
          <button
            className="session-context-menu-item manage-file-context-menu-item"
            disabled={isBusy}
            onClick={onRename}
            role="menuitem"
            type="button"
          >
            <IconEdit aria-hidden="true" size={14} stroke={1.8} />
            Rename
          </button>
          <button
            className="session-context-menu-item session-context-menu-item-danger manage-file-context-menu-item manage-file-context-menu-item-danger"
            data-confirming={String(confirmingDelete)}
            disabled={isBusy}
            onClick={onDelete}
            role="menuitem"
            type="button"
          >
            <IconTrash aria-hidden="true" size={14} stroke={1.8} />
            {pendingAction === "delete" ? "Deleting" : confirmingDelete ? "Confirm delete" : "Delete"}
          </button>
        </>
      ) : null}
    </SidebarContextMenuPortal>
  );
}

export function ManageRenameDialog({
  error,
  isRenaming,
  onCancel,
  onChange,
  onSubmit,
  value,
}: {
  error?: string;
  isRenaming: boolean;
  onCancel: () => void;
  onChange: (value: string) => void;
  onSubmit: () => void;
  value: string;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const isSubmitDisabled = isRenaming || value.trim().length === 0;

  useEffect(() => {
    const input = inputRef.current;
    if (!input) {
      return;
    }
    input.focus();
    input.select();
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onCancel();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onCancel]);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (isSubmitDisabled) {
      return;
    }
    onSubmit();
  };

  return createPortal(
    <>
      <button
        aria-label="Cancel rename"
        className="manage-rename-backdrop"
        onClick={onCancel}
        type="button"
      />
      <form className="manage-rename-dialog" onSubmit={submit}>
        <div className="manage-rename-header">
          <span>Rename item</span>
          <button
            aria-label="Cancel rename"
            className="manage-icon-button manage-rename-close"
            onClick={onCancel}
            type="button"
          >
            <IconX aria-hidden="true" size={15} stroke={1.8} />
          </button>
        </div>
        <input
          aria-label="Item name"
          className="manage-rename-input"
          disabled={isRenaming}
          onChange={(event) => onChange(event.currentTarget.value)}
          ref={inputRef}
          value={value}
        />
        {error ? <div className="manage-rename-error">{error}</div> : null}
        <div className="manage-rename-actions">
          <button className="manage-rename-secondary" disabled={isRenaming} onClick={onCancel} type="button">
            Cancel
          </button>
          <button className="manage-rename-primary" disabled={isSubmitDisabled} type="submit">
            {isRenaming ? "Renaming" : "Rename"}
          </button>
        </div>
      </form>
    </>,
    document.body,
  );
}

export function ManageEmptyState({ icon, text }: { icon: ReactNode; text: string }) {
  return (
    <div className="manage-empty">
      {icon}
      <span>{text}</span>
    </div>
  );
}
