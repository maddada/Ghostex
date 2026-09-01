import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { toDataURL } from 'qrcode';
import { Button } from '@/packages/components/ui/button';
import { Switch } from '@/packages/components/ui/switch';
import {
  IconAlertTriangle,
  IconChevronDown,
  IconChevronRight,
  IconCircleCheckFilled,
  IconCopy,
} from '@tabler/icons-react';
import {
  GXSERVER_PROTOCOL_VERSION,
  type GxserverProtocolVersion,
  type GxserverTailcatStateUpdate,
  type GxserverTailcatStatus,
} from '@/packages/shared/gxserver-protocol';
import { AppTooltip } from '../../app-tooltip';
import { SettingsInput, SettingsTextarea } from '../fields';

/**
 * CDXC:Tailcat 2026-08-31:
 * The Tailcat panel talks to the daemon that owns the sidecar, so it takes one
 * narrow RPC callback instead of a whole client. Each host supplies the
 * callback for the gxserver it is already connected to: the desktop Settings
 * window from its injected bootstrap, the web app from its local machine
 * connection. Where no daemon is reachable the callback is absent and the whole
 * section is left out, exactly like the Extensions store.
 */
export type TailcatSettingsRpc = (
  path: '/api/tailcatStatus' | '/api/updateTailcatState',
  params: Record<string, unknown>
) => Promise<unknown>;

export const TAILCAT_INSTALL_COMMAND = 'go install github.com/tailscale/tailcat/cmd/tailcat@latest';

export const TAILCAT_STATUS_REFRESH_MS = 4000;

export const TAILCAT_MIN_PORT = 1;
export const TAILCAT_MAX_PORT = 65535;

type GxserverBootstrap = {
  authToken: string;
  baseUrl: string;
  protocolVersion?: GxserverProtocolVersion;
};

function gpuiGxserverBootstrap(): GxserverBootstrap | undefined {
  const candidate = (window as unknown as { ghostexGpui?: { gxserverBootstrap?: unknown } }).ghostexGpui
    ?.gxserverBootstrap;
  if (!candidate || typeof candidate !== 'object') {
    return undefined;
  }
  const bootstrap = candidate as Partial<GxserverBootstrap>;
  if (
    typeof bootstrap.authToken !== 'string' ||
    !bootstrap.authToken.trim() ||
    typeof bootstrap.baseUrl !== 'string' ||
    !bootstrap.baseUrl.trim() ||
    (bootstrap.protocolVersion !== undefined && bootstrap.protocolVersion !== GXSERVER_PROTOCOL_VERSION)
  ) {
    return undefined;
  }
  return bootstrap as GxserverBootstrap;
}

const GPUI_BOOTSTRAP_TAILCAT_RPC: TailcatSettingsRpc = async (path, params) => {
  const bootstrap = gpuiGxserverBootstrap();
  if (!bootstrap) {
    throw new Error('The Ghostex server connection is unavailable.');
  }
  const response = await fetch(`${bootstrap.baseUrl}${path}`, {
    body: JSON.stringify({ params, protocolVersion: GXSERVER_PROTOCOL_VERSION }),
    headers: {
      authorization: `Bearer ${bootstrap.authToken}`,
      'content-type': 'application/json',
      'x-gxserver-protocol-version': String(GXSERVER_PROTOCOL_VERSION),
    },
    method: 'POST',
  });
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    body = undefined;
  }
  const envelope = body as { error?: { message?: string }; ok?: boolean; result?: unknown } | undefined;
  if (!response.ok || envelope?.ok !== true) {
    throw new Error(
      typeof envelope?.error?.message === 'string'
        ? envelope.error.message
        : `gxserver rejected ${path} (${response.status || 'no response'}).`
    );
  }
  return envelope.result;
};

/**
 * The desktop Settings window is a CEF page with the sidebar gxserver bootstrap
 * installed on it, so it can reach the local daemon without any host round
 * trip. Returns undefined while this page has no bootstrap; the identity is
 * stable so the caller may re-ask on every render once one arrives.
 */
export function gpuiBootstrapTailcatRpc(): TailcatSettingsRpc | undefined {
  return gpuiGxserverBootstrap() ? GPUI_BOOTSTRAP_TAILCAT_RPC : undefined;
}

export function readTailcatStatusResult(value: unknown): GxserverTailcatStatus {
  const status = (value as { status?: unknown } | undefined)?.status;
  if (!status || typeof status !== 'object' || typeof (status as GxserverTailcatStatus).enabled !== 'boolean') {
    throw new Error('gxserver returned an unreadable tailcat status.');
  }
  return status as GxserverTailcatStatus;
}

export function parseTailcatPortsInput(value: string): readonly number[] | undefined {
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
    if (port < TAILCAT_MIN_PORT || port > TAILCAT_MAX_PORT) {
      return undefined;
    }
    if (!ports.includes(port)) {
      ports.push(port);
    }
  }
  return ports;
}

export function formatTailcatPorts(ports: readonly number[]): string {
  return ports.join(', ');
}

export function parseTailcatAllowedClientKeys(value: string): readonly string[] {
  const keys: string[] = [];
  for (const line of value.split('\n')) {
    const key = line.trim();
    if (key.length > 0 && !keys.includes(key)) {
      keys.push(key);
    }
  }
  return keys;
}

export function formatTailcatAllowedClientKeys(keys: readonly string[]): string {
  return keys.join('\n');
}

export function getTailcatStatusBadge(status: GxserverTailcatStatus | undefined): {
  label: string;
  tone: 'active' | 'disabled' | 'failed' | 'needsSetup' | 'unknown';
} {
  if (!status) {
    return { label: 'Unknown', tone: 'unknown' };
  }
  if (!status.binaryFound) {
    return { label: 'Not installed', tone: 'needsSetup' };
  }
  if (!status.enabled) {
    return { label: 'Disabled', tone: 'disabled' };
  }
  if (status.running) {
    return { label: 'Running', tone: 'active' };
  }
  return { label: status.lastError ? 'Failed' : 'Stopped', tone: status.lastError ? 'failed' : 'unknown' };
}

export function TailcatSettingsPanel({ isActive, rpc }: { isActive: boolean; rpc: TailcatSettingsRpc }) {
  const enabledToggleId = useId();
  const portsFieldId = useId();
  const allowedKeysFieldId = useId();
  const [status, setStatus] = useState<GxserverTailcatStatus | undefined>(undefined);
  const [requestError, setRequestError] = useState<string | undefined>(undefined);
  const [portsDraft, setPortsDraft] = useState<string | undefined>(undefined);
  const [portsError, setPortsError] = useState<string | undefined>(undefined);
  const [allowedKeysDraft, setAllowedKeysDraft] = useState<string | undefined>(undefined);
  const [allowedKeysOpen, setAllowedKeysOpen] = useState(false);
  const [qrDataUrl, setQrDataUrl] = useState<string | undefined>(undefined);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const applyStatus = useCallback((next: GxserverTailcatStatus) => {
    if (!mountedRef.current) {
      return;
    }
    setStatus(next);
    setRequestError(undefined);
  }, []);

  const applyError = useCallback((error: unknown) => {
    if (!mountedRef.current) {
      return;
    }
    setRequestError(error instanceof Error ? error.message : String(error));
  }, []);

  /*
   * CDXC:Tailcat 2026-08-31:
   * The sidecar is supervised outside this page, so the panel re-reads the
   * status on a slow interval while the Remote tab is open and stops the timer
   * as soon as the tab is left or the modal closes.
   */
  useEffect(() => {
    if (!isActive) {
      return;
    }
    let cancelled = false;
    const refresh = () => {
      void rpc('/api/tailcatStatus', {})
        .then((result) => {
          if (!cancelled) {
            applyStatus(readTailcatStatusResult(result));
          }
        })
        .catch((error: unknown) => {
          if (!cancelled) {
            applyError(error);
          }
        });
    };
    refresh();
    const interval = window.setInterval(refresh, TAILCAT_STATUS_REFRESH_MS);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [applyError, applyStatus, isActive, rpc]);

  const token = status?.token ?? undefined;

  useEffect(() => {
    if (!token) {
      setQrDataUrl(undefined);
      return;
    }
    let cancelled = false;
    toDataURL(token, {
      color: { dark: '#111113', light: '#f4f4f5' },
      errorCorrectionLevel: 'M',
      margin: 2,
      width: 168,
    })
      .then((dataUrl) => {
        if (!cancelled) {
          setQrDataUrl(dataUrl);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setQrDataUrl(undefined);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [token]);

  const update = (stateUpdate: GxserverTailcatStateUpdate) => {
    void rpc('/api/updateTailcatState', { ...stateUpdate })
      .then((result) => applyStatus(readTailcatStatusResult(result)))
      .catch((error: unknown) => applyError(error));
  };

  const commitPorts = () => {
    if (portsDraft === undefined) {
      return;
    }
    const ports = parseTailcatPortsInput(portsDraft);
    if (!ports) {
      setPortsError(`Ports must be numbers between ${TAILCAT_MIN_PORT} and ${TAILCAT_MAX_PORT}, separated by commas.`);
      return;
    }
    setPortsError(undefined);
    setPortsDraft(undefined);
    if (status && formatTailcatPorts(status.ports) === formatTailcatPorts(ports)) {
      return;
    }
    update({ kind: 'setPorts', ports });
  };

  const commitAllowedClientKeys = () => {
    if (allowedKeysDraft === undefined) {
      return;
    }
    const allowedClientKeys = parseTailcatAllowedClientKeys(allowedKeysDraft);
    setAllowedKeysDraft(undefined);
    if (status && formatTailcatAllowedClientKeys(status.allowedClientKeys) === allowedClientKeys.join('\n')) {
      return;
    }
    update({ allowedClientKeys, kind: 'setAllowedClientKeys' });
  };

  const badge = getTailcatStatusBadge(status);
  const binaryFound = status?.binaryFound === true;
  const portsValue = portsDraft ?? formatTailcatPorts(status?.ports ?? []);
  const allowedKeysValue = allowedKeysDraft ?? formatTailcatAllowedClientKeys(status?.allowedClientKeys ?? []);

  return (
    <section className='settings-modal-section settings-tailcat-panel'>
      <div className='settings-tailcat-header'>
        <div className='settings-management-header-text'>
          <h3 className='settings-management-heading'>Tailcat</h3>
          <p className='settings-management-description'>
            Reach this machine from another device without the Tailscale VPN: pair once with the address below and the
            pairing keeps working until you delete its key.
          </p>
        </div>
        <span className='settings-tailcat-status-badge' data-status={badge.tone}>
          {badge.label}
        </span>
      </div>
      <div className='settings-tailcat-body'>
        <div className='settings-tailcat-row'>
          <div className='settings-management-main'>
            <label className='settings-management-title' htmlFor={enabledToggleId}>
              Enable Tailcat
            </label>
            <span className='settings-management-detail'>
              {binaryFound
                ? status?.running
                  ? 'The tailcat sidecar is running and accepting paired clients.'
                  : 'Turn on to run the tailcat sidecar alongside gxserver.'
                : 'Install the tailcat binary to enable this.'}
            </span>
          </div>
          <Switch
            aria-label='Enable Tailcat remote access'
            checked={status?.enabled === true}
            disabled={!status || !binaryFound}
            id={enabledToggleId}
            onCheckedChange={(checked) => update({ enabled: checked, kind: 'setEnabled' })}
          />
        </div>

        {status && !binaryFound ? (
          <div className='settings-tailcat-install'>
            <div className='settings-management-main'>
              <span className='settings-management-title'>tailcat is not installed</span>
              <span className='settings-management-detail'>
                gxserver could not find a tailcat binary on this machine. Install it, then reopen this page.
              </span>
            </div>
            <div className='settings-tailcat-command-row'>
              <code className='settings-tailcat-command'>{TAILCAT_INSTALL_COMMAND}</code>
              <TailcatCopyButton
                copyLabel='Copy the tailcat install command'
                value={TAILCAT_INSTALL_COMMAND}
                variant='outline'
              />
            </div>
          </div>
        ) : null}

        {binaryFound && status?.binaryPath ? (
          <div className='settings-tailcat-row settings-tailcat-binary-row'>
            <div className='settings-management-main'>
              <span className='settings-management-title'>Binary</span>
              <span className='settings-management-detail'>
                {status.binaryVersion ? `${status.binaryPath} (${status.binaryVersion})` : status.binaryPath}
              </span>
            </div>
          </div>
        ) : null}

        {status?.enabled && token ? (
          <div className='settings-tailcat-pairing'>
            <div className='settings-management-main'>
              <span className='settings-management-title'>Pairing address</span>
              <span className='settings-management-detail'>
                Scan this on the other device, or paste the address into its Ghostex connection field.
              </span>
            </div>
            <div className='settings-tailcat-pairing-body'>
              {qrDataUrl ? (
                <img alt='Tailcat pairing address QR code' className='settings-tailcat-qr' src={qrDataUrl} />
              ) : null}
              <div className='settings-tailcat-token-row'>
                <code className='settings-tailcat-token'>{token}</code>
                <TailcatCopyButton copyLabel='Copy the Tailcat pairing address' value={token} variant='secondary' />
              </div>
            </div>
          </div>
        ) : null}

        <div className='settings-tailcat-row settings-tailcat-ports-row'>
          <div className='settings-management-main'>
            <label className='settings-management-title' htmlFor={portsFieldId}>
              Served ports
            </label>
            <span className='settings-management-detail'>
              Comma-separated local ports tailcat exposes to paired clients.
            </span>
          </div>
          <SettingsInput
            aria-label='Tailcat served ports'
            className='settings-tailcat-ports-input'
            disabled={!status}
            id={portsFieldId}
            inputMode='numeric'
            onBlur={commitPorts}
            onChange={(event) => {
              setPortsDraft(event.currentTarget.value);
              setPortsError(undefined);
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                commitPorts();
              }
            }}
            placeholder='4310'
            value={portsValue}
          />
        </div>
        {portsError ? <p className='settings-tailcat-field-error'>{portsError}</p> : null}

        <div className='settings-tailcat-keys'>
          <Button
            aria-expanded={allowedKeysOpen}
            className='settings-tailcat-keys-toggle'
            onClick={() => setAllowedKeysOpen((open) => !open)}
            type='button'
            variant='ghost'
          >
            {allowedKeysOpen ? <IconChevronDown aria-hidden='true' /> : <IconChevronRight aria-hidden='true' />}
            Allowed client keys
          </Button>
          {allowedKeysOpen ? (
            <div className='settings-tailcat-keys-body'>
              <label className='settings-management-detail' htmlFor={allowedKeysFieldId}>
                One client key per line. Leave this empty to allow any client that knows the pairing address.
              </label>
              <SettingsTextarea
                aria-label='Tailcat allowed client keys'
                className='settings-tailcat-keys-textarea'
                disabled={!status}
                id={allowedKeysFieldId}
                onBlur={commitAllowedClientKeys}
                onChange={(event) => setAllowedKeysDraft(event.currentTarget.value)}
                rows={4}
                value={allowedKeysValue}
              />
            </div>
          ) : null}
        </div>

        {status?.lastError ? (
          <div className='settings-tailcat-error'>
            <IconAlertTriangle aria-hidden='true' />
            <span>{status.lastError}</span>
          </div>
        ) : null}
        {requestError ? (
          <div className='settings-tailcat-error'>
            <IconAlertTriangle aria-hidden='true' />
            <span>{requestError}</span>
          </div>
        ) : null}
      </div>
    </section>
  );
}

function TailcatCopyButton({
  copyLabel,
  value,
  variant,
}: {
  copyLabel: string;
  value: string;
  variant: 'outline' | 'secondary';
}) {
  const [copied, setCopied] = useState(false);
  return (
    <AppTooltip content={copied ? 'Copied' : copyLabel}>
      <Button
        aria-label={copyLabel}
        onClick={() => {
          void navigator.clipboard.writeText(value).then(
            () => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1200);
            },
            () => undefined
          );
        }}
        size='icon'
        type='button'
        variant={variant}
      >
        {copied ? <IconCircleCheckFilled aria-hidden='true' /> : <IconCopy aria-hidden='true' />}
      </Button>
    </AppTooltip>
  );
}
