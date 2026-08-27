import { IconAlertTriangle, IconChevronRight } from '@tabler/icons-react';
import { CollisionPriority } from '@dnd-kit/abstract';
import { useSortable } from '@dnd-kit/react/sortable';
import { useCallback, type MouseEvent as ReactMouseEvent, type ReactNode } from 'react';
import { createGroupDropData } from '../sidebar-dnd';
import { groupSensors } from '../session-group-section';
import { AppTooltip } from '../app-tooltip';
import { useSidebarItemTooltipDelayMs } from '../tooltip-delay';
import { SidebarV2ProjectIcon } from './sidebar-v2-icons';
import { useSidebarCollapsiblePresence } from '../sidebar-collapse-animation';
import type { SidebarV2GroupModel } from './sidebar-v2-view-model';

/*
 * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
 * Grouped V2 renders V1's PROJECT HEADER, with V2 cards underneath it.
 *
 * The way it does that is by emitting V1's own DOM shape and classnames —
 * `section.group[data-project-group] > .group-head > .group-title-wrap >
 * .group-title-row > (.group-collapse-button, .group-title-handle >
 * .group-title-button > .group-title, .group-title-spacer,
 * .group-header-actions)` — rather than by mounting V1's `SessionGroupSection`.
 * Two reasons:
 *
 * 1. `SessionGroupSection` renders the SESSION LIST too (V1 cards, pinned-row
 *    reorder, project-session-list overflow, per-session context menus). Mounting
 *    it to get a header would mean mounting the V1 list and then suppressing it,
 *    inside the hottest file in the sidebar while other agents work in it.
 * 2. The look does not live in that component; it lives in
 *    `packages/core-ui/styles/groups.css`, in the reference-layout override block keyed on
 *    `.sidebar-reference-layout[data-reference-sidebar="true"]`. The V2 root
 *    already mounts inside that element, so emitting V1's classnames inherits the
 *    real app look verbatim — the row padding and full-bleed hover surface, the
 *    16px/550 title with its active-project white/650 state, the drop-line
 *    pseudo-elements, the dragging opacity, and the hover-revealed trailing
 *    action cluster — with no CSS moved or duplicated.
 *
 * This component owns the `<section>` as well as the header row because the
 * SECTION is the draggable/droppable element (dnd-kit's `sortable.ref`) while
 * `.group-head` is the drag HANDLE and the drop-bounds element V1's pointer
 * resolvers measure. Splitting those across two files would split one sortable
 * across two owners.
 */

/**
 * V1's `feedback: "none"` is kept, so dnd-kit never reports a rect-overlap
 * target for a project drag (the source shape never leaves its slot) and the
 * source row stays visible as a faint placeholder. SidebarApp resolves the drop
 * from the POINTER instead, via `resolveGroupDropTargetFromPoint` →
 * `getSidebarGroupBoundaryTargetAtY`, which needs exactly two things from this
 * markup and both are emitted below: `data-sidebar-group-id` on the section, and
 * a `.group-head` child to measure the header midpoint from.
 */
export type SidebarV2ProjectGroupSectionProps = {
  /** The group body (V2 session cards and the per-project shelves). */
  children?: ReactNode;
  /** Live drop-line position for this row, from SidebarApp's group indicator. */
  dropPosition?: 'after' | 'before';
  group: SidebarV2GroupModel;
  /** Trailing hover-revealed controls; V2 passes its per-project create "+". */
  headerActions?: ReactNode;
  /** Display index among the rendered rows, as dnd-kit's sortable expects. */
  index: number;
  isCollapsed: boolean;
  /** True while the user's own project is the one this row represents. */
  isActive?: boolean;
  /** True when this collapsed project owns the currently active session. */
  containsActiveSession?: boolean;
  /**
   * True while SidebarApp is painting the cursor ghost for THIS row, so the
   * source keeps V1's faint-placeholder treatment for the whole drag even after
   * dnd-kit's own `isDragging` has settled.
   */
  isDragPreviewSource?: boolean;
  /**
   * Turns the row into a plain header. Set for the Quick collection (it is not a
   * project and has no persisted order) and whenever the sidebar is not in a
   * manual sort mode, matching V1's `draggingDisabled`.
   */
  isDragDisabled?: boolean;
  onContextMenu?: (event: ReactMouseEvent<HTMLElement>) => void;
  onResolveMissingProjectFolder?: () => void;
  onSetCollapsed: (collapsed: boolean) => void;
  projectPath?: string;
  projectPathState?: 'available' | 'missing' | 'notDirectory' | 'unavailable';
  showProjectIcons: boolean;
};

export function SidebarV2ProjectGroupSection({
  children,
  dropPosition,
  group,
  headerActions,
  index,
  isActive = false,
  containsActiveSession = false,
  isCollapsed,
  isDragPreviewSource = false,
  isDragDisabled = false,
  onContextMenu,
  onResolveMissingProjectFolder,
  onSetCollapsed,
  projectPath,
  projectPathState,
  showProjectIcons,
}: SidebarV2ProjectGroupSectionProps) {
  const sidebarItemTooltipDelayMs = useSidebarItemTooltipDelayMs();
  const sortable = useSortable({
    /*
     * `accept` mirrors V1's non-pinned project section: a group row is a drop
     * target for another group (reorder) and for a session (move into project).
     * V2 does not implement the session half yet, but accepting it keeps the
     * collision set identical so a session drag behaves the same in both
     * sidebars instead of finding a differently-shaped target list.
     */
    accept: ['group', 'session'],
    collisionPriority: CollisionPriority.Low,
    data: createGroupDropData(group.groupId),
    disabled: isDragDisabled,
    feedback: 'none',
    id: group.groupId,
    index,
    sensors: groupSensors,
    type: 'group',
  });
  const collapseLabel = `${isCollapsed ? 'Expand' : 'Collapse'} ${group.title}`;
  const {
    isPresent: shouldRenderBody,
    isVisuallyCollapsed: isBodyVisuallyCollapsed,
    setCollapsibleElement: setBodyElement,
  } = useSidebarCollapsiblePresence(isCollapsed);
  const toggleCollapsed = useCallback(
    (event: ReactMouseEvent<HTMLElement>) => {
      event.preventDefault();
      event.stopPropagation();
      onSetCollapsed(!isCollapsed);
    },
    [isCollapsed, onSetCollapsed]
  );

  return (
    <section
      className='group sidebar-v2-group'
      data-active={String(isActive)}
      data-collapsed={String(isCollapsed)}
      data-contains-active-session={String(containsActiveSession)}
      data-dragging={String(Boolean(sortable.isDragging || isDragPreviewSource))}
      data-group-drop-position={dropPosition}
      /*
       * The Quick collection is deliberately marked as a project group too. Its
       * `data-chat-collection` variant in V1 swaps in a message-circle glyph and
       * hides the trailing action cluster, chrome the V2 inbox does not have —
       * so V2 gives every grouped row the same header look and expresses
       * "not a project" through `isDragDisabled` plus its exclusion from
       * SidebarApp's drag candidate list, which is what actually matters.
       */
      data-project-group='true'
      data-project-path-state={projectPathState}
      data-sidebar-group-id={group.groupId}
      data-sidebar-v2-group-id={group.groupId}
      data-sidebar-v2-group-merged={String(group.isMerged)}
      ref={sortable.ref}
    >
      <div
        className='group-head'
        data-collapsible='true'
        onContextMenu={onContextMenu}
        ref={isDragDisabled ? undefined : sortable.handleRef}
      >
        <div className='group-title-wrap'>
          <div className='group-title-row' data-project-leading-icon={String(showProjectIcons)}>
            {/*
             * V1's collapse control, kept as a real button sibling of the title
             * button (a button inside a button is invalid markup). The chevron is
             * V2's own glyph rather than V1's folder/caret swap: V1 project rows
             * render no leading glyph at all any more, so there is no folder icon
             * to swap FROM, and the project's own icon sits right beside this.
             */}
            <button
              aria-expanded={!isCollapsed}
              aria-label={collapseLabel}
              className='group-collapse-button section-titlebar-toggle sidebar-v2-group-collapse'
              data-collapsed={String(isCollapsed)}
              onClick={toggleCollapsed}
              type='button'
            >
              <IconChevronRight
                aria-hidden='true'
                className='group-collapse-icon sidebar-v2-group-chevron'
                data-expanded={String(!isCollapsed)}
                size={14}
                stroke={2}
              />
            </button>
            {showProjectIcons ? (
              <SidebarV2ProjectIcon
                discoveredIconDataUrl={group.discoveredIconDataUrl}
                fallback={group.isWorktree ? 'worktree' : isCollapsed ? 'folder' : 'folder-open'}
                icon={group.icon}
                iconDataUrl={group.iconDataUrl}
                title={group.title}
                tooltipDelay={sidebarItemTooltipDelayMs}
              />
            ) : null}
            {projectPathState !== undefined && projectPathState !== 'available' ? (
              <AppTooltip content={projectPath ? `Folder not found: ${projectPath}` : 'Project folder unavailable'}>
                <button
                  aria-label={`Resolve missing folder for ${group.title}`}
                  className='group-project-path-warning'
                  onClick={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    onResolveMissingProjectFolder?.();
                  }}
                  type='button'
                >
                  <IconAlertTriangle aria-hidden='true' size={14} stroke={1.9} />
                </button>
              </AppTooltip>
            ) : null}
            <div className='group-title-handle' data-draggable='false'>
              <button
                aria-expanded={!isCollapsed}
                aria-label={collapseLabel}
                className='group-title-button'
                data-empty-project='false'
                onClick={toggleCollapsed}
                type='button'
              >
                <span className='group-title section-titlebar-label'>{group.title}</span>
              </button>
            </div>
            <div className='group-title-spacer' />
            <span className='sidebar-v2-group-count'>{group.sessionCount}</span>
            {headerActions ? (
              /*
               * `.group-header-actions` is not decoration: V1's CSS reveals it on
               * header hover/focus and pins it to the row's right edge, and
               * `shouldPreventGroupDragActivation` reads this exact classname to
               * keep clicks on these controls from starting a project drag.
               */
              <div className='group-header-actions'>{headerActions}</div>
            ) : null}
          </div>
        </div>
      </div>
      {shouldRenderBody ? (
        <div
          aria-hidden={isBodyVisuallyCollapsed}
          className='sidebar-v2-group-body sidebar-animated-collapse-body'
          data-collapsed={String(isBodyVisuallyCollapsed)}
          inert={isBodyVisuallyCollapsed ? true : undefined}
          ref={setBodyElement}
        >
          {children}
        </div>
      ) : null}
    </section>
  );
}
