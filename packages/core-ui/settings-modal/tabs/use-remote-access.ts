import { useCallback, useEffect, useRef, useState } from 'react';
import type {
  GxserverEnableSshAccessResult,
  GxserverPairedDevice,
  GxserverRemoteAccessStatus,
  GxserverRemotePairingCodeResult,
  GxserverTailcatStateUpdate,
  GxserverTailcatStatus,
} from '@/packages/shared/gxserver-protocol';
import type { RemoteSetupRpc } from '../../remote-setup-modal/gxserver-rpc';
import {
  REMOTE_FAST_REFRESH_MS,
  REMOTE_SLOW_REFRESH_MS,
  readPairedDevicesResult,
  readRemoteAccessStatusResult,
  readRemotePairingCodeResult,
  readTailcatStatusResult,
} from './remote-easy-connect-model';

export type SshEnableAttempt = { outcome: GxserverEnableSshAccessResult['outcome']; message: string | null };

export type RemoteAccessState = {
  /** Easy Connect sidecar status (`/api/tailcatStatus`). */
  easyConnect: GxserverTailcatStatus | undefined;
  /** SSH access, Tailscale, and this computer's identity (`/api/remoteAccessStatus`). */
  access: GxserverRemoteAccessStatus | undefined;
  /** The two QR payloads (`/api/remotePairingCode`); polled so a consumed secret rotates the QR. */
  pairingCode: GxserverRemotePairingCodeResult | undefined;
  pairedDevices: readonly GxserverPairedDevice[] | undefined;
  requestError: string | undefined;
  isEnablingSsh: boolean;
  /** Result of the last Turn on SSH access click; cleared once SSH access reads as on. */
  sshEnableAttempt: SshEnableAttempt | undefined;
  removingDeviceId: string | undefined;
  setEasyConnectState: (update: GxserverTailcatStateUpdate) => void;
  enableSshAccess: () => void;
  removePairedDevice: (deviceId: string) => void;
  refresh: () => void;
};

/**
 * CDXC:RemotePairing 2026-09-03:
 * One owner for everything the Remote page reads from the daemon, so the Easy
 * Connect card, the Tailscale card, and Advanced all render the same snapshot.
 * Two timers while the tab is active and the page is visible: a fast one for
 * the sidecar status and the pairing code (the code silently rotates after a
 * phone consumes the secret, and the QR must follow; minting also touches the
 * daemon's SQLite store), and a slow one for the SSH probe, `tailscale
 * status`, and the paired device list, which shell out on the daemon and do
 * not change on their own from one second to the next. Each poll is guarded
 * so a slow daemon never stacks overlapping requests, and both stop as soon
 * as the tab is left, the modal closes, or the page is hidden.
 */
export function useRemoteAccess(rpc: RemoteSetupRpc | undefined, isActive: boolean): RemoteAccessState {
  const [easyConnect, setEasyConnect] = useState<GxserverTailcatStatus>();
  const [access, setAccess] = useState<GxserverRemoteAccessStatus>();
  const [pairingCode, setPairingCode] = useState<GxserverRemotePairingCodeResult>();
  const [pairedDevices, setPairedDevices] = useState<readonly GxserverPairedDevice[]>();
  const [requestError, setRequestError] = useState<string>();
  const [isEnablingSsh, setIsEnablingSsh] = useState(false);
  const [sshEnableAttempt, setSshEnableAttempt] = useState<SshEnableAttempt>();
  const [removingDeviceId, setRemovingDeviceId] = useState<string>();
  const [refreshTick, setRefreshTick] = useState(0);
  const [isVisible, setIsVisible] = useState(() => document.visibilityState !== 'hidden');
  const mountedRef = useRef(true);
  const fastInFlightRef = useRef(false);
  const slowInFlightRef = useRef(false);

  useEffect(() => {
    const handleVisibility = () => setIsVisible(document.visibilityState !== 'hidden');
    document.addEventListener('visibilitychange', handleVisibility);
    return () => document.removeEventListener('visibilitychange', handleVisibility);
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const applyError = useCallback((error: unknown) => {
    if (mountedRef.current) {
      setRequestError(error instanceof Error ? error.message : String(error));
    }
  }, []);

  const refreshFast = useCallback(() => {
    if (!rpc || fastInFlightRef.current) {
      return;
    }
    fastInFlightRef.current = true;
    void Promise.all([
      rpc('/api/tailcatStatus', {}).then(readTailcatStatusResult),
      rpc('/api/remotePairingCode', {}).then(readRemotePairingCodeResult),
    ])
      .then(([nextEasyConnect, nextPairingCode]) => {
        if (!mountedRef.current) {
          return;
        }
        setEasyConnect(nextEasyConnect);
        setPairingCode(nextPairingCode);
        setRequestError(undefined);
      })
      .catch(applyError)
      .finally(() => {
        fastInFlightRef.current = false;
      });
  }, [applyError, rpc]);

  const refreshSlow = useCallback(() => {
    if (!rpc || slowInFlightRef.current) {
      return;
    }
    slowInFlightRef.current = true;
    void Promise.all([
      rpc('/api/remoteAccessStatus', {}).then(readRemoteAccessStatusResult),
      rpc('/api/pairedDevices', {}).then(readPairedDevicesResult),
    ])
      .then(([nextAccess, nextDevices]) => {
        if (!mountedRef.current) {
          return;
        }
        setAccess(nextAccess);
        setPairedDevices(nextDevices);
        if (nextAccess.ssh.enabled) {
          setSshEnableAttempt(undefined);
        }
      })
      .catch(applyError)
      .finally(() => {
        slowInFlightRef.current = false;
      });
  }, [applyError, rpc]);

  useEffect(() => {
    if (!isActive || !isVisible || !rpc) {
      return;
    }
    refreshFast();
    refreshSlow();
    const fast = window.setInterval(refreshFast, REMOTE_FAST_REFRESH_MS);
    const slow = window.setInterval(refreshSlow, REMOTE_SLOW_REFRESH_MS);
    return () => {
      window.clearInterval(fast);
      window.clearInterval(slow);
    };
  }, [isActive, isVisible, refreshFast, refreshSlow, refreshTick, rpc]);

  const setEasyConnectState = useCallback(
    (update: GxserverTailcatStateUpdate) => {
      if (!rpc) {
        return;
      }
      void rpc('/api/updateTailcatState', { ...update })
        .then((result) => {
          if (mountedRef.current) {
            setEasyConnect(readTailcatStatusResult(result));
            setRequestError(undefined);
          }
          // The address is published a moment after the sidecar starts.
          refreshFast();
        })
        .catch(applyError);
    },
    [applyError, refreshFast, rpc]
  );

  const enableSshAccess = useCallback(() => {
    if (!rpc || isEnablingSsh) {
      return;
    }
    setIsEnablingSsh(true);
    setSshEnableAttempt(undefined);
    void rpc('/api/enableSshAccess', {})
      .then((result) => {
        const enabled = result as GxserverEnableSshAccessResult;
        if (!mountedRef.current) {
          return;
        }
        setAccess((current) => (current ? { ...current, ssh: enabled.ssh } : current));
        setSshEnableAttempt(
          enabled.ssh.enabled ? undefined : { message: enabled.message ?? null, outcome: enabled.outcome }
        );
      })
      .catch((error: unknown) => {
        if (mountedRef.current) {
          setSshEnableAttempt({ message: error instanceof Error ? error.message : String(error), outcome: 'failed' });
        }
      })
      .finally(() => {
        if (mountedRef.current) {
          setIsEnablingSsh(false);
        }
      });
  }, [isEnablingSsh, rpc]);

  const removePairedDevice = useCallback(
    (deviceId: string) => {
      if (!rpc) {
        return;
      }
      setRemovingDeviceId(deviceId);
      void rpc('/api/removePairedDevice', { deviceId })
        .then((result) => {
          if (mountedRef.current) {
            setPairedDevices(readPairedDevicesResult(result));
            setRequestError(undefined);
          }
        })
        .catch(applyError)
        .finally(() => {
          if (mountedRef.current) {
            setRemovingDeviceId(undefined);
          }
        });
    },
    [applyError, rpc]
  );

  const refresh = useCallback(() => setRefreshTick((tick) => tick + 1), []);

  return {
    access,
    easyConnect,
    enableSshAccess,
    isEnablingSsh,
    pairedDevices,
    pairingCode,
    refresh,
    removePairedDevice,
    removingDeviceId,
    requestError,
    setEasyConnectState,
    sshEnableAttempt,
  };
}
