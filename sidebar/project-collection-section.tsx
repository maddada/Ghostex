import {
  IconArrowsDiagonal2,
  IconArrowsDiagonalMinimize,
  IconCaretRightFilled,
  IconCheck,
  IconMoon,
  IconPalette,
  IconPencil,
  IconPinned,
  IconPinnedOff,
  IconPlayerPlay,
  IconRefresh,
  IconTag,
  IconTrash,
  IconX,
} from "@tabler/icons-react";
import { KeyboardSensor, PointerActivationConstraints, PointerSensor } from "@dnd-kit/dom";
import { SortableKeyboardPlugin } from "@dnd-kit/dom/sortable";
import { useSortable } from "@dnd-kit/react/sortable";
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { SidebarSessionItem } from "../shared/session-grid-contract";
import {
  getSidebarSessionTagLabel,
  type SidebarSessionTagListItem,
} from "../shared/session-tags";
import { SidebarContextMenuPortal } from "./sidebar-context-menu-portal";
import { createProjectCollectionDragData } from "./sidebar-dnd";
import { SidebarFixedTooltipButton } from "./sidebar-fixed-tooltip-button";
import {
  getAwakeTerminalAndBrowserCount,
  getGroupSessionSummary,
} from "./group-session-summary";
import { useSidebarStore } from "./sidebar-store";
import {
  canSleepSidebarSession,
  canWakeSidebarSession,
  runSidebarBulkContextMenuActionInBackground,
} from "./sortable-session-card";
import type { WebviewApi } from "./webview-api";
import {
  SIDEBAR_PROJECT_COLLECTION_COLOR_LABELS,
  SIDEBAR_PROJECT_COLLECTION_COLORS,
  type SidebarProjectCollection,
} from "./project-collections";

type ProjectCollectionSectionProps = {
  autoEdit: boolean;
  children: ReactNode;
  collection: SidebarProjectCollection;
  draggingDisabled: boolean;
  index: number;
  onAutoEditHandled: () => void;
  onBulkProjectToggle: () => void;
  onChange: (collection: SidebarProjectCollection) => void;
  onDelete: () => void;
  onSelectSessions: (sessionIds: string[]) => void;
  sessionIds: readonly string[];
  sessionTagListItems: readonly SidebarSessionTagListItem[];
  sessionsById: Record<string, SidebarSessionItem | undefined>;
  bulkProjectActionLabel: "Collapse All" | "Expand Previous";
  vscode: WebviewApi;
};

type MenuView = "actions" | "colors" | "tags";

type ContextMenuPosition = {
  x: number;
  y: number;
};

const PROJECT_COLLECTION_DRAG_DISTANCE_PX = 8;
const TOUCH_PROJECT_COLLECTION_DRAG_HOLD_DELAY_MS = 320;
const TOUCH_PROJECT_COLLECTION_DRAG_HOLD_TOLERANCE_PX = 12;

const projectCollectionSensors = [
  PointerSensor.configure({
    activationConstraints(event) {
      if (event.pointerType === "touch") {
        return [
          new PointerActivationConstraints.Delay({
            tolerance: TOUCH_PROJECT_COLLECTION_DRAG_HOLD_TOLERANCE_PX,
            value: TOUCH_PROJECT_COLLECTION_DRAG_HOLD_DELAY_MS,
          }),
        ];
      }

      return [
        new PointerActivationConstraints.Distance({
          value: PROJECT_COLLECTION_DRAG_DISTANCE_PX,
        }),
      ];
    },
  }),
  KeyboardSensor,
];

export function ProjectCollectionSection({
  autoEdit,
  children,
  collection,
  draggingDisabled,
  index,
  onAutoEditHandled,
  onBulkProjectToggle,
  onChange,
  onDelete,
  onSelectSessions,
  sessionIds,
  sessionTagListItems,
  sessionsById,
  bulkProjectActionLabel,
  vscode,
}: ProjectCollectionSectionProps) {
  const [isEditing, setIsEditing] = useState(autoEdit);
  const [draftTitle, setDraftTitle] = useState(collection.title);
  const [menuView, setMenuView] = useState<MenuView>();
  const [menuPosition, setMenuPosition] = useState<ContextMenuPosition>();
  const menuRef = useRef<HTMLDivElement>(null);
  /*
   * The visible colored header is both the exact collapse click surface and
   * the drag handle. The collection section is the bounded drop target, so its
   * nested project rows keep their existing independent drag ownership.
   */
  const sortable = useSortable({
    accept: "project-collection",
    data: createProjectCollectionDragData(collection.collectionId),
    disabled: draggingDisabled || isEditing,
    feedback: "none",
    id: `project-collection:${collection.collectionId}`,
    index,
    plugins: [SortableKeyboardPlugin],
    sensors: projectCollectionSensors,
    type: "project-collection",
  });
  const uniqueSessionIds = [...new Set(sessionIds)].filter((sessionId) => sessionsById[sessionId]);
  const collectionSessions = uniqueSessionIds.flatMap((sessionId) => {
    const session = sessionsById[sessionId];
    return session ? [session] : [];
  });
  const sessionSummary = getGroupSessionSummary(collectionSessions);
  const awakeCount = getAwakeTerminalAndBrowserCount(collectionSessions);
  const hasActionStatus = sessionSummary.workingCount > 0 || sessionSummary.attentionCount > 0;
  const shouldShowCollapsedStatus =
    collection.collapsed && (hasActionStatus || awakeCount > 0);
  const sleepableSessionIds = uniqueSessionIds.filter((sessionId) =>
    canSleepSidebarSession(sessionsById[sessionId]),
  );
  const wakeableSessionIds = uniqueSessionIds.filter((sessionId) =>
    canWakeSidebarSession(sessionsById[sessionId]),
  );
  const pinnableSessionIds = uniqueSessionIds.filter(
    (sessionId) => sessionsById[sessionId]?.isPinned !== true,
  );
  const unpinnableSessionIds = uniqueSessionIds.filter(
    (sessionId) => sessionsById[sessionId]?.isPinned === true,
  );
  const reloadableSessionIds = uniqueSessionIds.filter((sessionId) => {
    const session = sessionsById[sessionId];
    return session?.kind !== "browser" && session?.sessionKind !== "browser";
  });
  const taggableSessionIds = uniqueSessionIds.filter((sessionId) => {
    const session = sessionsById[sessionId];
    return session?.kind !== "browser" && session?.sessionKind !== "browser";
  });
  const availableTags = sessionTagListItems.filter(
    (item) => item.type === "tag" && item.enabled && item.visible,
  );
  const style = { "--project-collection-color": collection.color } as CSSProperties;
  const BulkProjectIcon =
    bulkProjectActionLabel === "Collapse All"
      ? IconArrowsDiagonalMinimize
      : IconArrowsDiagonal2;

  useEffect(() => {
    if (!autoEdit) {
      return;
    }
    setDraftTitle(collection.title);
    setIsEditing(true);
    onAutoEditHandled();
  }, [autoEdit, collection.title, onAutoEditHandled]);

  const submitRename = () => {
    const title = draftTitle.trim().slice(0, 80);
    setIsEditing(false);
    if (title && title !== collection.title) {
      onChange({ ...collection, title });
      return;
    }
    setDraftTitle(collection.title);
  };

  const toggleCollapsed = () => {
    onChange({ ...collection, collapsed: !collection.collapsed });
  };

  const dismissMenu = () => {
    setMenuView(undefined);
    setMenuPosition(undefined);
  };
  const runForSessions = (
    targetSessionIds: readonly string[],
    run: (sessionId: string) => void,
  ) => {
    dismissMenu();
    onSelectSessions([]);
    runSidebarBulkContextMenuActionInBackground(targetSessionIds, run);
  };
  const setSleeping = (targetSessionIds: readonly string[], sleeping: boolean) => {
    if (targetSessionIds.length === 0) {
      return;
    }
    dismissMenu();
    onSelectSessions([]);
    if (!sleeping) {
      for (const sessionId of targetSessionIds) {
        useSidebarStore.getState().setSessionSleepingLocally(sessionId, false);
      }
    }
    vscode.postMessage({
      sessionIds: [...targetSessionIds],
      sleeping,
      type: "setSessionsSleeping",
    });
  };
  const closeSessions = () => {
    if (uniqueSessionIds.length === 0) {
      return;
    }
    dismissMenu();
    onSelectSessions([]);
    useSidebarStore.getState().hideSessionsLocally(uniqueSessionIds);
    runSidebarBulkContextMenuActionInBackground(uniqueSessionIds, (sessionId) => {
      vscode.postMessage({ sessionId, type: "closeSession" });
    });
  };

  const menuStyle = {
    left: `${menuPosition?.x ?? 12}px`,
    top: `${menuPosition?.y ?? 12}px`,
    width: "218px",
  };

  return (
    <section
      className="project-collection"
      data-collapsed={String(collection.collapsed)}
      data-dragging={String(Boolean(sortable.isDragging))}
      data-drop-target={String(Boolean(sortable.isDropTarget))}
      data-sidebar-project-collection-id={collection.collectionId}
      onContextMenu={(event) => {
        if (!event.defaultPrevented) {
          event.preventDefault();
        }
      }}
      ref={sortable.ref}
      style={style}
    >
      <div
        className="project-collection-header"
        onClick={(event) => {
          event.preventDefault();
          toggleCollapsed();
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setMenuPosition({ x: event.clientX, y: event.clientY });
          setMenuView("actions");
        }}
        ref={sortable.handleRef}
      >
        <button
          aria-expanded={!collection.collapsed}
          aria-label={`${collection.collapsed ? "Expand" : "Collapse"} ${collection.title}`}
          className="project-collection-collapse"
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            toggleCollapsed();
          }}
          type="button"
        >
          <IconCaretRightFilled aria-hidden="true" size={14} />
        </button>
        {isEditing ? (
          <input
            autoFocus
            className="project-collection-title-input"
            onBlur={submitRename}
            onChange={(event) => setDraftTitle(event.currentTarget.value)}
            onClick={(event) => event.stopPropagation()}
            onPointerDown={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                submitRename();
              } else if (event.key === "Escape") {
                event.preventDefault();
                setDraftTitle(collection.title);
                setIsEditing(false);
              }
            }}
            value={draftTitle}
          />
        ) : (
          <button
            className="project-collection-title"
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              toggleCollapsed();
            }}
            type="button"
          >
            {collection.title}
          </button>
        )}
        {shouldShowCollapsedStatus ? (
          <div
            aria-label={[
              sessionSummary.workingCount > 0
                ? `${sessionSummary.workingCount} working`
                : "",
              sessionSummary.attentionCount > 0
                ? `${sessionSummary.attentionCount} done`
                : "",
              !hasActionStatus && awakeCount > 0
                ? `${awakeCount} awake terminals and browsers`
                : "",
            ]
              .filter(Boolean)
              .join(", ")}
            className="group-collapsed-status-counts project-collection-status-counts"
          >
            {sessionSummary.workingCount > 0 ? (
              <span className="group-collapsed-status-count" data-activity="working">
                {sessionSummary.workingCount}
              </span>
            ) : null}
            {sessionSummary.attentionCount > 0 ? (
              <span className="group-collapsed-status-count" data-activity="attention">
                {sessionSummary.attentionCount}
              </span>
            ) : null}
            {!hasActionStatus && awakeCount > 0 ? (
              <span className="group-collapsed-status-count" data-activity="awake">
                {awakeCount}
              </span>
            ) : null}
          </div>
        ) : null}
        {!collection.collapsed ? (
          <SidebarFixedTooltipButton
            aria-label={bulkProjectActionLabel}
            className="project-collection-bulk-project-action"
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onBulkProjectToggle();
            }}
            onPointerDown={(event) => event.stopPropagation()}
            tooltip={bulkProjectActionLabel}
            tooltipAlign="end"
            tooltipSide="left"
            type="button"
          >
            <BulkProjectIcon aria-hidden="true" size={14} stroke={1.9} />
          </SidebarFixedTooltipButton>
        ) : null}
      </div>
      {!collection.collapsed ? <div className="project-collection-projects">{children}</div> : null}
      {menuView ? (
        <SidebarContextMenuPortal
          menuRef={menuRef}
          menuStyle={menuStyle}
          onDismiss={dismissMenu}
          vscode={vscode}
        >
          {menuView === "colors" ? (
            <>
              <button
                className="session-context-menu-item"
                onClick={() => setMenuView("actions")}
                role="menuitem"
                type="button"
              >
                <IconCaretRightFilled
                  className="session-context-menu-icon project-collection-menu-back"
                  size={14}
                />
                Back
              </button>
              <div className="session-context-menu-divider" role="separator" />
              {SIDEBAR_PROJECT_COLLECTION_COLORS.map((color) => (
                <button
                  aria-label={`Use ${SIDEBAR_PROJECT_COLLECTION_COLOR_LABELS[color]} for ${collection.title}`}
                  className="session-context-menu-item"
                  key={color}
                  onClick={() => {
                    onChange({ ...collection, color });
                    dismissMenu();
                  }}
                  role="menuitemradio"
                  type="button"
                >
                  <span className="project-collection-menu-swatch" style={{ background: color }} />
                  <span>{SIDEBAR_PROJECT_COLLECTION_COLOR_LABELS[color]}</span>
                  {color === collection.color ? <IconCheck size={14} /> : null}
                </button>
              ))}
            </>
          ) : menuView === "tags" ? (
            <>
              <button
                className="session-context-menu-item"
                onClick={() => setMenuView("actions")}
                role="menuitem"
                type="button"
              >
                <IconCaretRightFilled
                  className="session-context-menu-icon project-collection-menu-back"
                  size={14}
                />
                Back
              </button>
              <div className="session-context-menu-divider" role="separator" />
              <button
                className="session-context-menu-item"
                onClick={() =>
                  runForSessions(taggableSessionIds, (sessionId) =>
                    vscode.postMessage({ sessionId, sessionTag: null, type: "setSessionTag" }),
                  )
                }
                role="menuitem"
                type="button"
              >
                Clear tag
              </button>
              {availableTags.map((item) =>
                item.type === "tag" ? (
                  <button
                    className="session-context-menu-item"
                    key={item.id}
                    onClick={() =>
                      runForSessions(taggableSessionIds, (sessionId) =>
                        vscode.postMessage({
                          sessionId,
                          sessionTag: item.tag,
                          type: "setSessionTag",
                        }),
                      )
                    }
                    role="menuitem"
                    type="button"
                  >
                    {getSidebarSessionTagLabel(item.tag) ?? item.tag}
                  </button>
                ) : null,
              )}
            </>
          ) : (
            <>
              <button
                className="session-context-menu-item"
                disabled={uniqueSessionIds.length === 0}
                onClick={() => {
                  onSelectSessions(uniqueSessionIds);
                  dismissMenu();
                }}
                role="menuitem"
                type="button"
              >
                <IconCheck className="session-context-menu-icon" size={14} />
                Select all sessions
              </button>
              {sleepableSessionIds.length > 0 ? (
                <button
                  className="session-context-menu-item"
                  onClick={() => setSleeping(sleepableSessionIds, true)}
                  role="menuitem"
                  type="button"
                >
                  <IconMoon className="session-context-menu-icon" size={14} />
                  Sleep sessions
                </button>
              ) : null}
              {wakeableSessionIds.length > 0 ? (
                <button
                  className="session-context-menu-item"
                  onClick={() => setSleeping(wakeableSessionIds, false)}
                  role="menuitem"
                  type="button"
                >
                  <IconPlayerPlay className="session-context-menu-icon" size={14} />
                  Wake sessions
                </button>
              ) : null}
              {taggableSessionIds.length > 0 && availableTags.length > 0 ? (
                <button
                  className="session-context-menu-item"
                  onClick={() => setMenuView("tags")}
                  role="menuitem"
                  type="button"
                >
                  <IconTag className="session-context-menu-icon" size={14} />
                  Tag sessions
                </button>
              ) : null}
              {pinnableSessionIds.length > 0 ? (
                <button
                  className="session-context-menu-item"
                  onClick={() =>
                    runForSessions(pinnableSessionIds, (sessionId) =>
                      vscode.postMessage({ pinned: true, sessionId, type: "setSessionPinned" }),
                    )
                  }
                  role="menuitem"
                  type="button"
                >
                  <IconPinned className="session-context-menu-icon" size={14} />
                  Pin sessions
                </button>
              ) : null}
              {unpinnableSessionIds.length > 0 ? (
                <button
                  className="session-context-menu-item"
                  onClick={() =>
                    runForSessions(unpinnableSessionIds, (sessionId) =>
                      vscode.postMessage({ pinned: false, sessionId, type: "setSessionPinned" }),
                    )
                  }
                  role="menuitem"
                  type="button"
                >
                  <IconPinnedOff className="session-context-menu-icon" size={14} />
                  Unpin sessions
                </button>
              ) : null}
              {reloadableSessionIds.length > 0 ? (
                <button
                  className="session-context-menu-item"
                  onClick={() =>
                    runForSessions(reloadableSessionIds, (sessionId) =>
                      vscode.postMessage({ sessionId, type: "fullReloadSession" }),
                    )
                  }
                  role="menuitem"
                  type="button"
                >
                  <IconRefresh className="session-context-menu-icon" size={14} />
                  Full reload sessions
                </button>
              ) : null}
              <div className="session-context-menu-divider" role="separator" />
              <button
                className="session-context-menu-item"
                onClick={() => {
                  dismissMenu();
                  setDraftTitle(collection.title);
                  setIsEditing(true);
                }}
                role="menuitem"
                type="button"
              >
                <IconPencil className="session-context-menu-icon" size={14} />
                Rename group
              </button>
              <button
                className="session-context-menu-item"
                onClick={() => setMenuView("colors")}
                role="menuitem"
                type="button"
              >
                <IconPalette className="session-context-menu-icon" size={14} />
                Group color
              </button>
              <button
                className="session-context-menu-item session-context-menu-item-danger"
                onClick={() => {
                  dismissMenu();
                  onDelete();
                }}
                role="menuitem"
                type="button"
              >
                <IconTrash className="session-context-menu-icon" size={14} />
                Delete group
              </button>
              <button
                className="session-context-menu-item session-context-menu-item-danger"
                disabled={uniqueSessionIds.length === 0}
                onClick={closeSessions}
                role="menuitem"
                type="button"
              >
                <IconX className="session-context-menu-icon" size={14} />
                Close all sessions
              </button>
            </>
          )}
        </SidebarContextMenuPortal>
      ) : null}
    </section>
  );
}
