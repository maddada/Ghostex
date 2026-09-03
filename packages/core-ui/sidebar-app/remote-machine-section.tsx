import { PointerSensor } from '@dnd-kit/dom';
import { useSortable } from '@dnd-kit/react/sortable';
import type { ReactNode } from 'react';
import type { RemoteMachineSettings } from '../../shared/ghostex-settings';
import { createRemoteMachineDragData } from '../sidebar-dnd';
import { getSidebarReorderActivationConstraints } from '../sidebar-reorder-activation';
import { SpaceFilterRow } from '../space-filter-row';
import { createRemoteSidebarSpaceSectionKey } from './space-filtering';
import type { SidebarSpacesState } from '../spaces';
import type { WebviewApi } from '../webview-api';
import { createRemoteProjectListScopeId } from './drag-drop-geometry';
import { ProjectListEndUngroupDropZone } from './drag-ghosts';
import type { RemoteMachineRuntimeStatus, SidebarProjectCollectionRenderItem } from './types';

/*
 * Remote machines reorder only from their visible header. Pointer-only
 * activation keeps Space/Enter out of the drag path and prevents a keyboard
 * drag from leaving the shared manager in an unseen drag.
 */
export const remoteMachineSensors = [
  PointerSensor.configure({
    activationConstraints: getSidebarReorderActivationConstraints,
  }),
];
export function remoteMachineBusyLabel(status: RemoteMachineRuntimeStatus['state']): string | undefined {
  switch (status) {
    case 'connecting':
      return 'Connecting…';
    case 'installing':
      return 'Installing gxserver…';
    case 'downloadingRemoteServerPackage':
      return 'Downloading server package…';
    default:
      return undefined;
  }
}

export function remoteMachineFailureLabel(status: RemoteMachineRuntimeStatus['state']): string {
  switch (status) {
    case 'installApprovalRequired':
      return 'Install approval required.';
    case 'installFailed':
      return 'gxserver install failed.';
    case 'invalid':
      return 'Saved remote machine is incomplete.';
    case 'keychainFailed':
      return 'Could not save the auth token to Keychain.';
    case 'presentationStreamFailed':
    case 'presentationSubscribeFailed':
      return 'Remote session stream failed.';
    case 'sshFailed':
      return 'SSH connection failed.';
    case 'tokenUnavailable':
      return 'Remote auth token unavailable.';
    case 'tunnelFailed':
      return 'Secure tunnel failed.';
    case 'unsupported':
    case 'unsupportedRemotePlatform':
      return 'Remote platform not supported.';
    default:
      return 'Remote connect failed.';
  }
}

/*
 * CDXC:ContextMenus 2026-09-02:
 * The remote machine header is gone. Its connection control (Connect, busy
 * spinner, error cloud with retry) now lives on the machine's tab in the top
 * strip, and Add Project, Sort & Filter, Collapse All, and Edit Machine live at
 * the top of the More dropdown for whichever machine that strip has selected.
 * This section only renders the machine's Space row and project list.
 */
export function RemoteMachineSidebarSection({
  index,
  isDragPreviewSource,
  machine,
  onReorderSpaces,
  onSelectSpace,
  projectCollectionItems,
  projectUngroupDropIndicatorScopeId,
  projectGroupIds,
  remoteMachineDropIndicatorPosition,
  renderProjectCollection,
  renderProjectGroup,
  selectedSpaceId,
  spaces,
  status,
  vscode,
}: {
  index: number;
  isDragPreviewSource: boolean;
  machine: RemoteMachineSettings;
  /*
   * CDXC:Spaces 2026-08-27:
   * A remote gxserver's Spaces come from that server and are never mixed with
   * the local set, so this section takes its own Space state plus its own
   * selection and reorder callback. `spaces` is undefined for a machine whose
   * daemon has never delivered one: that machine is Space-incapable and shows
   * no Space row.
   */
  onReorderSpaces: (orderedSpaceIds: string[]) => void;
  onSelectSpace: (spaceId: string) => void;
  selectedSpaceId?: string;
  spaces?: SidebarSpacesState;
  vscode: WebviewApi;
  projectCollectionItems?: readonly SidebarProjectCollectionRenderItem[];
  projectUngroupDropIndicatorScopeId?: string;
  projectGroupIds: readonly string[];
  remoteMachineDropIndicatorPosition?: 'before' | 'after';
  renderProjectCollection?: (
    item: Extract<SidebarProjectCollectionRenderItem, { kind: 'collection' }>,
    itemIndex: number
  ) => ReactNode;
  renderProjectGroup: (groupId: string, groupIndex: number) => ReactNode;
  status: RemoteMachineRuntimeStatus['state'];
}) {
  const isConnected = status === 'connected';
  /*
   * CDXC:RemoteMachines 2026-07-12:
   * Disconnected machines keep listing their last-seen project groups faded
   * (the runtime marks those groups stale) instead of hiding the body, so
   * "No projects" is a connected-only empty state.
   */
  const showProjectList = isConnected || projectGroupIds.length > 0;
  const projectListScopeId = createRemoteProjectListScopeId(machine.id);
  /*
   * With machines switched through the top tab strip only one machine section
   * is ever mounted, and the header that used to be its drag handle is gone,
   * so the sortable stays registered for the shared drop-indicator plumbing
   * but never activates: without a handle the whole project list would
   * otherwise become the machine's drag source.
   */
  const sortable = useSortable({
    accept: 'remote-machine',
    data: createRemoteMachineDragData(machine.id),
    disabled: true,
    feedback: 'none',
    id: `remote-machine:${machine.id}`,
    index,
    sensors: remoteMachineSensors,
    type: 'remote-machine',
  });

  return (
    <div
      className='reference-remote-machine-section'
      data-disconnected={String(!isConnected)}
      data-dragging={String(Boolean(sortable.isDragging || isDragPreviewSource))}
      data-remote-machine-drop-position={remoteMachineDropIndicatorPosition}
      data-sidebar-remote-machine-id={machine.id}
      ref={sortable.ref}
    >
      {spaces ? (
        <SpaceFilterRow
          collapsed={false}
          onReorderSpaces={onReorderSpaces}
          onSelectSpace={onSelectSpace}
          remoteMachineId={machine.id}
          sectionKey={createRemoteSidebarSpaceSectionKey(machine.id)}
          selectedSpaceId={selectedSpaceId}
          spaces={spaces}
          vscode={vscode}
        />
      ) : null}
      {showProjectList ? (
        <div
          className='group-list workspace-group-list reference-project-group-list'
          data-sidebar-project-list-scope={projectListScopeId}
          data-sidebar-remote-project-list='true'
          data-sidebar-space-content-section={createRemoteSidebarSpaceSectionKey(machine.id)}
          data-stale={String(!isConnected)}
        >
          {projectGroupIds.length > 0 ? (
            <>
              {projectCollectionItems && renderProjectCollection
                ? projectCollectionItems.map((item, itemIndex) =>
                    item.kind === 'project'
                      ? renderProjectGroup(item.groupId, projectGroupIds.indexOf(item.groupId))
                      : renderProjectCollection(item, itemIndex)
                  )
                : projectGroupIds.map((groupId, groupIndex) => renderProjectGroup(groupId, groupIndex))}
              <ProjectListEndUngroupDropZone
                active={projectUngroupDropIndicatorScopeId === projectListScopeId}
                scopeId={projectListScopeId}
              />
            </>
          ) : (
            <div className='reference-sidebar-empty-state'>No projects</div>
          )}
        </div>
      ) : null}
    </div>
  );
}
