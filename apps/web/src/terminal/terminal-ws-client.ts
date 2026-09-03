import {
  GXSERVER_TERMINAL_WS_ENDPOINT,
  type GxserverProjectId,
  type GxserverSessionId,
  type GxserverTerminalWsErrorCode,
  type GxserverTerminalWsErrorMessage,
  type GxserverTerminalWsExitMessage,
  type GxserverTerminalWsReadyMessage,
  type GxserverTerminalWsServerControlMessage,
  type GxserverZmxSessionName,
} from '@/packages/shared/gxserver-protocol';

const TERMINAL_PROTOCOL_VERSION = '1';
const RESIZE_COALESCE_MS = 50;
const RECONNECT_DELAYS_MS = [1_000, 2_000, 4_000, 8_000, 16_000] as const;
const TERMINAL_ERROR_CODES = new Set<GxserverTerminalWsErrorCode>([
  'unauthorized',
  'protocolMismatch',
  'notFound',
  'providerNotRunning',
]);

interface TerminalSize {
  cols: number;
  rows: number;
}

export type TerminalWsClientError = GxserverTerminalWsErrorMessage | Error;

export interface TerminalWsClientOptions extends TerminalSize {
  authToken: string;
  baseUrl: string;
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  onError?(error: TerminalWsClientError): void;
  onExit?(message: GxserverTerminalWsExitMessage): void;
  onOutput?(bytes: Uint8Array): void;
  onReady?(message: GxserverTerminalWsReadyMessage): void;
  onReconnect?(): void;
}

function requireTerminalDimension(value: number, name: 'cols' | 'rows'): number {
  if (!Number.isInteger(value) || value < 1) {
    throw new RangeError(`Terminal ${name} must be a positive integer.`);
  }
  return value;
}

function createTerminalUrl(options: TerminalWsClientOptions, size: TerminalSize): string {
  const url = new URL(GXSERVER_TERMINAL_WS_ENDPOINT, options.baseUrl);
  if (url.protocol === 'http:') {
    url.protocol = 'ws:';
  } else if (url.protocol === 'https:') {
    url.protocol = 'wss:';
  } else if (url.protocol !== 'ws:' && url.protocol !== 'wss:') {
    throw new TypeError('Terminal baseUrl must use http, https, ws, or wss.');
  }
  url.searchParams.set('authToken', options.authToken);
  url.searchParams.set('protocolVersion', TERMINAL_PROTOCOL_VERSION);
  url.searchParams.set('projectId', options.projectId);
  url.searchParams.set('sessionId', options.sessionId);
  url.searchParams.set('cols', String(size.cols));
  url.searchParams.set('rows', String(size.rows));
  return url.toString();
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function parseServerControlMessage(value: string): GxserverTerminalWsServerControlMessage {
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    throw new Error('Terminal WebSocket sent malformed JSON control data.');
  }
  if (!isRecord(parsed) || typeof parsed.type !== 'string') {
    throw new Error('Terminal WebSocket sent an invalid control message.');
  }
  if (
    parsed.type === 'ready' &&
    typeof parsed.zmxName === 'string' &&
    Number.isInteger(parsed.cols) &&
    Number(parsed.cols) > 0 &&
    Number.isInteger(parsed.rows) &&
    Number(parsed.rows) > 0
  ) {
    return {
      cols: Number(parsed.cols),
      rows: Number(parsed.rows),
      type: 'ready',
      zmxName: parsed.zmxName as GxserverZmxSessionName,
    };
  }
  if (parsed.type === 'exit' && (parsed.code === null || Number.isInteger(parsed.code))) {
    return {
      code: parsed.code === null ? null : Number(parsed.code),
      type: 'exit',
    };
  }
  if (
    parsed.type === 'error' &&
    typeof parsed.code === 'string' &&
    TERMINAL_ERROR_CODES.has(parsed.code as GxserverTerminalWsErrorCode) &&
    typeof parsed.message === 'string'
  ) {
    return {
      code: parsed.code as GxserverTerminalWsErrorCode,
      message: parsed.message,
      type: 'error',
    };
  }
  throw new Error(`Terminal WebSocket sent an invalid ${parsed.type} control message.`);
}

export class TerminalWsClient {
  private currentSize: TerminalSize;
  private lastSentSize: TerminalSize | null = null;
  private pendingSize: TerminalSize | null = null;
  private ready = false;
  private reconnectAttempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private resizeTimer: ReturnType<typeof setTimeout> | null = null;
  private socket: WebSocket | null = null;
  private terminalOutcomeReceived = false;
  /*
  The last announced visibility, re-sent after every (re)connect because a
  fresh `zmx attach` starts out as a displaying client; `visibilityPending`
  is set whenever the server has not yet heard the current value.
  */
  private visibility: { hidden: boolean; size: TerminalSize } | null = null;
  private visibilityPending = false;
  private wantsConnection = true;

  constructor(private readonly options: TerminalWsClientOptions) {
    this.currentSize = {
      cols: requireTerminalDimension(options.cols, 'cols'),
      rows: requireTerminalDimension(options.rows, 'rows'),
    };
    this.openSocket();
  }

  sendInput(input: Uint8Array | string): boolean {
    const socket = this.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN || !this.ready) {
      return false;
    }
    const bytes = typeof input === 'string' ? new TextEncoder().encode(input) : input;
    socket.send(bytes);
    return true;
  }

  resize(cols: number, rows: number): void {
    const nextSize = {
      cols: requireTerminalDimension(cols, 'cols'),
      rows: requireTerminalDimension(rows, 'rows'),
    };
    if (nextSize.cols === this.currentSize.cols && nextSize.rows === this.currentSize.rows) {
      return;
    }
    this.currentSize = nextSize;
    this.pendingSize = nextSize;
    if (this.resizeTimer) {
      clearTimeout(this.resizeTimer);
    }
    this.resizeTimer = setTimeout(() => {
      this.resizeTimer = null;
      this.flushResize();
    }, RESIZE_COALESCE_MS);
  }

  /**
   * Tell gxserver whether this client is displaying the session. The server
   * resizes the pty to `size` and hands zmx the matching ZMX_HIDDEN /
   * ZMX_VISIBLE sequence, so `size` is the grid the local xterm now has: the
   * real fitted size when visible, 200 columns wide when hidden. It supersedes
   * any coalesced resize still waiting to be flushed.
   */
  setVisibility(hidden: boolean, size: TerminalSize): void {
    const nextSize = {
      cols: requireTerminalDimension(size.cols, 'cols'),
      rows: requireTerminalDimension(size.rows, 'rows'),
    };
    this.visibility = { hidden, size: nextSize };
    this.visibilityPending = true;
    this.currentSize = nextSize;
    this.pendingSize = null;
    if (this.resizeTimer) {
      clearTimeout(this.resizeTimer);
      this.resizeTimer = null;
    }
    this.flushVisibility();
  }

  close(): void {
    this.wantsConnection = false;
    this.ready = false;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.resizeTimer) {
      clearTimeout(this.resizeTimer);
      this.resizeTimer = null;
    }
    const socket = this.socket;
    this.socket = null;
    if (socket && socket.readyState < WebSocket.CLOSING) {
      socket.close(1000, 'terminal detached');
    }
  }

  private openSocket(): void {
    if (!this.wantsConnection) {
      return;
    }
    let socket: WebSocket;
    try {
      socket = new WebSocket(createTerminalUrl(this.options, this.currentSize));
    } catch (error) {
      this.wantsConnection = false;
      this.options.onError?.(error instanceof Error ? error : new Error('Unable to open terminal WebSocket.'));
      return;
    }
    socket.binaryType = 'arraybuffer';
    this.socket = socket;
    this.ready = false;
    this.terminalOutcomeReceived = false;
    this.lastSentSize = { ...this.currentSize };
    this.visibilityPending = this.visibility !== null;

    socket.onmessage = (event) => {
      if (this.socket !== socket) {
        return;
      }
      if (typeof event.data === 'string') {
        this.handleControlMessage(socket, event.data);
        return;
      }
      if (event.data instanceof ArrayBuffer) {
        this.options.onOutput?.(new Uint8Array(event.data));
        return;
      }
      this.failProtocol(socket, new Error('Terminal WebSocket sent a non-binary output frame.'));
    };
    socket.onerror = () => {
      if (this.socket === socket && this.wantsConnection) {
        this.options.onError?.(new Error('Terminal WebSocket transport failed.'));
      }
    };
    socket.onclose = (event) => {
      if (this.socket !== socket) {
        return;
      }
      this.socket = null;
      this.ready = false;
      const abnormalClose = !event.wasClean || event.code !== 1000;
      if (this.wantsConnection && !this.terminalOutcomeReceived && abnormalClose) {
        this.scheduleReconnect();
      }
    };
  }

  private handleControlMessage(socket: WebSocket, rawMessage: string): void {
    let message: GxserverTerminalWsServerControlMessage;
    try {
      message = parseServerControlMessage(rawMessage);
    } catch (error) {
      this.failProtocol(socket, error instanceof Error ? error : new Error('Terminal WebSocket protocol failed.'));
      return;
    }
    if (message.type === 'ready') {
      this.ready = true;
      this.reconnectAttempt = 0;
      this.options.onReady?.(message);
      this.flushVisibility();
      this.flushResize();
      return;
    }
    this.terminalOutcomeReceived = true;
    this.ready = false;
    if (message.type === 'exit') {
      this.options.onExit?.(message);
    } else {
      this.options.onError?.(message);
    }
    if (socket.readyState < WebSocket.CLOSING) {
      socket.close(1000, 'terminal complete');
    }
  }

  private failProtocol(socket: WebSocket, error: Error): void {
    this.terminalOutcomeReceived = true;
    this.ready = false;
    this.options.onError?.(error);
    if (socket.readyState < WebSocket.CLOSING) {
      socket.close(1002, 'invalid terminal protocol');
    }
  }

  private flushVisibility(): void {
    const visibility = this.visibility;
    const socket = this.socket;
    if (!this.visibilityPending || !visibility || !socket || socket.readyState !== WebSocket.OPEN || !this.ready) {
      return;
    }
    socket.send(JSON.stringify({ ...visibility.size, hidden: visibility.hidden, type: 'visibility' }));
    this.lastSentSize = visibility.size;
    this.visibilityPending = false;
  }

  private flushResize(): void {
    const pendingSize = this.pendingSize;
    const socket = this.socket;
    if (!pendingSize || !socket || socket.readyState !== WebSocket.OPEN || !this.ready) {
      return;
    }
    if (this.lastSentSize?.cols === pendingSize.cols && this.lastSentSize.rows === pendingSize.rows) {
      this.pendingSize = null;
      return;
    }
    socket.send(JSON.stringify({ ...pendingSize, type: 'resize' }));
    this.lastSentSize = pendingSize;
    this.pendingSize = null;
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer) {
      return;
    }
    const delay = RECONNECT_DELAYS_MS[Math.min(this.reconnectAttempt, RECONNECT_DELAYS_MS.length - 1)];
    this.reconnectAttempt += 1;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (!this.wantsConnection) {
        return;
      }
      this.options.onReconnect?.();
      this.openSocket();
    }, delay);
  }
}
