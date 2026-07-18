import type { GxserverRpcEndpointPath } from "@/shared/gxserver-protocol";
import { GxserverConnection } from "./gxserver-connection";
import type { GhostexWebMachine, MachineConnectionState } from "./types";

const connections = new Map<string, GxserverConnection>();
const connectionUnsubscribers = new Map<string, () => void>();
const listeners = new Set<() => void>();
let snapshot: readonly MachineConnectionState[] = [];

export function upsertMachineConnection(machine: GhostexWebMachine): void {
  const current = connections.get(machine.machineId);
  if (current && machinesEqual(current.machine, machine)) {
    return;
  }
  removeMachineConnection(machine.machineId);
  const connection = new GxserverConnection(machine);
  connections.set(machine.machineId, connection);
  connectionUnsubscribers.set(machine.machineId, connection.subscribe(publish));
  publish();
  connection.start();
}

export function removeMachineConnection(machineId: string): void {
  connectionUnsubscribers.get(machineId)?.();
  connectionUnsubscribers.delete(machineId);
  connections.get(machineId)?.stop();
  if (connections.delete(machineId)) {
    publish();
  }
}

export function getMachineConnection(machineId: string): GxserverConnection | undefined {
  return connections.get(machineId);
}

export function rpcForMachine<TResult>(
  machineId: string,
  path: GxserverRpcEndpointPath,
  params?: Record<string, unknown>,
): Promise<TResult> {
  const connection = connections.get(machineId);
  if (!connection) {
    return Promise.reject(new Error(`Machine ${machineId} is not connected.`));
  }
  return connection.rpc<TResult>(path, params);
}

export function subscribeConnectionStates(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getConnectionStates(): readonly MachineConnectionState[] {
  return snapshot;
}

function publish(): void {
  snapshot = [...connections.values()].map((connection) => connection.getState());
  for (const listener of listeners) {
    listener();
  }
}

function machinesEqual(left: GhostexWebMachine, right: GhostexWebMachine): boolean {
  return left.machineId === right.machineId
    && left.label === right.label
    && left.baseUrl === right.baseUrl
    && left.authToken === right.authToken;
}

