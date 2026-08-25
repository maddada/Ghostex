import { IconChevronDown, IconChevronUp, IconLoader2, IconPuzzle, IconX } from '@tabler/icons-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/packages/components/ui/dropdown-menu';
import type {
  GhostexChatBarBridgeChunkMessage,
  GhostexChatBarBridgeContextChangedMessage,
  GhostexChatBarBridgeReadyMessage,
  GhostexChatBarBridgeRequestMessage,
  GhostexChatBarBridgeResponseMessage,
} from '@/packages/shared/ghostex-extensions';
import {
  GHOSTEX_CHAT_BAR_BRIDGE_VERSION,
  GHOSTEX_CHAT_BAR_CONTEXT_CHANGED_EVENT,
} from '@/packages/shared/ghostex-extensions';

export interface SessionChatBarExtension {
  id: string;
  title: string;
  iconUrl?: string;
  url?: string;
  error?: string;
}

export interface SessionChatExtensionPanelProps {
  activeExtensionId?: string;
  extensions: readonly SessionChatBarExtension[];
  minimized: boolean;
  onActiveExtensionChange: (extensionId: string) => void;
  onBridgeRequest?: (
    extensionId: string,
    request: GhostexChatBarBridgeRequestMessage,
    onChunk: (chunk: GhostexChatBarBridgeChunkMessage['chunk']) => void
  ) => Promise<unknown>;
  onClose: () => void;
  onMinimizedChange: (minimized: boolean) => void;
}

function bridgeError(error: unknown): NonNullable<GhostexChatBarBridgeResponseMessage['error']> {
  if (error && typeof error === 'object') {
    const candidate = error as {
      code?: unknown;
      message?: unknown;
      permission?: unknown;
    };
    const code =
      candidate.code === 'invalidRequest' ||
      candidate.code === 'notFound' ||
      candidate.code === 'permissionDenied' ||
      candidate.code === 'operationFailed'
        ? candidate.code
        : 'operationFailed';
    return {
      code,
      message: typeof candidate.message === 'string' ? candidate.message : 'The extension call failed.',
      ...(candidate.permission === 'exec' ||
      candidate.permission === 'cli' ||
      candidate.permission === 'ssh' ||
      candidate.permission === 'network' ||
      candidate.permission === 'clipboard'
        ? { permission: candidate.permission }
        : {}),
    };
  }
  return {
    code: 'operationFailed',
    message: error instanceof Error ? error.message : 'The extension call failed.',
  };
}

function isBridgeRequest(value: unknown): value is GhostexChatBarBridgeRequestMessage {
  if (!value || typeof value !== 'object') {
    return false;
  }
  const candidate = value as Partial<GhostexChatBarBridgeRequestMessage>;
  return (
    candidate.type === 'ghostexChatBarBridgeRequest' &&
    candidate.bridgeVersion === GHOSTEX_CHAT_BAR_BRIDGE_VERSION &&
    typeof candidate.requestId === 'string' &&
    candidate.requestId.length > 0 &&
    candidate.requestId.length <= 128 &&
    (candidate.method === 'context' ||
      candidate.method === 'cli' ||
      candidate.method === 'exec' ||
      candidate.method === 'settings.get' ||
      candidate.method === 'settings.set' ||
      candidate.method === 'storage.get' ||
      candidate.method === 'storage.set' ||
      candidate.method === 'ui.toast' ||
      candidate.method === 'ui.close' ||
      candidate.method === 'ui.setBadge')
  );
}

function extensionOrigin(url: string | undefined): string | undefined {
  if (!url) {
    return undefined;
  }
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'http:' && parsed.hostname === '127.0.0.1' ? parsed.origin : undefined;
  } catch {
    return undefined;
  }
}

function ExtensionIcon({ extension }: { extension: SessionChatBarExtension }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [extension.iconUrl]);
  return extension.iconUrl && !failed ? (
    <img
      alt=''
      className='ghostex-chat-extension-icon'
      draggable={false}
      onError={() => setFailed(true)}
      src={extension.iconUrl}
    />
  ) : (
    <IconPuzzle aria-hidden='true' className='ghostex-chat-extension-icon' stroke={1.8} />
  );
}

export function SessionChatExtensionPanel({
  activeExtensionId,
  extensions,
  minimized,
  onActiveExtensionChange,
  onBridgeRequest,
  onClose,
  onMinimizedChange,
}: SessionChatExtensionPanelProps) {
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const activeExtension = useMemo(
    () => extensions.find((extension) => extension.id === activeExtensionId) ?? extensions[0],
    [activeExtensionId, extensions]
  );
  const activeOrigin = extensionOrigin(activeExtension?.url);

  const postToFrame = useCallback(
    (
      message:
        | GhostexChatBarBridgeChunkMessage
        | GhostexChatBarBridgeContextChangedMessage
        | GhostexChatBarBridgeResponseMessage
        | GhostexChatBarBridgeReadyMessage
    ) => {
      const frameWindow = iframeRef.current?.contentWindow;
      if (frameWindow && activeOrigin) {
        frameWindow.postMessage(message, activeOrigin);
      }
    },
    [activeOrigin]
  );

  useEffect(() => {
    const handleContextChanged = (event: Event): void => {
      const context = (event as CustomEvent<GhostexChatBarBridgeContextChangedMessage['context']>).detail;
      if (!context) {
        return;
      }
      postToFrame({
        type: 'ghostexChatBarBridgeContextChanged',
        bridgeVersion: GHOSTEX_CHAT_BAR_BRIDGE_VERSION,
        context,
      });
    };
    window.addEventListener(GHOSTEX_CHAT_BAR_CONTEXT_CHANGED_EVENT, handleContextChanged);
    return () => window.removeEventListener(GHOSTEX_CHAT_BAR_CONTEXT_CHANGED_EVENT, handleContextChanged);
  }, [postToFrame]);

  useEffect(() => {
    if (!activeExtension || !activeOrigin) {
      return;
    }
    const handleMessage = (event: MessageEvent<unknown>): void => {
      if (
        event.source !== iframeRef.current?.contentWindow ||
        event.origin !== activeOrigin ||
        !isBridgeRequest(event.data)
      ) {
        return;
      }
      const request = event.data;
      const requestWindow = event.source;
      const requestOrigin = event.origin;
      const respond = (response: Omit<GhostexChatBarBridgeResponseMessage, 'bridgeVersion' | 'requestId' | 'type'>) => {
        requestWindow?.postMessage(
          {
            type: 'ghostexChatBarBridgeResponse',
            bridgeVersion: GHOSTEX_CHAT_BAR_BRIDGE_VERSION,
            requestId: request.requestId,
            ...response,
          } satisfies GhostexChatBarBridgeResponseMessage,
          { targetOrigin: requestOrigin }
        );
      };
      if (!onBridgeRequest) {
        respond({ ok: false, error: { code: 'operationFailed', message: 'The chat-bar bridge is unavailable.' } });
        return;
      }
      const onChunk = (chunk: GhostexChatBarBridgeChunkMessage['chunk']): void => {
        requestWindow?.postMessage(
          {
            type: 'ghostexChatBarBridgeChunk',
            bridgeVersion: GHOSTEX_CHAT_BAR_BRIDGE_VERSION,
            requestId: request.requestId,
            chunk,
          } satisfies GhostexChatBarBridgeChunkMessage,
          { targetOrigin: requestOrigin }
        );
      };
      void onBridgeRequest(activeExtension.id, request, onChunk)
        .then((result) => {
          respond({ ok: true, result });
          if (request.method === 'ui.close') {
            onClose();
          }
        })
        .catch((error: unknown) => respond({ ok: false, error: bridgeError(error) }));
    };
    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, [activeExtension, activeOrigin, onBridgeRequest, onClose, postToFrame]);

  if (!activeExtension) {
    return null;
  }

  return (
    <section
      aria-label={`${activeExtension.title} chat extension`}
      className='ghostex-chat-extension-panel'
      data-minimized={minimized ? 'true' : 'false'}
    >
      <header className='ghostex-chat-extension-header'>
        <ExtensionIcon extension={activeExtension} />
        {extensions.length > 1 ? (
          <DropdownMenu>
            <DropdownMenuTrigger aria-label='Switch chat extension' className='ghostex-chat-extension-switcher'>
              <span className='truncate'>{activeExtension.title}</span>
              <IconChevronDown aria-hidden='true' className='size-3.5 shrink-0' stroke={2} />
            </DropdownMenuTrigger>
            <DropdownMenuContent align='start' className='min-w-48'>
              {extensions.map((extension) => (
                <DropdownMenuItem key={extension.id} onClick={() => onActiveExtensionChange(extension.id)}>
                  <ExtensionIcon extension={extension} />
                  <span className='truncate'>{extension.title}</span>
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        ) : (
          <span className='ghostex-chat-extension-title'>{activeExtension.title}</span>
        )}
        <div className='ml-auto flex items-center'>
          <Button
            aria-label={minimized ? 'Expand chat extension' : 'Minimize chat extension'}
            className='ghostex-chat-extension-header-button'
            onClick={() => onMinimizedChange(!minimized)}
            size='icon-sm'
            variant='ghost'
          >
            {minimized ? (
              <IconChevronUp aria-hidden='true' className='size-3.5' stroke={2} />
            ) : (
              <IconChevronDown aria-hidden='true' className='size-3.5' stroke={2} />
            )}
          </Button>
          <Button
            aria-label='Close chat extension'
            className='ghostex-chat-extension-header-button'
            onClick={onClose}
            size='icon-sm'
            variant='ghost'
          >
            <IconX aria-hidden='true' className='size-3.5' stroke={2} />
          </Button>
        </div>
      </header>
      {!minimized ? (
        <div className='ghostex-chat-extension-body'>
          {activeExtension.url ? (
            <iframe
              className='ghostex-chat-extension-frame'
              key={activeExtension.id}
              onLoad={() =>
                postToFrame({
                  type: 'ghostexChatBarBridgeReady',
                  bridgeVersion: GHOSTEX_CHAT_BAR_BRIDGE_VERSION,
                })
              }
              ref={iframeRef}
              src={activeExtension.url}
              title={activeExtension.title}
            />
          ) : activeExtension.error ? (
            <div className='ghostex-chat-extension-status' role='alert'>
              {activeExtension.error}
            </div>
          ) : (
            <div aria-label={`Loading ${activeExtension.title}`} className='ghostex-chat-extension-status'>
              <IconLoader2 aria-hidden='true' className='size-4 animate-spin' stroke={2} />
              Loading extension…
            </div>
          )}
        </div>
      ) : null}
    </section>
  );
}
