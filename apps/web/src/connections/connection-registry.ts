import type { GxserverRpcEndpointPath } from "@/shared/gxserver-protocol";
import type { SessionChatEventHandler } from "./gxserver-client";
import { GxserverConnection } from "./gxserver-connection";
import type { GhostexWebMachine, MachineConnectionState } from "./types";

const connections = new Map<string, GxserverConnection>();
const connectionUnsubscribers = new Map<string, () => void>();
const listeners = new Set<() => void>();
let snapshot: readonly MachineConnectionState[] = [];

interface RegistrySessionChatEntry {
  machineId: string;
  projectId: string;
  sessionId: string;
  onEvent: SessionChatEventHandler;
  /** Follower tail window to request, re-read on every (re)subscribe. */
  currentLimit?: () => number;
  detach?: () => void;
}

// Chat subscriptions are held at the registry level, not on a connection
// instance: upsertMachineConnection REPLACES the connection object whenever the
// machine record changes (e.g. the auth token arriving after bootstrap), which
// would strand any subscription attached directly to the old instance.
const sessionChatEntries = new Set<RegistrySessionChatEntry>();

function attachSessionChatEntry(entry: RegistrySessionChatEntry): void {
  const connection = connections.get(entry.machineId);
  entry.detach = connection
    ? connection.subscribeSessionChat(
        entry.projectId,
        entry.sessionId,
        entry.onEvent,
        entry.currentLimit,
      )
    : undefined;
}

function reattachSessionChatEntries(machineId: string): void {
  for (const entry of sessionChatEntries) {
    if (entry.machineId === machineId) {
      attachSessionChatEntry(entry);
    }
  }
}

function detachSessionChatEntries(machineId: string): void {
  for (const entry of sessionChatEntries) {
    if (entry.machineId === machineId) {
      entry.detach?.();
      entry.detach = undefined;
    }
  }
}

export function subscribeSessionChatForMachine(
  machineId: string,
  projectId: string,
  sessionId: string,
  onEvent: SessionChatEventHandler,
  currentLimit?: () => number,
): () => void {
  const entry: RegistrySessionChatEntry = {
    machineId,
    onEvent,
    projectId,
    sessionId,
    ...(currentLimit ? { currentLimit } : {}),
  };
  sessionChatEntries.add(entry);
  attachSessionChatEntry(entry);
  return () => {
    if (!sessionChatEntries.delete(entry)) {
      return;
    }
    entry.detach?.();
    entry.detach = undefined;
  };
}

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
  reattachSessionChatEntries(machine.machineId);
}

export function removeMachineConnection(machineId: string): void {
  detachSessionChatEntries(machineId);
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

