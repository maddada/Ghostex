import type { EasyConnectCode, TailscaleCode } from './ghostex-remote-pairing';
import { GXSERVER_LOCAL_API_PORT } from './gxserver-protocol-core';
import type {
  GxserverProduct,
  GxserverProtocolVersion,
  GxserverServerId,
  GxserverLifecycleState,
  GxserverMinimalHealthResponse,
  GxserverListenerConfig,
  GxserverMigrationStatus,
} from './gxserver-protocol-core';

/**
 * tailcat is the control-plane-free remote-access sidecar gxserver supervises.
 * `token` is the address blob clients dial; it is derived from the daemon-owned
 * server key at runtime and is null until the running sidecar has published it.
 */
export interface GxserverTailcatStatus {
  installing?: boolean;
  installProgress?: string | null;
  installError?: string | null;
  enabled: boolean;
  running: boolean;
  binaryFound: boolean;
  binaryPath: string | null;
  binaryVersion: string | null;
  token: string | null;
  ports: readonly number[];
  allowedClientKeys: readonly string[];
  lastError: string | null;
}

export interface GxserverTailcatStatusResult {
  status: GxserverTailcatStatus;
}

export type GxserverTailcatStateUpdate =
  | { kind: 'setEnabled'; enabled: boolean }
  | { kind: 'setPorts'; ports: readonly number[] }
  | { kind: 'setAllowedClientKeys'; allowedClientKeys: readonly string[] };

/*
CDXC:RemotePairing 2026-09-03:
Remote access status (SSH access + Tailscale + this computer's identity) and
the pairing codes shown in Settings → Remote. The structured pairing payloads
come from `/api/remotePairingCode`, never from the raw sidecar token, so the
QR can carry the user, ports, and the one-time pairing secret.
*/
export type GxserverRemoteAccessPlatform = 'macos' | 'windows' | 'linux';

export interface GxserverRemoteSshAccessStatus {
  enabled: boolean;
  /** The SSH port that was probed (the one Easy Connect serves). */
  port: number;
  /** ISO timestamp of the probe. */
  checkedAt: string;
  /** Per-OS service detail (launchd / systemd / Windows service state). */
  detail: string | null;
}

export interface GxserverRemoteTailscaleStatus {
  installed: boolean;
  running: boolean;
  /** Signed-in login name, or the tailnet name when the login is unknown. */
  account: string | null;
  /** MagicDNS name without the trailing dot. */
  magicDnsName: string | null;
  /** First Tailscale IP (100.x.y.z). */
  ip: string | null;
  /** Whether this node runs Tailscale SSH; null when Tailscale is not running. */
  sshEnabled: boolean | null;
  /** What `tailscale status --json` printed when it could not report a status; absent when it answered. */
  probeError?: string;
}

export interface GxserverRemoteAccessStatus {
  computerName: string;
  username: string;
  platform: GxserverRemoteAccessPlatform;
  ssh: GxserverRemoteSshAccessStatus;
  tailscale: GxserverRemoteTailscaleStatus;
}

export type GxserverEnableSshAccessOutcome = 'enabled' | 'cancelled' | 'failed';

export interface GxserverEnableSshAccessResult {
  outcome: GxserverEnableSshAccessOutcome;
  message: string | null;
  /** SSH status re-read after the enable attempt. */
  ssh: GxserverRemoteSshAccessStatus;
}

export interface GxserverRemotePairingCodeParams {
  /**
   * The Easy Connect QR is on screen: while "Only paired devices can connect" is on, lift the
   * allow-list so a new phone can pair; it returns 60 seconds after the last such ask.
   */
  openTunnel?: boolean;
}

export interface GxserverRemotePairingCodeResult {
  /*
   * `payload` is what the QR and the copy button carry: the encoded code wrapped in
   * `PAIRING_LINK_PREFIX` (`https://ghostex.dev/pair#ghostex-ec1:…`) so a phone's system camera
   * opens the app. `readPairingCode` reads it and the bare code alike.
   */
  /** Present only while Easy Connect is running with a published address. */
  easyConnect?: { payload: string; code: EasyConnectCode };
  /** Present only while Tailscale is running with a MagicDNS name or IP. */
  tailscale?: { payload: string; code: TailscaleCode };
}

export interface GxserverPairedDevice {
  id: string;
  name: string;
  platform: string;
  pairedAt: string;
  lastSeenAt: string | null;
  sshKeyFingerprint: string;
  /** The phone's Easy Connect client key; null for a phone paired by an older Ghostex app. */
  tailcatClientKey: string | null;
}

// M2 (paired devices + pairing registration): served by
// `server/src/remote_access/{paired_devices,pair_device}.rs`.
export interface GxserverPairedDevicesResult {
  devices: readonly GxserverPairedDevice[];
}

export interface GxserverRemovePairedDeviceParams {
  deviceId: string;
}

export interface GxserverRemovePairedDeviceResult {
  devices: readonly GxserverPairedDevice[];
}

export interface GxserverPairDeviceParams {
  secret: string;
  deviceName: string;
  platform: string;
  sshPublicKey: string;
  tailcatClientKey?: string;
}

export interface GxserverPairDeviceResult {
  deviceId: string;
  user: string;
  computerName: string;
  /** The phone's own API token; removing the device revokes it. Only its hash is stored. */
  deviceApiToken: string;
}

export interface GxserverPairedDeviceSeenParams {
  deviceId: string;
}

export interface GxserverPairedDeviceSeenResult {
  device: GxserverPairedDevice;
}

export interface GxserverServerHealthResponse extends GxserverMinimalHealthResponse {
  buildIdentity: string;
  capabilities: readonly string[];
  listeners: {
    local: GxserverListenerConfig;
    remote: GxserverListenerConfig;
  };
  migration: GxserverMigrationStatus;
  pid: number;
  port: typeof GXSERVER_LOCAL_API_PORT;
  serverId: GxserverServerId;
  startedAt: string;
  tools: readonly GxserverToolCapabilityStatus[];
}

export type GxserverToolName = 'zmx' | 'bd';
export type GxserverToolAvailability = 'available' | 'missing' | 'notExecutable' | 'unsupported';
export type GxserverToolResolutionSource = 'devSubmodule' | 'appResource' | 'gxserverBundle';

export interface GxserverToolCapabilityStatus {
  availability: GxserverToolAvailability;
  candidatePaths?: readonly string[];
  capability: 'zmxLifecycle' | 'beadsProjectBoard' | 'deferred';
  executablePath?: string;
  guidance?: string;
  message: string;
  source?: GxserverToolResolutionSource;
  tool: GxserverToolName;
}

export interface GxserverRuntimeMetadata {
  buildIdentity: string;
  pid: number;
  port: typeof GXSERVER_LOCAL_API_PORT;
  protocolVersion: GxserverProtocolVersion;
  serverId: GxserverServerId;
  startedAt: string;
  version: string;
}

export interface GxserverStatusResponse {
  health?: GxserverServerHealthResponse;
  metadata?: GxserverRuntimeMetadata;
  message: string;
  ok: boolean;
  product: GxserverProduct;
  state: GxserverLifecycleState;
}

export interface GxserverProtocolMismatch {
  actualProtocolVersion: unknown;
  expectedProtocolVersion: GxserverProtocolVersion;
  message: string;
  product: GxserverProduct;
}
