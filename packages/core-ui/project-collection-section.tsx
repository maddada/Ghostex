import {
  IconArrowsDiagonal2,
  IconArrowsDiagonalMinimize,
  IconCaretRightFilled,
  IconCheck,
  IconEyeOff,
  IconMoon,
  IconPalette,
  IconPencil,
  IconPinned,
  IconPinnedOff,
  IconPlayerPlay,
  IconPlus,
  IconChevronRight,
  IconRefresh,
  IconStack,
  IconTag,
  IconTrash,
  IconX,
} from '@tabler/icons-react';
import { PointerSensor } from '@dnd-kit/dom';
import { useSortable } from '@dnd-kit/react/sortable';
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import type { SidebarSessionItem } from '../shared/session-grid-contract';
import { getSidebarSessionTagLabel, type SidebarSessionTagListItem } from '../shared/session-tags';
import { SidebarContextMenuPortal } from './sidebar-context-menu-portal';
import { createProjectCollectionDragData } from './sidebar-dnd';
import { SidebarFixedTooltipButton } from './sidebar-fixed-tooltip-button';
import { getSidebarReorderActivationConstraints } from './sidebar-reorder-activation';
import { useSidebarCollapsiblePresence } from './sidebar-collapse-animation';
import { getAwakeTerminalAndBrowserCount, getGroupSessionSummary } from './group-session-summary';
import { useSidebarStore } from './sidebar-store';
import {
  canSleepSidebarSession,
  canWakeSidebarSession,
  runSidebarBulkContextMenuActionInBackground,
} from './sortable-session-card';
import type { WebviewApi } from './webview-api';
import {
  SIDEBAR_PROJECT_COLLECTION_COLOR_LABELS,
  SIDEBAR_PROJECT_COLLECTION_COLORS,
  type SidebarProjectCollection,
} from './project-collections';
import { SidebarCommandIconGlyph } from './sidebar-command-icon';
import { openAppModal } from './app-modal-host-bridge';
import { resolveSidebarSpaceIcon } from './space-filter-row';
import { createRemoteSidebarSpaceSectionKey, LOCAL_SIDEBAR_SPACE_SECTION_KEY } from './sidebar-app/space-filtering';
import { getSidebarSpaceIdsContainingCollection, type SidebarSpacesState } from './spaces';

type ProjectCollectionSectionProps = {
  autoEdit: boolean;
  children: ReactNode;
  collapsed: boolean;
  collection: SidebarProjectCollection;
  draggingDisabled: boolean;
  /*
   * CDXC:CollectionReorder 2026-07-21:
   * Pointer-resolved insertion boundary while another collection is being
   * dragged; renders the drop line above/below this panel.
   */
  dropIndicatorPosition?: 'before' | 'after';
  index: number;
  containsActiveSession?: boolean;
  /*
   * CDXC:CollectionDragPreview 2026-07-22:
   * With feedback "none" dnd-kit never flips sortable.isDragging (only its
   * feedback plugin sets a draggable's status to "dragging"), so the app's
   * drag-preview state marks the grabbed section as the faint placeholder.
   */
  isDragPreviewSource?: boolean;
  isHidden?: boolean;
  onAutoEditHandled: () => void;
  onBulkProjectToggle: () => void;
  onChange: (collection: SidebarProjectCollection) => void;
  onCollapsedChange: (collapsed: boolean) => void;
  onDelete: () => void;
  onHide: () => void;
  onSelectSessions: (sessionIds: string[]) => void;
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * Space membership for this group, per the Spaces decision that membership is
   * managed only from an item's own context menu. `spaces` is this section's
   * gxserver's Space set: `undefined` means that daemon delivered no Space state
   * at all, and the Spaces entry is then hidden rather than shown inert. A
   * delivered-but-empty set still shows the entry, with a disabled placeholder
   * row, so the menu never lies about the feature existing.
   */
  onToggleSpaceMembership?: (spaceId: string) => void;
  remoteMachineId?: string;
  sessionIds: readonly string[];
  sessionTagListItems: readonly SidebarSessionTagListItem[];
  sessionsById: Record<string, SidebarSessionItem | undefined>;
  spaces?: SidebarSpacesState;
  bulkProjectActionLabel: 'Collapse All' | 'Expand Previous';
  /*
   * CDXC:RemoteProjectCollections 2026-07-21:
   * The same collection can render once in the local Projects section and once
   * per remote machine section. dnd-kit sortable ids must stay unique across
   * the app, so remote instances pass a machine-scoped id.
   */
  sortableId?: string;
  vscode: WebviewApi;
};

type MenuView = 'actions' | 'colors' | 'spaces' | 'tags';

type ContextMenuPosition = {
  x: number;
  y: number;
};

/*
 * CDXC:CollectionReorder 2026-07-21:
 * Pointer-only on purpose. dnd-kit's KeyboardSensor starts a drag on
 * Space/Enter whenever the focusable header has focus, and with feedback
 * "none" that drag is completely invisible: nothing on screen indicates a
 * drag is active, and until it is committed or cancelled the stuck operation
 * silently swallows every other drag in the sidebar (manager.dragOperation
 * never returns to idle). That exact stranded keyboard drag is what made all
 * sidebar drag-and-drop "stop working". No keyboard sensor, no trap.
 */
const projectCollectionSensors = [
  PointerSensor.configure({
    activationConstraints: getSidebarReorderActivationConstraints,
  }),
];

export function ProjectCollectionSection({
  autoEdit,
  children,
  collapsed,
  collection,
  containsActiveSession = false,
  draggingDisabled,
  dropIndicatorPosition,
  index,
  isDragPreviewSource = false,
  isHidden = false,
  onAutoEditHandled,
  onBulkProjectToggle,
  onChange,
  onCollapsedChange,
  onDelete,
  onHide,
  onSelectSessions,
  onToggleSpaceMembership,
  remoteMachineId,
  sessionIds,
  sessionTagListItems,
  sessionsById,
  spaces,
  bulkProjectActionLabel,
  sortableId,
  vscode,
}: ProjectCollectionSectionProps) {
  const [isEditing, setIsEditing] = useState(autoEdit);
  const [draftTitle, setDraftTitle] = useState(collection.title);
  const [menuView, setMenuView] = useState<MenuView>();
  const [menuPosition, setMenuPosition] = useState<ContextMenuPosition>();
  const menuRef = useRef<HTMLDivElement>(null);
  const titleInputRef = useRef<HTMLInputElement>(null);
  /*
   * The visible colored header is both the exact collapse click surface and
   * the drag handle. The collection section is the bounded drop target, so its
   * nested project rows keep their existing independent drag ownership.
   */
  const sortable = useSortable({
    accept: 'project-collection',
    data: createProjectCollectionDragData(collection.collectionId),
    disabled: draggingDisabled || isEditing,
    feedback: 'none',
    id: sortableId ?? `project-collection:${collection.collectionId}`,
    index,
    sensors: projectCollectionSensors,
    type: 'project-collection',
  });
  const uniqueSessionIds = [...new Set(sessionIds)].filter((sessionId) => sessionsById[sessionId]);
  const collectionSessions = uniqueSessionIds.flatMap((sessionId) => {
    const session = sessionsById[sessionId];
    return session ? [session] : [];
  });
  const sessionSummary = getGroupSessionSummary(collectionSessions);
  const awakeCount = getAwakeTerminalAndBrowserCount(collectionSessions);
  const hasActionStatus = sessionSummary.workingCount > 0 || sessionSummary.attentionCount > 0;
  const shouldShowCollapsedStatus = collapsed && (hasActionStatus || awakeCount > 0);
  const sleepableSessionIds = uniqueSessionIds.filter((sessionId) => canSleepSidebarSession(sessionsById[sessionId]));
  const wakeableSessionIds = uniqueSessionIds.filter((sessionId) => canWakeSidebarSession(sessionsById[sessionId]));
  const pinnableSessionIds = uniqueSessionIds.filter((sessionId) => sessionsById[sessionId]?.isPinned !== true);
  const unpinnableSessionIds = uniqueSessionIds.filter((sessionId) => sessionsById[sessionId]?.isPinned === true);
  const reloadableSessionIds = uniqueSessionIds.filter((sessionId) => {
    const session = sessionsById[sessionId];
    return session?.kind !== 'browser' && session?.sessionKind !== 'browser';
  });
  const taggableSessionIds = uniqueSessionIds.filter((sessionId) => {
    const session = sessionsById[sessionId];
    return session?.kind !== 'browser' && session?.sessionKind !== 'browser';
  });
  const availableTags = sessionTagListItems.filter((item) => item.type === 'tag' && item.enabled && item.visible);
  const style = { '--project-collection-color': collection.color } as CSSProperties;
  const BulkProjectIcon = bulkProjectActionLabel === 'Collapse All' ? IconArrowsDiagonalMinimize : IconArrowsDiagonal2;
  const {
    isPresent: shouldRenderProjects,
    isVisuallyCollapsed: areProjectsVisuallyCollapsed,
    setCollapsibleElement: setProjectsElement,
  } = useSidebarCollapsiblePresence(collapsed);

  useEffect(() => {
    if (!autoEdit) {
      return;
    }
    setDraftTitle(collection.title);
    setIsEditing(true);
    onAutoEditHandled();
  }, [autoEdit, collection.title, onAutoEditHandled]);

  /*
   * Renaming a group starts from its current name, so the field opens with
   * that name selected: typing replaces it outright and editing it stays one
   * arrow key away. Focus is taken here instead of through `autoFocus` because
   * the selection has to be applied to the same element in the same pass.
   */
  useLayoutEffect(() => {
    if (!isEditing) {
      return;
    }

    const input = titleInputRef.current;
    input?.focus({ preventScroll: true });
    input?.select();
  }, [isEditing]);

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
    onCollapsedChange(!collapsed);
  };

  const dismissMenu = () => {
    setMenuView(undefined);
    setMenuPosition(undefined);
  };
  const runForSessions = (targetSessionIds: readonly string[], run: (sessionId: string) => void) => {
    dismissMenu();
    onSelectSessions([]);
    runSidebarBulkContextMenuActionInBackground(targetSessionIds, run);
  };
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * A group may belong to any number of Spaces, and every project inside it
   * inherits those memberships. Toggling closes the menu, which is the Tags
   * submenu's own behaviour and what the Spaces decision asks this submenu to
   * match.
   */
  const spaceMenuEnabled = Boolean(spaces && onToggleSpaceMembership);
  const memberSpaceIds = spaces ? getSidebarSpaceIdsContainingCollection(spaces, collection.collectionId) : [];
  const toggleSpaceMembership = (spaceId: string) => {
    dismissMenu();
    onToggleSpaceMembership?.(spaceId);
  };
  const createSpaceForCollection = () => {
    dismissMenu();
    openAppModal({
      memberCollectionId: collection.collectionId,
      mode: 'create',
      modal: 'sidebarSpaceEditor',
      ...(remoteMachineId ? { remoteMachineId } : {}),
      sectionKey: remoteMachineId
        ? createRemoteSidebarSpaceSectionKey(remoteMachineId)
        : LOCAL_SIDEBAR_SPACE_SECTION_KEY,
      type: 'open',
    });
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
      type: 'setSessionsSleeping',
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
      vscode.postMessage({ sessionId, type: 'closeSession' });
    });
  };

  const menuStyle = {
    left: `${menuPosition?.x ?? 12}px`,
    top: `${menuPosition?.y ?? 12}px`,
    width: '218px',
  };

  return (
    <section
      className='project-collection'
      data-collapsed={String(collapsed)}
      data-collection-drop-position={dropIndicatorPosition}
      data-contains-active-session={String(containsActiveSession)}
      data-dragging={String(Boolean(sortable.isDragging || isDragPreviewSource))}
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
        className='project-collection-header'
        onClick={(event) => {
          event.preventDefault();
          toggleCollapsed();
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setMenuPosition({ x: event.clientX, y: event.clientY });
          setMenuView('actions');
        }}
        ref={sortable.handleRef}
      >
        <button
          aria-expanded={!collapsed}
          aria-label={`${collapsed ? 'Expand' : 'Collapse'} ${collection.title}`}
          className='project-collection-collapse'
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            toggleCollapsed();
          }}
          type='button'
        >
          <IconCaretRightFilled aria-hidden='true' size={14} />
        </button>
        {isEditing ? (
          <input
            className='project-collection-title-input'
            onBlur={submitRename}
            onChange={(event) => setDraftTitle(event.currentTarget.value)}
            onClick={(event) => event.stopPropagation()}
            onPointerDown={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                submitRename();
              } else if (event.key === 'Escape') {
                event.preventDefault();
                setDraftTitle(collection.title);
                setIsEditing(false);
              }
            }}
            ref={titleInputRef}
            value={draftTitle}
          />
        ) : (
          <button
            className='project-collection-title'
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              toggleCollapsed();
            }}
            type='button'
          >
            {collection.title}
            {isHidden ? <IconEyeOff aria-label='Hidden' className='sidebar-hidden-item-icon' size={13} /> : null}
          </button>
        )}
        {shouldShowCollapsedStatus ? (
          <div
            aria-label={[
              sessionSummary.workingCount > 0 ? `${sessionSummary.workingCount} working` : '',
              sessionSummary.attentionCount > 0 ? `${sessionSummary.attentionCount} done` : '',
              !hasActionStatus && awakeCount > 0 ? `${awakeCount} awake terminals and browsers` : '',
            ]
              .filter(Boolean)
              .join(', ')}
            className='group-collapsed-status-counts project-collection-status-counts'
          >
            {sessionSummary.workingCount > 0 ? (
              <span className='group-collapsed-status-count' data-activity='working'>
                {sessionSummary.workingCount}
              </span>
            ) : null}
            {sessionSummary.attentionCount > 0 ? (
              <span className='group-collapsed-status-count' data-activity='attention'>
                {sessionSummary.attentionCount}
              </span>
            ) : null}
            {!hasActionStatus && awakeCount > 0 ? (
              <span className='group-collapsed-status-count' data-activity='awake'>
                {awakeCount}
              </span>
            ) : null}
          </div>
        ) : null}
        {!collapsed ? (
          <SidebarFixedTooltipButton
            aria-label={bulkProjectActionLabel}
            className='project-collection-bulk-project-action'
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onBulkProjectToggle();
            }}
            onPointerDown={(event) => event.stopPropagation()}
            tooltip={bulkProjectActionLabel}
            tooltipAlign='end'
            tooltipSide='left'
            type='button'
          >
            <BulkProjectIcon aria-hidden='true' size={14} stroke={1.9} />
          </SidebarFixedTooltipButton>
        ) : null}
      </div>
      {shouldRenderProjects ? (
        <div
          aria-hidden={areProjectsVisuallyCollapsed}
          className='project-collection-projects sidebar-animated-collapse-body'
          data-collapsed={String(areProjectsVisuallyCollapsed)}
          inert={areProjectsVisuallyCollapsed ? true : undefined}
          ref={setProjectsElement}
        >
          {children}
        </div>
      ) : null}
      {menuView ? (
        <SidebarContextMenuPortal menuRef={menuRef} menuStyle={menuStyle} onDismiss={dismissMenu} vscode={vscode}>
          {menuView === 'colors' ? (
            <>
              <button
                className='session-context-menu-item'
                onClick={() => setMenuView('actions')}
                role='menuitem'
                type='button'
              >
                <IconCaretRightFilled className='session-context-menu-icon project-collection-menu-back' size={14} />
                Back
              </button>
              <div className='session-context-menu-divider' role='separator' />
              {SIDEBAR_PROJECT_COLLECTION_COLORS.map((color) => (
                <button
                  aria-label={`Use ${SIDEBAR_PROJECT_COLLECTION_COLOR_LABELS[color]} for ${collection.title}`}
                  className='session-context-menu-item'
                  key={color}
                  onClick={() => {
                    onChange({ ...collection, color });
                    dismissMenu();
                  }}
                  role='menuitemradio'
                  type='button'
                >
                  <span className='project-collection-menu-swatch' style={{ background: color }} />
                  <span>{SIDEBAR_PROJECT_COLLECTION_COLOR_LABELS[color]}</span>
                  {color === collection.color ? <IconCheck size={14} /> : null}
                </button>
              ))}
            </>
          ) : menuView === 'spaces' ? (
            <>
              <button
                className='session-context-menu-item'
                onClick={() => setMenuView('actions')}
                role='menuitem'
                type='button'
              >
                <IconCaretRightFilled className='session-context-menu-icon project-collection-menu-back' size={14} />
                Back
              </button>
              <div className='session-context-menu-divider' role='separator' />
              <button
                className='session-context-menu-item'
                onClick={createSpaceForCollection}
                role='menuitem'
                type='button'
              >
                <IconPlus aria-hidden='true' className='session-context-menu-icon' size={14} />
                New Space
              </button>
              {spaces && spaces.order.length > 0 ? (
                <div className='session-context-menu-divider' role='separator' />
              ) : null}
              {spaces && spaces.order.length > 0
                ? spaces.order.flatMap((spaceId) => {
                    const space = spaces.spaces[spaceId];
                    if (!space) {
                      return [];
                    }
                    const isMember = memberSpaceIds.includes(spaceId);
                    return [
                      <button
                        aria-checked={isMember}
                        className='session-context-menu-item'
                        key={spaceId}
                        onClick={() => toggleSpaceMembership(spaceId)}
                        role='menuitemcheckbox'
                        type='button'
                      >
                        <SidebarCommandIconGlyph
                          className='session-context-menu-icon'
                          color={space.color}
                          icon={resolveSidebarSpaceIcon(space.icon)}
                          size={14}
                        />
                        <span className='sidebar-space-filter-menu-name'>{space.name}</span>
                        {isMember ? (
                          <IconCheck aria-hidden='true' className='session-context-menu-trailing-icon' size={14} />
                        ) : null}
                      </button>,
                    ];
                  })
                : null}
            </>
          ) : menuView === 'tags' ? (
            <>
              <button
                className='session-context-menu-item'
                onClick={() => setMenuView('actions')}
                role='menuitem'
                type='button'
              >
                <IconCaretRightFilled className='session-context-menu-icon project-collection-menu-back' size={14} />
                Back
              </button>
              <div className='session-context-menu-divider' role='separator' />
              <button
                className='session-context-menu-item'
                onClick={() =>
                  runForSessions(taggableSessionIds, (sessionId) =>
                    vscode.postMessage({ sessionId, sessionTag: null, type: 'setSessionTag' })
                  )
                }
                role='menuitem'
                type='button'
              >
                Clear tag
              </button>
              {availableTags.map((item) =>
                item.type === 'tag' ? (
                  <button
                    className='session-context-menu-item'
                    key={item.id}
                    onClick={() =>
                      runForSessions(taggableSessionIds, (sessionId) =>
                        vscode.postMessage({
                          sessionId,
                          sessionTag: item.tag,
                          type: 'setSessionTag',
                        })
                      )
                    }
                    role='menuitem'
                    type='button'
                  >
                    {getSidebarSessionTagLabel(item.tag) ?? item.tag}
                  </button>
                ) : null
              )}
            </>
          ) : (
            <>
              <button
                className='session-context-menu-item'
                disabled={uniqueSessionIds.length === 0}
                onClick={() => {
                  onSelectSessions(uniqueSessionIds);
                  dismissMenu();
                }}
                role='menuitem'
                type='button'
              >
                <IconCheck className='session-context-menu-icon' size={14} />
                Select all sessions
              </button>
              {sleepableSessionIds.length > 0 ? (
                <button
                  className='session-context-menu-item'
                  onClick={() => setSleeping(sleepableSessionIds, true)}
                  role='menuitem'
                  type='button'
                >
                  <IconMoon className='session-context-menu-icon' size={14} />
                  Sleep sessions
                </button>
              ) : null}
              {wakeableSessionIds.length > 0 ? (
                <button
                  className='session-context-menu-item'
                  onClick={() => setSleeping(wakeableSessionIds, false)}
                  role='menuitem'
                  type='button'
                >
                  <IconPlayerPlay className='session-context-menu-icon' size={14} />
                  Wake sessions
                </button>
              ) : null}
              {taggableSessionIds.length > 0 && availableTags.length > 0 ? (
                <button
                  className='session-context-menu-item'
                  onClick={() => setMenuView('tags')}
                  role='menuitem'
                  type='button'
                >
                  <IconTag className='session-context-menu-icon' size={14} />
                  Tag sessions
                </button>
              ) : null}
              {pinnableSessionIds.length > 0 ? (
                <button
                  className='session-context-menu-item'
                  onClick={() =>
                    runForSessions(pinnableSessionIds, (sessionId) =>
                      vscode.postMessage({ pinned: true, sessionId, type: 'setSessionPinned' })
                    )
                  }
                  role='menuitem'
                  type='button'
                >
                  <IconPinned className='session-context-menu-icon' size={14} />
                  Pin sessions
                </button>
              ) : null}
              {unpinnableSessionIds.length > 0 ? (
                <button
                  className='session-context-menu-item'
                  onClick={() =>
                    runForSessions(unpinnableSessionIds, (sessionId) =>
                      vscode.postMessage({ pinned: false, sessionId, type: 'setSessionPinned' })
                    )
                  }
                  role='menuitem'
                  type='button'
                >
                  <IconPinnedOff className='session-context-menu-icon' size={14} />
                  Unpin sessions
                </button>
              ) : null}
              {reloadableSessionIds.length > 0 ? (
                <button
                  className='session-context-menu-item'
                  onClick={() =>
                    runForSessions(reloadableSessionIds, (sessionId) =>
                      vscode.postMessage({ sessionId, type: 'fullReloadSession' })
                    )
                  }
                  role='menuitem'
                  type='button'
                >
                  <IconRefresh className='session-context-menu-icon' size={14} />
                  Full reload sessions
                </button>
              ) : null}
              <div className='session-context-menu-divider' role='separator' />
              <button
                className='session-context-menu-item'
                onClick={() => {
                  dismissMenu();
                  setDraftTitle(collection.title);
                  setIsEditing(true);
                }}
                role='menuitem'
                type='button'
              >
                <IconPencil className='session-context-menu-icon' size={14} />
                Rename group
              </button>
              <button
                className='session-context-menu-item'
                onClick={() => setMenuView('colors')}
                role='menuitem'
                type='button'
              >
                <IconPalette className='session-context-menu-icon' size={14} />
                Group color
              </button>
              {spaceMenuEnabled ? (
                <button
                  className='session-context-menu-item'
                  onClick={() => setMenuView('spaces')}
                  role='menuitem'
                  type='button'
                >
                  <IconStack className='session-context-menu-icon' size={14} />
                  Spaces
                  <IconChevronRight aria-hidden='true' className='session-context-menu-trailing-icon' size={14} />
                </button>
              ) : null}
              <button
                className='session-context-menu-item'
                onClick={() => {
                  dismissMenu();
                  onHide();
                }}
                role='menuitem'
                type='button'
              >
                <IconEyeOff className='session-context-menu-icon' size={14} />
                {isHidden ? 'Unhide group' : 'Hide group'}
              </button>
              <button
                className='session-context-menu-item session-context-menu-item-danger'
                onClick={() => {
                  dismissMenu();
                  onDelete();
                }}
                role='menuitem'
                type='button'
              >
                <IconTrash className='session-context-menu-icon' size={14} />
                Delete group
              </button>
              <button
                className='session-context-menu-item session-context-menu-item-danger'
                disabled={uniqueSessionIds.length === 0}
                onClick={closeSessions}
                role='menuitem'
                type='button'
              >
                <IconX className='session-context-menu-icon' size={14} />
                Close all sessions
              </button>
            </>
          )}
        </SidebarContextMenuPortal>
      ) : null}
    </section>
  );
}
