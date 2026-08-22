import { PointerSensor } from "@dnd-kit/dom";
import { useSortable } from "@dnd-kit/react/sortable";
import type { ReactNode } from "react";
import type { SidebarActiveSessionsSortMode } from "../../shared/session-grid-contract";
import type {
  RemoteMachineSettings,
  SidebarV2Layout,
  SidebarVersion,
} from "../../shared/ghostex-settings";
import type { SidebarSessionTagListItem } from "../../shared/session-tags";
import { useSidebarCollapsiblePresence } from "../sidebar-collapse-animation";
import { createRemoteMachineDragData } from "../sidebar-dnd";
import { getSidebarReorderActivationConstraints } from "../sidebar-reorder-activation";
import type { SidebarSessionTagFilter } from "../session-tag-ui";
import { createRemoteProjectListScopeId } from "./drag-drop-geometry";
import { ProjectListEndUngroupDropZone } from "./drag-ghosts";
import { SidebarReferenceSectionHeader } from "./reference-chrome";
import type {
  RemoteMachineHeaderConnectionControl,
  RemoteMachineRuntimeStatus,
  SidebarProjectCollectionRenderItem,
  SidebarSectionSessionSummary,
} from "./types";

/*
 * Remote machines reorder only from their visible header. Pointer-only
 * activation keeps Space/Enter owned by the existing collapse button and
 * prevents a keyboard drag from leaving the shared manager in an unseen drag.
 */
export const remoteMachineSensors = [
  PointerSensor.configure({
    activationConstraints: getSidebarReorderActivationConstraints,
  }),
];
export function remoteMachineBusyLabel(
  status: RemoteMachineRuntimeStatus[ "state" ],
): string | undefined {
  switch (status) {
    case "connecting":
      return "Connecting…";
    case "installing":
      return "Installing gxserver…";
    case "downloadingRemoteServerPackage":
      return "Downloading server package…";
    default:
      return undefined;
  }
}

export function remoteMachineFailureLabel(status: RemoteMachineRuntimeStatus[ "state" ]): string {
  switch (status) {
    case "installApprovalRequired":
      return "Install approval required.";
    case "installFailed":
      return "gxserver install failed.";
    case "invalid":
      return "Saved remote machine is incomplete.";
    case "keychainFailed":
      return "Could not save the auth token to Keychain.";
    case "presentationStreamFailed":
    case "presentationSubscribeFailed":
      return "Remote session stream failed.";
    case "sshFailed":
      return "SSH connection failed.";
    case "tokenUnavailable":
      return "Remote auth token unavailable.";
    case "tunnelFailed":
      return "Secure tunnel failed.";
    case "unsupported":
    case "unsupportedRemotePlatform":
      return "Remote platform not supported.";
    default:
      return "Remote connect failed.";
  }
}

export function RemoteMachineSidebarSection({
  activeSessionsSortMode,
  bulkActionLabel,
  collapsed,
  containsActiveSession,
  index,
  isDragPreviewSource,
  machine,
  onAddProject,
  onBulkProjectToggle,
  onEdit,
  onReconnect,
  onSetActiveSessionsSortMode,
  onSetSidebarV2Layout,
  onSetSidebarVersion,
  onToggleSessionTagFilter,
  onToggleCollapsed,
  projectCollectionItems,
  projectUngroupDropIndicatorScopeId,
  projectGroupIds,
  remoteMachineDropIndicatorPosition,
  renderProjectCollection,
  renderProjectGroup,
  selectedSessionTagFilters,
  sessionSummary,
  sessionTagListItems,
  sidebarV2Layout,
  sidebarVersion,
  status,
  statusMessage,
}: {
  activeSessionsSortMode: SidebarActiveSessionsSortMode;
  bulkActionLabel?: string;
  collapsed: boolean;
  containsActiveSession: boolean;
  index: number;
  isDragPreviewSource: boolean;
  machine: RemoteMachineSettings;
  onAddProject: () => void;
  onBulkProjectToggle?: () => void;
  onEdit: () => void;
  onReconnect: () => void;
  onSetActiveSessionsSortMode: (sortMode: SidebarActiveSessionsSortMode) => void;
  onSetSidebarV2Layout: (layout: SidebarV2Layout) => void;
  onSetSidebarVersion: (sidebarVersion: SidebarVersion) => void;
  onToggleSessionTagFilter: (tag: SidebarSessionTagFilter) => void;
  onToggleCollapsed: () => void;
  projectCollectionItems?: readonly SidebarProjectCollectionRenderItem[];
  projectUngroupDropIndicatorScopeId?: string;
  projectGroupIds: readonly string[];
  remoteMachineDropIndicatorPosition?: "before" | "after";
  renderProjectCollection?: (
    item: Extract<SidebarProjectCollectionRenderItem, { kind: "collection" }>,
    itemIndex: number,
  ) => ReactNode;
  renderProjectGroup: (groupId: string, groupIndex: number) => ReactNode;
  selectedSessionTagFilters: readonly SidebarSessionTagFilter[];
  sessionSummary?: SidebarSectionSessionSummary;
  sessionTagListItems: readonly SidebarSessionTagListItem[];
  sidebarV2Layout: SidebarV2Layout;
  sidebarVersion: SidebarVersion;
  status: RemoteMachineRuntimeStatus[ "state" ];
  statusMessage?: string;
}) {
  const isConnected = status === "connected";
  const busyLabel = remoteMachineBusyLabel(status);
  const isBusy = busyLabel !== undefined;
  /*
   * CDXC:GPUIRemoteConnectFeedback 2026-07-21:
   * Only connected remote machines keep the collapsible chevron. Every other
   * state replaces it with an always-visible header control: Connect while
   * disconnected, a spinner during connect/install/download, or an error
   * button whose tooltip carries the native host's sanitized failure reason.
   * Native owns the matching viewport-level toast for progress and failures.
   */
  const isFailure = !isConnected && !isBusy && status !== "disconnected";
  const remoteConnectionControl: RemoteMachineHeaderConnectionControl | undefined = isConnected
    ? undefined
    : isBusy
      ? {
          kind: "busy",
          label: busyLabel,
        }
      : isFailure
        ? {
            kind: "error",
            label: `Error: ${statusMessage ?? remoteMachineFailureLabel(status)}`,
            onClick: onReconnect,
          }
        : {
            kind: "connect",
            label: "Connect",
            onClick: onReconnect,
          };
  /*
   * CDXC:GPUIRemoteLastSeen 2026-07-12:
   * Disconnected machines keep listing their last-seen project groups faded
   * (the runtime marks those groups stale) instead of hiding the body, so
   * "No projects" is a connected-only empty state.
   */
  const showProjectList = isConnected || projectGroupIds.length > 0;
  const projectListScopeId = createRemoteProjectListScopeId(machine.id);
  const projectListPresence = useSidebarCollapsiblePresence(collapsed);
  const sortable = useSortable({
    accept: "remote-machine",
    data: createRemoteMachineDragData(machine.id),
    feedback: "none",
    id: `remote-machine:${machine.id}`,
    index,
    sensors: remoteMachineSensors,
    type: "remote-machine",
  });

  return (
    <div
      className="reference-remote-machine-section"
      data-disconnected={String(!isConnected)}
      data-dragging={String(Boolean(sortable.isDragging || isDragPreviewSource))}
      data-remote-machine-drop-position={remoteMachineDropIndicatorPosition}
      data-sidebar-remote-machine-id={machine.id}
      ref={sortable.ref}
    >
      <SidebarReferenceSectionHeader
        activeSessionsSortMode={activeSessionsSortMode}
        actionsAlwaysVisible={false}
        bulkActionLabel={bulkActionLabel}
        collapsed={collapsed}
        containsActiveSession={containsActiveSession}
        dragHandleRef={sortable.handleRef}
        onAddProject={isConnected ? onAddProject : undefined}
        onBulkProjectToggle={onBulkProjectToggle}
        onEdit={onEdit}
        onSetActiveSessionsSortMode={onSetActiveSessionsSortMode}
        onSetSidebarV2Layout={onSetSidebarV2Layout}
        onSetSidebarVersion={onSetSidebarVersion}
        onToggleSessionTagFilter={onToggleSessionTagFilter}
        onToggleCollapsed={onToggleCollapsed}
        remoteConnectionControl={remoteConnectionControl}
        sectionKey="remote"
        selectedSessionTagFilters={selectedSessionTagFilters}
        sessionSummary={sessionSummary}
        sessionTagListItems={sessionTagListItems}
        sidebarV2Layout={sidebarV2Layout}
        sidebarVersion={sidebarVersion}
        title={machine.name}
      />
      {showProjectList && projectListPresence.isPresent ? (
        <div
          aria-hidden={projectListPresence.isVisuallyCollapsed}
          className="group-list workspace-group-list reference-project-group-list reference-sidebar-collapsible-body"
          data-animate-children="false"
          data-collapsed={String(projectListPresence.isVisuallyCollapsed)}
          inert={projectListPresence.isVisuallyCollapsed ? true : undefined}
          ref={projectListPresence.setCollapsibleElement}
          data-sidebar-project-list-scope={projectListScopeId}
          data-sidebar-remote-project-list="true"
          data-stale={String(!isConnected)}
        >
          {projectGroupIds.length > 0 ? (
            <>
              {projectCollectionItems && renderProjectCollection
                ? projectCollectionItems.map((item, itemIndex) =>
                  item.kind === "project"
                    ? renderProjectGroup(item.groupId, projectGroupIds.indexOf(item.groupId))
                    : renderProjectCollection(item, itemIndex),
                )
                : projectGroupIds.map((groupId, groupIndex) =>
                  renderProjectGroup(groupId, groupIndex),
                )}
              <ProjectListEndUngroupDropZone
                active={projectUngroupDropIndicatorScopeId === projectListScopeId}
                scopeId={projectListScopeId}
              />
            </>
          ) : (
            <div className="reference-sidebar-empty-state">No projects</div>
          )}
        </div>
      ) : null}
    </div>
  );
}

