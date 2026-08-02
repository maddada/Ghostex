// SessionChatView — root layout (upstream chat spec §11.1 port): message list
// over an interactive-card slot over the composer. The question card replaces
// the composer while showing. Hosts inject a SessionChatTransport; everything
// else is derived by useSessionChat.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { cn } from "../../lib/utils";
import {
  SessionChatComposer,
  type SessionChatComposerHandle,
} from "./session-chat-composer";
import { sessionChatEmptyStateCopy } from "./session-chat-empty-state";
import {
  SessionChatHostActionsCluster,
  type SessionChatHostAction,
  type SessionChatHostActions,
} from "./session-chat-host-actions-cluster";
import { SessionChatImageViewerProvider } from "./session-chat-image-viewer";
import { SessionChatInteractiveCard } from "./session-chat-interactive-card";
import { SessionChatMessageList } from "./session-chat-message-list";
import {
  SessionChatSessionOptionPills,
  useSessionChatSessionOptions,
} from "./session-chat-option-pills";
import { sessionChatOptionCommandNames } from "./session-chat-session-options";
import {
  sessionChatSlashCommandsForAgent,
  sessionChatSlashHeadingForAgent,
} from "./session-chat-slash-commands";
import type { SessionChatTransport } from "./session-chat-transport";
import { useSessionChat } from "./use-session-chat";

const INTERACTIVE_TARGET_SELECTOR = [
  "a[href]",
  "button",
  "input",
  "select",
  "textarea",
  '[contenteditable]:not([contenteditable="false"])',
  '[role="button"]',
  '[role="checkbox"]',
  '[role="combobox"]',
  '[role="menuitem"]',
  '[role="option"]',
  '[role="radio"]',
  '[role="slider"]',
  '[role="switch"]',
  '[role="textbox"]',
  '[data-session-chat-typing-redirect-ignore="true"]',
].join(", ");

export type { SessionChatHostAction, SessionChatHostActions };

export interface SessionChatViewProps {
  /** Host-injected transport scoped to one (projectId, sessionId). */
  transport: SessionChatTransport;
  /** Display label for the agent in the empty state ("claude", "codex", …). */
  agentLabel?: string | null;
  /** Live assistant preview text (hook status) for the streaming bubble. */
  previewText?: string | null;
  /** Optional external live-work signal merged with the server status. */
  working?: boolean;
  /** False when input is held elsewhere; disables composer and cards. */
  canSend?: boolean;
  /** Verified command catalog for local "Ran /x" markers. */
  commandCatalog?: readonly string[];
  /**
   * Stable identity of this conversation, used to persist the last chosen
   * session options (model/effort) per session. Hosts that cannot name the
   * session omit it, which simply means the pills start from the defaults.
   */
  sessionKey?: string;
  /** Top-right Terminal View / Agent Actions cluster (see the type doc). */
  hostActions?: SessionChatHostActions;
  /**
   * Base URL of monaco-editor's min/vs directory on this surface; when set,
   * the composer input is a Monaco editor. Hosts that cannot serve Monaco's
   * sibling assets (the mobile single-file bundle) omit it.
   */
  monacoVsBaseUrl?: string;
  className?: string;
}

function EmptyState({
  detail,
  spinning,
  title,
}: {
  detail: string;
  spinning: boolean;
  title: string;
}) {
  return (
    <div className="ghostex-chat-empty-state">
      {spinning ? <span className="ghostex-chat-empty-spinner" /> : null}
      <div className="ghostex-chat-empty-title">{title}</div>
      <div className="ghostex-chat-empty-detail">{detail}</div>
    </div>
  );
}

export function SessionChatView({
  agentLabel,
  canSend = true,
  className,
  commandCatalog,
  hostActions,
  monacoVsBaseUrl,
  previewText,
  sessionKey,
  transport,
  working,
}: SessionChatViewProps) {
  const slashCommands = useMemo(
    () => sessionChatSlashCommandsForAgent(agentLabel ?? null),
    [agentLabel],
  );
  // The option pills type commands the "/" picker does not offer (/effort,
  // /fast). They still have to classify as commands so a dispatched pill
  // renders the same muted "Ran /model sonnet" row a typed one does.
  const slashCommandNames = useMemo(
    () => [
      ...slashCommands.map((command) => command.name),
      ...sessionChatOptionCommandNames(agentLabel ?? null),
    ],
    [agentLabel, slashCommands],
  );
  const chat = useSessionChat({
    commandCatalog: commandCatalog ?? slashCommandNames,
    previewText,
    transport,
    working,
  });
  const sessionOptions = useSessionChatSessionOptions({
    agent: agentLabel ?? null,
    ...(sessionKey !== undefined ? { sessionKey } : {}),
  });
  /*
  What the agent is actually running, read by gxserver out of the session's
  terminal. Keyed on detectedAt so a repeated identical detection does not
  re-run the fold, and a no-detection session never enters it at all.
  */
  const applyDetectedOptions = sessionOptions.applyDetected;
  const detectedOptions = chat.selectedOptions;
  const detectedAt = detectedOptions?.detectedAt ?? null;
  useEffect(() => {
    if (!detectedOptions || detectedAt === null) {
      return;
    }
    applyDetectedOptions(detectedOptions);
    // detectedOptions is re-created per frame; detectedAt identifies the read.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [applyDetectedOptions, detectedAt]);
  const composerRef = useRef<SessionChatComposerHandle | null>(null);
  const pasteImage = useMemo(() => {
    const saveImage = transport.saveImage?.bind(transport);
    return saveImage
      ? async (payload: { base64Data: string; suggestedName?: string }) =>
          (await saveImage(payload)).path
      : undefined;
  }, [transport]);
  const attachFile = useMemo(() => {
    const saveAttachment = transport.saveAttachment?.bind(transport);
    return saveAttachment
      ? async (payload: { base64Data: string; suggestedName?: string }) =>
          (await saveAttachment(payload)).path
      : undefined;
  }, [transport]);
  const pickPaths = useMemo(() => {
    const pickAttachmentPaths = transport.pickAttachmentPaths?.bind(transport);
    return pickAttachmentPaths ? () => pickAttachmentPaths() : undefined;
  }, [transport]);
  // Machine-path image bytes as a data URL: chat-log overlay + picked-image
  // composer thumbnails both read through it.
  const loadImageDataUrl = useMemo(() => {
    const loadImage = transport.loadImage?.bind(transport);
    return loadImage
      ? async (path: string) => {
          const result = await loadImage({ path });
          return `data:${result.mediaType};base64,${result.base64Data}`;
        }
      : undefined;
  }, [transport]);
  const [questionActive, setQuestionActive] = useState(false);

  const interrupt = useCallback((): void => {
    void chat.interrupt();
  }, [chat]);

  // A command the user types themselves reconciles the pills (§1.4), so the
  // Model pill follows a hand-typed "/model opus" without a second dispatch.
  const chatSend = chat.send;
  const reconcileTypedCommand = sessionOptions.reconcileTypedCommand;
  const send = useCallback(
    (text: string): Promise<void> => {
      reconcileTypedCommand(text);
      return chatSend(text);
    },
    [chatSend, reconcileTypedCommand],
  );

  // Typing anywhere in the pane lands in the composer (§11.1): a single
  // printable character without Ctrl/Meta is redirected; unmodified
  // Backspace/Delete focuses the composer without inserting anything.
  const handleKeyDownCapture = useCallback(
    (event: KeyboardEvent<HTMLDivElement>): void => {
      if (event.defaultPrevented || questionActive) {
        return;
      }
      const target = event.target as HTMLElement | null;
      if (target?.closest?.(INTERACTIVE_TARGET_SELECTOR)) {
        return;
      }
      if (
        (event.key === "Backspace" || event.key === "Delete") &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey
      ) {
        composerRef.current?.focus();
        return;
      }
      if (
        event.key.length === 1 &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.nativeEvent.isComposing
      ) {
        if (composerRef.current?.insertTypedText(event.key)) {
          event.preventDefault();
          event.stopPropagation();
        }
      }
    },
    [questionActive],
  );

  const emptyKind =
    chat.view.kind === "ready"
      ? null
      : chat.view.kind === "error"
        ? ("error" as const)
        : chat.view.kind;

  return (
    <div
      className={cn(
        // The app theme zeroes --radius for its square chrome; restore the
        // shadcn default inside the chat so bubbles and cards keep their
        // rounded look. The scope class lifts the SquareTheme border-radius
        // override (sidebar/styles.css) for controls inside the chat.
        "ghostex-session-chat-scope relative flex h-full min-h-0 flex-col bg-background text-foreground outline-none [--radius:0.625rem]",
        className,
      )}
      onKeyDownCapture={handleKeyDownCapture}
      tabIndex={-1}
    >
      <SessionChatImageViewerProvider
        {...(loadImageDataUrl ? { loadImage: loadImageDataUrl } : {})}
      >
      {hostActions ? (
        <SessionChatHostActionsCluster hostActions={hostActions} surface="chat" />
      ) : null}
      <div className="flex min-h-0 flex-1 flex-col">
        {chat.view.kind === "ready" ? (
          <SessionChatMessageList
            hasMore={chat.hasMore}
            isWorking={chat.view.isWorking}
            loadingEarlier={chat.loadingEarlier}
            messages={chat.messages}
            onLoadEarlier={chat.loadEarlier}
          />
        ) : emptyKind ? (
          chat.view.kind === "error" ? (
            <EmptyState
              detail={sessionChatEmptyStateCopy("error").detail}
              spinning={false}
              title={sessionChatEmptyStateCopy("error").title}
            />
          ) : (
            <EmptyState
              detail={sessionChatEmptyStateCopy(emptyKind, agentLabel).detail}
              spinning={emptyKind === "loading" || emptyKind === "starting"}
              title={sessionChatEmptyStateCopy(emptyKind, agentLabel).title}
            />
          )
        ) : null}
      </div>
      <div className="mx-auto grid w-full max-w-3xl flex-none gap-2 px-4 pt-2 pb-3">
        <SessionChatInteractiveCard
          canSend={canSend}
          onAnswer={chat.answerPrompt}
          onInterrupt={interrupt}
          onShowingQuestionChange={setQuestionActive}
          onSwitchToTerminal={hostActions?.onSwitchToTerminal}
          prompt={chat.prompt}
        />
        {questionActive ? null : (
          <SessionChatComposer
            disabled={!canSend}
            isWorking={chat.working}
            monacoVsBaseUrl={monacoVsBaseUrl}
            onAttachFile={attachFile}
            onInterrupt={interrupt}
            onLoadImagePreview={loadImageDataUrl}
            onPasteImage={pasteImage}
            onPickPaths={pickPaths}
            onSend={send}
            optionPills={
              <SessionChatSessionOptionPills
                canSend={canSend}
                canSendKey={chat.sendKey !== undefined}
                controller={sessionOptions}
                isWorking={chat.working}
                onDispatchCommand={send}
                onDispatchKey={async (key, marker) => {
                  await chat.sendKey?.(key, marker);
                }}
                {...(hostActions?.onSwitchToTerminal
                  ? { onSwitchToTerminal: hostActions.onSwitchToTerminal }
                  : {})}
              />
            }
            placeholder={canSend ? undefined : "Input is held by another device."}
            ref={composerRef}
            slashCommands={slashCommands}
            slashHeading={sessionChatSlashHeadingForAgent(agentLabel ?? null)}
          />
        )}
      </div>
      </SessionChatImageViewerProvider>
    </div>
  );
}
