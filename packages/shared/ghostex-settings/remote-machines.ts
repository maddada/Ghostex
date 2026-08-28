import { isRecord, readLooseString } from './primitives';

export type RemoteMachineSettings = {
  id: string;
  name: string;
  sshHost: string;
  sshIdentityFile?: string;
  sshPasswordSaved?: boolean;
  sshPort?: number;
  sshUser?: string;
  wslDistribution?: string;
  /**
   * CDXC:RemoteMachines 2026-08-28:
   * A saved machine can stay in Settings while being omitted from the sidebar.
   * Missing or false means shown; true hides the tab and skips auto-connect.
   */
  disabled?: boolean;
};

export function isRemoteMachineEnabledInSidebar(machine: Pick<RemoteMachineSettings, 'disabled'>): boolean {
  return machine.disabled !== true;
}

export function applyEnabledRemoteMachineOrder(
  machines: readonly RemoteMachineSettings[],
  nextEnabledIds: readonly string[]
): RemoteMachineSettings[] | undefined {
  const enabledMachines = machines.filter(isRemoteMachineEnabledInSidebar);
  if (
    nextEnabledIds.length !== enabledMachines.length ||
    new Set(nextEnabledIds).size !== nextEnabledIds.length ||
    enabledMachines.some((machine) => !nextEnabledIds.includes(machine.id))
  ) {
    return undefined;
  }
  const machineById = new Map(machines.map((machine) => [machine.id, machine]));
  let enabledIndex = 0;
  return machines.map((machine) => {
    if (!isRemoteMachineEnabledInSidebar(machine)) {
      return machine;
    }
    const nextId = nextEnabledIds[enabledIndex++];
    return machineById.get(nextId) ?? machine;
  });
}

export function normalizeRemoteMachineSettings(candidate: unknown): RemoteMachineSettings[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  const seenIds = new Set<string>();
  const normalized: RemoteMachineSettings[] = [];
  for (const item of candidate) {
    if (!isRecord(item)) {
      continue;
    }
    const name = readLooseString(item.name).slice(0, 80);
    const sshHost = readLooseString(item.sshHost).slice(0, 200);
    if (!name || !sshHost) {
      continue;
    }
    let id = normalizeRemoteMachineId(item.id);
    if (!id || seenIds.has(id)) {
      id = `remote-${normalized.length + 1}`;
      while (seenIds.has(id)) {
        id = `remote-${normalized.length + 1}-${seenIds.size + 1}`;
      }
    }
    seenIds.add(id);
    const sshUser = readLooseString(item.sshUser).slice(0, 120);
    const sshIdentityFile = readLooseString(item.sshIdentityFile).slice(0, 500);
    const sshPort = normalizeRemoteMachineSshPort(item.sshPort);
    const wslDistribution = normalizeRemoteMachineWslDistribution(item.wslDistribution);
    normalized.push({
      id,
      name,
      sshHost,
      /*
      CDXC:RemoteMachines 2026-06-09-18:23:
      Remote SSH passwords are stored only in macOS Keychain. Settings may keep
      this boolean marker so the UI can show a saved credential state, but raw
      password fields from drafts/imports must be ignored by normalization.
      */
      ...(item.sshPasswordSaved === true ? { sshPasswordSaved: true } : {}),
      ...(item.disabled === true ? { disabled: true } : {}),
      ...(sshIdentityFile ? { sshIdentityFile } : {}),
      ...(sshPort ? { sshPort } : {}),
      ...(sshUser ? { sshUser } : {}),
      ...(wslDistribution ? { wslDistribution } : {}),
    });
  }
  return normalized;
}

function normalizeRemoteMachineWslDistribution(value: unknown): string {
  const distribution = readLooseString(value).slice(0, 120);
  if (!distribution || distribution.startsWith('-') || !/^[A-Za-z0-9][A-Za-z0-9._+() -]*$/u.test(distribution)) {
    return '';
  }
  return distribution;
}

function normalizeRemoteMachineId(input: unknown): string | undefined {
  const id = readLooseString(input).slice(0, 80);
  return /^remote-[a-z0-9_-]+$/iu.test(id) ? id : undefined;
}

function normalizeRemoteMachineSshPort(input: unknown): number | undefined {
  if (input === undefined || input === null || input === '') {
    return undefined;
  }
  const value = typeof input === 'number' ? input : Number(input);
  if (!Number.isInteger(value) || value < 1 || value > 65535) {
    return undefined;
  }
  return value;
}
