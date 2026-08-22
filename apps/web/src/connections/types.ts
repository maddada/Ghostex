import type { GxserverPresentationSnapshot } from "@/packages/shared/gxserver-protocol";

export type GhostexWebMachine = {
  authToken: string;
  baseUrl: string;
  label: string;
  machineId: string;
};

export type MachineConnectionStatus = "connected" | "connecting" | "disconnected";

export type MachineConnectionState = {
  error?: string;
  machine: GhostexWebMachine;
  presentation?: GxserverPresentationSnapshot;
  reconnectAt?: number;
  status: MachineConnectionStatus;
};

