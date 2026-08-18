// SessionChatView — root layout (upstream chat spec §11.1 port): message list
// over an interactive-card slot over the composer. The question card replaces
// the composer while showing. Hosts inject a SessionChatTransport; everything
// else is derived by useSessionChat.

import { IconEyeFilled, IconEyeOff, IconRobot } from "@tabler/icons-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { Button } from "../../components/ui/button";
import { cn } from "../../lib/utils";
import type { SessionChatSkill, SessionChatTheme } from "../../shared/session-chat";
import { getDefaultSidebarAgentById } from "../../shared/sidebar-agents";
import { getBrandAgentLogoStyle } from "../agent-logos";
import { AppTooltip, TooltipProvider } from "../app-tooltip";
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
import {
  SessionChatHostLinksProvider,
  type SessionChatHostLinks,
} from "./session-chat-links";
import { SessionChatInteractiveCard } from "./session-chat-interactive-card";
import { SessionChatMessageList } from "./session-chat-message-list";
import { SessionChatSearch } from "./session-chat-search";
import {
  SessionChatSessionOptionPills,
  useSessionChatSessionOptions,
} from "./session-chat-option-pills";
import { sessionChatOptionCommandNames } from "./session-chat-session-options";
import {
  readStoredSessionChatVerbose,
  writeStoredSessionChatVerbose,
} from "./session-chat-verbose-override";
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

export type { SessionChatHostAction, SessionChatHostActions, SessionChatHostLinks };

export interface SessionChatHostComposerActions {
  focus: () => void;
  handoffToTerminal: () => Promise<string>;
  insertPrompt: (content: string) => boolean;
  requestStash: () => void;
}

export interface SessionChatHostComposerBridge {
  register: (actions: SessionChatHostComposerActions) => () => void;
  stashPrompt: (content: string, options?: { transient?: boolean }) => Promise<void>;
}

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
   * session options and the unsent composer draft per session. Hosts that
   * cannot name the session omit it, which keeps both values per mount.
   */
  sessionKey?: string;
  /** Top-right Terminal View / Agent Actions cluster (see the type doc). */
  hostActions?: SessionChatHostActions;
  /** Host-only terminal switch for an agent-owned model picker. */
  onSwitchToTerminalForAgentPicker?: () => void;
  /** Native-host requests that must act on this chat composer's draft. */
  hostComposerBridge?: SessionChatHostComposerBridge;
  /**
   * What the host does with links in the conversation (web URLs, machine file
   * paths). Omitted means browser defaults: URLs open in a new tab and file
   * paths are inert.
   */
  hostLinks?: SessionChatHostLinks;
  /**
   * Base URL of monaco-editor's min/vs directory on this surface; when set,
   * the composer input is a Monaco editor. Hosts that cannot serve Monaco's
   * sibling assets (the mobile single-file bundle) omit it.
   */
  monacoVsBaseUrl?: string;
  /** Chat-only palette. It does not change the host application's chrome. */
  theme?: SessionChatTheme;
  /** Reveal thinking-owned tool calls without requiring a click. */
  verboseMode?: boolean;
  /** Show the touch-friendly search affordance used by the mobile host. */
  showSearchButton?: boolean;
  /** Show the agent name beside its composer icon. */
  showComposerAgentName?: boolean;
  /** Show the prompt beneath the agent logo for a new session. */
  showNewSessionWelcomeTitle?: boolean;
  /** Whether plain Enter sends from the composer instead of inserting a newline. */
  sendOnEnter?: boolean;
  className?: string;
}

function EmptyState({
  detail,
  title,
}: {
  detail: string;
  title: string;
}) {
  return (
    <div className="ghostex-chat-empty-state">
      <div className="ghostex-chat-empty-title">{title}</div>
      <div className="ghostex-chat-empty-detail">{detail}</div>
    </div>
  );
}

function displayAgentName(agentLabel?: string | null): string | null {
  const normalized = agentLabel?.trim();
  if (!normalized) {
    return null;
  }
  return (
    getDefaultSidebarAgentById(normalized)?.name ??
    normalized
      .replace(/[-_]+/g, " ")
      .replace(/\b\p{L}/gu, (letter) => letter.toLocaleUpperCase())
  );
}

function NewSessionWelcome({
  agentLabel,
  showTitle = true,
}: {
  agentLabel?: string | null;
  showTitle?: boolean;
}) {
  const agent = agentLabel ? getDefaultSidebarAgentById(agentLabel) : undefined;
  const agentName = displayAgentName(agentLabel);

  return (
    <div className="ghostex-chat-new-session pointer-events-none absolute inset-0 justify-center">
      <div
        aria-label={agentName ?? "Agent"}
        className="ghostex-chat-new-session-agent"
        role="img"
      >
        {agent?.icon ? (
          <span
            aria-hidden="true"
            className="ghostex-chat-new-session-agent-logo"
            style={getBrandAgentLogoStyle(agent.icon)}
          />
        ) : (
          <IconRobot aria-hidden="true" size={28} stroke={1.7} />
        )}
      </div>
      {showTitle ? (
        <div className="ghostex-chat-new-session-title">
          {agentName ? (
            <>
              What should we build with {agentName}?
            </>
          ) : (
            "What should we work on?"
          )}
        </div>
      ) : null}
    </div>
  );
}

function SessionAgentIdentity({
  agentLabel,
  showName = true,
}: {
  agentLabel?: string | null;
  showName?: boolean;
}) {
  const agent = agentLabel ? getDefaultSidebarAgentById(agentLabel) : undefined;
  const agentName = displayAgentName(agentLabel);
  if (!agentName) {
    return null;
  }

  return (
    <div
      aria-label={`Agent ${agentName}`}
      className="ghostex-chat-agent-identity flex min-w-0 items-center gap-1.5 px-1 text-xs font-medium text-muted-foreground"
    >
      {agent?.icon ? (
        <span
          aria-hidden="true"
          className="block size-3.5 shrink-0"
          style={getBrandAgentLogoStyle(agent.icon)}
        />
      ) : (
        <IconRobot aria-hidden="true" className="size-3.5 shrink-0" stroke={1.8} />
      )}
      {showName ? (
        <span
          className="ghostex-chat-agent-name min-w-0 truncate"
          style={{ maxWidth: "6rem" }}
        >
          {agentName}
        </span>
      ) : null}
    </div>
  );
}

/*
Verbose pill: a per-session override of the sessionChatVerboseMode setting.
The setting stays the default for chats that never touch the pill; a session
that does keeps its own value (session-chat-verbose-override.ts).
*/
function SessionVerbosePill({
  onToggle,
  verbose,
}: {
  onToggle: () => void;
  verbose: boolean;
}) {
  const Icon = verbose ? IconEyeFilled : IconEyeOff;
  const verboseLabel = verbose ? "Verbose mode on" : "Verbose mode off";
  return (
    <AppTooltip content={verboseLabel}>
      <span className="ghostex-chat-verbose-wrapper inline-flex">
        <Button
          aria-label={verboseLabel}
          aria-pressed={verbose}
          className={cn(
            "ghostex-chat-footer-control ghostex-chat-verbose-control rounded-full",
            verbose ? "text-foreground" : "text-muted-foreground",
          )}
          onClick={onToggle}
          size="icon-xs"
          variant={verbose ? "secondary" : "ghost"}
        >
          <Icon aria-hidden="true" className="size-3 shrink-0" stroke={2} />
        </Button>
      </span>
    </AppTooltip>
  );
}

export function SessionChatView({
  agentLabel,
  canSend = true,
  className,
  commandCatalog,
  hostActions,
  hostComposerBridge,
  hostLinks,
  monacoVsBaseUrl,
  onSwitchToTerminalForAgentPicker,
  previewText,
  sendOnEnter = true,
  sessionKey,
  showComposerAgentName = true,
  showNewSessionWelcomeTitle = true,
  showSearchButton = false,
  theme = "dark",
  transport,
  verboseMode = false,
  working,
}: SessionChatViewProps) {
  useEffect(() => {
    // Chat dropdowns are portaled outside this root. Stamp the chat-only
    // palette on body so those explicitly scoped popup surfaces match.
    document.body.dataset.sessionChatTheme = theme;
  }, [theme]);
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
  const initialTranscriptLoading = chat.view.kind === "loading";
  const [skills, setSkills] = useState<readonly SessionChatSkill[]>([]);
  useEffect(() => {
    const readSkills = transport.readSkills?.bind(transport);
    if (!readSkills) {
      setSkills([]);
      return;
    }
    let active = true;
    void readSkills()
      .then((result) => {
        if (active) {
          setSkills(result.skills);
        }
      })
      .catch(() => {
        if (active) {
          setSkills([]);
        }
      });
    return () => {
      active = false;
    };
  }, [transport]);
  const sessionOptions = useSessionChatSessionOptions({
    agent: agentLabel ?? null,
    ...(sessionKey !== undefined ? { sessionKey } : {}),
  });
  // null = this chat has never been toggled, so it follows the global setting.
  const [verboseOverride, setVerboseOverride] = useState<boolean | null>(() =>
    readStoredSessionChatVerbose(sessionKey),
  );
  useEffect(() => {
    setVerboseOverride(readStoredSessionChatVerbose(sessionKey));
  }, [sessionKey]);
  const verbose = verboseOverride ?? verboseMode;
  const toggleVerbose = useCallback(() => {
    const next = !verbose;
    writeStoredSessionChatVerbose(sessionKey, next);
    setVerboseOverride(next);
  }, [sessionKey, verbose]);
  /*
  What the agent is actually running, confirmed by gxserver from structured
  transcript metadata and the terminal statusline. Keyed on detectedAt so a
  repeated identical detection does not re-run the fold.
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
  const stashComposerDraft = useCallback((): void => {
    const composer = composerRef.current;
    const draft = composer?.getDraft() ?? "";
    if (!hostComposerBridge || !draft.trim()) {
      return;
    }
    void hostComposerBridge
      .stashPrompt(draft)
      .then(() => {
        // A save can overlap more typing. Move only the exact saved snapshot;
        // never clear text the user added while gxserver was answering.
        const currentComposer = composerRef.current;
        if (currentComposer?.clearDraft(draft)) {
          currentComposer.focus();
        }
      })
      .catch(() => {
        // Keep the draft intact so a failed stash can be retried.
      });
  }, [hostComposerBridge]);
  const handoffComposerDraft = useCallback(async (): Promise<string> => {
    const composer = composerRef.current;
    const draft = composer?.getDraft() ?? "";
    if (!hostComposerBridge || !draft.trim()) {
      if (draft.length > 0) {
        composer?.clearDraft(draft);
      }
      return "";
    }
    await hostComposerBridge.stashPrompt(draft, { transient: true });
    // The exact snapshot that became durable must still own the composer.
    // If more text arrived during the save, remain in chat with all text
    // intact instead of switching with a partial draft.
    if (composerRef.current?.clearDraft(draft) !== true) {
      throw new Error("The draft changed while it was being moved.");
    }
    return draft;
  }, [hostComposerBridge]);
  useEffect(() => {
    if (!hostComposerBridge || initialTranscriptLoading) {
      return;
    }
    return hostComposerBridge.register({
      focus: () => composerRef.current?.focus(),
      handoffToTerminal: handoffComposerDraft,
      insertPrompt: (content) => composerRef.current?.insertTypedText(content) ?? false,
      requestStash: stashComposerDraft,
    });
  }, [
    handoffComposerDraft,
    hostComposerBridge,
    initialTranscriptLoading,
    stashComposerDraft,
  ]);
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
  const chatRootRef = useRef<HTMLDivElement | null>(null);

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

  // The initial read cannot yet distinguish an existing transcript from a
  // genuinely empty session. Keep that indeterminate phase visually blank so
  // an existing conversation never flashes the new-session welcome/composer.
  if (initialTranscriptLoading) {
    return (
      <div
        aria-busy="true"
        className={cn(
          "ghostex-session-chat-scope h-full min-h-0 bg-background",
          theme === "dark" && "dark",
          className,
        )}
        data-chat-theme={theme}
      />
    );
  }

  const emptyKind =
    chat.view.kind === "ready"
      ? null
      : chat.view.kind === "error"
        ? ("error" as const)
        : chat.view.kind;
  const showNewSessionWelcome =
    // A new agent reports `starting` until its first transcript file exists.
    // Keep the designed welcome visible throughout that pre-transcript window.
    emptyKind === "starting" || emptyKind === "empty";

  return (
    <TooltipProvider>
      <div
        className={cn(
        // The app theme zeroes --radius for its square chrome; restore the
        // shadcn default inside the chat so bubbles and cards keep their
        // rounded look. The scope class lifts the SquareTheme border-radius
        // override (sidebar/styles.css) for controls inside the chat.
        "ghostex-session-chat-scope relative flex h-full min-h-0 flex-col bg-background text-foreground outline-none [--radius:0.625rem]",
        theme === "dark" && "dark",
        className,
      )}
        data-chat-theme={theme}
        onKeyDownCapture={handleKeyDownCapture}
        ref={chatRootRef}
        tabIndex={-1}
      >
      <SessionChatImageViewerProvider
        {...(loadImageDataUrl ? { loadImage: loadImageDataUrl } : {})}
      >
      <SessionChatHostLinksProvider {...(hostLinks ? { links: hostLinks } : {})}>
      <div className="relative flex min-h-0 flex-1 flex-col">
      <SessionChatSearch
        rootRef={chatRootRef}
        searchRevision={chat.messages}
        showButton={showSearchButton}
      />
      <div className="relative flex min-h-0 flex-1 flex-col">
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
            verboseMode={verbose}
          />
        ) : showNewSessionWelcome ? (
          <NewSessionWelcome
            agentLabel={agentLabel}
            showTitle={showNewSessionWelcomeTitle}
          />
        ) : emptyKind ? (
          chat.view.kind === "error" ? (
            <EmptyState
              detail={sessionChatEmptyStateCopy("error").detail}
              title={sessionChatEmptyStateCopy("error").title}
            />
          ) : (
            <EmptyState
              detail={sessionChatEmptyStateCopy(emptyKind, agentLabel).detail}
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
            key={sessionKey}
            monacoVsBaseUrl={monacoVsBaseUrl}
            sessionKey={sessionKey}
            theme={theme}
            onAttachFile={attachFile}
            onInterrupt={interrupt}
            onLoadImagePreview={loadImageDataUrl}
            onPasteImage={pasteImage}
            onPickPaths={pickPaths}
            onSend={send}
            sendOnEnter={sendOnEnter}
            {...(hostComposerBridge ? { onStash: stashComposerDraft } : {})}
            optionPills={
              <>
                {chat.view.kind === "ready" ? (
                  <SessionAgentIdentity
                    agentLabel={agentLabel}
                    showName={showComposerAgentName}
                  />
                ) : null}
                <SessionChatSessionOptionPills
                  canSend={canSend}
                  canSendKey={chat.sendKey !== undefined}
                  controller={sessionOptions}
                  isWorking={chat.working}
                  onDispatchCommand={send}
                  onDispatchKey={async (key, marker) => {
                    await chat.sendKey?.(key, marker);
                  }}
                  {...(onSwitchToTerminalForAgentPicker || hostActions?.onSwitchToTerminal
                    ? {
                      onSwitchToTerminal:
                          onSwitchToTerminalForAgentPicker ??
                          hostActions?.onSwitchToTerminalForAgentPicker ??
                          hostActions?.onSwitchToTerminal,
                      }
                    : {})}
                />
                <SessionVerbosePill onToggle={toggleVerbose} verbose={verbose} />
              </>
            }
            placeholder={canSend ? undefined : "Input is held by another device."}
            ref={composerRef}
            slashCommands={slashCommands}
            slashHeading={sessionChatSlashHeadingForAgent(agentLabel ?? null)}
            skills={skills}
            skillHeading={`${displayAgentName(agentLabel) ?? "Agent"} skills`}
          />
        )}
      </div>
      </div>
      </div>
      </SessionChatHostLinksProvider>
      </SessionChatImageViewerProvider>
      </div>
    </TooltipProvider>
  );
}
