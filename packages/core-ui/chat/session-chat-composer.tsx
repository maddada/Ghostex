// Session chat composer (upstream chat spec §1.1/§11.6 port). Enter sends by
// default, hosts can reserve it for newlines, Escape interrupts, the IME guard swallows
// composition Enter, and ArrowUp/Down recall draft history. Typing a
// line-leading "/" opens the slash-command picker (per-agent catalog):
// ArrowUp/Down highlight, Tab/Enter complete, Enter on an exact match sends,
// Escape dismisses the picker without interrupting. A "$" token opens the same
// picker over the session's skills and an "@" token over the project's files;
// both read the token under the caret (see session-chat-composer-trigger.ts),
// so they open wherever in the draft the mention is being typed. Every picker
// row carries `data-chat-picker-option`, which keeps the highlighted row's
// fill out of the dark chat theme's button flattening (packages/core-ui/styles/chat.css)
// — without it the keyboard selection moves invisibly.
//
// Layout (§1.1): input row, then a footer row — session identity/options on
// the left, with Attach, Maximize and Send/Stop on the right. Styled with
// shadcn tokens to sit under the shadcn chat conversation.
//
// Maximize lifts the whole field onto a centered overlay (see
// `.ghostex-chat-composer-maximized` in packages/core-ui/styles/chat.css) so long
// prompts can be edited without scrolling a 160px-tall input. The field keeps
// its place in the React tree while maximized — only its box changes — so the
// monaco instance, caret, undo stack and pending attachments all survive the
// toggle.

import {
  IconArrowUp,
  IconClipboard,
  IconCopy,
  IconCut,
  IconDeviceMobileMessage,
  IconFile,
  IconLoader2,
  IconPlayerStopFilled,
  IconSelectAll,
  IconX,
} from '@tabler/icons-react';
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { cn } from '@/packages/components/utils';
import type { GxserverReadSessionTerminalTailResult, GxserverRpcErrorCode } from '@/packages/shared/gxserver-protocol';
import { gxserverRpcErrorCode } from '@/packages/shared/gxserver-rpc-error';
import { Button } from '../../components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '../../components/ui/context-menu';
import { Field, FieldError } from '../../components/ui/field';
import { AppTooltip } from '../app-tooltip';
import {
  EMPTY_SESSION_CHAT_COMPOSER_HISTORY,
  pushSessionChatComposerHistory,
  recallNextSessionChatDraft,
  recallPreviousSessionChatDraft,
  resetSessionChatComposerHistoryIndex,
} from './session-chat-composer-state';
import {
  clearStoredSessionChatDraftIfUnchanged,
  readStoredSessionChatDraft,
  readStoredSessionChatDraftEntry,
  writeStoredSessionChatDraft,
} from './session-chat-draft-storage';
import {
  filterSessionChatSlashCommands,
  sessionChatSlashQuery,
  type SessionChatSlashCommand,
} from './session-chat-slash-commands';
import {
  detectSessionChatComposerTrigger,
  filterSessionChatFiles,
  filterSessionChatSkills,
  linkedSessionChatSkillMention,
  sessionChatDisplaySkillDirectoryPath,
  sessionChatFileBasename,
  sessionChatFileDirectory,
  sessionChatFileMention,
} from './session-chat-composer-trigger';
import { SessionChatMonacoInput } from './session-chat-monaco-input';
import { SessionChatPlainInput } from './session-chat-plain-input';
import { sessionChatImageTargetForHref, useSessionChatImageViewer } from './session-chat-image-viewer';
import { SessionChatAgentFleetStrip } from './session-chat-agent-fleet-strip';
import { SessionChatQueueRows } from './session-chat-queue-rows';
import { SessionChatComposerActions } from './session-chat-composer-actions';
import { SessionChatComposerNotReadyNotice } from './session-chat-composer-not-ready';
import {
  readSessionChatDroppedAttachments,
  sessionChatDataTransferHasFiles,
  sessionChatNativeDropPaths,
  uploadSessionChatDroppedAttachments,
} from './session-chat-drop-attachments';
import type { SessionChatHostActions } from './session-chat-host-actions';
import {
  isNewerSessionChatDraftStamp,
  SESSION_CHAT_QUEUE_LONG_PRESS_MS,
  shouldOfferSessionChatDraft,
  shouldRestoreOwnSessionChatDraft,
} from './session-chat-queue';
import type { SessionChatDraftController, SessionChatQueueController } from './use-session-chat';
import type {
  SessionChatAgentFleet,
  SessionChatDraft,
  SessionChatQueuedPrompt,
  SessionChatSkill,
  SessionChatTheme,
} from '../../shared/session-chat';

export interface SessionChatComposerHandle {
  /** Append text after the current draft, separated as its own Markdown block. */
  appendText: (text: string) => boolean;
  /** Clear the draft only when it still matches the supplied snapshot. */
  clearDraft: (expected: string) => boolean;
  /*
  CDXC:DraftAgentSwitch 2026-08-28:
  Persist what is in the field RIGHT NOW: the localStorage copy synchronously,
  and gxserver's durable copy through the same debounced sync the composer uses
  everywhere else (the push is a no-op when the server already has this text).
  Callers use it before an action that restarts the agent CLI under the pane.
  */
  flushDraft: () => void;
  focus: () => void;
  getDraft: () => string;
  /** Insert a Saved Prompt at the caret as one editor operation. */
  insertSavedPrompt: (text: string) => boolean;
  /** Insert text at the caret; returns false when the composer cannot take it. */
  insertTypedText: (text: string) => boolean;
  /** Attach files/folders dropped anywhere on the chat view. */
  attachDroppedFiles: (data: DataTransfer) => boolean;
  /**
   * Clipboard payload redirected from the chat background: images become
   * attachments, text lands at the caret. Returns false when the composer
   * cannot take anything the clipboard holds.
   */
  pasteClipboard: (data: DataTransfer) => boolean;
}

/**
 * Backend-neutral key event: the textarea path adapts React's KeyboardEvent,
 * the Monaco path adapts monaco's IKeyboardEvent (whose preventDefault also
 * stops monaco's own handling of the key).
 */
export interface SessionChatComposerKeyEvent {
  altKey: boolean;
  ctrlKey: boolean;
  isComposing: boolean;
  key: string;
  metaKey: boolean;
  shiftKey: boolean;
  preventDefault: () => void;
}

/**
 * Imperative surface of the active input backend. `draft` state stays the
 * source of truth; applyValue only synchronizes the visual input (and caret)
 * after the composer has already updated the draft itself.
 */
export interface SessionChatComposerInputApi {
  applyValue: (next: string, caret: number) => void;
  focus: () => void;
  getSelection: () => { end: number; start: number };
  getValue: () => string;
  insertSavedPrompt: (text: string) => boolean;
  insertText: (text: string) => boolean;
  selectAll: () => void;
}

export interface SessionChatComposerProps {
  /**
   * Host-provided diagnostic breadcrumb sink (desktop support logs). Called on
   * composer mount/unmount and on focus entering/leaving the composer so a
   * native log can time focus loss against server state frames. Hosts without
   * disk logging omit it; the callback itself gates on the host's scenario.
   */
  diagnosticLog?: (event: string, details?: Record<string, unknown>) => void;
  /**
   * Host-provided notice of whether this composer currently holds anything
   * unsent — draft text with any non-whitespace, or attached/pasted images.
   * Called once when the composer mounts (drafts are restored from storage, so
   * a fresh mount can already be non-empty) and then only when the answer
   * flips, never per keystroke, and never with the draft itself. The desktop
   * host uses it to refuse to destroy an idle hidden chat page that still holds
   * typed text; hosts that never destroy their page omit it.
   */
  onDraftEmptyChange?: (empty: boolean) => void;
  disabled?: boolean;
  isWorking: boolean;
  /** Whether plain Enter sends instead of inserting a newline. */
  sendOnEnter?: boolean;
  /**
   * Use the platform's own text-selection/editing menu inside the input
   * instead of the chat's custom Cut/Copy/Paste menu (the React Native
   * webview host). The custom menu's trigger also opens on long press, which
   * on a phone replaces the system selection menu and dismisses the keyboard
   * mid-selection.
   */
  nativeContextMenu?: boolean;
  /** Stable conversation identity used to restore this session's unsent draft. */
  sessionKey?: string;
  placeholder?: string;
  /** Agent slash commands offered by the "/" picker; empty disables it. */
  slashCommands?: readonly SessionChatSlashCommand[];
  /** Section heading shown above the picker rows (usually the agent name). */
  slashHeading?: string;
  /** Skills available to this session's agent, resolved on its machine. */
  skills?: readonly SessionChatSkill[];
  /** Section heading shown above the skill mention rows. */
  skillHeading?: string;
  /**
   * Project-relative file paths offered by the "@" picker, listed on the
   * session's machine. Undefined while the host has not answered yet.
   */
  files?: readonly string[];
  /** Section heading shown above the file mention rows. */
  fileHeading?: string;
  /**
   * Asked once the first "@" token is typed so the host can list the project
   * lazily instead of on every chat mount.
   */
  onRequestFiles?: () => void;
  /** True while the host is listing files for the "@" picker. */
  filesLoading?: boolean;
  onSend: (text: string) => void | Promise<void>;
  /**
   * Reads the session's terminal screen for the `composerNotReady` refusal
   * notice (see session-chat-composer-not-ready.tsx) and, polled, for the
   * footer's Terminal View readiness tint and hover preview (see
   * use-session-terminal-tail.ts). Absent when the host's transport has no
   * route to /api/readSessionTerminalTail, which hides the notice's "Show
   * terminal" disclosure and leaves the button neutral and unadorned.
   */
  onReadTerminalTail?: () => Promise<GxserverReadSessionTerminalTailResult>;
  onInterrupt: () => void;
  /**
   * Per-session actions the host owns (surface switch, Rename, Sleep, Fork, …).
   * The switch renders as its own footer control beside Send and the rest fold
   * into the footer's More actions menu. Hosts whose own chrome already offers
   * these (e.g. the mobile app's native header) simply omit the prop.
   */
  hostActions?: SessionChatHostActions;
  /** Open the host's delayed actions for this session. */
  onDelayedActions?: () => void;
  /** Save the current draft for later and clear it after the save succeeds. */
  onStash?: () => void;
  /**
   * Opens the host's Saved Prompts surface when the stash control is clicked
   * with a whitespace-only draft, or from the control's context click. Absent
   * when the host cannot open that surface.
   */
  onShowStashedPrompts?: () => void;
  /**
   * How many prompts are currently stashed from this conversation, painted as a
   * corner badge on the stash control. Omitted or 0 renders no badge.
   */
  stashedPromptCount?: number;
  /**
   * Toggle this session's note panel. Unlike Stash this acts on the session,
   * not on the draft, so it stays enabled with an empty input. Absent when the
   * host cannot reach the note endpoints, which hides the control.
   */
  onSessionNote?: () => void;
  /** True while the note panel is open, for the button's pressed styling. */
  sessionNoteActive?: boolean;
  /** True when the session note contains non-whitespace text. */
  sessionNoteHasText?: boolean;
  /** Whether the composer renders shortcut chords in its controls and menu. */
  showShortcutLabels?: boolean;
  /** Per-session Verbose mode value shown with the right-hand composer actions. */
  verboseMode?: boolean;
  /** Toggle the per-session Verbose mode override. Omitted hides the action. */
  onToggleVerbose?: () => void;
  /** Per-session Summary mode value shown below Verbose mode in the actions menu. */
  summaryMode?: boolean;
  /** Toggle the per-session Summary mode. Omitted hides the action. */
  onToggleSummary?: () => void;
  /**
   * Saves a pasted image onto the session's machine and resolves with the
   * absolute path there. When set, pasting an image inserts the terminal
   * paste reference "[Image #N](path)" and shows a preview thumbnail above
   * the input; when omitted, image pastes fall through untouched.
   */
  onPasteImage?: (payload: { base64Data: string; suggestedName?: string }) => Promise<string>;
  /**
   * Saves any non-image attachment onto the session's machine and resolves
   * with the absolute path there, inserted as "[File #N](path)". When
   * omitted, the attach button only accepts images.
   */
  onAttachFile?: (payload: {
    base64Data: string;
    directory?: boolean;
    uploadId?: string;
    relativePath?: string;
    suggestedName?: string;
  }) => Promise<string>;
  /**
   * Host-native attach picker resolving with absolute paths on the session's
   * machine (may include folders). When set, the attach button uses it
   * instead of the browser file input; image paths insert "[Image #N](path)"
   * and everything else "[File #N](path)".
   */
  onPickPaths?: () => Promise<string[]>;
  /**
   * Absolute paths of the OS drag currently over this page, captured by the
   * host shell at drag-enter (Chromium never exposes `File.path` to a page).
   * Only hosts whose session runs on this machine provide it; drops elsewhere
   * upload bytes through onAttachFile instead.
   */
  onNativeDropPaths?: () => readonly string[];
  /**
   * Loads a preview data URL for an image path picked natively (no bytes in
   * the page otherwise). Optional garnish: picks insert their reference even
   * when the preview cannot load.
   */
  onLoadImagePreview?: (path: string) => Promise<string>;
  /**
   * Session-option pills rendered in the footer, left of Send (§1.1). The view
   * builds them so the composer stays about input mechanics; agents without an
   * option catalog pass nothing.
   */
  optionPills?: ReactNode;
  /**
   * Base URL of monaco-editor's min/vs directory on this surface. When set,
   * the input is a Monaco editor (editing hotkeys work); when omitted (the
   * mobile single-file bundle, where Monaco's sibling assets are
   * unreachable), the plain textarea renders instead.
   */
  monacoVsBaseUrl?: string;
  /** Palette used by the chat-owned Monaco prompt input. */
  theme?: SessionChatTheme;
  /*
  CDXC:SessionChatPromptQueue 2026-08-21:
  Ghostex's own prompt queue (plan 016). Rows render above the input, inside
  this composer's container, and Tab / a long-press on Send add to them. Absent
  — or present with `capabilities.supported === false`, which is what an old
  daemon looks like — hides every queue control instead of offering buttons
  that would 404. This is NOT `SessionChatMessage.queued`, the agent CLI's own
  internal queue that renders in the transcript.
  */
  queue?: SessionChatQueueController;
  /**
   * Cross-client composer draft. The composer pushes on blur / unmount /
   * backgrounding (never per keystroke) and offers a newer draft from another
   * device behind a Use / Dismiss bar. Absent keeps drafts local-only; the
   * per-client localStorage cache is unaffected either way.
   */
  draftSync?: SessionChatDraftController;
  /*
  CDXC:SessionChatAgentFleet 2026-08-23:
  Sub-agents read off the agent's terminal screen. Rendered ABOVE this
  composer's container, unlike the queue rows above, because it is work the
  agent already owns rather than input the user still owns. Null/absent renders
  nothing at all.
  */
  agentFleet?: SessionChatAgentFleet | null;
}

interface PastedImagePreview {
  dataUrl: string;
  id: string;
  path: string;
}

/** Rich Prompt Editor numbering: max existing [Image #N]( in the draft, +1. */
function nextImageReferenceIndex(text: string): number {
  let highest = 0;
  for (const match of text.matchAll(/\[Image #(\d+)\]\(/g)) {
    const index = Number.parseInt(match[1] ?? '', 10);
    if (Number.isFinite(index)) {
      highest = Math.max(highest, index);
    }
  }
  return highest + 1;
}

/** Same numbering scheme for non-image attachments: "[File #N](path)". */
function nextFileReferenceIndex(text: string): number {
  let highest = 0;
  for (const match of text.matchAll(/\[File #(\d+)\]\(/g)) {
    const index = Number.parseInt(match[1] ?? '', 10);
    if (Number.isFinite(index)) {
      highest = Math.max(highest, index);
    }
  }
  return highest + 1;
}

/** Mentions queue and the two composer pickers so they are discoverable without docs. */
const DESKTOP_SESSION_CHAT_PLACEHOLDER =
  'Press Enter to send a message and Tab to Queue.\nUse @ to mention a file and $ for using skills.';
const MOBILE_SESSION_CHAT_PLACEHOLDER = 'Tap ↑ to send or hold it to queue; use @ for files and $ for skills.';

const IMAGE_PATH_PATTERN = /\.(avif|bmp|gif|heic|heif|ico|jpe?g|png|svg|tiff?|webp)$/i;
const LINKED_IMAGE_REFERENCE_PATTERN = /\[Image #\d+\]\(([^)\r\n]+)\)/g;
const SESSION_CHAT_STOP_BUTTON_COOLDOWN_MS = 2_000;

/*
 * CDXC:DraftCrashSafety 2026-08-28:
 * The per-keystroke localStorage cache is NOT durable: Chromium batches its
 * LevelDB commits and an app quit that skips a clean CEF shutdown (a dev
 * restart, a crash, a force quit) drops everything uncommitted. gxserver's
 * synced draft is the durable copy — its SQLite store outlives the app because
 * the daemon deliberately survives restarts — so typing must reach it while it
 * happens, not only on blur/unmount. A trailing debounce keeps the chatter
 * down, and the max interval bounds staleness while the user types without a
 * pause.
 */
const SESSION_CHAT_DRAFT_SYNC_DEBOUNCE_MS = 2_000;
const SESSION_CHAT_DRAFT_SYNC_MAX_INTERVAL_MS = 10_000;
/**
 * A push that failed is retried on this delay, this many times. Without a
 * retry the empty push after a send was the only thing clearing gxserver's
 * copy, and one dropped request left the sent message on the daemon as an
 * "unsent draft" that every later app start restored into the composer.
 */
const SESSION_CHAT_DRAFT_SYNC_RETRY_MS = 2_000;
const SESSION_CHAT_DRAFT_SYNC_MAX_RETRIES = 3;

function linkedImageReferenceHrefs(text: string): string[] {
  return [...text.matchAll(LINKED_IMAGE_REFERENCE_PATTERN)].map((match) => match[1]?.trim() ?? '').filter(Boolean);
}

function isImageFile(file: File): boolean {
  return file.type.startsWith('image/') || IMAGE_PATH_PATTERN.test(file.name);
}

function clipboardImageFiles(data: DataTransfer): File[] {
  const files: File[] = [];
  for (const item of Array.from(data.items)) {
    if (item.kind !== 'file') {
      continue;
    }
    const file = item.getAsFile();
    if (file && isImageFile(file)) {
      files.push(file);
    }
  }
  return files;
}

const CLIPBOARD_IMAGE_EXTENSIONS: Readonly<Record<string, string>> = {
  'image/avif': 'avif',
  'image/bmp': 'bmp',
  'image/gif': 'gif',
  'image/heic': 'heic',
  'image/heif': 'heif',
  'image/jpeg': 'jpg',
  'image/png': 'png',
  'image/svg+xml': 'svg',
  'image/tiff': 'tiff',
  'image/webp': 'webp',
};

/**
 * A custom browser menu does not receive the native paste event payload, so
 * read the system clipboard during the menu item's user gesture and rebuild
 * the same DataTransfer shape that keyboard paste already sends through the
 * composer. Text and images therefore keep one insertion/attachment path.
 */
async function readSessionChatSystemClipboard(): Promise<DataTransfer> {
  if (!navigator.clipboard?.read) {
    throw new Error('Clipboard reading is not available in this browser.');
  }

  const transfer = new DataTransfer();
  const items = await navigator.clipboard.read();
  for (const [index, item] of items.entries()) {
    const textType = item.types.find((type) => type === 'text/plain');
    if (textType && transfer.getData('text/plain') === '') {
      transfer.setData('text/plain', await (await item.getType(textType)).text());
    }

    const imageType = item.types.find((type) => type.startsWith('image/'));
    if (!imageType) {
      continue;
    }
    const blob = await item.getType(imageType);
    const extension = CLIPBOARD_IMAGE_EXTENSIONS[imageType];
    const fileName = extension ? `clipboard-image-${index + 1}.${extension}` : `clipboard-image-${index + 1}`;
    transfer.items.add(
      new File([blob], fileName, {
        type: imageType,
      })
    );
  }
  return transfer;
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error('Could not read the pasted image.'));
    reader.readAsDataURL(file);
  });
}

function reactKeyEventAdapter(event: KeyboardEvent<HTMLElement>): SessionChatComposerKeyEvent {
  return {
    altKey: event.altKey,
    ctrlKey: event.ctrlKey,
    isComposing: event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229,
    key: event.key,
    metaKey: event.metaKey,
    preventDefault: () => event.preventDefault(),
    shiftKey: event.shiftKey,
  };
}

export const SessionChatComposer = forwardRef<SessionChatComposerHandle, SessionChatComposerProps>(
  function SessionChatComposer(
    {
      agentFleet,
      diagnosticLog,
      disabled = false,
      draftSync,
      fileHeading,
      files,
      filesLoading = false,
      hostActions,
      isWorking,
      monacoVsBaseUrl,
      nativeContextMenu = false,
      onAttachFile,
      onDelayedActions,
      onDraftEmptyChange,
      onInterrupt,
      onLoadImagePreview,
      onNativeDropPaths,
      onPasteImage,
      onPickPaths,
      onReadTerminalTail,
      onRequestFiles,
      onSend,
      onSessionNote,
      onShowStashedPrompts,
      onStash,
      onToggleSummary,
      onToggleVerbose,
      optionPills,
      placeholder,
      queue,
      sendOnEnter = true,
      sessionKey,
      sessionNoteActive = false,
      sessionNoteHasText = false,
      showShortcutLabels = true,
      slashCommands,
      slashHeading,
      skills,
      skillHeading,
      stashedPromptCount = 0,
      summaryMode = false,
      theme = 'dark',
      verboseMode = false,
    },
    ref
  ) {
    const [draft, setDraft] = useState(() => readStoredSessionChatDraft(sessionKey));
    const [history, setHistory] = useState(EMPTY_SESSION_CHAT_COMPOSER_HISTORY);
    const [slashDismissed, setSlashDismissed] = useState(false);
    const [slashIndex, setSlashIndex] = useState(0);
    const [skillDismissed, setSkillDismissed] = useState(false);
    const [skillIndex, setSkillIndex] = useState(0);
    const [fileDismissed, setFileDismissed] = useState(false);
    const [fileIndex, setFileIndex] = useState(0);
    /**
     * Caret offset the pickers read. The draft alone cannot say where the caret
     * is, and a mention is only "being typed" when the caret sits at its end.
     */
    const [caret, setCaret] = useState<number | null>(null);
    const [pastedImages, setPastedImages] = useState<readonly PastedImagePreview[]>([]);
    const [pendingImagePastes, setPendingImagePastes] = useState(0);
    const [monacoFailed, setMonacoFailed] = useState(false);
    const [maximized, setMaximized] = useState(false);
    const [sendError, setSendError] = useState<string | null>(null);
    /**
     * The daemon's refusal code for `sendError`, when the rejection carried
     * one. Only `composerNotReady` changes what is rendered; every other code
     * keeps the generic message.
     */
    const [sendErrorCode, setSendErrorCode] = useState<GxserverRpcErrorCode | null>(null);
    const [stopButtonCoolingDown, setStopButtonCoolingDown] = useState(false);
    const [contextSelection, setContextSelection] = useState({ end: 0, start: 0 });
    /** A newer draft from another device, waiting behind the Use / Dismiss bar. */
    const [incomingDraft, setIncomingDraft] = useState<SessionChatDraft | null>(null);
    const imageViewer = useSessionChatImageViewer();
    const fileInputRef = useRef<HTMLInputElement | null>(null);
    const slashListRef = useRef<HTMLDivElement | null>(null);
    const skillListRef = useRef<HTMLDivElement | null>(null);
    const fileListRef = useRef<HTMLDivElement | null>(null);
    const pasteSequenceRef = useRef(0);
    const previewLoadsRef = useRef(new Set<string>());
    const draftRef = useRef(draft);
    draftRef.current = draft;
    const monacoApiRef = useRef<SessionChatComposerInputApi | null>(null);
    const plainApiRef = useRef<SessionChatComposerInputApi | null>(null);
    const pendingFocusRef = useRef(false);
    const pendingInsertTextRef = useRef('');
    const pendingSavedPromptRef = useRef('');
    const sendInFlightRef = useRef(false);
    /** Newest draft stamp already applied or dismissed here (never re-offered). */
    const lastHandledDraftAtRef = useRef<string | null>(null);
    /** Exact content of the last successful push, so blur cannot spam gxserver. */
    const lastPushedDraftRef = useRef<string | null>(null);
    const longPressTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const longPressFiredRef = useRef(false);
    const stopButtonCooldownTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const useMonaco = monacoVsBaseUrl !== undefined && !monacoFailed;
    const diagnosticLogRef = useRef(diagnosticLog);
    diagnosticLogRef.current = diagnosticLog;
    useEffect(() => {
      diagnosticLogRef.current?.('sessionChat.composerMounted');
      return () => {
        diagnosticLogRef.current?.('sessionChat.composerUnmounted');
      };
    }, []);
    const onDraftEmptyChangeRef = useRef(onDraftEmptyChange);
    onDraftEmptyChangeRef.current = onDraftEmptyChange;
    // Anything unsent the page alone holds: typed text, finished image pastes,
    // and pastes still being read. The effect depends on the boolean, not the
    // draft, so a host hears about it on mount and on each flip only.
    const draftEmpty = draft.trim().length === 0 && pastedImages.length === 0 && pendingImagePastes === 0;
    const draftEmptyRef = useRef(draftEmpty);
    draftEmptyRef.current = draftEmpty;
    useEffect(() => {
      onDraftEmptyChangeRef.current?.(draftEmpty);
    }, [draftEmpty]);
    useEffect(
      () => () => {
        // The composer can leave the page while the conversation stays (an
        // active question replaces it), and its draft outlives it in storage.
        // From here on the host cannot know what this page holds, and "unknown"
        // must never read as "empty" to a host that destroys pages on it.
        onDraftEmptyChangeRef.current?.(false);
      },
      []
    );

    // Previews mirror the draft: deleting a reference (by any means, including
    // sending, which clears the draft) drops its thumbnail.
    useEffect(() => {
      const referencedHrefs = new Set(linkedImageReferenceHrefs(draft));
      setPastedImages((current) => current.filter((image) => referencedHrefs.has(image.path)));
    }, [draft]);

    const slashQuery = sessionChatSlashQuery(draft);
    const slashMatches = useMemo(
      () =>
        slashQuery !== null && !slashDismissed && slashCommands !== undefined
          ? filterSessionChatSlashCommands(slashCommands, slashQuery)
          : [],
      [slashCommands, slashDismissed, slashQuery]
    );
    const slashOpen = slashMatches.length > 0 && !disabled;
    const highlightedIndex = Math.min(slashIndex, Math.max(slashMatches.length - 1, 0));
    const trigger = detectSessionChatComposerTrigger(draft, caret ?? draft.length);
    const skillQuery = trigger?.kind === 'skill' ? trigger.query : null;
    const skillMatches = useMemo(
      () => (skillQuery !== null && !skillDismissed ? filterSessionChatSkills(skills ?? [], skillQuery) : []),
      [skillDismissed, skillQuery, skills]
    );
    const skillOpen = skillMatches.length > 0 && !disabled && !slashOpen;
    const highlightedSkillIndex = Math.min(skillIndex, Math.max(skillMatches.length - 1, 0));
    const fileQuery = trigger?.kind === 'path' ? trigger.query : null;
    const fileMatches = useMemo(
      () => (fileQuery !== null && !fileDismissed ? filterSessionChatFiles(files ?? [], fileQuery) : []),
      [fileDismissed, fileQuery, files]
    );
    const filePickerActive = fileQuery !== null && !fileDismissed && !disabled && !slashOpen;
    // The picker stays up while the host is still listing so "@" never looks
    // dead on the first use of a session, when nothing is cached yet.
    const fileOpen = filePickerActive && (fileMatches.length > 0 || (filesLoading && !files));
    const highlightedFileIndex = Math.min(fileIndex, Math.max(fileMatches.length - 1, 0));

    // Lazy list: the first "@" of a session asks the host for the project files.
    useEffect(() => {
      if (filePickerActive && files === undefined) {
        onRequestFiles?.();
      }
    }, [filePickerActive, files, onRequestFiles]);

    useEffect(() => {
      if (!slashOpen) {
        return;
      }
      slashListRef.current?.querySelector('[data-highlighted="true"]')?.scrollIntoView({ block: 'nearest' });
    }, [highlightedIndex, slashOpen]);

    useEffect(() => {
      if (!skillOpen) {
        return;
      }
      skillListRef.current?.querySelector('[data-highlighted="true"]')?.scrollIntoView({ block: 'nearest' });
    }, [highlightedSkillIndex, skillOpen]);

    useEffect(() => {
      if (!fileOpen) {
        return;
      }
      fileListRef.current?.querySelector('[data-highlighted="true"]')?.scrollIntoView({ block: 'nearest' });
    }, [highlightedFileIndex, fileOpen]);

    const updateDraft = (next: string, nextCaret?: number): void => {
      const caretOffset = nextCaret ?? next.length;
      writeStoredSessionChatDraft(sessionKey, next);
      setDraft(next);
      setCaret(caretOffset);
      setSendError(null);
      setHistory((current) => resetSessionChatComposerHistoryIndex(current));
      if (sessionChatSlashQuery(next) === null) {
        setSlashDismissed(false);
      }
      // Leaving a token re-arms its picker, so a dismissed mention does not stay
      // dismissed for the next one typed in the same draft.
      const nextTrigger = detectSessionChatComposerTrigger(next, caretOffset);
      if (nextTrigger?.kind !== 'skill') {
        setSkillDismissed(false);
      }
      if (nextTrigger?.kind !== 'path') {
        setFileDismissed(false);
      }
      setSlashIndex(0);
      setSkillIndex(0);
      setFileIndex(0);
    };

    // Resolved lazily: the Monaco backend registers its api into a ref after
    // an async load, without a re-render, so a render-scoped const would go
    // stale between load and the next state change.
    const getInputApi = (): SessionChatComposerInputApi | null =>
      useMonaco ? monacoApiRef.current : plainApiRef.current;

    useEffect(() => {
      const plainApi = plainApiRef.current;
      if (!useMonaco && plainApi) {
        if (pendingInsertTextRef.current) {
          const pending = pendingInsertTextRef.current;
          pendingInsertTextRef.current = '';
          plainApi.insertText(pending);
        }
        if (pendingSavedPromptRef.current) {
          const pending = pendingSavedPromptRef.current;
          pendingSavedPromptRef.current = '';
          plainApi.insertSavedPrompt(pending);
        }
        if (pendingFocusRef.current) {
          pendingFocusRef.current = false;
          plainApi.focus();
        }
      }
    }, [useMonaco]);

    useImperativeHandle(ref, () => ({
      attachDroppedFiles: (data: DataTransfer): boolean => consumeDroppedAttachments(data),
      appendText: (text: string): boolean => {
        if (disabled || text === '') {
          return false;
        }
        const input = getInputApi();
        const current = input?.getValue() ?? draftRef.current;
        const separator = current === '' || current.endsWith('\n\n') ? '' : current.endsWith('\n') ? '\n' : '\n\n';
        const next = `${current}${separator}${text}`;
        draftRef.current = next;
        updateDraft(next, next.length);
        if (!input) {
          pendingInsertTextRef.current += `${separator}${text}`;
          pendingFocusRef.current = true;
          return true;
        }
        input?.applyValue(next, next.length);
        input?.focus();
        return true;
      },
      clearDraft: (expected: string): boolean => {
        const current = getInputApi()?.getValue() ?? draftRef.current;
        if (current !== expected) {
          return false;
        }
        writeStoredSessionChatDraft(sessionKey, '');
        draftRef.current = '';
        setDraft('');
        setCaret(0);
        setHistory((value) => resetSessionChatComposerHistoryIndex(value));
        getInputApi()?.applyValue('', 0);
        setSlashDismissed(false);
        setSlashIndex(0);
        setSkillDismissed(false);
        setSkillIndex(0);
        setFileDismissed(false);
        setFileIndex(0);
        setSendError(null);
        return true;
      },
      flushDraft: (): void => {
        /*
        The editor's own value, not `draftRef`: a keystroke that has not been
        committed to state yet is exactly the text this flush exists for. The
        write goes through the same helper the per-keystroke path uses, so an
        empty field clears the stored copy instead of leaving a stale one.
        */
        writeStoredSessionChatDraft(sessionKey, getInputApi()?.getValue() ?? draftRef.current);
        pushDraftRef.current();
      },
      focus: () => {
        const input = getInputApi();
        if (!input) {
          // Monaco loads asynchronously. Preserve the host's one-shot focus
          // handoff until the real editor API exists instead of dropping it.
          pendingFocusRef.current = true;
          return;
        }
        pendingFocusRef.current = false;
        input.focus();
      },
      getDraft: () => getInputApi()?.getValue() ?? draftRef.current,
      insertSavedPrompt: (text: string): boolean => {
        if (disabled) {
          return false;
        }
        const input = getInputApi();
        if (!input) {
          pendingSavedPromptRef.current += text;
          return true;
        }
        return input.insertSavedPrompt(text);
      },
      insertTypedText: (text: string): boolean => {
        if (disabled) {
          return false;
        }
        const input = getInputApi();
        if (!input) {
          pendingInsertTextRef.current += text;
          return true;
        }
        return input.insertText(text);
      },
      pasteClipboard: (data: DataTransfer): boolean => {
        return pasteClipboardData(data);
      },
    }));

    /**
     * Empties the field and every picker's dismissal state. Shared by send and
     * by queueing (Tab / long-press), because both hand the text off somewhere
     * else and both must leave a clean composer behind.
     *
     * `retainStoredDraft` keeps the persisted copy while the text is still only
     * in flight. The visual clear is optimistic; the durable copy must not be,
     * because this component can unmount before the request settles (a mode
     * switch, or an interactive question card replacing the composer) and take
     * the only remaining copy of the message with it. The caller drops the
     * stored copy once the send is acknowledged.
     */
    const vacateComposer = (options?: { retainStoredDraft?: boolean }): void => {
      if (options?.retainStoredDraft !== true) {
        writeStoredSessionChatDraft(sessionKey, '');
      }
      draftRef.current = '';
      setDraft('');
      setHistory((value) => resetSessionChatComposerHistoryIndex(value));
      getInputApi()?.applyValue('', 0);
      setCaret(0);
      setSlashDismissed(false);
      setSlashIndex(0);
      setSkillDismissed(false);
      setSkillIndex(0);
      setFileDismissed(false);
      setFileIndex(0);
    };

    /** Puts text back in the field, keeping anything typed since it left. */
    const restoreComposerText = (text: string): void => {
      const current = getInputApi()?.getValue() ?? draftRef.current;
      const restored = current === '' || current === text ? text : `${text}\n${current}`;
      writeStoredSessionChatDraft(sessionKey, restored);
      draftRef.current = restored;
      setDraft(restored);
      setCaret(restored.length);
      setHistory((value) => resetSessionChatComposerHistoryIndex(value));
      getInputApi()?.applyValue(restored, restored.length);
      getInputApi()?.focus();
    };

    const send = (text: string = draft): void => {
      if (text.trim() === '' || disabled || sendInFlightRef.current) {
        return;
      }
      sendInFlightRef.current = true;
      // Sending closes the maximize overlay: the user's next look is at the
      // transcript, not an empty full-height editor.
      setMaximized(false);
      setSendError(null);
      setSendErrorCode(null);

      // The optimistic transcript echo is created synchronously by onSend.
      // Vacate the composer first so the submit gesture feels immediate and
      // any typing that follows belongs to the next draft. The stored draft
      // stays until the send is acknowledged: it is the copy a remount reads,
      // and the only one left if this composer is gone when the send fails.
      vacateComposer({ retainStoredDraft: true });

      const sendRequest = (() => {
        try {
          return Promise.resolve(onSend(text));
        } catch (error) {
          return Promise.reject(error);
        }
      })();
      void sendRequest
        .then(() => {
          clearStoredSessionChatDraftIfUnchanged(sessionKey, text);
          setHistory((value) => pushSessionChatComposerHistory(value, text));
          /*
          CDXC:SessionChatDraftRetireOnSend 2026-09-02:
          gxserver retires its own copy of a delivered message, but this
          client still owns what it last pushed: flush the empty composer NOW
          rather than after the typing debounce, so no window is left in which
          a kill, a hidden page or a dropped request can leave the sent text
          standing as the daemon's draft. Anything typed meanwhile is a new
          draft and keeps its own debounce.
          */
          if ((getInputApi()?.getValue() ?? draftRef.current) === '') {
            if (draftSyncTimerRef.current !== null) {
              clearTimeout(draftSyncTimerRef.current);
              draftSyncTimerRef.current = null;
            }
            pushDraftRef.current();
          }
        })
        .catch((error: unknown) => {
          // Do not overwrite a next draft typed while the send was in flight.
          // Put the failed message first so retrying still preserves send order.
          restoreComposerText(text);
          /*
          CDXC:SessionChatComposerReady 2026-08-26:
          `composerNotReady` means the daemon wrote NOTHING — the agent CLI has
          no input box on screen yet (booting, or a trust/auth/setup screen owns
          the terminal). That is a fixable state with a place to go, so it gets
          its own notice; every other rejection keeps the generic sentence.
          */
          const code = gxserverRpcErrorCode(error);
          setSendErrorCode(code);
          setSendError(
            code === 'composerNotReady' && error instanceof Error && error.message !== ''
              ? error.message
              : 'Message could not be sent. Your draft was restored.'
          );
        })
        .finally(() => {
          sendInFlightRef.current = false;
        });
    };

    // --- Ghostex prompt queue (plan 016) ---------------------------------------
    // Enter is untouched by everything below: it still sends immediately,
    // mid-turn included. Tab and a long-press on Send are the ONLY gestures that
    // make a queued row.
    const queueCapabilities = queue?.capabilities;
    const canQueueDraft = queueCapabilities?.canQueue === true && !disabled && draft.trim() !== '';

    /** Loads text into the field, replacing whatever is there. */
    const loadComposerText = (text: string): void => {
      writeStoredSessionChatDraft(sessionKey, text);
      draftRef.current = text;
      setDraft(text);
      setCaret(text.length);
      setHistory((value) => resetSessionChatComposerHistoryIndex(value));
      getInputApi()?.applyValue(text, text.length);
      getInputApi()?.focus();
    };

    const queueCurrentDraft = (): void => {
      const controller = queue;
      if (!controller?.capabilities.canQueue || disabled) {
        return;
      }
      const stored = getInputApi()?.getValue() ?? draftRef.current;
      const text = stored.trim();
      if (text === '') {
        return;
      }
      setSendError(null);
      // Vacate first: the queued row becomes the only copy of the text, exactly
      // as a send does, so the gesture reads as "this left the composer". The
      // stored copy is kept until the row exists, for the same reason a send
      // keeps it: this composer may be gone by the time the queueing fails.
      vacateComposer({ retainStoredDraft: true });
      void controller
        .queuePrompt(text)
        .then(() => {
          clearStoredSessionChatDraftIfUnchanged(sessionKey, stored);
        })
        .catch(() => {
          restoreComposerText(text);
          setSendError('The prompt could not be queued. Your draft was restored.');
        });
    };

    /*
  Edit, in the order the plan fixes: remove the row (its text rides back on the
  answer), queue whatever the composer already held at the END, then load the
  removed text. Nothing the user typed is ever dropped on the floor.
  */
    const editQueuedPrompt = (prompt: SessionChatQueuedPrompt): void => {
      const controller = queue;
      if (!controller?.capabilities.canEdit || disabled) {
        return;
      }
      void (async () => {
        const removed = await controller.removePrompt(prompt.id);
        const current = getInputApi()?.getValue() ?? draftRef.current;
        if (current.trim() !== '') {
          await controller.queuePrompt(current);
        }
        loadComposerText(removed?.text ?? prompt.text);
      })().catch(() => {
        setSendError('The queued prompt could not be edited.');
      });
    };

    // --- Long-press on Send ------------------------------------------------------
    // Pointer events, not touch events, so one implementation covers the mouse,
    // the pen and the phone. Long-press is the ONLY queue gesture on mobile, so
    // it has to survive a touch stream that also fires a click afterwards.
    const cancelSendLongPress = (): void => {
      if (longPressTimerRef.current !== null) {
        clearTimeout(longPressTimerRef.current);
        longPressTimerRef.current = null;
      }
    };

    const beginSendLongPress = (): void => {
      longPressFiredRef.current = false;
      cancelSendLongPress();
      if (!canQueueDraft) {
        return;
      }
      longPressTimerRef.current = setTimeout(() => {
        longPressTimerRef.current = null;
        longPressFiredRef.current = true;
        queueCurrentDraft();
      }, SESSION_CHAT_QUEUE_LONG_PRESS_MS);
    };

    const handleSendClick = (): void => {
      cancelSendLongPress();
      if (longPressFiredRef.current) {
        // The press already queued; the click that closes the same gesture must
        // not also send. (The composer is empty by now, so send() would bail
        // anyway — but a tap after a failed queue restore would not.)
        longPressFiredRef.current = false;
        return;
      }
      send();
    };

    useEffect(() => cancelSendLongPress, []);

    const handleStopClick = (): void => {
      if (stopButtonCooldownTimerRef.current !== null) {
        return;
      }
      setStopButtonCoolingDown(true);
      stopButtonCooldownTimerRef.current = setTimeout(() => {
        stopButtonCooldownTimerRef.current = null;
        setStopButtonCoolingDown(false);
      }, SESSION_CHAT_STOP_BUTTON_COOLDOWN_MS);
      onInterrupt();
    };

    useEffect(
      () => () => {
        if (stopButtonCooldownTimerRef.current !== null) {
          clearTimeout(stopButtonCooldownTimerRef.current);
        }
      },
      []
    );

    // --- Cross-client draft sync -------------------------------------------------
    // Pushed on a short typing debounce plus blur / session switch / unmount /
    // backgrounding. The per-client localStorage cache above is untouched:
    // gxserver is the durable copy (see SESSION_CHAT_DRAFT_SYNC_DEBOUNCE_MS),
    // localStorage the instant one.
    /** The draft this composer mounted with, unpushed until it changes. */
    const initialDraftRef = useRef(draft);
    /** True once anything mutated the composer — typing, load, send, clear. */
    const composerTouchedRef = useRef(false);
    const draftSyncTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const lastDraftSyncAtRef = useRef(0);
    const draftSyncRetriesRef = useRef(0);
    /** Set by the unmount cleanup so a late failure cannot arm a timer into a dead composer. */
    const draftSyncUnmountedRef = useRef(false);
    const pushDraftIfChanged = (): void => {
      const controller = draftSync;
      if (!controller?.canSync) {
        return;
      }
      lastDraftSyncAtRef.current = Date.now();
      const content = getInputApi()?.getValue() ?? draftRef.current;
      if (content === lastPushedDraftRef.current) {
        return;
      }
      lastPushedDraftRef.current = content;
      void controller
        .push(content)
        .then(() => {
          draftSyncRetriesRef.current = 0;
        })
        .catch(() => {
          // Forget the push so the next flush sends the live value again, and
          // schedule that flush instead of waiting for a keystroke that may
          // never come: after a send the composer is empty and idle, and the
          // empty push is exactly the one that must not be lost.
          lastPushedDraftRef.current = null;
          if (
            draftSyncUnmountedRef.current ||
            draftSyncTimerRef.current !== null ||
            draftSyncRetriesRef.current >= SESSION_CHAT_DRAFT_SYNC_MAX_RETRIES
          ) {
            return;
          }
          draftSyncRetriesRef.current += 1;
          draftSyncTimerRef.current = setTimeout(() => {
            draftSyncTimerRef.current = null;
            pushDraftRef.current();
          }, SESSION_CHAT_DRAFT_SYNC_RETRY_MS);
        });
    };
    const pushDraftRef = useRef(pushDraftIfChanged);
    pushDraftRef.current = pushDraftIfChanged;

    useEffect(() => {
      const flush = (): void => {
        if (document.visibilityState === 'hidden') {
          pushDraftRef.current();
        }
      };
      document.addEventListener('visibilitychange', flush);
      return () => {
        document.removeEventListener('visibilitychange', flush);
        // Unmount covers both closing the chat and switching sessions: the view
        // keys this composer on sessionKey, so a switch remounts it.
        pushDraftRef.current();
      };
    }, []);

    useEffect(() => {
      if (!composerTouchedRef.current) {
        if (draft === initialDraftRef.current) {
          // Mount, or the synced echo of the mount value: nothing new to push,
          // and pushing here would race the crash-restore below — an empty
          // composer must not overwrite a surviving gxserver draft with ''.
          return;
        }
        composerTouchedRef.current = true;
      }
      if (draftSyncTimerRef.current !== null) {
        clearTimeout(draftSyncTimerRef.current);
        draftSyncTimerRef.current = null;
      }
      // Steady typing keeps resetting the debounce, so the max interval is
      // what bounds how much a kill mid-sentence can lose.
      if (Date.now() - lastDraftSyncAtRef.current >= SESSION_CHAT_DRAFT_SYNC_MAX_INTERVAL_MS) {
        pushDraftRef.current();
        return;
      }
      draftSyncTimerRef.current = setTimeout(() => {
        draftSyncTimerRef.current = null;
        pushDraftRef.current();
      }, SESSION_CHAT_DRAFT_SYNC_DEBOUNCE_MS);
    }, [draft]);
    useEffect(
      () => () => {
        // The unmount cleanup above already flushes; the pending timer must
        // not fire into a dead composer after it.
        draftSyncUnmountedRef.current = true;
        if (draftSyncTimerRef.current !== null) {
          clearTimeout(draftSyncTimerRef.current);
        }
      },
      []
    );

    /*
  "Newer draft from another device": offered, never applied. The three
  conditions live in shouldOfferSessionChatDraft; the important half is here —
  a draft that is NOT offered still marks its stamp handled, so a later
  keystroke cannot make an old draft look new again and pop the bar.
  */
    const syncedDraft = draftSync?.synced ?? null;
    const draftClientId = draftSync?.clientId ?? '';
    useEffect(() => {
      if (!syncedDraft) {
        return;
      }
      const composerText = getInputApi()?.getValue() ?? draftRef.current;
      /*
      CDXC:DraftCrashSafety 2026-08-28:
      This client's own synced draft is normally a mute echo — but when it is
      fresher than the stored copy, the stored copy LOST data (the app died
      before Chromium committed the localStorage batch) and gxserver holds the
      only surviving text. Load it back silently, exactly as a localStorage
      restore would have; the predicate refuses the moment anything has touched
      the composer, so it can never replace text the user is working on.
      */
      if (
        shouldRestoreOwnSessionChatDraft({
          clientId: draftClientId,
          composerText,
          composerTouched: composerTouchedRef.current,
          incoming: syncedDraft,
          stored: readStoredSessionChatDraftEntry(sessionKey),
        })
      ) {
        lastHandledDraftAtRef.current = syncedDraft.updatedAt;
        // The restore IS the synced content; pushing it back would be an echo.
        lastPushedDraftRef.current = syncedDraft.content;
        loadComposerText(syncedDraft.content);
        return;
      }
      if (
        shouldOfferSessionChatDraft({
          clientId: draftClientId,
          composerText,
          incoming: syncedDraft,
          lastHandledUpdatedAt: lastHandledDraftAtRef.current,
        })
      ) {
        setIncomingDraft(syncedDraft);
        return;
      }
      if (isNewerSessionChatDraftStamp(syncedDraft.updatedAt, lastHandledDraftAtRef.current)) {
        lastHandledDraftAtRef.current = syncedDraft.updatedAt;
      }
      // getInputApi is resolved lazily and draftRef is a ref; the draft this
      // reads is deliberately the live one, not a render-scoped copy.
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [draftClientId, syncedDraft]);

    const acceptIncomingDraft = (): void => {
      if (!incomingDraft) {
        return;
      }
      lastHandledDraftAtRef.current = incomingDraft.updatedAt;
      // Only ever reached by pressing Use: nothing above writes the composer.
      loadComposerText(incomingDraft.content);
      setIncomingDraft(null);
    };

    const dismissIncomingDraft = (): void => {
      if (incomingDraft) {
        lastHandledDraftAtRef.current = incomingDraft.updatedAt;
      }
      setIncomingDraft(null);
    };

    const insertReference = (reference: string): void => {
      const api = getInputApi();
      const current = api?.getValue() ?? draft;
      const { end, start } = api?.getSelection() ?? {
        end: current.length,
        start: current.length,
      };
      const needsLeadingSpace = start > 0 && !/\s/.test(current[start - 1] ?? '');
      const inserted = `${needsLeadingSpace ? ' ' : ''}${reference} `;
      const next = `${current.slice(0, start)}${inserted}${current.slice(end)}`;
      updateDraft(next, start + inserted.length);
      api?.focus();
      api?.applyValue(next, start + inserted.length);
    };

    const addImagePreview = useCallback((path: string, dataUrl: string): void => {
      setPastedImages((currentImages) => {
        if (currentImages.some((image) => image.path === path)) {
          return currentImages;
        }
        pasteSequenceRef.current += 1;
        return [...currentImages, { dataUrl, id: `${path}#${pasteSequenceRef.current}`, path }];
      });
    }, []);

    // A pasted/typed literal "[Image #N](path)" is the same attachment as one
    // inserted by the paperclip. Resolve it through the shared image viewer so
    // it gains a thumbnail without requiring a second attach action.
    useEffect(() => {
      if (!imageViewer) {
        return;
      }
      for (const href of linkedImageReferenceHrefs(draft)) {
        if (pastedImages.some((image) => image.path === href) || previewLoadsRef.current.has(href)) {
          continue;
        }
        const pending = imageViewer.resolve(sessionChatImageTargetForHref(href));
        if (!pending) {
          continue;
        }
        previewLoadsRef.current.add(href);
        void pending
          .then((dataUrl) => {
            if (linkedImageReferenceHrefs(draftRef.current).includes(href)) {
              addImagePreview(href, dataUrl);
            }
          })
          .catch(() => {
            // Keep the literal reference when its preview cannot be loaded.
          })
          .finally(() => {
            previewLoadsRef.current.delete(href);
          });
      }
    }, [addImagePreview, draft, imageViewer, pastedImages]);

    const insertImageReference = (path: string, dataUrl?: string): void => {
      const api = getInputApi();
      const current = api?.getValue() ?? draft;
      insertReference(`[Image #${nextImageReferenceIndex(current)}](${path})`);
      if (dataUrl !== undefined) {
        addImagePreview(path, dataUrl);
      }
    };

    const insertFileReference = (path: string): void => {
      const api = getInputApi();
      const current = api?.getValue() ?? draft;
      insertReference(`[File #${nextFileReferenceIndex(current)}](${path})`);
    };

    const appendFileReferences = (paths: readonly string[]): void => {
      if (paths.length === 0) {
        return;
      }
      const api = getInputApi();
      const current = api?.getValue() ?? draftRef.current;
      const firstIndex = nextFileReferenceIndex(current);
      const references = paths.map((path, index) => `[File #${firstIndex + index}](${path})`).join('\n');
      const separator = current === '' || current.endsWith('\n\n') ? '' : current.endsWith('\n') ? '\n' : '\n\n';
      const next = `${current}${separator}${references}`;
      updateDraft(next, next.length);
      api?.applyValue(next, next.length);
      api?.focus();
    };

    /**
     * Inserts references for absolute paths on the session's machine (the
     * native picker and local drops): image paths keep the image reference
     * format and fetch their preview thumbnail lazily; everything else
     * becomes a "[File #N](path)" reference.
     */
    const insertNativePathReferences = async (paths: readonly string[]): Promise<void> => {
      for (const path of paths) {
        if (IMAGE_PATH_PATTERN.test(path)) {
          insertImageReference(path);
          onLoadImagePreview?.(path)
            .then((dataUrl) => {
              addImagePreview(path, dataUrl);
            })
            .catch(() => {
              // The preview is garnish; the reference is already inserted.
            });
        } else {
          insertFileReference(path);
        }
        // Let the input backend commit before the next caret-relative insert
        // (the textarea backend applies values on the next frame).
        await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
      }
    };

    const consumeDroppedAttachments = (data: DataTransfer): boolean => {
      if (disabled || !sessionChatDataTransferHasFiles(data)) {
        return false;
      }

      // A local GPUI session resolves the drop to the drag's real absolute
      // paths, folders included, captured natively by the shell at drag-enter.
      // Remote GPUI chats omit onNativeDropPaths, so local paths are never
      // handed to an agent running on another machine; those bytes follow the
      // upload path below.
      const nativePaths = sessionChatNativeDropPaths(onNativeDropPaths?.());
      if (nativePaths.length > 0) {
        void insertNativePathReferences(nativePaths);
        return true;
      }
      if (!onAttachFile) {
        return false;
      }

      // Snapshot synchronously: the DataTransfer dies with the drop event.
      const dropped = readSessionChatDroppedAttachments(data);
      setPendingImagePastes((count) => count + 1);
      void dropped
        .then(async ({ directories, files }) => {
          // Dropped images take the shared image intake so they insert
          // "[Image #N](path)" with a thumbnail, exactly like a pasted one.
          const imageFiles = onPasteImage ? files.filter(isImageFile) : [];
          if (imageFiles.length > 0) {
            consumeImageFiles(imageFiles);
          }
          const attachmentFiles = files.filter((file) => !imageFiles.includes(file));
          if (attachmentFiles.length > 0 || directories.length > 0) {
            appendFileReferences(
              await uploadSessionChatDroppedAttachments({ directories, files: attachmentFiles }, onAttachFile)
            );
          }
        })
        .catch((error: unknown) => {
          console.error('[session-chat] dropped attachment failed', error);
          setSendErrorCode(null);
          setSendError('The dropped file or folder could not be attached.');
        })
        .finally(() => {
          setPendingImagePastes((count) => count - 1);
        });
      return true;
    };

    const removePastedImage = (image: PastedImagePreview): void => {
      const api = getInputApi();
      const current = api?.getValue() ?? draft;
      const escapedPath = image.path.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const pattern = new RegExp(`\\s?\\[Image #\\d+\\]\\(${escapedPath}\\) ?`);
      const matchIndex = current.search(pattern);
      if (matchIndex < 0) {
        // The reference text is already gone; just drop the thumbnail.
        setPastedImages((images) => images.filter((entry) => entry.id !== image.id));
        return;
      }
      const next = current.replace(pattern, '');
      updateDraft(next, matchIndex);
      api?.applyValue(next, matchIndex);
    };

    /**
     * The image intake path: clipboard paste and the footer's attach button
     * both land here, so an attached image becomes the same "[Image #N](path)"
     * reference plus preview thumbnail a pasted one does.
     */
    const consumeImageFiles = (files: readonly File[]): void => {
      void (async () => {
        for (const file of files) {
          setPendingImagePastes((count) => count + 1);
          try {
            const dataUrl = await readFileAsDataUrl(file);
            const base64Data = dataUrl.split(',', 2)[1] ?? '';
            if (base64Data === '') {
              continue;
            }
            const path = await onPasteImage?.({
              base64Data,
              ...(file.name ? { suggestedName: file.name } : {}),
            });
            if (path !== undefined) {
              insertImageReference(path, dataUrl);
            }
          } catch (error) {
            console.error('[session-chat] image attach failed', error);
          } finally {
            setPendingImagePastes((count) => count - 1);
          }
        }
      })();
    };

    /** Non-image attach intake: upload the bytes, insert "[File #N](path)". */
    const consumeAttachmentFiles = (files: readonly File[]): void => {
      void (async () => {
        for (const file of files) {
          setPendingImagePastes((count) => count + 1);
          try {
            const dataUrl = await readFileAsDataUrl(file);
            const base64Data = dataUrl.split(',', 2)[1] ?? '';
            if (base64Data === '') {
              continue;
            }
            const path = await onAttachFile?.({
              base64Data,
              ...(file.name ? { suggestedName: file.name } : {}),
            });
            if (path !== undefined) {
              insertFileReference(path);
            }
          } catch (error) {
            console.error('[session-chat] file attach failed', error);
          } finally {
            setPendingImagePastes((count) => count - 1);
          }
        }
      })();
    };

    /**
     * Host-native picker intake: absolute paths on the session's machine
     * (folders included), no byte upload, inserted through the shared
     * native-path reference logic.
     */
    const attachFromNativePicker = (): void => {
      void (async () => {
        try {
          await insertNativePathReferences((await onPickPaths?.()) ?? []);
        } catch (error) {
          console.error('[session-chat] attach picker failed', error);
        }
      })();
    };

    /** Returns true when the clipboard held images this composer consumed. */
    const processClipboardData = (data: DataTransfer): boolean => {
      if (!onPasteImage || disabled) {
        return false;
      }
      const files = clipboardImageFiles(data);
      if (files.length === 0) {
        return false;
      }
      consumeImageFiles(files);
      return true;
    };

    const pasteClipboardData = (data: DataTransfer): boolean => {
      if (disabled) {
        return false;
      }
      if (processClipboardData(data)) {
        // Images were consumed as attachments; put the caret back in the input
        // so either keyboard paste or the custom menu ends with a ready composer.
        getInputApi()?.focus();
        return true;
      }
      const text = data.getData('text/plain');
      if (text === '') {
        return false;
      }
      const input = getInputApi();
      if (!input) {
        pendingInsertTextRef.current += text;
        return true;
      }
      return input.insertText(text);
    };

    const copyContextSelection = (cut: boolean): void => {
      const input = getInputApi();
      if (!input || contextSelection.start === contextSelection.end) {
        return;
      }
      const current = input.getValue();
      const start = Math.min(contextSelection.start, current.length);
      const end = Math.min(contextSelection.end, current.length);
      const selectedText = current.slice(start, end);
      void navigator.clipboard
        .writeText(selectedText)
        .then(() => {
          if (!cut || input.getValue() !== current) {
            return;
          }
          const next = `${current.slice(0, start)}${current.slice(end)}`;
          updateDraft(next, start);
          input.applyValue(next, start);
          input.focus();
        })
        .catch((error: unknown) => {
          console.error('[session-chat] clipboard write failed', error);
          setSendError('The clipboard could not be written.');
        });
    };

    const pasteFromContextMenu = (): void => {
      if (hostActions?.onPasteIntoComposer) {
        // Radix restores focus to the context-menu trigger as it closes. Wait
        // until that completes, then put Monaco's textarea back in charge
        // before the native CEF paste command delivers the real paste event.
        window.setTimeout(() => {
          getInputApi()?.focus();
          hostActions.onPasteIntoComposer?.();
        }, 0);
        return;
      }
      void readSessionChatSystemClipboard()
        .then((data) => {
          pasteClipboardData(data);
        })
        .catch((error: unknown) => {
          console.error('[session-chat] clipboard read failed', error);
          setSendError('The clipboard could not be read.');
        });
    };

    const completeSlashCommand = (command: SessionChatSlashCommand): void => {
      const next = `/${command.name}`;
      updateDraft(next, next.length);
      const api = getInputApi();
      api?.focus();
      api?.applyValue(next, next.length);
    };

    const handleSlashKeyDown = (event: SessionChatComposerKeyEvent): boolean => {
      if (!slashOpen) {
        return false;
      }
      const highlighted = slashMatches[highlightedIndex];
      if (event.key === 'Escape') {
        event.preventDefault();
        setSlashDismissed(true);
        return true;
      }
      if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
        event.preventDefault();
        const delta = event.key === 'ArrowUp' ? -1 : 1;
        setSlashIndex((current) => {
          const currentIndex = Math.min(current, slashMatches.length - 1);
          return (currentIndex + delta + slashMatches.length) % slashMatches.length;
        });
        return true;
      }
      if (event.key === 'Tab') {
        event.preventDefault();
        completeSlashCommand(highlighted);
        return true;
      }
      if (sendOnEnter && event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault();
        // A fully typed (or previously completed) command sends immediately;
        // a partial token completes first so arguments can still be added.
        if (draft === `/${highlighted.name}`) {
          send();
        } else {
          completeSlashCommand(highlighted);
        }
        return true;
      }
      return false;
    };

    /** Replaces the token under the caret and leaves the caret just past it. */
    const completeMention = (replacement: string): void => {
      if (!trigger) {
        return;
      }
      const next = `${draft.slice(0, trigger.start)}${replacement}${draft.slice(trigger.end)}`;
      const nextCaret = trigger.start + replacement.length;
      updateDraft(next, nextCaret);
      const api = getInputApi();
      api?.focus();
      api?.applyValue(next, nextCaret);
    };

    const completeSkillMention = (skill: SessionChatSkill): void => {
      completeMention(`${linkedSessionChatSkillMention(skill)} `);
    };

    const completeFileMention = (path: string): void => {
      completeMention(`${sessionChatFileMention(path)} `);
    };

    const handleSkillKeyDown = (event: SessionChatComposerKeyEvent): boolean => {
      if (!skillOpen) {
        return false;
      }
      const highlighted = skillMatches[highlightedSkillIndex];
      if (event.key === 'Escape') {
        event.preventDefault();
        setSkillDismissed(true);
        return true;
      }
      if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
        event.preventDefault();
        const delta = event.key === 'ArrowUp' ? -1 : 1;
        setSkillIndex((current) => {
          const currentIndex = Math.min(current, skillMatches.length - 1);
          return (currentIndex + delta + skillMatches.length) % skillMatches.length;
        });
        return true;
      }
      if (event.key === 'Tab' || (sendOnEnter && event.key === 'Enter' && !event.shiftKey)) {
        event.preventDefault();
        completeSkillMention(highlighted);
        return true;
      }
      return false;
    };

    const handleFileKeyDown = (event: SessionChatComposerKeyEvent): boolean => {
      if (!fileOpen) {
        return false;
      }
      if (event.key === 'Escape') {
        event.preventDefault();
        setFileDismissed(true);
        return true;
      }
      if (fileMatches.length === 0) {
        // Still listing: swallow nothing but Escape so typing and sending work.
        return false;
      }
      const highlighted = fileMatches[highlightedFileIndex];
      if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
        event.preventDefault();
        const delta = event.key === 'ArrowUp' ? -1 : 1;
        setFileIndex((current) => {
          const currentIndex = Math.min(current, fileMatches.length - 1);
          return (currentIndex + delta + fileMatches.length) % fileMatches.length;
        });
        return true;
      }
      if (event.key === 'Tab' || (sendOnEnter && event.key === 'Enter' && !event.shiftKey)) {
        event.preventDefault();
        if (highlighted !== undefined) {
          completeFileMention(highlighted);
        }
        return true;
      }
      return false;
    };

    const setMaximizedAndFocus = (next: boolean): void => {
      setMaximized(next);
      // The field never leaves the React tree, so the live input element is the
      // same node before and after the toggle and can be refocused right away.
      getInputApi()?.focus();
    };

    const handleKeyDown = (event: SessionChatComposerKeyEvent): void => {
      // IME guard: composition Enter confirms the composition; letting it fall
      // through would submit a partial draft. (The textarea wrapper additionally
      // preventDefaults composition Enter; Monaco manages its own IME.)
      if (event.isComposing) {
        return;
      }
      if (handleSkillKeyDown(event) || handleFileKeyDown(event) || handleSlashKeyDown(event)) {
        return;
      }
      /*
    Tab queues (plan 016 §1). It reaches here only after the three picker
    handlers above have declined it, so completing a slash command, a $skill or
    an @file still wins — that is the whole reason this sits below them and not
    in its own branch further up. Modified Tab (Shift/Cmd/Ctrl/Alt) is left to
    the platform so focus traversal and Monaco's own bindings survive; the
    accepted cost of taking plain Tab is losing tab-indent in the composer.
    */
      if (
        event.key === 'Tab' &&
        !event.shiftKey &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        canQueueDraft
      ) {
        event.preventDefault();
        queueCurrentDraft();
        return;
      }
      if (event.key === 'Escape') {
        event.preventDefault();
        // Maximize is an overlay, so Escape closes it first; the next Escape
        // interrupts the agent as usual.
        if (maximized) {
          setMaximizedAndFocus(false);
          return;
        }
        onInterrupt();
        return;
      }
      if (sendOnEnter && event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault();
        send();
        return;
      }
      if (event.key === 'ArrowUp' && (draft === '' || history.index !== null)) {
        const recalled = recallPreviousSessionChatDraft(history);
        if (recalled) {
          event.preventDefault();
          setHistory(recalled.history);
          setDraft(recalled.draft);
          setCaret(recalled.draft.length);
          getInputApi()?.applyValue(recalled.draft, recalled.draft.length);
        }
        return;
      }
      if (event.key === 'ArrowDown' && history.index !== null) {
        const recalled = recallNextSessionChatDraft(history);
        if (recalled) {
          event.preventDefault();
          setHistory(recalled.history);
          setDraft(recalled.draft);
          setCaret(recalled.draft.length);
          getInputApi()?.applyValue(recalled.draft, recalled.draft.length);
        }
      }
    };

    /*
  While the agent is working the footer button becomes Send as soon as the
  composer holds non-whitespace text; an empty composer keeps Stop (plan 016
  §1). Send is therefore only ever rendered with something to send, which
  collapses the old isWorking-specific enablement into one condition.
  */
    const hasSendableDraft = draft.trim() !== '';
    const showStopButton = (isWorking || stopButtonCoolingDown) && !hasSendableDraft;
    const sendDisabled = disabled || !hasSendableDraft;
    const composerInput = useMonaco ? (
      <SessionChatMonacoInput
        disabled={disabled}
        fillHeight={maximized}
        initialValue={draft}
        onCaretChange={setCaret}
        onChange={updateDraft}
        onKeyDown={handleKeyDown}
        onLoadFailed={(error) => {
          console.error('[session-chat] Monaco failed to load; using the plain input.', error);
          setMonacoFailed(true);
        }}
        onPasteData={processClipboardData}
        placeholder={placeholder ?? (sendOnEnter ? DESKTOP_SESSION_CHAT_PLACEHOLDER : MOBILE_SESSION_CHAT_PLACEHOLDER)}
        registerApi={(api) => {
          monacoApiRef.current = api;
          if (api && pendingInsertTextRef.current) {
            const pending = pendingInsertTextRef.current;
            pendingInsertTextRef.current = '';
            api.insertText(pending);
          }
          if (api && pendingSavedPromptRef.current) {
            const pending = pendingSavedPromptRef.current;
            pendingSavedPromptRef.current = '';
            api.insertSavedPrompt(pending);
          }
          if (api && pendingFocusRef.current) {
            pendingFocusRef.current = false;
            api.focus();
          }
        }}
        theme={theme}
        vsBaseUrl={monacoVsBaseUrl ?? ''}
      />
    ) : (
      <SessionChatPlainInput
        disabled={disabled}
        initialValue={draft}
        invalid={sendError !== null}
        onCaretChange={setCaret}
        onChange={updateDraft}
        onKeyDown={(event) => {
          const adapted = reactKeyEventAdapter(event);
          if (adapted.isComposing) {
            if (adapted.key === 'Enter') {
              event.preventDefault();
            }
            return;
          }
          handleKeyDown(adapted);
        }}
        onPasteData={processClipboardData}
        placeholder={placeholder ?? (sendOnEnter ? DESKTOP_SESSION_CHAT_PLACEHOLDER : MOBILE_SESSION_CHAT_PLACEHOLDER)}
        registerApi={(api) => {
          plainApiRef.current = api;
          if (api && pendingInsertTextRef.current) {
            const pending = pendingInsertTextRef.current;
            pendingInsertTextRef.current = '';
            api.insertText(pending);
          }
          if (api && pendingSavedPromptRef.current) {
            const pending = pendingSavedPromptRef.current;
            pendingSavedPromptRef.current = '';
            api.insertSavedPrompt(pending);
          }
          if (api && pendingFocusRef.current) {
            pendingFocusRef.current = false;
            api.focus();
          }
        }}
      />
    );
    return (
      <>
        {maximized ? (
          <div
            aria-hidden='true'
            className='ghostex-chat-composer-backdrop'
            onClick={() => {
              setMaximizedAndFocus(false);
            }}
          />
        ) : null}
        {/* min-w-0 all the way down to the input: this sits in a grid/flex column,
          whose items are min-width:auto by default, so an unbreakable pasted run
          would otherwise widen the composer past the pane and scroll the page. */}
        <Field
          className={cn('relative min-w-0 gap-2', maximized && 'ghostex-chat-composer-maximized')}
          data-invalid={sendError !== null ? true : undefined}
        >
          {slashOpen ? (
            <div className='ghostex-chat-composer-picker absolute inset-x-0 bottom-full z-10 mb-2 overflow-hidden rounded-2xl border border-input bg-popover shadow-xl'>
              <div
                className='max-h-72 overflow-y-auto p-1.5'
                ref={slashListRef}
                role='listbox'
                aria-label='Slash commands'
              >
                <div className='px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground'>
                  {slashHeading ?? 'Commands'}
                </div>
                {slashMatches.map((command, index) => (
                  <button
                    aria-selected={index === highlightedIndex}
                    className={cn(
                      'grid w-full min-w-0 grid-cols-[200px_minmax(0,1fr)] items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm',
                      index === highlightedIndex ? 'bg-accent text-accent-foreground' : 'text-foreground'
                    )}
                    data-chat-picker-option='true'
                    data-highlighted={index === highlightedIndex ? 'true' : undefined}
                    key={command.name}
                    onMouseDown={(event) => {
                      // Keep textarea focus; complete on the same gesture.
                      event.preventDefault();
                      completeSlashCommand(command);
                    }}
                    onMouseMove={() => {
                      if (index !== highlightedIndex) {
                        setSlashIndex(index);
                      }
                    }}
                    role='option'
                    type='button'
                  >
                    <span className='min-w-0 truncate font-normal' title={`/${command.name}`}>
                      /{command.name}
                    </span>
                    <span className='min-w-0 truncate text-left text-muted-foreground' title={command.description}>
                      {command.description}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          ) : null}
          {skillOpen ? (
            <div className='ghostex-chat-composer-picker absolute inset-x-0 bottom-full z-10 mb-2 overflow-hidden rounded-2xl border border-input bg-popover shadow-xl'>
              <div
                aria-label='Available skills'
                className='max-h-72 overflow-y-auto p-1.5'
                ref={skillListRef}
                role='listbox'
              >
                <div className='px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground'>
                  {skillHeading ?? 'Skills'}
                </div>
                {skillMatches.map((skill, index) => (
                  <button
                    aria-selected={index === highlightedSkillIndex}
                    className={cn(
                      'grid w-full min-w-0 grid-cols-[200px_minmax(0,1fr)] items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm',
                      index === highlightedSkillIndex ? 'bg-accent text-accent-foreground' : 'text-foreground'
                    )}
                    data-chat-picker-option='true'
                    data-highlighted={index === highlightedSkillIndex ? 'true' : undefined}
                    key={`${skill.name}:${skill.directoryPath}`}
                    onMouseDown={(event) => {
                      event.preventDefault();
                      completeSkillMention(skill);
                    }}
                    onMouseMove={() => {
                      if (index !== highlightedSkillIndex) {
                        setSkillIndex(index);
                      }
                    }}
                    role='option'
                    type='button'
                  >
                    <span className='min-w-0 truncate font-normal' title={`$${skill.name}`}>
                      ${skill.name}
                    </span>
                    <span
                      className='min-w-0 truncate text-left text-muted-foreground'
                      title={sessionChatDisplaySkillDirectoryPath(skill.directoryPath)}
                    >
                      {sessionChatDisplaySkillDirectoryPath(skill.directoryPath)}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          ) : null}
          {fileOpen ? (
            <div className='ghostex-chat-composer-picker absolute inset-x-0 bottom-full z-10 mb-2 overflow-hidden rounded-2xl border border-input bg-popover shadow-xl'>
              <div
                aria-label='Project files'
                className='max-h-72 overflow-y-auto p-1.5'
                ref={fileListRef}
                role='listbox'
              >
                <div className='px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground'>
                  {fileHeading ?? 'Files'}
                </div>
                {fileMatches.length === 0 ? (
                  <div className='flex items-center gap-2.5 px-3 py-2 text-sm text-muted-foreground'>
                    <IconLoader2 aria-hidden='true' className='size-4 shrink-0 animate-spin' stroke={2} />
                    Listing project files…
                  </div>
                ) : null}
                {fileMatches.map((path, index) => (
                  <button
                    aria-selected={index === highlightedFileIndex}
                    className={cn(
                      'flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm',
                      index === highlightedFileIndex ? 'bg-accent text-accent-foreground' : 'text-foreground'
                    )}
                    data-chat-picker-option='true'
                    data-highlighted={index === highlightedFileIndex ? 'true' : undefined}
                    key={path}
                    onMouseDown={(event) => {
                      event.preventDefault();
                      completeFileMention(path);
                    }}
                    onMouseMove={() => {
                      if (index !== highlightedFileIndex) {
                        setFileIndex(index);
                      }
                    }}
                    role='option'
                    type='button'
                  >
                    <IconFile aria-hidden='true' className='size-4 shrink-0 text-muted-foreground' stroke={1.6} />
                    <span className='shrink-0 font-semibold'>{sessionChatFileBasename(path)}</span>
                    <span className='truncate text-muted-foreground'>{sessionChatFileDirectory(path)}</span>
                  </button>
                ))}
              </div>
            </div>
          ) : null}
          {sendError && sendErrorCode === 'composerNotReady' ? (
            <SessionChatComposerNotReadyNotice
              reason={sendError}
              {...(onReadTerminalTail ? { onReadTerminalTail } : {})}
              {...(hostActions?.onSwitchToTerminal ? { onOpenTerminal: hostActions.onSwitchToTerminal } : {})}
            />
          ) : sendError ? (
            <FieldError className='px-2'>{sendError}</FieldError>
          ) : null}
          <SessionChatAgentFleetStrip fleet={agentFleet ?? null} />
          {incomingDraft ? (
            <div className='ghostex-chat-draft-conflict' role='status'>
              <IconDeviceMobileMessage aria-hidden='true' size={14} stroke={1.8} />
              <span className='ghostex-chat-draft-conflict-text'>Newer draft from another device</span>
              <button className='ghostex-chat-draft-conflict-action' onClick={acceptIncomingDraft} type='button'>
                Use
              </button>
              <button className='ghostex-chat-draft-conflict-action' onClick={dismissIncomingDraft} type='button'>
                Dismiss
              </button>
            </div>
          ) : null}
          <div
            className={cn(
              'ghostex-chat-composer min-w-0 rounded-3xl border border-input bg-card px-4 py-2.5 transition-colors focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/20',
              disabled && 'opacity-60'
            )}
            data-disabled={disabled ? 'true' : undefined}
            onBlur={(event) => {
              // focusout bubbles from both input backends. Only a focus move that
              // LEAVES the composer is a "the user stopped typing" moment.
              if (event.currentTarget.contains(event.relatedTarget)) {
                return;
              }
              // relatedTarget null = focus left the page (native first-responder
              // move), an element = an in-page steal. Distinguishing the two is
              // the point of this breadcrumb.
              diagnosticLogRef.current?.('sessionChat.composerFocusLeft', {
                relatedTag: event.relatedTarget instanceof Element ? event.relatedTarget.tagName : 'none',
              });
              pushDraftIfChanged();
              // Re-assert the composer content state at the moment focus leaves
              // — hiding the surface blurs it, and no text can be typed while
              // hidden, so this report is the authoritative one an eviction
              // pass 20 minutes later relies on. A flip report lost earlier is
              // repaired here rather than trusted forever.
              onDraftEmptyChangeRef.current?.(draftEmptyRef.current);
            }}
            onFocus={(event) => {
              if (event.currentTarget.contains(event.relatedTarget)) {
                return;
              }
              diagnosticLogRef.current?.('sessionChat.composerFocusEntered');
            }}
          >
            {pastedImages.length > 0 || pendingImagePastes > 0 ? (
              <div className='flex flex-wrap items-center gap-2 pb-2'>
                {pastedImages.map((image) => (
                  <div className='relative' key={image.id}>
                    <button
                      aria-label='View pasted image'
                      className='block cursor-zoom-in rounded-lg'
                      disabled={!imageViewer}
                      onClick={() =>
                        imageViewer?.open({
                          alt: 'Pasted image',
                          url: image.dataUrl,
                        })
                      }
                      type='button'
                    >
                      <img
                        alt='Pasted image'
                        className='h-12 w-12 rounded-lg border border-input object-cover'
                        src={image.dataUrl}
                      />
                    </button>
                    <button
                      aria-label='Remove image'
                      className='absolute -right-1.5 -top-1.5 flex size-4 items-center justify-center rounded-full border border-input bg-card text-muted-foreground hover:text-foreground'
                      onClick={() => removePastedImage(image)}
                      type='button'
                    >
                      <IconX aria-hidden='true' size={10} stroke={2.4} />
                    </button>
                  </div>
                ))}
                {pendingImagePastes > 0 ? (
                  <div
                    aria-label='Saving attachment'
                    className='flex h-12 w-12 items-center justify-center rounded-lg border border-dashed border-input text-muted-foreground'
                  >
                    <IconLoader2 aria-hidden='true' className='animate-spin' size={16} stroke={2} />
                  </div>
                ) : null}
              </div>
            ) : null}
            {/* Queued prompts sit directly above the input, inside this
              container. Never in the transcript — that lane belongs to the
              agent CLI's own queue (SessionChatMessage.queued). */}
            {queueCapabilities?.supported && queue ? (
              <SessionChatQueueRows
                disabled={disabled}
                prompts={queue.prompts}
                {...(queueCapabilities.canEdit ? { onEdit: editQueuedPrompt } : {})}
                {...(queueCapabilities.canRemove
                  ? {
                      onDelete: (prompt: SessionChatQueuedPrompt) => {
                        void queue.removePrompt(prompt.id);
                      },
                    }
                  : {})}
                {...(queueCapabilities.canSendNow
                  ? {
                      onSendNow: (prompt: SessionChatQueuedPrompt) => {
                        void queue.sendNow(prompt.id);
                      },
                    }
                  : {})}
                {...(queueCapabilities.canRetry
                  ? {
                      onRetry: (prompt: SessionChatQueuedPrompt) => {
                        void queue.retryPrompt(prompt.id);
                      },
                    }
                  : {})}
                {...(queueCapabilities.canReorder
                  ? {
                      onReorder: (promptIds: string[]) => {
                        void queue.reorder(promptIds);
                      },
                    }
                  : {})}
              />
            ) : null}
            {nativeContextMenu ? (
              <div className='ghostex-chat-composer-row flex min-w-0 select-text items-end gap-2 pb-1.5'>
                {composerInput}
              </div>
            ) : (
              <ContextMenu
                onOpenChange={(open) => {
                  if (open) {
                    setContextSelection(getInputApi()?.getSelection() ?? { end: 0, start: 0 });
                  }
                }}
              >
                <ContextMenuTrigger className='ghostex-chat-composer-row flex min-w-0 select-text items-end gap-2 pb-1.5'>
                  {composerInput}
                </ContextMenuTrigger>
                <ContextMenuContent>
                  <ContextMenuGroup>
                    <ContextMenuItem
                      disabled={disabled || contextSelection.start === contextSelection.end}
                      onClick={() => copyContextSelection(true)}
                    >
                      <IconCut aria-hidden='true' />
                      Cut
                    </ContextMenuItem>
                    <ContextMenuItem
                      disabled={contextSelection.start === contextSelection.end}
                      onClick={() => copyContextSelection(false)}
                    >
                      <IconCopy aria-hidden='true' />
                      Copy
                    </ContextMenuItem>
                    <ContextMenuItem disabled={disabled} onClick={pasteFromContextMenu}>
                      <IconClipboard aria-hidden='true' />
                      Paste
                    </ContextMenuItem>
                  </ContextMenuGroup>
                  <ContextMenuSeparator />
                  <ContextMenuGroup>
                    <ContextMenuItem disabled={draft.length === 0} onClick={() => getInputApi()?.selectAll()}>
                      <IconSelectAll aria-hidden='true' />
                      Select all
                    </ContextMenuItem>
                  </ContextMenuGroup>
                </ContextMenuContent>
              </ContextMenu>
            )}
            <div className='ghostex-chat-composer-footer flex w-full items-center justify-between gap-2'>
              <div className='ghostex-chat-composer-footer-options flex min-w-0 items-center gap-0.5'>
                {optionPills}
              </div>
              <div className='ghostex-chat-composer-footer-actions ml-auto flex items-center gap-1.5'>
                {onPasteImage || onAttachFile || onPickPaths ? (
                  <>
                    {onPickPaths ? null : (
                      <input
                        className='hidden'
                        multiple
                        onChange={(event) => {
                          const files = Array.from(event.target.files ?? []);
                          // Same input element every time: clear it so re-picking
                          // the same file still fires change.
                          event.target.value = '';
                          const images = files.filter((file) => isImageFile(file) && onPasteImage !== undefined);
                          const others = files.filter((file) => !images.includes(file) && onAttachFile !== undefined);
                          if (images.length > 0) {
                            consumeImageFiles(images);
                          }
                          if (others.length > 0) {
                            consumeAttachmentFiles(others);
                          }
                        }}
                        ref={fileInputRef}
                        tabIndex={-1}
                        type='file'
                        {...(onAttachFile ? {} : { accept: 'image/*' })}
                      />
                    )}
                  </>
                ) : null}
                <SessionChatComposerActions
                  disabled={disabled}
                  hasSendableDraft={hasSendableDraft}
                  maximized={maximized}
                  onToggleMaximized={() => {
                    setMaximizedAndFocus(!maximized);
                  }}
                  sessionNoteActive={sessionNoteActive}
                  sessionNoteHasText={sessionNoteHasText}
                  showShortcutLabels={showShortcutLabels}
                  stashedPromptCount={stashedPromptCount}
                  summaryMode={summaryMode}
                  verboseMode={verboseMode}
                  {...(hostActions ? { hostActions } : {})}
                  {...(onDelayedActions ? { onDelayedActions } : {})}
                  {...(onReadTerminalTail ? { onReadTerminalTail } : {})}
                  {...(onSessionNote ? { onSessionNote } : {})}
                  {...(onShowStashedPrompts ? { onShowStashedPrompts } : {})}
                  {...(onStash ? { onStash } : {})}
                  {...(onToggleSummary ? { onToggleSummary } : {})}
                  {...(onToggleVerbose ? { onToggleVerbose } : {})}
                  {...(onPasteImage || onAttachFile || onPickPaths
                    ? {
                        onAttach: () => {
                          if (onPickPaths) {
                            attachFromNativePicker();
                          } else {
                            fileInputRef.current?.click();
                          }
                        },
                      }
                    : {})}
                />
                {showStopButton ? (
                  <Button
                    aria-label='Stop the agent'
                    className='size-6'
                    disabled={stopButtonCoolingDown}
                    onClick={handleStopClick}
                    size='icon'
                    variant='secondary'
                  >
                    <IconPlayerStopFilled aria-hidden='true' className='size-3' stroke={1.6} />
                  </Button>
                ) : (
                  <Button
                    aria-label={canQueueDraft ? 'Send (hold to queue)' : 'Send'}
                    className='ghostex-chat-send-button size-6'
                    disabled={sendDisabled}
                    onClick={handleSendClick}
                    onContextMenu={(event) => {
                      // A touch long-press otherwise raises the platform callout
                      // menu on top of the queue gesture.
                      if (canQueueDraft) {
                        event.preventDefault();
                      }
                    }}
                    onPointerCancel={cancelSendLongPress}
                    onPointerDown={beginSendLongPress}
                    onPointerLeave={cancelSendLongPress}
                    onPointerUp={cancelSendLongPress}
                    size='icon'
                  >
                    <IconArrowUp aria-hidden='true' className='size-3' stroke={2.2} />
                  </Button>
                )}
              </div>
            </div>
          </div>
        </Field>
      </>
    );
  }
);
