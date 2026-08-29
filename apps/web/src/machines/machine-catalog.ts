import {
  GXSERVER_PRODUCT,
  GXSERVER_PROTOCOL_VERSION,
  GXSERVER_WEB_BOOTSTRAP_ENDPOINT,
  type GxserverWebBootstrapResult,
} from '@/packages/shared/gxserver-protocol';
import { createGxserverClient } from '../connections/gxserver-client';
import { removeMachineConnection, upsertMachineConnection } from '../connections/connection-registry';
import type { GhostexWebMachine } from '../connections/types';
import { reconcileWebSessionChatDraftCache } from '../sidebar-runtime/draft-session-cache';
import { applyRemoteMachineOrder } from './machine-order';

export const MACHINES_STORAGE_KEY = 'ghostexWeb.machines.v1';

export type MachineCatalogState = {
  bootstrapError?: string;
  initialized: boolean;
  initializing: boolean;
  machines: readonly GhostexWebMachine[];
};

export type AddMachineInput = {
  authToken: string;
  baseUrl: string;
  label: string;
};

const listeners = new Set<() => void>();
let initializePromise: Promise<void> | undefined;
let state: MachineCatalogState = {
  initialized: false,
  initializing: false,
  machines: readPersistedMachines(),
};

export function subscribeMachineCatalog(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getMachineCatalogState(): MachineCatalogState {
  return state;
}

export function initializeMachineCatalog(): Promise<void> {
  if (initializePromise) {
    return initializePromise;
  }
  initializePromise = initializeMachineCatalogOnce();
  return initializePromise;
}

export async function addMachine(input: AddMachineInput): Promise<GhostexWebMachine> {
  const label = input.label.trim();
  const authToken = input.authToken.trim();
  const baseUrl = normalizeBaseUrl(input.baseUrl);
  if (!label) {
    throw new Error('Enter a machine name.');
  }
  if (!authToken) {
    throw new Error('Enter the machine auth token.');
  }
  if (state.machines.some((machine) => machine.baseUrl === baseUrl)) {
    throw new Error('That gxserver is already in the machine list.');
  }

  const candidate: GhostexWebMachine = {
    authToken,
    baseUrl,
    label,
    machineId: createMachineId(),
  };
  await createGxserverClient(candidate).fetchHealth();

  const machines = [...state.machines, candidate];
  updateState({ machines });
  persistAddedMachines(machines);
  upsertMachineConnection(candidate);
  reconcileWebSessionChatDraftCache(candidate.machineId);
  return candidate;
}

export function removeMachine(machineId: string): void {
  if (machineId === 'local') {
    return;
  }
  const machines = state.machines.filter((machine) => machine.machineId !== machineId);
  if (machines.length === state.machines.length) {
    return;
  }
  updateState({ machines });
  persistAddedMachines(machines);
  removeMachineConnection(machineId);
}

export function reorderRemoteMachines(orderedRemoteMachineIds: readonly string[]): boolean {
  const machines = applyRemoteMachineOrder(state.machines, orderedRemoteMachineIds);
  if (machines === state.machines) {
    return false;
  }
  updateState({ machines });
  persistAddedMachines(machines);
  return true;
}

async function initializeMachineCatalogOnce(): Promise<void> {
  updateState({ initializing: true });
  for (const machine of state.machines) {
    upsertMachineConnection(machine);
    reconcileWebSessionChatDraftCache(machine.machineId);
  }

  try {
    const localMachine = await fetchPrimaryMachine();
    const machines = [localMachine, ...state.machines.filter((machine) => machine.machineId !== 'local')];
    updateState({
      bootstrapError: undefined,
      initialized: true,
      initializing: false,
      machines,
    });
    upsertMachineConnection(localMachine);
    reconcileWebSessionChatDraftCache(localMachine.machineId);
  } catch (error) {
    updateState({
      bootstrapError: error instanceof Error ? error.message : String(error),
      initialized: true,
      initializing: false,
    });
  }
}

async function fetchPrimaryMachine(): Promise<GhostexWebMachine> {
  const response = await fetch(GXSERVER_WEB_BOOTSTRAP_ENDPOINT, {
    body: JSON.stringify({ params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION }),
    headers: { 'content-type': 'application/json' },
    method: 'POST',
  });
  const body = await readJson(response);
  if (!response.ok || !isBootstrapEnvelope(body)) {
    throw new Error(readErrorMessage(body, `Local gxserver bootstrap failed (${response.status}).`));
  }
  const result = body.result;
  if (result.protocolVersion !== GXSERVER_PROTOCOL_VERSION || !result.authToken || !result.machineLabel) {
    throw new Error('Local gxserver returned an invalid web bootstrap response.');
  }
  return {
    authToken: result.authToken,
    baseUrl: normalizeBaseUrl(result.baseUrl),
    label: result.machineLabel,
    machineId: 'local',
  };
}

function readPersistedMachines(): GhostexWebMachine[] {
  try {
    const serialized = window.localStorage.getItem(MACHINES_STORAGE_KEY);
    if (!serialized) {
      return [];
    }
    const parsed = JSON.parse(serialized) as unknown;
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed.filter(isPersistedMachine).map((machine) => ({
      ...machine,
      baseUrl: normalizeBaseUrl(machine.baseUrl),
    }));
  } catch {
    return [];
  }
}

function persistAddedMachines(machines: readonly GhostexWebMachine[]): void {
  window.localStorage.setItem(
    MACHINES_STORAGE_KEY,
    JSON.stringify(machines.filter((machine) => machine.machineId !== 'local'))
  );
}

function normalizeBaseUrl(value: string): string {
  let url: URL;
  try {
    url = new URL(value.trim());
  } catch {
    throw new Error('Enter a valid gxserver URL.');
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new Error('Machine URLs must use http or https.');
  }
  if (url.username || url.password || url.search || url.hash || (url.pathname !== '/' && url.pathname !== '')) {
    throw new Error('Enter the gxserver origin without a path, query, or credentials.');
  }
  return url.origin;
}

function createMachineId(): string {
  return `machine-${crypto.randomUUID()}`;
}

function updateState(update: Partial<MachineCatalogState>): void {
  state = { ...state, ...update };
  for (const listener of listeners) {
    listener();
  }
}

async function readJson(response: Response): Promise<unknown> {
  const text = await response.text();
  return text.trim() ? (JSON.parse(text) as unknown) : undefined;
}

function isBootstrapEnvelope(value: unknown): value is { result: GxserverWebBootstrapResult } {
  if (!isObject(value) || value.ok !== true || value.product !== GXSERVER_PRODUCT || !isObject(value.result)) {
    return false;
  }
  const result = value.result;
  return (
    typeof result.authToken === 'string' &&
    typeof result.baseUrl === 'string' &&
    typeof result.machineLabel === 'string' &&
    typeof result.protocolVersion === 'number'
  );
}

function isPersistedMachine(value: unknown): value is GhostexWebMachine {
  return (
    isObject(value) &&
    typeof value.machineId === 'string' &&
    value.machineId !== 'local' &&
    typeof value.label === 'string' &&
    typeof value.baseUrl === 'string' &&
    typeof value.authToken === 'string'
  );
}

function readErrorMessage(value: unknown, defaultMessage: string): string {
  return isObject(value) && typeof value.message === 'string' ? value.message : defaultMessage;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
