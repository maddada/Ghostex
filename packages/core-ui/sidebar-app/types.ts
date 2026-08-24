import type { ExtensionToSidebarMessage } from '../../shared/session-grid-contract';
import type { KeepAwakeDurationMinutes } from '../../shared/ghostex-settings';
import type { SidebarProjectCollection } from '../project-collections';
import type { getGroupSessionSummary } from '../group-session-summary';
import type { useSidebarStore } from '../sidebar-store';

export type SidebarEventSource = Pick<Window, 'addEventListener' | 'removeEventListener'>;
export type SessionIdsByGroup = Record<string, string[]>;
export type SidebarStoreState = ReturnType<typeof useSidebarStore.getState>;
export type SidebarGroupsById = SidebarStoreState['groupsById'];
export type SidebarSessionsById = SidebarStoreState['sessionsById'];
export type SidebarSectionSessionSummary = ReturnType<typeof getGroupSessionSummary> & {
  awakeCount: number;
};
export type RemoteMachineRuntimeStatus = Extract<ExtensionToSidebarMessage, { type: 'remoteMachineStatus' }>;
export type RemoteMachineRuntimeStatuses = Record<string, RemoteMachineRuntimeStatus['state']>;
export type RemoteMachineStatusMessages = Record<string, string>;
export type HeaderSortMenuPosition = {
  left: number;
  top: number;
};

export type RemoteMachineHeaderConnectionControl = {
  kind: 'busy' | 'connect' | 'error';
  label: string;
  onClick?: () => void;
};
export type SidebarKeepAwakeRuntimeState = {
  durationMinutes: KeepAwakeDurationMinutes;
};
export type SidebarProjectCollectionRenderItem =
  | { collection: SidebarProjectCollection; groupIds: string[]; kind: 'collection' }
  | { groupId: string; kind: 'project' };
export type ReferenceSidebarSectionId = 'projects' | 'quick' | 'remote';
