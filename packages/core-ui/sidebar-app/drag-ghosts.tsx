import { IconCaretRightFilled, IconCloud } from "@tabler/icons-react";
import type { CSSProperties } from "react";

export type SidebarGroupDragPreview = {
  groupId: string;
  isCollapsed: boolean;
  left: number;
  pointerOffsetY: number;
  themeColor?: string;
  title: string;
  top: number;
  width: number;
};

/*
 * CDXC:CollectionDragPreview 2026-07-22:
 * Collection reordering uses feedback "none", and dnd-kit only flips a
 * draggable's status to "dragging" inside its feedback plugin, so with "none"
 * sortable.isDragging never becomes true and the drag starts with zero visual
 * feedback. Like project headers, the app owns the drag visuals: this preview
 * drives a cursor-following collapsed-header ghost plus the faint source
 * placeholder.
 */
export type SidebarProjectCollectionDragPreview = {
  collectionId: string;
  color: string;
  left: number;
  pointerOffsetY: number;
  title: string;
  top: number;
  width: number;
};

export type SidebarRemoteMachineDragPreview = {
  collapsed: boolean;
  left: number;
  machineId: string;
  pointerOffsetY: number;
  title: string;
  top: number;
  width: number;
};
export function ProjectGroupDragGhost({ preview }: { preview: SidebarGroupDragPreview; }) {
  const style = {
    left: `${preview.left}px`,
    top: `${preview.top}px`,
    width: `${preview.width}px`,
    ...(preview.themeColor ? { "--workspace-project-theme-color": preview.themeColor } : {}),
  } as CSSProperties;

  /*
   * CDXC:ProjectDragPreview 2026-07-02-13:05:
   * The ghost mirrors the real project header DOM (group > group-head >
   * group-title-wrap > group-title-row) so it picks up the exact header
   * padding, font, and theme color instead of a bespoke approximation. It
   * renders the title only — trailing header action buttons are omitted.
   *
   * CDXC:ProjectDragPreview 2026-07-02-21:10:
   * The reference-layout .group-head uses negative scroll-bleed margins to
   * extend the row past the panel clip. The ghost's fixed shell is already
   * anchored to the measured header rect (which reflects those margins), so
   * the nested head keeps the scoped padding but must not re-apply the
   * margins, or the title would shift left of the grabbed header.
   */
  return (
    <div
      aria-hidden="true"
      className="project-drag-ghost group"
      data-project-group="true"
      data-workspace-custom-theme={String(Boolean(preview.themeColor))}
      style={style}
    >
      <div className="group-head" data-collapsible="true" style={{ margin: 0 }}>
        <div className="group-title-wrap">
          <div className="group-title-row" data-project-leading-icon="false">
            <div className="group-title-handle" data-draggable="true">
              <button
                aria-disabled="false"
                aria-expanded={!preview.isCollapsed}
                aria-label={preview.title}
                className="group-title-button"
                data-empty-project="false"
                tabIndex={-1}
                type="button"
              >
                <span className="group-title section-titlebar-label">{preview.title}</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

export function ProjectCollectionDragGhost({ preview }: { preview: SidebarProjectCollectionDragPreview; }) {
  const style = {
    left: `${preview.left}px`,
    top: `${preview.top}px`,
    width: `${preview.width}px`,
    "--project-collection-color": preview.color,
  } as CSSProperties;

  /*
   * CDXC:CollectionDragPreview 2026-07-22:
   * The ghost mirrors a collapsed collection panel's DOM
   * (section.project-collection > .project-collection-header > caret + title)
   * so it inherits the exact reference-panel skin and typography. It renders
   * the caret and title only — trailing header actions are omitted, matching
   * the project drag ghost.
   */
  return (
    <section
      aria-hidden="true"
      className="project-collection project-collection-drag-ghost"
      data-collapsed="true"
      style={style}
    >
      <div className="project-collection-header">
        <span className="project-collection-collapse">
          <IconCaretRightFilled aria-hidden="true" size={14} />
        </span>
        <span className="project-collection-title">{preview.title}</span>
      </div>
    </section>
  );
}

export function RemoteMachineDragGhost({ preview }: { preview: SidebarRemoteMachineDragPreview; }) {
  const style = {
    left: `${preview.left}px`,
    top: `${preview.top}px`,
    width: `${preview.width}px`,
  } as CSSProperties;

  return (
    <div aria-hidden="true" className="remote-machine-drag-ghost" style={style}>
      <div
        className="reference-sidebar-section-row"
        data-actions-always-visible="false"
        data-has-remote-connection-control="false"
        data-reference-section="remote"
      >
        <button
          aria-expanded={!preview.collapsed}
          className="reference-sidebar-section-heading"
          tabIndex={-1}
          type="button"
        >
          <IconCloud
            aria-hidden="true"
            className="reference-sidebar-section-icon"
            size={15}
            stroke={1.8}
          />
          <span className="reference-sidebar-section-title">{preview.title}</span>
          <IconCaretRightFilled
            aria-hidden="true"
            className="reference-sidebar-section-chevron"
            size={13}
          />
        </button>
      </div>
    </div>
  );
}
export function ProjectListEndUngroupDropZone({
  active,
  scopeId,
}: {
  active: boolean;
  scopeId: string;
}) {
  return (
    <div
      aria-hidden="true"
      className="project-list-end-ungroup-drop-zone"
      data-active={String(active)}
      data-sidebar-project-ungroup-drop-zone={scopeId}
    />
  );
}
