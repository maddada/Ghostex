// SessionChatView — root layout (upstream chat spec §11.1 port): message list
// over an interactive-card slot over the composer. The question card replaces
// the composer while showing. Hosts inject a SessionChatTransport; everything
// else is derived by useSessionChat.

import { IconBlockquote, IconCopy, IconLoader2 } from '@tabler/icons-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ClipboardEvent, DragEvent, KeyboardEvent as ReactKeyboardEvent, MouseEvent, RefObject } from 'react';
import { Button } from '../../components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuTrigger,
} from '../../components/ui/context-menu';
import { cn } from '@/packages/components/utils';
import type { GxserverSessionForkBranch } from '../../shared/gxserver-protocol';
import type { SessionChatSkill, SessionChatTheme } from '../../shared/session-chat';
import { ghostexHotkeyTextFromKeyboardEvent } from '../../shared/ghostex-hotkeys';
import { AppTooltip, TooltipProvider } from '../app-tooltip';
import { displayAgentName, NewSessionWelcome } from './session-chat-new-session-welcome';
import { SessionChatComposer, type SessionChatComposerHandle } from './session-chat-composer';
import { sessionChatDataTransferHasFiles } from './session-chat-drop-attachments';
import { sessionChatEmptyStateCopy } from './session-chat-empty-state';
import { SESSION_CHAT_FILE_PATH_ATTRIBUTE } from './session-chat-file-paths';
import {
  SessionChatExtensionPanel,
  type SessionChatBarExtension,
  type SessionChatExtensionPanelProps,
} from './session-chat-extension-panel';
import type { SessionChatHostAction, SessionChatHostActions } from './session-chat-host-actions';
import { SessionChatImageViewerProvider } from './session-chat-image-viewer';
import { SessionChatForkBranchSwitcher } from './session-chat-fork-branch-switcher';
import { SessionChatHostLinksProvider, type SessionChatHostLinks } from './session-chat-links';
import { SessionChatInteractiveCard } from './session-chat-interactive-card';
import { SessionChatMessageList } from './session-chat-message-list';
import { SessionChatNotePanel } from './session-chat-note-panel';
import { SessionChatSearch, type SessionChatHostSearchBridge } from './session-chat-search';
import {
  readStoredSessionChatSummary,
  sessionChatSummaryToggleHotkey,
  writeStoredSessionChatSummary,
} from './session-chat-summary-override';
import {
  SessionChatTerminalNoticeCard,
  sessionChatTerminalNoticeDismissKey,
} from './session-chat-terminal-notice-card';
import { SessionChatSessionOptionPills, useSessionChatSessionOptions } from './session-chat-option-pills';
import { sessionChatOptionCommandNames } from './session-chat-session-options';
import { readStoredSessionChatVerbose, writeStoredSessionChatVerbose } from './session-chat-verbose-override';
import { sessionChatSlashCommandsForAgent, sessionChatSlashHeadingForAgent } from './session-chat-slash-commands';
import type { SessionChatTransport } from './session-chat-transport';
import { useSessionChat } from './use-session-chat';

const INTERACTIVE_TARGET_SELECTOR = [
  'a[href]',
  'button',
  'input',
  'select',
  'textarea',
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
].join(', ');

/*
The indeterminate transcript phase renders blank on purpose (see the early
return below), but a stalled read must not leave the pane blank forever. These
stage that hold: a short hold nobody perceives, then a quiet indicator, then
the manual recycle once waiting has clearly stopped being normal.
*/
const LOADING_INDICATOR_DELAY_MS = 600;
const LOADING_RETRY_DELAY_MS = 12_000;

/*
CDXC:DraftSessions 2026-08-28:
Host actions that mean nothing on a draft. See the filter that uses them.
*/
const DRAFT_HIDDEN_HOST_ACTION_IDS = new Set(['fork', 'fullReload']);

/*
CDXC:DraftAgentSwitch 2026-08-28:
When to look again after a draft's agent CLI was switched. The endpoint answers
as soon as it has queued the swap — three interrupts, a settle, then the new
agent's launch command typed into the same live pane — so the CLI that read
answers about is still the OLD one, or a bare login shell. These are the two
reads that see the NEW agent: one shortly after it has been launched, one after
it has had time to paint its footer. Without them the pills would sit in their
"reading…" state until something else happened to move the session, because the
follower publishes detection only when the value it last published CHANGES —
and switching between two agents of the same family often lands on the very
same model string.
*/
const DRAFT_AGENT_SWITCH_REREAD_DELAYS_MS = [2_000, 6_000];

export type { SessionChatHostAction, SessionChatHostActions, SessionChatHostLinks, SessionChatHostSearchBridge };

/** Where a stash left the durable copy of the text it was given. */
export interface SessionChatStashedPrompt {
  /**
   * The Saved Prompts row this stash created, and therefore the only row the
   * caller is allowed to delete again. Absent when the save matched a prompt
   * the user had already saved by hand: that one stays in Saved Prompts.
   */
  promptId?: string;
}

/** The draft that left the composer, and the durable copy that outlives it. */
export interface SessionChatComposerHandoff {
  /** Exact text the terminal must receive. Empty means nothing moved. */
  content: string;
  /**
   * Saved Prompts row holding `content` until the host confirms a terminal
   * actually took it. The host deletes this row only on that confirmation;
   * on every other outcome the row stays, so the text is never only in RAM.
   */
  stashedPromptId?: string;
}

export interface SessionChatHostComposerActions {
  /** Clears only when the composer still holds this exact acknowledged send. */
  clearDraft: (expectedContent: string) => boolean;
  focus: () => void;
  handoffToTerminal: () => Promise<SessionChatComposerHandoff>;
  insertPrompt: (content: string) => boolean;
  requestStash: () => void;
}

export interface SessionChatHostComposerBridge {
  register: (actions: SessionChatHostComposerActions) => () => void;
  /**
   * Tells the host whether the composer currently holds anything unsent (draft
   * text or attached images). Sent on composer mount and on every flip, never
   * per keystroke, and it carries the boolean only — never the draft. Optional
   * because only a host that can destroy and rebuild this page needs it: the
   * desktop shell reclaims the RAM of long-hidden chat surfaces and must not
   * take one down while it still holds something the user typed.
   */
  reportDraftState?: (state: { empty: boolean }) => void;
  /**
   * Parks the composer draft in Saved Prompts. Optional because a host can
   * want the registration channel (to insert text into the composer, say)
   * without being able to stash. Absent it, the composer's stash control is
   * not rendered and the chat → terminal handoff is unavailable.
   */
  stashPrompt?: (content: string, options?: { transient?: boolean }) => Promise<SessionChatStashedPrompt | undefined>;
  /*
  CDXC:SessionChatStashBadge 2026-08-24:
  The two halves of "the prompts stashed from this conversation": how many
  there are, and how to show them. Both are optional and both are gated on the
  host owning a Saved Prompts surface. The count badge plus the empty-draft
  open simply do not appear when the host omits them.
  */
  /**
   * Counts the prompts stashed from `agentSessionId` (null before the provider
   * conversation id resolves — the host then falls back to whatever session
   * identity it was built with). Rejections are swallowed by the caller: a
   * missing count only hides the badge.
   */
  countSessionStashedPrompts?: (agentSessionId: string | null) => Promise<number>;
  /** Opens the host's Saved Prompts surface with this session's context. */
  showStashedPrompts?: () => void;
}

/** Lets native chrome reveal the shared per-conversation note editor. */
export interface SessionChatHostSessionNoteBridge {
  register: (actions: { open: () => void }) => () => void;
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
  /** Current session title used for generated document names. */
  sessionTitle?: string;
  /** Top-right Terminal View / Agent Actions cluster (see the type doc). */
  hostActions?: SessionChatHostActions;
  /** Host-only terminal switch for an agent-owned model picker. */
  onSwitchToTerminalForAgentPicker?: () => void;
  /** Native-host requests that must act on this chat composer's draft. */
  hostComposerBridge?: SessionChatHostComposerBridge;
  /** Native-host entry point for the shared Session Note panel. */
  hostSessionNoteBridge?: SessionChatHostSessionNoteBridge;
  /** Open delayed actions for this session in the host-owned modal. */
  onDelayedActions?: () => void;
  /*
  CDXC:SessionForkFamilies 2026-08-28:
  Navigates the host to another branch of this conversation, picked in the
  chat's branch switcher. Only the host knows how it selects a session, so a
  host with no way to do it from this surface omits the callback and the
  switcher lists the family read-only instead of pretending a click will land.
  */
  onSelectForkBranch?: (branch: GxserverSessionForkBranch) => void;
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
  /** Let the transcript use its configured percentage instead of the composer column. */
  customTranscriptWidthEnabled?: boolean;
  /** Reveal thinking-owned tool calls without requiring a click. */
  verboseMode?: boolean;
  /** Presentation of the transcript search box (see SessionChatSearch). */
  searchLayout?: 'inline' | 'overlay';
  /** Lets a native host open transcript search from its own chrome. */
  hostSearchBridge?: SessionChatHostSearchBridge;
  /** Show the composer's per-session Verbose mode action. */
  showVerbosePill?: boolean;
  /** Show the prompt beneath the agent logo for a new session. */
  showNewSessionWelcomeTitle?: boolean;
  /** Whether plain Enter sends from the composer instead of inserting a newline. */
  sendOnEnter?: boolean;
  /**
   * Use the platform's own text-selection/editing menus (the React Native
   * webview host). The chat's custom menus are right-click/long-press
   * affordances built for desktop pointers; on a phone their long-press
   * trigger replaces the system selection menu — in the composer it also
   * dismisses the keyboard mid-selection. With this set, the composer gets the
   * stock system menu and the transcript swaps the right-click menu for a
   * selection-anchored Copy / Add to Chat toolbar.
   */
  nativeSelectionMenus?: boolean;
  /**
   * Host-provided diagnostic breadcrumb sink (desktop support logs). Called on
   * the composer-affecting transitions (prompt kind, question card, view kind,
   * working) plus the composer's own mount/focus events, so native logs can
   * time a typing-focus loss against server state frames. Hosts without disk
   * logging omit it; the callback gates on the host's diagnostic scenario.
   */
  diagnosticLog?: (event: string, details?: Record<string, unknown>) => void;
  /** Enabled extensions whose selected placement is below this chat's composer. */
  chatBarExtensions?: readonly SessionChatBarExtension[];
  /** Per-session persisted panel state supplied by the gxserver-backed host. */
  chatBarPanelState?: { activeExtensionId?: string; minimized: boolean; open: boolean };
  /** Persists a partial panel-state update for this session. */
  onChatBarPanelStateChange?: (patch: { activeExtensionId?: string; minimized?: boolean; open?: boolean }) => void;
  /** Strict typed SDK proxy used because CEF does not inject the bridge into chat subframes. */
  onChatBarBridgeRequest?: SessionChatExtensionPanelProps['onBridgeRequest'];
  className?: string;
}

function EmptyState({ detail, title }: { detail: string; title: string }) {
  return (
    <div className='ghostex-chat-empty-state'>
      <div className='ghostex-chat-empty-title'>{title}</div>
      <div className='ghostex-chat-empty-detail'>{detail}</div>
    </div>
  );
}

function readTranscriptSelection(container: HTMLElement | null): string {
  const selection = window.getSelection();
  if (!container || !selection || selection.isCollapsed || selection.rangeCount === 0) {
    return '';
  }
  const commonAncestor = selection.getRangeAt(0).commonAncestorContainer;
  const commonElement =
    commonAncestor.nodeType === Node.ELEMENT_NODE ? (commonAncestor as Element) : commonAncestor.parentElement;
  if (!commonElement || !container.contains(commonElement)) {
    return '';
  }
  return selection.toString().trim();
}

function asMarkdownQuote(text: string): string {
  return text
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map((line) => (line === '' ? '>' : `> ${line}`))
    .join('\n');
}

/*
The nativeSelectionMenus transcript replacement for the desktop right-click
menu: a long press there belongs to the system selection handles, so the
chat's own actions ride a small toolbar anchored under the selection instead.
Copy goes through execCommand while the selection is still live because the
RN webview page has no secure origin, which leaves navigator.clipboard absent.
*/
function TranscriptSelectionToolbar({
  addToChatEnabled,
  containerRef,
  onAddToChat,
}: {
  addToChatEnabled: boolean;
  containerRef: RefObject<HTMLDivElement | null>;
  onAddToChat: (text: string) => void;
}) {
  const [anchor, setAnchor] = useState<{ left: number; text: string; top: number } | null>(null);

  useEffect(() => {
    let timer: number | undefined;
    const measure = (): void => {
      const container = containerRef.current;
      const text = readTranscriptSelection(container);
      const selection = window.getSelection();
      if (!container || text === '' || !selection || selection.rangeCount === 0) {
        setAnchor(null);
        return;
      }
      const rect = selection.getRangeAt(0).getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0) {
        setAnchor(null);
        return;
      }
      const containerRect = container.getBoundingClientRect();
      setAnchor({
        left: Math.min(Math.max(rect.left + rect.width / 2 - containerRect.left, 84), containerRect.width - 84),
        text,
        top: rect.bottom - containerRect.top + 8,
      });
    };
    // The selection changes continuously while a handle drags; settle first so
    // the toolbar lands once instead of chasing the handle.
    const schedule = (): void => {
      window.clearTimeout(timer);
      timer = window.setTimeout(measure, 250);
    };
    document.addEventListener('selectionchange', schedule);
    const container = containerRef.current;
    container?.addEventListener('scroll', schedule, true);
    return () => {
      window.clearTimeout(timer);
      document.removeEventListener('selectionchange', schedule);
      container?.removeEventListener('scroll', schedule, true);
    };
  }, [containerRef]);

  if (!anchor) {
    return null;
  }
  // pointerdown + preventDefault acts before the tap collapses the selection.
  return (
    <div
      className='absolute z-30 flex -translate-x-1/2 select-none items-center overflow-hidden rounded-full bg-popover text-popover-foreground shadow-lg ring-1 ring-foreground/10'
      style={{ left: anchor.left, top: anchor.top }}
    >
      <button
        className='flex items-center gap-1.5 px-3 py-1.5 text-sm'
        onPointerDown={(event) => {
          event.preventDefault();
          document.execCommand('copy');
          window.getSelection()?.removeAllRanges();
        }}
        type='button'
      >
        <IconCopy aria-hidden='true' className='size-4' />
        Copy
      </button>
      {addToChatEnabled ? (
        <button
          className='flex items-center gap-1.5 border-l border-foreground/10 px-3 py-1.5 text-sm'
          onPointerDown={(event) => {
            event.preventDefault();
            onAddToChat(anchor.text);
            window.getSelection()?.removeAllRanges();
          }}
          type='button'
        >
          <IconBlockquote aria-hidden='true' className='size-4' />
          Add to Chat
        </button>
      ) : null}
    </div>
  );
}

export function SessionChatView({
  agentLabel,
  canSend = true,
  chatBarExtensions = [],
  chatBarPanelState,
  className,
  commandCatalog,
  customTranscriptWidthEnabled = false,
  diagnosticLog,
  hostActions,
  hostComposerBridge,
  hostSessionNoteBridge,
  hostLinks,
  monacoVsBaseUrl,
  nativeSelectionMenus = false,
  onSwitchToTerminalForAgentPicker,
  onDelayedActions,
  onChatBarBridgeRequest,
  onChatBarPanelStateChange,
  onSelectForkBranch,
  previewText,
  sendOnEnter = true,
  sessionKey,
  sessionTitle,
  hostSearchBridge,
  searchLayout = 'inline',
  showNewSessionWelcomeTitle = true,
  showVerbosePill = true,
  theme = 'dark',
  transport,
  verboseMode = false,
  working,
}: SessionChatViewProps) {
  useEffect(() => {
    // Chat dropdowns are portaled outside this root. Stamp the chat-only
    // palette on body so those explicitly scoped popup surfaces match.
    document.body.dataset.sessionChatTheme = theme;
  }, [theme]);
  /*
  CDXC:DraftSessions 2026-08-28:
  Which agent this chat renders as. `agentLabel` is the host's BOOT-TIME value
  (a URL parameter on desktop), and a draft's agent can change under it: picking
  a different agent in the composer's "Agents" section rewrites the session's
  identity, and nothing reloads the page. The read state is the live truth, so
  everything agent-shaped below — slash commands, option catalogs, the empty
  state's logo and headline, the skills heading — follows it.

  It is held in state rather than derived inline because of a genuine ordering
  cycle: the transcript hook is SEEDED with this agent's command catalog, and
  the hook is what produces the read state. The entry is stamped with the
  transport it was read through, so a different session never inherits it — the
  view falls straight back to the new host label until that session's own read
  lands.
  */
  const [readAgentEntry, setReadAgentEntry] = useState<{
    agent: string | null;
    transport: SessionChatTransport;
  } | null>(null);
  const agentLabelFromRead = readAgentEntry?.transport === transport ? readAgentEntry.agent : null;
  const resolvedAgentLabel = agentLabelFromRead ?? agentLabel ?? null;
  const slashCommands = useMemo(() => sessionChatSlashCommandsForAgent(resolvedAgentLabel), [resolvedAgentLabel]);
  // The option pills type commands the "/" picker does not offer (/effort,
  // /fast). They still have to classify as commands so a dispatched pill
  // renders the same muted "Ran /model sonnet" row a typed one does.
  const slashCommandNames = useMemo(
    () => [...slashCommands.map((command) => command.name), ...sessionChatOptionCommandNames(resolvedAgentLabel)],
    [resolvedAgentLabel, slashCommands]
  );
  const chat = useSessionChat({
    commandCatalog: commandCatalog ?? slashCommandNames,
    previewText,
    transport,
    working,
    ...(diagnosticLog ? { diagnosticLog } : {}),
  });
  /*
  The transcript's working gate. `chat.view.isWorking` settles the moment the
  turn lifecycle looks terminal, but the session process can still be running
  then (hooks, background tasks, an immediate follow-up turn) with the
  user-visible session status still saying "working" — and the transcript
  folding the turn into "Worked for Xs" in that window is exactly the mid-run
  fold flash. So the list also holds on `chat.workingSignal`, the raw live
  signal, and only settles once BOTH agree the session is quiet. Stop-vs-Send
  and the composer keep `chat.working` so they cannot get stuck on a stale
  signal.
  */
  const transcriptWorking = (chat.view.kind === 'ready' && chat.view.isWorking) || chat.workingSignal;
  /*
  CDXC:DraftSessions 2026-08-28:
  The draft the switcher acts on. `availableAgents` is present only while the
  session IS a draft, so its absence is what hides the "Agents" section on a
  promoted session — and `sessionAgentId` (not the transcript family in
  `chat.agent`) is what ticks the current row, because a project custom agent
  built on Claude reports `claude` there.
  */
  const draftAgents = chat.availableAgents;
  const draftAgentRow = useMemo(
    () => draftAgents?.find((row) => row.agentId === chat.sessionAgentId) ?? null,
    [chat.sessionAgentId, draftAgents]
  );
  /*
  The chat-supported family the read state names: the draft's base family when
  the session is a draft (a custom agent's own id has no catalog of its own),
  otherwise the transcript family gxserver resolved.
  */
  const readStateAgent = draftAgentRow?.baseAgentId ?? chat.agent ?? null;
  const chatRefresh = chat.refresh;
  useEffect(() => {
    setReadAgentEntry({ agent: readStateAgent, transport });
  }, [readStateAgent, transport]);
  const composerRef = useRef<SessionChatComposerHandle | null>(null);
  const draftAgentSwitchTimersRef = useRef<ReturnType<typeof setTimeout>[]>([]);
  const clearDraftAgentSwitchTimers = useCallback((): void => {
    for (const timer of draftAgentSwitchTimersRef.current) {
      clearTimeout(timer);
    }
    draftAgentSwitchTimersRef.current = [];
  }, []);
  // Unmount, and a move to another session: a follow-up read belongs to the
  // draft it was armed for and to nothing else.
  useEffect(() => clearDraftAgentSwitchTimers, [clearDraftAgentSwitchTimers, transport]);
  const switchDraftAgent = useMemo(() => {
    const switchAgent = transport.switchDraftAgent?.bind(transport);
    return switchAgent
      ? async (agentId: string): Promise<void> => {
          /*
          CDXC:DraftAgentSwitch 2026-08-28:
          The switch restarts the agent CLI inside the draft's pane, so the
          typed-but-unsent text is persisted BEFORE the request goes out: the
          localStorage copy synchronously, and gxserver's durable copy through
          the composer's own sync. Nothing here unmounts the composer any more,
          which makes this belt-and-braces — but the text is the one thing in
          this flow that cannot be reconstructed if something does go wrong.
          */
          composerRef.current?.flushDraft();
          clearDraftAgentSwitchTimers();
          /*
          The agent identity lives on the read state alone — no frame type
          carries it — so the switch is followed by a read, which is also what
          re-seeds the option catalogs and the "/" command set for the agent
          now running. The read happens on failure too: the most likely
          refusal is "this session is no longer a draft", and then the switcher
          itself is what is out of date. The rejection still reaches the pills,
          which show the daemon's own sentence.
          */
          try {
            await switchAgent({ agentId });
          } finally {
            chatRefresh();
            // The new CLI is launched after this call returns; see the delays.
            for (const delay of DRAFT_AGENT_SWITCH_REREAD_DELAYS_MS) {
              draftAgentSwitchTimersRef.current.push(setTimeout(chatRefresh, delay));
            }
          }
        }
      : undefined;
  }, [chatRefresh, clearDraftAgentSwitchTimers, transport]);
  const initialTranscriptLoading = chat.view.kind === 'loading';
  /*
  How far the blank hold has been allowed to progress. Keyed to the moment
  loading started, so it restarts from 'blank' whenever loading clears or the
  session identity changes — an already-loaded conversation never inherits a
  previous session's expired timers.
  */
  const [loadingStage, setLoadingStage] = useState<'blank' | 'indicator' | 'retry'>('blank');
  useEffect(() => {
    setLoadingStage('blank');
    if (!initialTranscriptLoading) {
      return;
    }
    const indicatorTimer = setTimeout(() => setLoadingStage('indicator'), LOADING_INDICATOR_DELAY_MS);
    const retryTimer = setTimeout(() => setLoadingStage('retry'), LOADING_RETRY_DELAY_MS);
    return () => {
      clearTimeout(indicatorTimer);
      clearTimeout(retryTimer);
    };
  }, [initialTranscriptLoading, sessionKey, transport]);
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
    agent: resolvedAgentLabel,
    /*
    CDXC:DraftAgentSwitch 2026-08-28:
    Drafts only — `chat.sessionAgentId` is carried for every session, and a
    session that has never been a draft must keep reading the options it
    already stored under the unsuffixed key.
    */
    draftAgentId: draftAgents !== null ? chat.sessionAgentId : null,
    ...(sessionKey !== undefined ? { sessionKey } : {}),
  });
  // null = this chat has never been toggled, so it follows the global setting.
  const [verboseOverride, setVerboseOverride] = useState<boolean | null>(() =>
    readStoredSessionChatVerbose(sessionKey)
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
  const [summaryMode, setSummaryMode] = useState(() => readStoredSessionChatSummary(sessionKey));
  useEffect(() => {
    setSummaryMode(readStoredSessionChatSummary(sessionKey));
  }, [sessionKey]);
  const toggleSummary = useCallback(() => {
    const next = !summaryMode;
    writeStoredSessionChatSummary(sessionKey, next);
    setSummaryMode(next);
  }, [sessionKey, summaryMode]);
  useEffect(() => {
    const handleSummaryHotkey = (event: globalThis.KeyboardEvent): void => {
      if (ghostexHotkeyTextFromKeyboardEvent(event) !== sessionChatSummaryToggleHotkey()) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      if (!event.repeat) {
        toggleSummary();
      }
    };
    window.addEventListener('keydown', handleSummaryHotkey, true);
    return () => window.removeEventListener('keydown', handleSummaryHotkey, true);
  }, [toggleSummary]);
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
  /*
  CDXC:SessionChatStashBadge 2026-08-24:
  The stash control carries how many prompts are already stashed from THIS
  conversation, which only the host can answer (it owns the gxserver
  connection). The count is keyed on the provider conversation id so it
  survives a compaction-resume rewrite, and it is re-read on every event that
  can change it behind this page's back: a new conversation, a stash from this
  composer, and a return of window focus — the Saved Prompts modal is a
  separate native window and can delete rows while this page sits idle.
  */
  const [stashedPromptCount, setStashedPromptCount] = useState(0);
  const countSessionStashedPrompts = hostComposerBridge?.countSessionStashedPrompts;
  const chatAgentSessionId = chat.agentSessionId;
  // Answers that land after the chat moved on must not paint the previous
  // conversation's count, so every read carries the generation it started in.
  const stashedPromptCountGenerationRef = useRef(0);
  const refreshStashedPromptCount = useCallback((): void => {
    if (!countSessionStashedPrompts) {
      return;
    }
    // Every read claims a fresh generation, so a slower read started under the
    // previous conversation id (compaction rewrites it mid-session) can never
    // land after this one and paint a stale count.
    const generation = ++stashedPromptCountGenerationRef.current;
    void countSessionStashedPrompts(chatAgentSessionId)
      .then((count) => {
        if (stashedPromptCountGenerationRef.current !== generation) {
          return;
        }
        setStashedPromptCount(Number.isFinite(count) && count > 0 ? Math.floor(count) : 0);
      })
      .catch(() => {
        // A count that cannot be read only hides the badge.
      });
  }, [chatAgentSessionId, countSessionStashedPrompts]);
  useEffect(() => {
    // Runs before the refresh effect below, so the new conversation starts from
    // no badge and every in-flight read for the old one is discarded.
    stashedPromptCountGenerationRef.current += 1;
    setStashedPromptCount(0);
  }, [transport]);
  useEffect(() => {
    refreshStashedPromptCount();
  }, [refreshStashedPromptCount, transport]);
  useEffect(() => {
    if (!countSessionStashedPrompts) {
      return;
    }
    const handleFocus = (): void => {
      refreshStashedPromptCount();
    };
    window.addEventListener('focus', handleFocus);
    return () => {
      window.removeEventListener('focus', handleFocus);
    };
  }, [countSessionStashedPrompts, refreshStashedPromptCount]);
  const stashComposerDraft = useCallback((): void => {
    const composer = composerRef.current;
    const draft = composer?.getDraft() ?? '';
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
        // The badge moves with the click, then the host's own count corrects
        // it — a save that matched a prompt the user had already stashed by
        // hand adds no row.
        setStashedPromptCount((count) => count + 1);
        refreshStashedPromptCount();
      })
      .catch(() => {
        // Keep the draft intact so a failed stash can be retried.
      });
  }, [hostComposerBridge, refreshStashedPromptCount]);
  const handoffComposerDraft = useCallback(async (): Promise<SessionChatComposerHandoff> => {
    const composer = composerRef.current;
    const draft = composer?.getDraft() ?? '';
    if (!draft.trim()) {
      if (draft.length > 0) {
        composer?.clearDraft(draft);
      }
      return { content: '' };
    }
    const stashPrompt = hostComposerBridge?.stashPrompt;
    if (!stashPrompt) {
      /*
      Clearing the composer here would make the host's in-memory copy the only
      copy of the text. A host that cannot stash simply cannot move a draft, so
      say so and stay in chat with every character intact.
      */
      throw new Error('This host cannot move the draft out of chat.');
    }
    const stashed = await stashPrompt(draft, { transient: true });
    // The exact snapshot that became durable must still own the composer.
    // If more text arrived during the save, remain in chat with all text
    // intact instead of switching with a partial draft. The stash row created
    // above stays in Saved Prompts: a visible duplicate is the correct price
    // for never being able to lose the text.
    if (composerRef.current?.clearDraft(draft) !== true) {
      throw new Error('The draft changed while it was being moved.');
    }
    return {
      content: draft,
      ...(stashed?.promptId ? { stashedPromptId: stashed.promptId } : {}),
    };
  }, [hostComposerBridge]);
  useEffect(() => {
    if (!hostComposerBridge || initialTranscriptLoading) {
      return;
    }
    return hostComposerBridge.register({
      clearDraft: (expectedContent) => composerRef.current?.clearDraft(expectedContent) ?? false,
      focus: () => composerRef.current?.focus(),
      handoffToTerminal: handoffComposerDraft,
      insertPrompt: (content) => composerRef.current?.insertSavedPrompt(content) ?? false,
      requestStash: stashComposerDraft,
    });
  }, [handoffComposerDraft, hostComposerBridge, initialTranscriptLoading, stashComposerDraft]);
  const reportDraftState = hostComposerBridge?.reportDraftState;
  const reportComposerDraftState = useCallback(
    (empty: boolean) => {
      reportDraftState?.({ empty });
    },
    [reportDraftState]
  );
  const pasteImage = useMemo(() => {
    const saveImage = transport.saveImage?.bind(transport);
    return saveImage
      ? async (payload: { base64Data: string; suggestedName?: string }) => (await saveImage(payload)).path
      : undefined;
  }, [transport]);
  const attachFile = useMemo(() => {
    const saveAttachment = transport.saveAttachment?.bind(transport);
    return saveAttachment
      ? async (payload: { base64Data: string; suggestedName?: string }) => (await saveAttachment(payload)).path
      : undefined;
  }, [transport]);
  const pickPaths = useMemo(() => {
    const pickAttachmentPaths = transport.pickAttachmentPaths?.bind(transport);
    return pickAttachmentPaths ? () => pickAttachmentPaths() : undefined;
  }, [transport]);
  const nativeDropPaths = useMemo(() => {
    const readDropPaths = transport.readDropPaths?.bind(transport);
    return readDropPaths ? () => readDropPaths() : undefined;
  }, [transport]);
  const saveImageAs = useMemo(() => {
    const save = transport.saveImageAs?.bind(transport);
    return save ? (params: { base64Data: string; suggestedName: string }) => save(params) : undefined;
  }, [transport]);
  const listMessageMarkdownPaths = useMemo(() => {
    const list = transport.listMessageMarkdownPaths?.bind(transport);
    return list ? () => list() : undefined;
  }, [transport]);
  const saveMessageMarkdown = useMemo(() => {
    const save = transport.saveMessageMarkdown?.bind(transport);
    return save ? (params: { content: string; path: string }) => save(params) : undefined;
  }, [transport]);
  /*
  CDXC:SessionChatComposerReady 2026-08-26:
  Evidence read for the composer's `composerNotReady` notice. Bound the same way
  as every other optional transport call here, so a host without the endpoint
  hands the composer nothing and the notice drops its terminal excerpt.
  */
  const readTerminalTail = useMemo(() => {
    const read = transport.readTerminalTail?.bind(transport);
    return read ? () => read() : undefined;
  }, [transport]);
  /*
  CDXC:SessionForkFamilies 2026-08-28:
  The branch switcher's one read. Bound like every other optional transport call
  here, so a host without a route to `/api/sessionForkBranches` hands the
  switcher nothing and it never renders.
  */
  const loadForkBranches = useMemo(() => {
    const load = transport.forkBranches?.bind(transport);
    return load ? () => load() : undefined;
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
  /*
  CDXC:SessionAgentNotes 2026-08-24:
  The note is filed under the PROVIDER conversation id, so the control appears
  only once this session has one — before that there is nothing to key a note
  to and gxserver would refuse the save. Both transport methods are required:
  a host that could save but not read would open an empty editor over an
  existing note and overwrite it on the first blur.
  */
  const [noteOpen, setNoteOpen] = useState(false);
  const readSessionNote = useMemo(() => {
    const read = transport.readSessionNote?.bind(transport);
    return read ? () => read() : undefined;
  }, [transport]);
  const saveSessionNote = useMemo(() => {
    const save = transport.saveSessionNote?.bind(transport);
    return save ? (note: string) => save(note) : undefined;
  }, [transport]);
  const sessionNoteAvailable =
    readSessionNote !== undefined && saveSessionNote !== undefined && chat.agentSessionId !== null;
  const [sessionNoteHasText, setSessionNoteHasText] = useState(false);
  const sessionNotePresenceGenerationRef = useRef(0);
  const refreshSessionNotePresence = useCallback((): void => {
    if (!sessionNoteAvailable || !readSessionNote) {
      return;
    }
    const generation = ++sessionNotePresenceGenerationRef.current;
    void readSessionNote()
      .then((result) => {
        if (sessionNotePresenceGenerationRef.current !== generation) {
          return;
        }
        setSessionNoteHasText((result.note ?? '').trim() !== '');
      })
      .catch(() => {
        // Keep the last known indicator state when the daemon cannot be read.
      });
  }, [readSessionNote, sessionNoteAvailable]);
  useEffect(() => {
    // Notes belong to one conversation: switching sessions (or losing the
    // provider id) must not leave the previous session's panel open.
    setNoteOpen(false);
    sessionNotePresenceGenerationRef.current += 1;
    setSessionNoteHasText(false);
    refreshSessionNotePresence();
  }, [chat.agentSessionId, refreshSessionNotePresence, sessionNoteAvailable, transport]);
  useEffect(() => {
    if (!sessionNoteAvailable) {
      return;
    }
    const handleFocus = (): void => refreshSessionNotePresence();
    window.addEventListener('focus', handleFocus);
    return () => window.removeEventListener('focus', handleFocus);
  }, [refreshSessionNotePresence, sessionNoteAvailable]);
  const toggleSessionNote = useCallback((): void => {
    setNoteOpen((open) => !open);
  }, []);
  const closeSessionNote = useCallback((): void => {
    setNoteOpen(false);
  }, []);
  useEffect(() => {
    if (!hostSessionNoteBridge || !sessionNoteAvailable) {
      return;
    }
    return hostSessionNoteBridge.register({
      open: () => {
        setNoteOpen(true);
      },
    });
  }, [hostSessionNoteBridge, sessionNoteAvailable]);
  const [questionActive, setQuestionActive] = useState(false);
  const diagnosticLogRef = useRef(diagnosticLog);
  diagnosticLogRef.current = diagnosticLog;
  // Breadcrumbs for the composer-affecting transitions only: a question flip
  // unmounts the composer, a view-kind change unmounts the whole pane body,
  // and a prompt-kind change is the raw server signal behind the first two.
  const promptKind = chat.prompt?.kind ?? 'none';
  useEffect(() => {
    diagnosticLogRef.current?.('sessionChat.promptKindChanged', { kind: promptKind });
  }, [promptKind]);
  useEffect(() => {
    diagnosticLogRef.current?.('sessionChat.questionActiveChanged', { active: questionActive });
  }, [questionActive]);
  const viewKind = chat.view.kind;
  // The inputs selectSessionChatViewState decided from, snapshotted every
  // render so the kind-change breadcrumb reports the values that produced it.
  const viewKindInputsRef = useRef<Record<string, unknown>>({});
  viewKindInputsRef.current = {
    hasAgentSessionId: chat.agentSessionId !== null,
    hasError: chat.error !== null,
    messageCount: chat.messages.length,
    status: chat.status,
    working: chat.working,
  };
  useEffect(() => {
    diagnosticLogRef.current?.('sessionChat.viewKindChanged', {
      kind: viewKind,
      ...viewKindInputsRef.current,
    });
  }, [viewKind]);
  useEffect(() => {
    diagnosticLogRef.current?.('sessionChat.loadingStageChanged', { stage: loadingStage });
  }, [loadingStage]);
  useEffect(() => {
    diagnosticLogRef.current?.('sessionChat.workingChanged', { working: chat.working });
  }, [chat.working]);
  // Cards stacked above the composer own their own visibility (per-detection
  // dismissal, prompt identity), so each reports it back here. While one is up
  // the new-session headline stands down instead of competing for the same
  // vertical space.
  const [noticeCardVisible, setNoticeCardVisible] = useState(false);
  const [interactiveCardVisible, setInteractiveCardVisible] = useState(false);
  const chatRootRef = useRef<HTMLDivElement | null>(null);
  const transcriptRef = useRef<HTMLDivElement | null>(null);
  const [transcriptSelection, setTranscriptSelection] = useState('');
  const [transcriptFilePath, setTranscriptFilePath] = useState<string | null>(null);

  const interrupt = useCallback((): void => {
    void chat.interrupt();
  }, [chat]);

  // A terminal-notice `sendKeys` action writes its raw bytes through the
  // approval lane of answerSessionChatPrompt — the same verbatim-write path the
  // interactive card's Allow/Deny buttons use.
  const chatAnswerPrompt = chat.answerPrompt;
  const sendNoticeKeys = useCallback(
    (send: string): Promise<void> => chatAnswerPrompt({ approvalSend: send, kind: 'approval' }),
    [chatAnswerPrompt]
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
        await chatAnswerPrompt({ choiceIndex, kind: 'terminalChoice' });
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
    [chatAnswerPrompt, noticeKey]
  );
  const terminalChoicePending = (chat.terminalNotice?.choices?.length ?? 0) > 0 && noticeKey !== retiredNoticeKey;
  const [sessionOptionSwitching, setSessionOptionSwitching] = useState(false);
  const composerEnabled = canSend && !terminalChoicePending && !sessionOptionSwitching;

  // A command the user types themselves reconciles the pills (§1.4), so the
  // Model pill follows a hand-typed "/model opus" without a second dispatch.
  const chatSend = chat.send;
  const reconcileTypedCommand = sessionOptions.reconcileTypedCommand;
  const isDraft = draftAgents !== null;
  const send = useCallback(
    async (text: string): Promise<void> => {
      reconcileTypedCommand(text);
      await chatSend(text);
      /*
      CDXC:DraftSessions 2026-08-28:
      A delivered prompt PROMOTES a draft server-side (option commands like
      /model are carved out and do not). Whether it did is only visible on the
      read state — `availableAgents` simply stops being sent — so a send from a
      draft is followed by a read, which is what retires the "Agents" section
      once the conversation exists.
      */
      if (isDraft) {
        chatRefresh();
      }
    },
    [chatRefresh, chatSend, isDraft, reconcileTypedCommand]
  );

  /*
  CDXC:DraftSessions 2026-08-28:
  Fork and Full reload have nothing to act on while the session is a draft:
  there is no conversation to fork, and no agent run to reload — the CLI has
  never been given a prompt. The action list is host-built and the draft flag is
  read state, so this view is the one place that holds both; filtering here
  covers every surface the list reaches (desktop, web, mobile) at once. Rename,
  Sleep, Delayed actions, Prompt editor, Stash prompt, Saved prompts, Attach and
  Export are untouched — they all still mean something on a draft.
  */
  const draftAwareHostActions = useMemo(() => {
    const actions = hostActions?.actions;
    if (!isDraft || !hostActions || !actions) {
      return hostActions;
    }
    const kept = actions.filter((action) => !DRAFT_HIDDEN_HOST_ACTION_IDS.has(action.id));
    return kept.length === actions.length ? hostActions : { ...hostActions, actions: kept };
  }, [hostActions, isDraft]);

  // Typing anywhere in the pane lands in the composer (§11.1): a single
  // printable character without Ctrl/Meta is redirected; unmodified
  // Backspace/Delete focuses the composer without inserting anything.
  const handleKeyDownCapture = useCallback(
    (event: ReactKeyboardEvent<HTMLDivElement>): void => {
      if (event.defaultPrevented || questionActive) {
        return;
      }
      const target = event.target as HTMLElement | null;
      if (target?.closest?.(INTERACTIVE_TARGET_SELECTOR)) {
        return;
      }
      if ((event.key === 'Backspace' || event.key === 'Delete') && !event.metaKey && !event.ctrlKey && !event.altKey) {
        composerRef.current?.focus();
        return;
      }
      if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.nativeEvent.isComposing) {
        if (composerRef.current?.insertTypedText(event.key)) {
          event.preventDefault();
          event.stopPropagation();
        }
      }
    },
    [questionActive]
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
    [questionActive]
  );

  const handleDragOverCapture = useCallback(
    (event: DragEvent<HTMLDivElement>): void => {
      if (
        (!attachFile && !pickPaths) ||
        !composerEnabled ||
        questionActive ||
        !sessionChatDataTransferHasFiles(event.dataTransfer)
      ) {
        return;
      }
      event.preventDefault();
      event.dataTransfer.dropEffect = 'copy';
    },
    [attachFile, composerEnabled, pickPaths, questionActive]
  );

  const handleDropCapture = useCallback(
    (event: DragEvent<HTMLDivElement>): void => {
      if (questionActive || !composerRef.current?.attachDroppedFiles(event.dataTransfer)) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
    },
    [questionActive]
  );

  const captureTranscriptContext = useCallback((event: MouseEvent<HTMLDivElement>): void => {
    setTranscriptSelection(readTranscriptSelection(transcriptRef.current));
    const target = event.target instanceof Element ? event.target : null;
    const fileChip = target?.closest(`[${SESSION_CHAT_FILE_PATH_ATTRIBUTE}]`);
    setTranscriptFilePath(
      fileChip && event.currentTarget.contains(fileChip)
        ? fileChip.getAttribute(SESSION_CHAT_FILE_PATH_ATTRIBUTE)
        : null
    );
  }, []);

  const copyTranscriptFilePath = useCallback((): void => {
    if (transcriptFilePath === null) {
      return;
    }
    void navigator.clipboard.writeText(transcriptFilePath).catch((error: unknown) => {
      console.error('[session-chat] file path clipboard write failed', error);
    });
  }, [transcriptFilePath]);

  const copyTranscriptSelection = useCallback((): void => {
    if (transcriptSelection === '') {
      return;
    }
    void navigator.clipboard.writeText(transcriptSelection).catch((error: unknown) => {
      console.error('[session-chat] transcript clipboard write failed', error);
    });
  }, [transcriptSelection]);

  const addTranscriptSelectionToChat = useCallback((): void => {
    if (transcriptSelection === '') {
      return;
    }
    composerRef.current?.appendText(asMarkdownQuote(transcriptSelection));
  }, [transcriptSelection]);

  // The initial read cannot yet distinguish an existing transcript from a
  // genuinely empty session. Keep that indeterminate phase visually blank so
  // an existing conversation never flashes the new-session welcome/composer.
  // Blank is only the FIRST stage though: a read or socket that stalls here
  // would otherwise leave nothing on screen and no way out but leaving the
  // session, so the wait becomes visible and then offers a manual recycle.
  if (initialTranscriptLoading) {
    return (
      <div
        aria-busy='true'
        className={cn(
          'ghostex-session-chat-scope flex h-full min-h-0 items-center justify-center bg-background text-foreground [--radius:0.625rem]',
          theme === 'dark' && 'dark',
          className
        )}
        data-chat-custom-transcript-width={customTranscriptWidthEnabled ? 'true' : 'false'}
        data-chat-theme={theme}
        {...(nativeSelectionMenus
          ? {}
          : { onContextMenu: (event: MouseEvent<HTMLDivElement>) => event.preventDefault() })}
      >
        {loadingStage === 'blank' ? null : (
          <div className='flex flex-col items-center gap-3 text-muted-foreground text-sm'>
            <div className='flex items-center gap-2'>
              <IconLoader2 aria-hidden='true' className='size-4 animate-spin' stroke={2} />
              <span>Loading conversation…</span>
            </div>
            {loadingStage === 'retry' ? (
              <Button onClick={chat.retry} size='sm' variant='outline'>
                Retry
              </Button>
            ) : null}
          </div>
        )}
      </div>
    );
  }

  const emptyKind =
    chat.view.kind === 'ready' ? null : chat.view.kind === 'error' ? ('error' as const) : chat.view.kind;
  const bottomCardVisible = noticeCardVisible || interactiveCardVisible;
  const showNewSessionWelcome =
    // A new agent reports `starting` until its first transcript file exists.
    // Keep the designed welcome visible throughout that pre-transcript window.
    emptyKind === 'starting' || emptyKind === 'empty';

  return (
    <TooltipProvider>
      <div
        className={cn(
          // The app theme zeroes --radius for its square chrome; restore the
          // shadcn default inside the chat so bubbles and cards keep their
          // rounded look. The scope class lifts the SquareTheme border-radius
          // override (packages/core-ui/styles.css) for controls inside the chat.
          'ghostex-session-chat-scope relative flex h-full min-h-0 flex-col bg-background text-foreground outline-none [--radius:0.625rem]',
          theme === 'dark' && 'dark',
          className
        )}
        data-chat-custom-transcript-width={customTranscriptWidthEnabled ? 'true' : 'false'}
        data-chat-theme={theme}
        {...(nativeSelectionMenus
          ? {}
          : { onContextMenu: (event: MouseEvent<HTMLDivElement>) => event.preventDefault() })}
        onKeyDownCapture={handleKeyDownCapture}
        onPasteCapture={handlePasteCapture}
        onDragOverCapture={handleDragOverCapture}
        onDropCapture={handleDropCapture}
        ref={chatRootRef}
        tabIndex={-1}
      >
        <SessionChatImageViewerProvider
          {...(loadImageDataUrl ? { loadImage: loadImageDataUrl } : {})}
          {...(saveImageAs ? { saveImageAs } : {})}
          {...(sessionTitle ? { sessionTitle } : {})}
        >
          <SessionChatHostLinksProvider {...(hostLinks ? { links: hostLinks } : {})}>
            <div className='relative flex min-h-0 flex-1 flex-col'>
              <SessionChatSearch
                {...(hostSearchBridge ? { hostBridge: hostSearchBridge } : {})}
                layout={searchLayout}
                rootRef={chatRootRef}
                searchRevision={chat.messages}
              />
              <div className='relative flex min-h-0 flex-1 flex-col'>
                {/*
                CDXC:SessionForkFamilies 2026-08-28:
                The chat has no title bar of its own (desktop draws the title
                natively, the web app draws it in the workspace chrome), so the
                branch switcher owns this thin strip above the transcript. It
                renders nothing at all until the daemon reports a family of two
                or more, which is why an unforked session shows no empty row.
                */}
                <div className='mx-auto flex w-full max-w-3xl flex-none justify-end px-4 pt-1 empty:hidden'>
                  <SessionChatForkBranchSwitcher
                    {...(loadForkBranches ? { loadBranches: loadForkBranches } : {})}
                    {...(onSelectForkBranch ? { onSelectBranch: onSelectForkBranch } : {})}
                    sessionKey={sessionKey ?? 'session-chat'}
                  />
                </div>
                <div className='flex min-h-0 flex-1 flex-col'>
                  {chat.view.kind === 'ready' ? (
                    nativeSelectionMenus ? (
                      <div className='relative flex min-h-0 flex-1 select-text' ref={transcriptRef}>
                        <SessionChatMessageList
                          hasMore={chat.hasMore}
                          isWorking={transcriptWorking}
                          loadingEarlier={chat.loadingEarlier}
                          messages={chat.messages}
                          onLoadEarlier={chat.loadEarlier}
                          {...(listMessageMarkdownPaths ? { listMessageMarkdownPaths } : {})}
                          {...(saveMessageMarkdown ? { saveMessageMarkdown } : {})}
                          sessionTitle={sessionTitle}
                          terminalActivity={chat.terminalActivity}
                          theme={theme}
                          summaryMode={summaryMode}
                          verboseMode={verbose}
                        />
                        <TranscriptSelectionToolbar
                          addToChatEnabled={composerEnabled && !questionActive}
                          containerRef={transcriptRef}
                          onAddToChat={(text) => composerRef.current?.appendText(asMarkdownQuote(text))}
                        />
                      </div>
                    ) : (
                      <ContextMenu>
                        <ContextMenuTrigger
                          className='flex min-h-0 flex-1 select-text'
                          onContextMenu={captureTranscriptContext}
                          ref={transcriptRef}
                        >
                          <SessionChatMessageList
                            hasMore={chat.hasMore}
                            isWorking={transcriptWorking}
                            loadingEarlier={chat.loadingEarlier}
                            messages={chat.messages}
                            onLoadEarlier={chat.loadEarlier}
                            {...(listMessageMarkdownPaths ? { listMessageMarkdownPaths } : {})}
                            {...(saveMessageMarkdown ? { saveMessageMarkdown } : {})}
                            sessionTitle={sessionTitle}
                            terminalActivity={chat.terminalActivity}
                            theme={theme}
                            summaryMode={summaryMode}
                            verboseMode={verbose}
                          />
                        </ContextMenuTrigger>
                        <ContextMenuContent>
                          <ContextMenuGroup>
                            {transcriptFilePath !== null ? (
                              <ContextMenuItem onClick={copyTranscriptFilePath}>
                                <IconCopy aria-hidden='true' />
                                Copy path
                              </ContextMenuItem>
                            ) : null}
                            {transcriptFilePath === null || transcriptSelection !== '' ? (
                              <ContextMenuItem disabled={transcriptSelection === ''} onClick={copyTranscriptSelection}>
                                <IconCopy aria-hidden='true' />
                                Copy
                              </ContextMenuItem>
                            ) : null}
                            {transcriptSelection !== '' ? (
                              <ContextMenuItem
                                disabled={!composerEnabled || questionActive}
                                onClick={addTranscriptSelectionToChat}
                              >
                                <IconBlockquote aria-hidden='true' />
                                Add to Chat
                              </ContextMenuItem>
                            ) : null}
                          </ContextMenuGroup>
                        </ContextMenuContent>
                      </ContextMenu>
                    )
                  ) : showNewSessionWelcome ? (
                    <NewSessionWelcome
                      agentLabel={resolvedAgentLabel}
                      {...(draftAgentRow ? { agentIcon: draftAgentRow.icon, agentName: draftAgentRow.name } : {})}
                      showTitle={showNewSessionWelcomeTitle && !bottomCardVisible}
                    />
                  ) : emptyKind ? (
                    chat.view.kind === 'error' ? (
                      <EmptyState
                        detail={sessionChatEmptyStateCopy('error').detail}
                        title={sessionChatEmptyStateCopy('error').title}
                      />
                    ) : (
                      <EmptyState
                        detail={sessionChatEmptyStateCopy(emptyKind, resolvedAgentLabel).detail}
                        title={sessionChatEmptyStateCopy(emptyKind, resolvedAgentLabel).title}
                      />
                    )
                  ) : null}
                </div>
                <div className='mx-auto grid w-full max-w-3xl flex-none gap-2 px-4 pt-2 pb-3'>
                  <SessionChatTerminalNoticeCard
                    canSend={canSend}
                    notice={chat.terminalNotice}
                    onAnswerChoice={answerNoticeChoice}
                    onSendKeys={sendNoticeKeys}
                    onVisibleChange={setNoticeCardVisible}
                    {...(sessionKey !== undefined ? { sessionKey } : {})}
                    {...(hostActions?.onSwitchToTerminal ? { onSwitchToTerminal: hostActions.onSwitchToTerminal } : {})}
                    {...(hostActions?.switchViewShortcut
                      ? { switchToTerminalShortcut: hostActions.switchViewShortcut }
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
                  {/*
                  While a question card shows, the composer hides instead of
                  unmounting: unmounting disposed the Monaco editor on every
                  question flip (a visible hitch) and destroyed the caret
                  position and focus, so a transient prompt-detection flap —
                  or just answering a question — cost the user their typing
                  state. display:contents keeps the grid layout identical
                  when visible.
                  */}
                  <div className={questionActive ? 'hidden' : 'contents'}>
                    {noteOpen && readSessionNote && saveSessionNote ? (
                      <SessionChatNotePanel
                        /*
                        NOT the bare sessionKey: the composer sibling below
                        already uses it, and duplicate keys among siblings
                        break reconciliation (the panel stopped unmounting on
                        close). The prefix keeps the per-session state reset
                        without colliding.
                        */
                        key={`session-note:${sessionKey}`}
                        onClose={closeSessionNote}
                        onHasNoteChange={setSessionNoteHasText}
                        readNote={readSessionNote}
                        saveNote={saveSessionNote}
                      />
                    ) : null}
                    <SessionChatComposer
                      agentFleet={chat.agentFleet}
                      {...(diagnosticLog ? { diagnosticLog } : {})}
                      disabled={!composerEnabled}
                      draftSync={chat.draft}
                      isWorking={chat.working}
                      key={sessionKey}
                      monacoVsBaseUrl={monacoVsBaseUrl}
                      nativeContextMenu={nativeSelectionMenus}
                      queue={chat.queue}
                      sessionKey={sessionKey}
                      theme={theme}
                      onAttachFile={attachFile}
                      onInterrupt={interrupt}
                      onLoadImagePreview={loadImageDataUrl}
                      onNativeDropPaths={nativeDropPaths}
                      onPasteImage={pasteImage}
                      onPickPaths={pickPaths}
                      {...(readTerminalTail ? { onReadTerminalTail: readTerminalTail } : {})}
                      onSend={send}
                      sendOnEnter={sendOnEnter}
                      {...(draftAwareHostActions ? { hostActions: draftAwareHostActions } : {})}
                      {...(onDelayedActions ? { onDelayedActions } : {})}
                      {...(sessionNoteAvailable ? { onSessionNote: toggleSessionNote } : {})}
                      sessionNoteActive={noteOpen}
                      sessionNoteHasText={sessionNoteHasText}
                      summaryMode={summaryMode}
                      verboseMode={verbose}
                      {...(showVerbosePill ? { onToggleSummary: toggleSummary } : {})}
                      {...(showVerbosePill ? { onToggleVerbose: toggleVerbose } : {})}
                      {...(hostComposerBridge?.stashPrompt ? { onStash: stashComposerDraft } : {})}
                      {...(hostComposerBridge?.showStashedPrompts
                        ? { onShowStashedPrompts: hostComposerBridge.showStashedPrompts }
                        : {})}
                      stashedPromptCount={stashedPromptCount}
                      {...(reportDraftState ? { onDraftEmptyChange: reportComposerDraftState } : {})}
                      optionPills={
                        <SessionChatSessionOptionPills
                          canSend={canSend}
                          canSendKey={chat.sendKey !== undefined}
                          controller={sessionOptions}
                          {...(draftAgents ? { draftAgents } : {})}
                          {...(chat.sessionAgentId !== null ? { draftAgentId: chat.sessionAgentId } : {})}
                          {...(switchDraftAgent ? { onSwitchDraftAgent: switchDraftAgent } : {})}
                          isWorking={chat.working}
                          screenProbed={chat.screenProbed}
                          onDispatchCommand={send}
                          onDispatchKey={async (key, marker) => {
                            await chat.sendKey?.(key, marker);
                          }}
                          onSwitchingChange={setSessionOptionSwitching}
                          {...(onSwitchToTerminalForAgentPicker || hostActions?.onSwitchToTerminal
                            ? {
                                onSwitchToTerminal:
                                  onSwitchToTerminalForAgentPicker ??
                                  hostActions?.onSwitchToTerminalForAgentPicker ??
                                  hostActions?.onSwitchToTerminal,
                              }
                            : {})}
                        />
                      }
                      placeholder={
                        !canSend
                          ? 'Input is held by another device.'
                          : terminalChoicePending
                            ? 'Answer the question above to continue.'
                            : sessionOptionSwitching
                              ? 'Switching Claude mode…'
                              : undefined
                      }
                      ref={composerRef}
                      slashCommands={slashCommands}
                      slashHeading={sessionChatSlashHeadingForAgent(resolvedAgentLabel)}
                      skills={skills}
                      files={files}
                      filesLoading={filesLoading}
                      onRequestFiles={requestFiles}
                      fileHeading='Project files'
                      skillHeading={`${draftAgentRow?.name ?? displayAgentName(resolvedAgentLabel) ?? 'Agent'} skills`}
                    />
                  </div>
                  {chatBarPanelState?.open && chatBarExtensions.length > 0 ? (
                    <SessionChatExtensionPanel
                      activeExtensionId={chatBarPanelState.activeExtensionId}
                      extensions={chatBarExtensions}
                      minimized={chatBarPanelState.minimized}
                      onActiveExtensionChange={(activeExtensionId) =>
                        onChatBarPanelStateChange?.({ activeExtensionId, minimized: false, open: true })
                      }
                      onBridgeRequest={onChatBarBridgeRequest}
                      onClose={() => onChatBarPanelStateChange?.({ open: false })}
                      onMinimizedChange={(minimized) => onChatBarPanelStateChange?.({ minimized })}
                    />
                  ) : null}
                </div>
              </div>
            </div>
          </SessionChatHostLinksProvider>
        </SessionChatImageViewerProvider>
      </div>
    </TooltipProvider>
  );
}
