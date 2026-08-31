import {
  IconClock,
  IconClockCheck,
  IconDots,
  IconEyeFilled,
  IconEyeOff,
  IconFileExport,
  IconGitBranch,
  IconListCheck,
  IconListDetails,
  IconMaximize,
  IconMinimize,
  IconNote,
  IconPaperclip,
  IconPencil,
  IconRefresh,
  IconStackPush,
  IconTerminal2,
  type Icon as TablerIcon,
} from '@tabler/icons-react';
import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type ReactNode } from 'react';
import { Button } from '../../components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from '../../components/ui/dropdown-menu';
import { cn } from '@/packages/components/utils';
import type { GxserverReadSessionTerminalTailResult } from '@/packages/shared/gxserver-protocol';
import { AppTooltip } from '../app-tooltip';
import { formatSidebarHotkeyLabel } from '../hotkey-label';
import type { SessionChatHostAction, SessionChatHostActions } from './session-chat-host-actions';
import { sessionChatSummaryToggleHotkey } from './session-chat-summary-override';
import { formatSessionTerminalTailPreview, useSessionTerminalTail } from './use-session-terminal-tail';

/**
 * Host actions intentionally excluded from the dots menu. Most already render
 * as footer controls; Prompt editor is omitted from the chat overflow menu.
 */
const COMPOSER_MENU_EXCLUDED_HOST_ACTION_IDS = new Set(['attachPath', 'promptEditor', 'stashPrompt', 'stashedPrompts']);

/** Per-session lifecycle actions, shown under the menu's "Agent" heading. */
const AGENT_HOST_ACTION_IDS = new Set(['fork', 'fullReload', 'rename', 'sleep']);

const HOST_ACTION_ICONS: Record<string, TablerIcon> = {
  closeAfterDone: IconClock,
  delayedActions: IconClockCheck,
  exportTranscript: IconFileExport,
  fork: IconGitBranch,
  fullReload: IconRefresh,
  rename: IconPencil,
};

// Copied verbatim from apps/desktop/assets/titlebar/moon.svg (the same glyph the
// gpui titlebar and the floating host-actions cluster use for Sleep).
function SleepMoonIcon() {
  return (
    <svg aria-hidden='true' className='size-4' fill='currentColor' viewBox='0 0 32 32'>
      <path
        d='M30.4422 21.7576L30.4116 21.7051C30.2498 21.4554 29.954 21.3157 29.6478 21.3697C29.5454 21.3877 29.4525 21.4254 29.3705 21.4785L29.375 21.4756C28.2165 22.2303 26.8137 22.7975 25.2833 23.0673C19.1647 24.1462 13.3295 20.0604 12.2506 13.9418C11.4414 9.3526 13.5372 4.9234 17.2172 2.5401L17.2852 2.4997L17.4776 2.3754C17.72 2.2129 17.8546 1.9221 17.8014 1.6207C17.7363 1.2514 17.4105 0.9931 17.0476 1.0022L17.0435 1.0019C16.3825 1.0139 15.6299 1.0877 14.8745 1.2209C6.8533 2.6353 1.4972 10.2846 2.9116 18.3058C4.3259 26.3271 11.9752 31.6832 19.9965 30.2688C24.6723 29.4443 28.4435 26.5007 30.4942 22.5994L30.5129 22.5615C30.5867 22.4216 30.616 22.254 30.586 22.0836C30.5639 21.9585 30.5128 21.8467 30.4404 21.7529L30.443 21.7564Z'
        transform='rotate(-10 16 16)'
      />
    </svg>
  );
}

function hostActionIcon(id: string): ReactNode {
  if (id === 'sleep') {
    return <SleepMoonIcon />;
  }
  const Icon = HOST_ACTION_ICONS[id];
  return Icon ? <Icon aria-hidden='true' /> : null;
}

interface SessionChatComposerActionsProps {
  disabled: boolean;
  hasSendableDraft: boolean;
  /**
   * Per-session host actions: the surface switch renders as its own footer
   * control next to Send, the rest fold into the dots menu.
   */
  hostActions?: SessionChatHostActions;
  maximized: boolean;
  onAttach?: () => void;
  onDelayedActions?: () => void;
  /**
   * Session-scoped terminal tail read. Drives the Terminal View button's
   * readiness tint and its hover preview; hosts whose transport has no route to
   * /api/readSessionTerminalTail omit it and the button stays neutral.
   */
  onReadTerminalTail?: () => Promise<GxserverReadSessionTerminalTailResult>;
  onSessionNote?: () => void;
  onShowStashedPrompts?: () => void;
  onStash?: () => void;
  onToggleMaximized: () => void;
  onToggleSummary?: () => void;
  onToggleVerbose?: () => void;
  sessionNoteActive: boolean;
  sessionNoteHasText: boolean;
  stashedPromptCount: number;
  summaryMode: boolean;
  verboseMode: boolean;
}

export function SessionChatComposerActions({
  disabled,
  hasSendableDraft,
  hostActions,
  maximized,
  onAttach,
  onDelayedActions,
  onReadTerminalTail,
  onSessionNote,
  onShowStashedPrompts,
  onStash,
  onToggleMaximized,
  onToggleSummary,
  onToggleVerbose,
  sessionNoteActive,
  sessionNoteHasText,
  stashedPromptCount,
  summaryMode,
  verboseMode,
}: SessionChatComposerActionsProps) {
  /*
  Every footer control names its shortcut in its tooltip, the way the desktop
  terminal's action bar does, so the two surfaces teach the same key. The host's
  action list is where the effective (user-configurable) chords come from — the
  composer cannot resolve them itself — and a control whose action the host did
  not supply simply shows no chord rather than a guessed one.
  */
  const hostActionShortcut = (id: string): string | undefined =>
    hostActions?.actions?.find((action) => action.id === id)?.shortcut;
  const withShortcut = (label: string, shortcut?: string): string => (shortcut ? `${label} (${shortcut})` : label);

  const stashLabel = 'Stash prompt';
  const stashTooltip = [
    withShortcut(stashLabel, hostActionShortcut('stashPrompt')),
    ...(onShowStashedPrompts
      ? [withShortcut('Right-click to open Saved prompts', hostActionShortcut('stashedPrompts'))]
      : []),
  ].join('\n');
  const stashCurrentPrompt = () => {
    if (!hasSendableDraft) {
      onShowStashedPrompts?.();
      return;
    }
    if (!disabled) {
      onStash?.();
    }
  };
  const openSavedPromptsFromContextMenu = (event: ReactMouseEvent<HTMLElement>) => {
    if (!onShowStashedPrompts) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    onShowStashedPrompts();
  };
  const maximizeLabel = maximized ? 'Exit maximize' : 'Maximize';
  const verboseLabel = verboseMode ? 'Verbose mode on' : 'Verbose mode off';
  const VerboseIcon = verboseMode ? IconEyeFilled : IconEyeOff;

  /*
  Host actions that carry `input` (Rename) swap the footer's control row for an
  inline field, the way the floating cluster they replaced did. The field takes
  focus from an effect rather than `autoFocus`: picking the action unmounts the
  dots menu together with the row it lives in, and the effect runs after that
  removal, so the menu's own focus handling cannot pull the caret back out.
  */
  const [inputAction, setInputAction] = useState<SessionChatHostAction | null>(null);
  const [inputValue, setInputValue] = useState('');
  const inputRef = useRef<HTMLInputElement | null>(null);
  const inputSettledRef = useRef(false);
  useEffect(() => {
    if (!inputAction) {
      return;
    }
    inputSettledRef.current = false;
    const input = inputRef.current;
    if (input) {
      input.focus();
      input.select();
    }
  }, [inputAction]);
  const closeHostActionInput = () => {
    inputSettledRef.current = true;
    setInputAction(null);
  };
  const submitHostActionInput = () => {
    if (inputSettledRef.current || !inputAction) {
      return;
    }
    inputSettledRef.current = true;
    hostActions?.onAction?.(inputAction.id, inputValue);
    setInputAction(null);
  };

  const hostActionList = hostActions?.actions ?? [];
  const runHostAction = (action: SessionChatHostAction) => {
    if (action.input) {
      setInputValue(action.input.initialValue ?? '');
      setInputAction(action);
      return;
    }
    hostActions?.onAction?.(action.id);
  };
  const hostActionMenuItem = (action: SessionChatHostAction) => (
    <DropdownMenuItem key={action.id} onClick={() => runHostAction(action)}>
      {hostActionIcon(action.id)}
      {action.label}
      {action.shortcut ? <DropdownMenuShortcut>{action.shortcut}</DropdownMenuShortcut> : null}
    </DropdownMenuItem>
  );
  /*
  The host's Delayed Actions entry and the composer's own open the same surface,
  so only one of them may render. The host's is preferred as the click target
  only when the composer has no handler of its own; either way the host entry
  supplies the shortcut label the composer cannot know.
  */
  const delayedHostAction = hostActionList.find((action) => action.id === 'delayedActions');
  const closeAfterDoneHostAction = hostActionList.find((action) => action.id === 'closeAfterDone');
  const foldedHostActions = hostActionList.filter(
    (action) =>
      action.id !== 'delayedActions' &&
      action.id !== 'closeAfterDone' &&
      !COMPOSER_MENU_EXCLUDED_HOST_ACTION_IDS.has(action.id)
  );
  const agentHostActions = foldedHostActions.filter((action) => AGENT_HOST_ACTION_IDS.has(action.id));
  const otherHostActions = foldedHostActions.filter((action) => !AGENT_HOST_ACTION_IDS.has(action.id));

  // Chat presentation toggles, Delayed actions, and Close After Done live only
  // inside the dots menu, on every footer width, so both menus share them.
  const verboseMenuItem = onToggleVerbose ? (
    <DropdownMenuCheckboxItem
      className='whitespace-nowrap'
      checked={verboseMode}
      closeOnClick={false}
      onCheckedChange={(checked: boolean) => {
        if (checked !== verboseMode) {
          onToggleVerbose();
        }
      }}
    >
      <VerboseIcon aria-hidden='true' />
      Verbose mode
    </DropdownMenuCheckboxItem>
  ) : null;
  const summaryMenuItem = onToggleSummary ? (
    <DropdownMenuCheckboxItem
      className='pr-2 whitespace-nowrap'
      checked={summaryMode}
      closeOnClick={false}
      onCheckedChange={(checked: boolean) => {
        if (checked !== summaryMode) {
          onToggleSummary();
        }
      }}
      showIndicator={false}
    >
      {summaryMode ? <IconListCheck aria-hidden='true' /> : <IconListDetails aria-hidden='true' />}
      Summary mode
      <DropdownMenuShortcut>{formatSidebarHotkeyLabel(sessionChatSummaryToggleHotkey())}</DropdownMenuShortcut>
    </DropdownMenuCheckboxItem>
  ) : null;
  const delayedActionsMenuItem =
    onDelayedActions || delayedHostAction ? (
      <DropdownMenuItem
        className='whitespace-nowrap'
        disabled={onDelayedActions ? disabled : false}
        onClick={onDelayedActions ?? (delayedHostAction ? () => runHostAction(delayedHostAction) : undefined)}
      >
        <IconClockCheck aria-hidden='true' />
        {onDelayedActions ? 'Delayed actions' : (delayedHostAction?.label ?? 'Delayed actions')}
        {delayedHostAction?.shortcut ? <DropdownMenuShortcut>{delayedHostAction.shortcut}</DropdownMenuShortcut> : null}
      </DropdownMenuItem>
    ) : null;
  const closeAfterDoneMenuItem = closeAfterDoneHostAction ? hostActionMenuItem(closeAfterDoneHostAction) : null;
  const stashCountBadge =
    stashedPromptCount > 0 ? (
      <span aria-hidden='true' className='ghostex-chat-stash-count-badge'>
        {Math.min(stashedPromptCount, 9)}
      </span>
    ) : null;

  const agentMenuSection =
    agentHostActions.length > 0 ? (
      <DropdownMenuGroup>
        <DropdownMenuLabel>Agent</DropdownMenuLabel>
        {agentHostActions.map(hostActionMenuItem)}
      </DropdownMenuGroup>
    ) : null;
  const otherHostMenuSection =
    otherHostActions.length > 0 ? (
      <DropdownMenuGroup>{otherHostActions.map(hostActionMenuItem)}</DropdownMenuGroup>
    ) : null;
  /** `precededByItems` says whether the menu already has rows above these. */
  const hostMenuSections = (precededByItems: boolean) => (
    <>
      {agentMenuSection ? (
        <>
          {precededByItems ? <DropdownMenuSeparator /> : null}
          {agentMenuSection}
        </>
      ) : null}
      {otherHostMenuSection ? (
        <>
          {precededByItems || agentMenuSection ? <DropdownMenuSeparator /> : null}
          {otherHostMenuSection}
        </>
      ) : null}
    </>
  );
  const hasBaseMenuItems =
    verboseMenuItem !== null ||
    summaryMenuItem !== null ||
    delayedActionsMenuItem !== null ||
    closeAfterDoneMenuItem !== null;
  const hasExpandedMenu = hasBaseMenuItems || agentMenuSection !== null || otherHostMenuSection !== null;

  /*
  CDXC:ComposerTerminalReadiness 2026-08-28:
  The Terminal View button doubles as the composer's readiness light. Its tint
  comes from the last terminal tail read (use-session-terminal-tail.ts) and only
  ever takes a color for a *measured* verdict: `unknown`, an uncaptured screen,
  a host without the endpoint, and the time before the first hover all keep the
  neutral footer color, because the daemon fails open on `unknown` and a red
  button there would accuse a session that sends fine.

  Hovering reads before the tooltip's open delay elapses. There is deliberately
  no background timer: an unseen terminal preview costs no captures or RPCs.
  */
  const { refreshNow: refreshTerminalTail, tail: terminalTail } = useSessionTerminalTail(onReadTerminalTail);
  const terminalReadiness =
    terminalTail?.captured && terminalTail.composerState !== 'unknown' ? terminalTail.composerState : null;
  const terminalTailPreview =
    terminalTail?.captured === true ? formatSessionTerminalTailPreview(terminalTail.lines) : '';
  const switchViewTitle = withShortcut('Click to Switch to Terminal View', hostActions?.switchViewShortcut);
  const switchViewNotReadyReason = terminalReadiness === 'notReady' ? (terminalTail?.reason ?? null) : null;
  const switchViewTooltip: ReactNode = (
    <div className='flex min-w-0 flex-col gap-1.5'>
      <div className='flex flex-col gap-0.5'>
        <strong className='font-semibold'>Agent CLI Preview</strong>
        <div>{switchViewTitle}</div>
      </div>
      {switchViewNotReadyReason ? <div className='opacity-70'>{switchViewNotReadyReason}</div> : null}
      {terminalTailPreview.length > 0 ? (
        <pre className='max-w-full font-mono text-[14px] leading-[1.5] whitespace-pre-wrap opacity-85'>
          {terminalTailPreview}
        </pre>
      ) : null}
    </div>
  );

  const switchViewButton = hostActions ? (
    <AppTooltip
      content={switchViewTooltip}
      {...(terminalTailPreview.length > 0 ? { contentStyle: { maxWidth: 'min(92vw, calc(46rem + 200px))' } } : {})}
    >
      <span className='inline-flex' onFocus={refreshTerminalTail} onMouseEnter={refreshTerminalTail}>
        <Button
          aria-label='Terminal View'
          className='ghostex-chat-footer-control rounded-full'
          onClick={hostActions.onSwitchToTerminal}
          size='icon-sm'
          variant='ghost'
          {...(terminalReadiness ? { 'data-terminal-ready': terminalReadiness } : {})}
        >
          <IconTerminal2 aria-hidden='true' stroke={2} />
        </Button>
      </span>
    </AppTooltip>
  ) : null;

  if (inputAction) {
    return (
      <input
        aria-label={inputAction.label}
        className='ghostex-chat-host-action-input h-8 max-w-full min-w-0 rounded-full border border-input bg-transparent px-3 text-xs text-foreground outline-none focus:border-ring'
        onBlur={closeHostActionInput}
        onChange={(event) => setInputValue(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault();
            submitHostActionInput();
          } else if (event.key === 'Escape') {
            event.preventDefault();
            closeHostActionInput();
          }
        }}
        placeholder={inputAction.input?.placeholder ?? inputAction.label}
        ref={inputRef}
        value={inputValue}
      />
    );
  }

  return (
    <>
      <div className='ghostex-chat-composer-footer-actions-expanded items-center gap-1.5'>
        {hasExpandedMenu ? (
          <DropdownMenu>
            <AppTooltip content={withShortcut('More actions', hostActions?.moreActionsShortcut)}>
              <DropdownMenuTrigger
                render={
                  <Button
                    aria-label='More actions'
                    className='ghostex-chat-footer-control rounded-full'
                    size='icon-sm'
                    variant='ghost'
                  />
                }
              >
                <IconDots aria-hidden='true' stroke={2.2} />
              </DropdownMenuTrigger>
            </AppTooltip>
            <DropdownMenuContent align='end' className='w-60' side='top'>
              {hasBaseMenuItems ? (
                <DropdownMenuGroup>
                  {/*
                  CDXC:ChatMenuGroupLabels 2026-08-26:
                  The dots menu already names its host-action block "Agent"; the
                  rows above it are chat-surface toggles, so they get the
                  matching "Chat" heading instead of reading as an unlabeled
                  preamble.
                  */}
                  <DropdownMenuLabel>Chat</DropdownMenuLabel>
                  {verboseMenuItem}
                  {summaryMenuItem}
                  {delayedActionsMenuItem}
                  {closeAfterDoneMenuItem}
                </DropdownMenuGroup>
              ) : null}
              {hostMenuSections(hasBaseMenuItems)}
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null}
        {onSessionNote ? (
          <AppTooltip content={withShortcut('Session note', hostActions?.sessionNoteShortcut)}>
            <span className='ghostex-chat-session-note-control relative inline-flex'>
              <Button
                aria-label='Session note'
                aria-pressed={sessionNoteActive}
                className={cn(
                  'ghostex-chat-footer-control rounded-full',
                  sessionNoteActive ? 'text-foreground' : undefined
                )}
                onClick={onSessionNote}
                size='icon-sm'
                variant={sessionNoteActive ? 'secondary' : 'ghost'}
              >
                <IconNote aria-hidden='true' stroke={2} />
              </Button>
              {sessionNoteHasText ? (
                <span aria-hidden='true' className='ghostex-chat-session-note-presence-dot'>
                  1
                </span>
              ) : null}
            </span>
          </AppTooltip>
        ) : null}
        {onStash ? (
          <AppTooltip content={stashTooltip}>
            <span className='ghostex-chat-stash-control relative inline-flex'>
              <Button
                aria-label={stashLabel}
                className='ghostex-chat-footer-control rounded-full'
                disabled={hasSendableDraft ? disabled : onShowStashedPrompts === undefined}
                onClick={stashCurrentPrompt}
                onContextMenu={openSavedPromptsFromContextMenu}
                size='icon-sm'
                variant='ghost'
              >
                <IconStackPush aria-hidden='true' stroke={2} />
              </Button>
              {stashCountBadge}
            </span>
          </AppTooltip>
        ) : null}
        {onAttach ? (
          <AppTooltip content={withShortcut('Attach a file or folder', hostActionShortcut('attachPath'))}>
            <span className='inline-flex'>
              <Button
                aria-label='Attach a file or folder'
                className='ghostex-chat-footer-control rounded-full'
                disabled={disabled}
                onClick={onAttach}
                size='icon-sm'
                variant='ghost'
              >
                <IconPaperclip aria-hidden='true' stroke={2} />
              </Button>
            </span>
          </AppTooltip>
        ) : null}
        <AppTooltip content={maximizeLabel}>
          <span className='inline-flex'>
            <Button
              aria-label={maximizeLabel}
              aria-pressed={maximized}
              className='ghostex-chat-footer-control rounded-full'
              onClick={onToggleMaximized}
              size='icon-sm'
              variant='ghost'
            >
              {maximized ? (
                <IconMinimize aria-hidden='true' stroke={2} />
              ) : (
                <IconMaximize aria-hidden='true' stroke={2} />
              )}
            </Button>
          </span>
        </AppTooltip>
        {/* Last in the cluster so it sits directly beside Send/Stop. */}
        {switchViewButton}
      </div>

      <div className='ghostex-chat-composer-footer-actions-compact items-center gap-1.5'>
        <DropdownMenu>
          <AppTooltip content={withShortcut('More actions', hostActions?.moreActionsShortcut)}>
            <DropdownMenuTrigger
              render={
                <Button
                  aria-label='More actions'
                  className='ghostex-chat-footer-control rounded-full'
                  size='icon-sm'
                  variant='ghost'
                />
              }
            >
              <IconDots aria-hidden='true' stroke={2.2} />
            </DropdownMenuTrigger>
          </AppTooltip>
          <DropdownMenuContent align='end' className='w-60' side='top'>
            <DropdownMenuGroup>
              <DropdownMenuLabel>Chat</DropdownMenuLabel>
              {verboseMenuItem}
              {summaryMenuItem}
              {delayedActionsMenuItem}
              {onSessionNote ? (
                <DropdownMenuCheckboxItem
                  checked={sessionNoteActive}
                  onCheckedChange={(checked: boolean) => {
                    if (checked !== sessionNoteActive) {
                      onSessionNote();
                    }
                  }}
                >
                  <span className='ghostex-chat-composer-menu-indicator relative inline-flex'>
                    <IconNote aria-hidden='true' />
                    {sessionNoteHasText ? (
                      <span aria-hidden='true' className='ghostex-chat-session-note-presence-dot'>
                        1
                      </span>
                    ) : null}
                  </span>
                  Session note
                  {hostActions?.sessionNoteShortcut ? (
                    <DropdownMenuShortcut>{hostActions.sessionNoteShortcut}</DropdownMenuShortcut>
                  ) : null}
                </DropdownMenuCheckboxItem>
              ) : null}
              {onStash ? (
                <AppTooltip content={stashTooltip} side='left'>
                  <DropdownMenuItem
                    aria-label={stashLabel}
                    disabled={hasSendableDraft ? disabled : onShowStashedPrompts === undefined}
                    onClick={stashCurrentPrompt}
                    onContextMenu={openSavedPromptsFromContextMenu}
                  >
                    <span className='ghostex-chat-composer-menu-indicator relative inline-flex'>
                      <IconStackPush aria-hidden='true' />
                      {stashCountBadge}
                    </span>
                    {stashLabel}
                  </DropdownMenuItem>
                </AppTooltip>
              ) : null}
              {onAttach ? (
                <DropdownMenuItem disabled={disabled} onClick={onAttach}>
                  <IconPaperclip aria-hidden='true' />
                  Attach a file or folder
                </DropdownMenuItem>
              ) : null}
              <DropdownMenuItem onClick={onToggleMaximized}>
                {maximized ? <IconMinimize aria-hidden='true' /> : <IconMaximize aria-hidden='true' />}
                {maximizeLabel}
              </DropdownMenuItem>
            </DropdownMenuGroup>
            {hostMenuSections(true)}
          </DropdownMenuContent>
        </DropdownMenu>
        {/* The surface toggle stays a button at every width: it is the one
            control users flip constantly, and burying it costs two clicks. */}
        {switchViewButton}
      </div>
    </>
  );
}
