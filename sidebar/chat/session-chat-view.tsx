// SessionChatView — root layout (upstream chat spec §11.1 port): message list
// over an interactive-card slot over the composer. The question card replaces
// the composer while showing. Hosts inject a SessionChatTransport; everything
// else is derived by useSessionChat.

import { IconEyeFilled, IconEyeOff, IconRobot } from "@tabler/icons-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ClipboardEvent, KeyboardEvent } from "react";
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
import {
  SessionChatSearch,
  type SessionChatHostSearchBridge,
} from "./session-chat-search";
import {
  SessionChatTerminalNoticeCard,
  sessionChatTerminalNoticeDismissKey,
} from "./session-chat-terminal-notice-card";
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

export type {
  SessionChatHostAction,
  SessionChatHostActions,
  SessionChatHostLinks,
  SessionChatHostSearchBridge,
};

export interface SessionChatHostComposerActions {
  focus: () => void;
  handoffToTerminal: () => Promise<string>;
  insertPrompt: (content: string) => boolean;
  requestStash: () => void;
}

export interface SessionChatHostComposerBridge {
  register: (actions: SessionChatHostComposerActions) => () => void;
  /**
   * Parks the composer draft in Saved Prompts. Optional because a host can
   * want the registration channel (to insert text into the composer, say)
   * without being able to stash: the mobile host reaches gxserver over SSH
   * CLI verbs and has no stash verb, and offering a Stash button that cannot
   * work would be worse than not offering one. Absent it, the composer's stash
   * control is not rendered and the chat → terminal handoff is unavailable.
   */
  stashPrompt?: (content: string, options?: { transient?: boolean }) => Promise<void>;
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
  /** Presentation of the transcript search box (see SessionChatSearch). */
  searchLayout?: "inline" | "overlay";
  /** Lets a native host open transcript search from its own chrome. */
  hostSearchBridge?: SessionChatHostSearchBridge;
  /**
   * Show the composer's per-session Verbose Mode pill. The mobile host hides
   * it: there, Verbose Mode is owned by the app's Settings screen.
   */
  showVerbosePill?: boolean;
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

/*
The welcome fills the transcript region and nothing else. It used to be an
`absolute inset-0` overlay spanning the whole chat column, which painted its
centered logo and title straight through the terminal-notice / interactive
cards stacked above the composer. Living in flow means the cards take their
height first and the welcome centers in whatever is left; `showTitle` drops the
headline once a card is up, so the remaining space belongs to the logo alone.
*/
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
    <div className="ghostex-chat-new-session pointer-events-none min-h-0 flex-1 overflow-hidden">
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
  hostSearchBridge,
  searchLayout = "inline",
  showComposerAgentName = true,
  showNewSessionWelcomeTitle = true,
  showVerbosePill = true,
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
  /*
  Composer "@" mentions. The project walk is server work, so it runs on first
  use and the answer is cached for the rest of the mount; `undefined` means
  "not listed yet" and keeps the picker in its loading state.
  */
  const [files, setFiles] = useState<readonly string[] | undefined>(undefined);
  const [filesLoading, setFilesLoading] = useState(false);
  const filesRequestedRef = useRef(false);
  useEffect(() => {
    filesRequestedRef.current = false;
    setFiles(undefined);
    setFilesLoading(false);
  }, [transport]);
  const requestFiles = useCallback(() => {
    if (filesRequestedRef.current) {
      return;
    }
    filesRequestedRef.current = true;
    const readFiles = transport.readFiles?.bind(transport);
    if (!readFiles) {
      setFiles([]);
      return;
    }
    setFilesLoading(true);
    void readFiles()
      .then((result) => {
        setFiles(result.files);
      })
      .catch(() => {
        setFiles([]);
      })
      .finally(() => {
        setFilesLoading(false);
      });
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
    const stashPrompt = hostComposerBridge?.stashPrompt;
    if (!stashPrompt || !draft.trim()) {
      return;
    }
    void stashPrompt(draft)
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
    const stashPrompt = hostComposerBridge?.stashPrompt;
    if (!stashPrompt || !draft.trim()) {
      if (draft.length > 0) {
        composer?.clearDraft(draft);
      }
      return "";
    }
    await stashPrompt(draft, { transient: true });
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
  const saveImageAs = useMemo(() => {
    const save = transport.saveImageAs?.bind(transport);
    return save
      ? (params: { base64Data: string; suggestedName: string }) => save(params)
      : undefined;
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
  // Cards stacked above the composer own their own visibility (per-detection
  // dismissal, prompt identity), so each reports it back here. While one is up
  // the new-session headline stands down instead of competing for the same
  // vertical space.
  const [noticeCardVisible, setNoticeCardVisible] = useState(false);
  const [interactiveCardVisible, setInteractiveCardVisible] = useState(false);
  const chatRootRef = useRef<HTMLDivElement | null>(null);

  const interrupt = useCallback((): void => {
    void chat.interrupt();
  }, [chat]);

  // A terminal-notice `sendKeys` action writes its raw bytes through the
  // approval lane of answerSessionChatPrompt — the same verbatim-write path the
  // interactive card's Allow/Deny buttons use.
  const chatAnswerPrompt = chat.answerPrompt;
  const sendNoticeKeys = useCallback(
    (send: string): Promise<void> =>
      chatAnswerPrompt({ approvalSend: send, kind: "approval" }),
    [chatAnswerPrompt],
  );

  /*
  CDXC:SessionChatTerminalPicker 2026-08-21:
  A terminal notice carrying rows is a picker that owns the agent CLI's input
  line, so the composer is held shut behind it: a message sent now would be
  typed into the picker and its Enter would confirm whichever row is
  highlighted. The daemon refuses such a send anyway, but a disabled composer
  that says WHY beats a red delivery failure after the fact.
  */
  const noticeKey = sessionChatTerminalNoticeDismissKey(chat.terminalNotice);
  const [retiredNoticeKey, setRetiredNoticeKey] = useState<string | null>(null);
  const answerNoticeChoice = useCallback(
    async (choiceIndex: number): Promise<void> => {
      try {
        await chatAnswerPrompt({ choiceIndex, kind: "terminalChoice" });
      } catch (error) {
        /*
        The answer did not land, which the daemon only reports after PROVING
        the picker is gone from the live screen. Releasing the composer here is
        what keeps a card that outlived its picker — a session slept out from
        under it, or it was answered in the terminal — from locking the user
        out of a session that is perfectly willing to take a message. The card
        stays up with its own failure line so the reason is still on screen,
        and the send path re-detects anyway, so nothing can be typed into a
        picker that really is still there.
        */
        setRetiredNoticeKey(noticeKey);
        throw error;
      }
    },
    [chatAnswerPrompt, noticeKey],
  );
  const terminalChoicePending =
    (chat.terminalNotice?.choices?.length ?? 0) > 0 && noticeKey !== retiredNoticeKey;
  const composerEnabled = canSend && !terminalChoicePending;

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

  // Pasting after a click on the pane background lands in the composer too,
  // maximized or not: clipboard images become attachments and text lands at
  // the caret, instead of the paste dying on a non-editable focus target.
  const handlePasteCapture = useCallback(
    (event: ClipboardEvent<HTMLDivElement>): void => {
      if (event.defaultPrevented || questionActive) {
        return;
      }
      const target = event.target as HTMLElement | null;
      if (target?.closest?.(INTERACTIVE_TARGET_SELECTOR)) {
        return;
      }
      if (composerRef.current?.pasteClipboard(event.clipboardData)) {
        event.preventDefault();
        event.stopPropagation();
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
        onContextMenu={(event) => event.preventDefault()}
      />
    );
  }

  const emptyKind =
    chat.view.kind === "ready"
      ? null
      : chat.view.kind === "error"
        ? ("error" as const)
        : chat.view.kind;
  const bottomCardVisible = noticeCardVisible || interactiveCardVisible;
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
        onContextMenu={(event) => event.preventDefault()}
        onKeyDownCapture={handleKeyDownCapture}
        onPasteCapture={handlePasteCapture}
        ref={chatRootRef}
        tabIndex={-1}
      >
      <SessionChatImageViewerProvider
        {...(loadImageDataUrl ? { loadImage: loadImageDataUrl } : {})}
        {...(saveImageAs ? { saveImageAs } : {})}
      >
      <SessionChatHostLinksProvider {...(hostLinks ? { links: hostLinks } : {})}>
      <div className="relative flex min-h-0 flex-1 flex-col">
      <SessionChatSearch
        {...(hostSearchBridge ? { hostBridge: hostSearchBridge } : {})}
        layout={searchLayout}
        rootRef={chatRootRef}
        searchRevision={chat.messages}
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
            terminalActivity={chat.terminalActivity}
            verboseMode={verbose}
          />
        ) : showNewSessionWelcome ? (
          <NewSessionWelcome
            agentLabel={agentLabel}
            showTitle={showNewSessionWelcomeTitle && !bottomCardVisible}
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
        <SessionChatTerminalNoticeCard
          canSend={canSend}
          notice={chat.terminalNotice}
          onAnswerChoice={answerNoticeChoice}
          onSendKeys={sendNoticeKeys}
          onVisibleChange={setNoticeCardVisible}
          {...(hostActions?.onSwitchToTerminal
            ? { onSwitchToTerminal: hostActions.onSwitchToTerminal }
            : {})}
        />
        <SessionChatInteractiveCard
          canSend={canSend}
          onAnswer={chat.answerPrompt}
          onInterrupt={interrupt}
          onShowingChange={setInteractiveCardVisible}
          onShowingQuestionChange={setQuestionActive}
          onSwitchToTerminal={hostActions?.onSwitchToTerminal}
          prompt={chat.prompt}
        />
        {questionActive ? null : (
          <SessionChatComposer
            disabled={!composerEnabled}
            draftSync={chat.draft}
            isWorking={chat.working}
            key={sessionKey}
            monacoVsBaseUrl={monacoVsBaseUrl}
            queue={chat.queue}
            sessionKey={sessionKey}
            theme={theme}
            onAttachFile={attachFile}
            onInterrupt={interrupt}
            onLoadImagePreview={loadImageDataUrl}
            onPasteImage={pasteImage}
            onPickPaths={pickPaths}
            onSend={send}
            sendOnEnter={sendOnEnter}
            {...(hostComposerBridge?.stashPrompt ? { onStash: stashComposerDraft } : {})}
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
                  screenProbed={chat.screenProbed}
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
                {showVerbosePill ? (
                  <SessionVerbosePill onToggle={toggleVerbose} verbose={verbose} />
                ) : null}
              </>
            }
            placeholder={
              !canSend
                ? "Input is held by another device."
                : terminalChoicePending
                  ? "Answer the question above to continue."
                  : undefined
            }
            ref={composerRef}
            slashCommands={slashCommands}
            slashHeading={sessionChatSlashHeadingForAgent(agentLabel ?? null)}
            skills={skills}
            files={files}
            filesLoading={filesLoading}
            onRequestFiles={requestFiles}
            fileHeading="Project files"
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
