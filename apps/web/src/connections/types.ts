import type { GxserverPresentationSnapshot } from '@/packages/shared/gxserver-protocol';

export type GhostexWebMachine = {
  authToken: string;
  baseUrl: string;
  label: string;
  machineId: string;
};

export type MachineConnectionStatus = 'connected' | 'connecting' | 'disconnected';

export type MachineConnectionState = {
  error?: string;
  /**
   * CDXC:AgentLauncher 2026-08-29:
   * The presentation revision the daemon last announced a Global Action write
   * at. Global Actions are not project metadata, so nothing in the
   * presentation snapshot moves when one changes; readers key their
   * `/api/readSidebarHud` refetch on this instead.
   */
  globalSidebarCommandsRevision?: number;
  machine: GhostexWebMachine;
  presentation?: GxserverPresentationSnapshot;
  reconnectAt?: number;
  status: MachineConnectionStatus;
};
