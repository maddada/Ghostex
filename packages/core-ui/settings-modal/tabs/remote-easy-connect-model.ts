import type {
  GxserverPairedDevice,
  GxserverPairedDevicesResult,
  GxserverRemoteAccessStatus,
  GxserverRemotePairingCodeResult,
  GxserverTailcatStatus,
} from '@/packages/shared/gxserver-protocol';

/*
 * CDXC:RemotePairing 2026-09-03:
 * Pure readers and formatters for the Settings → Remote page. Everything the
 * daemon returns passes through a reader here so a malformed reply surfaces
 * as one error line on the page instead of a component rendering `undefined`.
 * Identifiers still say "tailcat" where they name the gxserver endpoint or
 * its payload; user-facing copy says "Easy Connect".
 */

export const EASY_CONNECT_INSTALL_COMMAND = 'go install github.com/tailscale/tailcat/cmd/tailcat@latest';

/** Easy Connect status + pairing code poll (the code rotates after a device pairs). */
export const REMOTE_FAST_REFRESH_MS = 4000;
/** SSH probe, Tailscale status, and paired devices poll. */
export const REMOTE_SLOW_REFRESH_MS = 10_000;

/** A paired device counts as "connected now" while its last check-in is this recent. */
export const PAIRED_DEVICE_CONNECTED_WINDOW_MS = 3 * 60 * 1000;

export const EASY_CONNECT_MIN_PORT = 1;
export const EASY_CONNECT_MAX_PORT = 65535;

export function readTailcatStatusResult(value: unknown): GxserverTailcatStatus {
  const status = (value as { status?: unknown } | undefined)?.status;
  if (!status || typeof status !== 'object' || typeof (status as GxserverTailcatStatus).enabled !== 'boolean') {
    throw new Error('gxserver returned an unreadable Easy Connect status.');
  }
  return status as GxserverTailcatStatus;
}

export function readRemoteAccessStatusResult(value: unknown): GxserverRemoteAccessStatus {
  const status = value as Partial<GxserverRemoteAccessStatus> | undefined;
  if (
    !status ||
    typeof status !== 'object' ||
    typeof status.computerName !== 'string' ||
    typeof status.username !== 'string' ||
    !status.ssh ||
    typeof status.ssh.enabled !== 'boolean' ||
    !status.tailscale
  ) {
    throw new Error('gxserver returned an unreadable remote access status.');
  }
  return status as GxserverRemoteAccessStatus;
}

export function readRemotePairingCodeResult(value: unknown): GxserverRemotePairingCodeResult {
  if (!value || typeof value !== 'object') {
    throw new Error('gxserver returned an unreadable pairing code.');
  }
  const result = value as GxserverRemotePairingCodeResult;
  return {
    ...(result.easyConnect && typeof result.easyConnect.payload === 'string'
      ? { easyConnect: result.easyConnect }
      : {}),
    ...(result.tailscale && typeof result.tailscale.payload === 'string' ? { tailscale: result.tailscale } : {}),
  };
}

export function readPairedDevicesResult(value: unknown): readonly GxserverPairedDevice[] {
  const devices = (value as Partial<GxserverPairedDevicesResult> | undefined)?.devices;
  if (!Array.isArray(devices)) {
    throw new Error('gxserver returned an unreadable paired device list.');
  }
  return devices.filter(
    (device): device is GxserverPairedDevice =>
      !!device && typeof device === 'object' && typeof device.id === 'string' && typeof device.name === 'string'
  );
}

export function parseEasyConnectPortsInput(value: string): readonly number[] | undefined {
  const entries = value
    .split(',')
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);
  const ports: number[] = [];
  for (const entry of entries) {
    if (!/^\d{1,5}$/u.test(entry)) {
      return undefined;
    }
    const port = Number(entry);
    if (port < EASY_CONNECT_MIN_PORT || port > EASY_CONNECT_MAX_PORT) {
      return undefined;
    }
    if (!ports.includes(port)) {
      ports.push(port);
    }
  }
  return ports;
}

export function formatEasyConnectPorts(ports: readonly number[]): string {
  return ports.join(', ');
}

export function parseEasyConnectAllowedClientKeys(value: string): readonly string[] {
  const keys: string[] = [];
  for (const line of value.split(/\r?\n/u)) {
    const key = line.trim();
    if (key.length > 0 && !keys.includes(key)) {
      keys.push(key);
    }
  }
  return keys;
}

export function formatEasyConnectAllowedClientKeys(keys: readonly string[]): string {
  return keys.join('\n');
}

export type EasyConnectStatusBadge = {
  label: string;
  tone: 'active' | 'disabled' | 'failed' | 'needsSetup' | 'unknown';
};

export function getEasyConnectStatusBadge(status: GxserverTailcatStatus | undefined): EasyConnectStatusBadge {
  if (!status) {
    return { label: 'Unknown', tone: 'unknown' };
  }
  if (!status.binaryFound) {
    return { label: 'Not installed', tone: 'needsSetup' };
  }
  if (!status.enabled) {
    return { label: 'Off', tone: 'disabled' };
  }
  if (status.running) {
    return { label: 'Running', tone: 'active' };
  }
  return { label: status.lastError ? 'Failed' : 'Starting', tone: status.lastError ? 'failed' : 'unknown' };
}

export function isPairedDeviceConnectedNow(device: GxserverPairedDevice, now: number): boolean {
  if (!device.lastSeenAt) {
    return false;
  }
  const lastSeen = Date.parse(device.lastSeenAt);
  return Number.isFinite(lastSeen) && now - lastSeen < PAIRED_DEVICE_CONNECTED_WINDOW_MS;
}

const DAY_MS = 24 * 60 * 60 * 1000;

function startOfDay(timestamp: number): number {
  const date = new Date(timestamp);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

/** "today", "yesterday", or a short calendar date ("Aug 28", "Aug 28, 2025" when not this year). */
export function formatPairedDeviceDay(iso: string, now: number): string {
  const timestamp = Date.parse(iso);
  if (!Number.isFinite(timestamp)) {
    return 'unknown';
  }
  const dayDelta = Math.round((startOfDay(now) - startOfDay(timestamp)) / DAY_MS);
  if (dayDelta <= 0) {
    return 'today';
  }
  if (dayDelta === 1) {
    return 'yesterday';
  }
  const date = new Date(timestamp);
  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  return date.toLocaleDateString(undefined, {
    day: 'numeric',
    month: 'short',
    ...(sameYear ? {} : { year: 'numeric' }),
  });
}

export function formatPairedDeviceDetail(device: GxserverPairedDevice, now: number): string {
  const paired = `Paired ${formatPairedDeviceDay(device.pairedAt, now)}`;
  if (isPairedDeviceConnectedNow(device, now)) {
    return `${paired} · connected now`;
  }
  if (device.lastSeenAt) {
    return `${paired} · last seen ${formatPairedDeviceDay(device.lastSeenAt, now)}`;
  }
  return `${paired} · not connected yet`;
}
