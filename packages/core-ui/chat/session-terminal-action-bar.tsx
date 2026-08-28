// The terminal surface's bottom action bar: the chat composer's footer row,
// replayed below a terminal in real layout space with no composer box around
// it. Design reference:
// docs/2026-08-25/chat-terminal-buttons/option-1-mockup.html — the
// "Terminal view — bare bottom bar" frame.
//
// It is a bar, not an overlay: hosts mount it as a sibling below the terminal
// so the terminal never renders underneath it. Geometry mirrors the chat
// composer's wrapper (`mx-auto w-full max-w-3xl … px-4 pb-3` in
// session-chat-view.tsx) plus the composer's own `border + px-4`, so the view
// toggle lands on the same x as the chat footer's toggle at every pane width.
//
// Everything but the ⋯ menu and the view toggle is driven by the host's action
// list, so a host that cannot run an action simply omits it: on the web app
// only ⋯ and Chat View render.

import {
  IconClockCheck,
  IconDots,
  IconEdit,
  IconFileExport,
  IconGitBranch,
  IconMessage,
  IconPaperclip,
  IconPencil,
  IconRefresh,
  IconStackPush,
  type Icon as TablerIcon,
} from '@tabler/icons-react';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Button } from '../../components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from '../../components/ui/dropdown-menu';
import { AppTooltip } from '../app-tooltip';
import type { SessionChatHostAction, SessionChatHostActions } from './session-chat-host-actions';

/**
 * Host actions the bar renders as their own control, so the dots menu must not
 * offer them a second time. `stashPrompt` is in the set without a control of
 * its own: there is no draft on a terminal surface, so stashing one is not an
 * action this bar can offer at all.
 */
const BAR_OWNED_HOST_ACTION_IDS = new Set(['attachPath', 'promptEditor', 'stashPrompt', 'stashedPrompts']);

/** Per-session lifecycle actions, shown under the menu's "Agent" heading. */
const AGENT_HOST_ACTION_IDS = new Set(['fork', 'fullReload', 'rename', 'sleep']);

const HOST_ACTION_ICONS: Record<string, TablerIcon> = {
  delayedActions: IconClockCheck,
  exportTranscript: IconFileExport,
  fork: IconGitBranch,
  fullReload: IconRefresh,
  rename: IconPencil,
};

// Copied verbatim from apps/desktop/assets/titlebar/moon.svg (the same glyph the
// gpui titlebar and the chat composer's dots menu use for Sleep).
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

function withShortcut(label: string, shortcut?: string): string {
  return shortcut ? `${label} (${shortcut})` : label;
}

interface SessionTerminalActionBarProps {
  hostActions: SessionChatHostActions;
  /**
   * The detected agent session id, already shortened by the host — the bar
   * renders it as given because only the host knows what its ids look like.
   */
  sessionId?: string;
  /** Badge on the stashed-prompts control; 0 hides the badge. */
  stashedPromptCount?: number;
}

export function SessionTerminalActionBar({
  hostActions,
  sessionId,
  stashedPromptCount = 0,
}: SessionTerminalActionBarProps) {
  /*
  Host actions that carry `input` (Rename) swap the bar's control cluster for
  an inline field, the way the chat composer's dots menu does. The field takes
  focus from an effect rather than `autoFocus`: picking the action unmounts the
  dots menu together with the cluster it lives in, and the effect runs after
  that removal, so the menu's own focus handling cannot pull the caret back out.
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
    hostActions.onAction?.(inputAction.id, inputValue);
    setInputAction(null);
  };

  const hostActionList = hostActions.actions ?? [];
  const runHostAction = (action: SessionChatHostAction) => {
    if (action.input) {
      setInputValue(action.input.initialValue ?? '');
      setInputAction(action);
      return;
    }
    hostActions.onAction?.(action.id);
  };
  const hostActionMenuItem = (action: SessionChatHostAction) => (
    <DropdownMenuItem key={action.id} onClick={() => runHostAction(action)}>
      {hostActionIcon(action.id)}
      {action.label}
      {action.shortcut ? <DropdownMenuShortcut>{action.shortcut}</DropdownMenuShortcut> : null}
    </DropdownMenuItem>
  );
  const findAction = (id: string) => hostActionList.find((action) => action.id === id);
  const promptEditorAction = findAction('promptEditor');
  const stashedPromptsAction = findAction('stashedPrompts');
  const attachAction = findAction('attachPath');

  const delayedHostAction = findAction('delayedActions');
  const foldedHostActions = hostActionList.filter(
    (action) => action.id !== 'delayedActions' && !BAR_OWNED_HOST_ACTION_IDS.has(action.id)
  );
  const agentHostActions = foldedHostActions.filter((action) => AGENT_HOST_ACTION_IDS.has(action.id));
  const otherHostActions = foldedHostActions.filter((action) => !AGENT_HOST_ACTION_IDS.has(action.id));
  const hasMenuItems = delayedHostAction !== undefined || foldedHostActions.length > 0;

  return (
    <div className='ghostex-terminal-action-bar mx-auto mb-3 flex h-9 w-full max-w-3xl flex-none items-center gap-1.5'>
      {sessionId ? (
        <span className='min-w-0 truncate text-xs text-muted-foreground' title={sessionId}>
          {sessionId}
        </span>
      ) : null}
      {inputAction ? (
        <input
          aria-label={inputAction.label}
          className='ghostex-terminal-action-bar-input ml-auto h-8 max-w-full min-w-0 rounded-full border border-input bg-transparent px-3 text-xs text-foreground outline-none focus:border-ring'
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
      ) : (
        <div className='ml-auto flex items-center gap-1.5'>
          {hasMenuItems ? (
            <DropdownMenu>
              <AppTooltip content={withShortcut('More actions', hostActions.moreActionsShortcut)}>
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
              <DropdownMenuContent align='end' className='min-w-52' side='top'>
                {delayedHostAction ? (
                  <DropdownMenuGroup>{hostActionMenuItem(delayedHostAction)}</DropdownMenuGroup>
                ) : null}
                {agentHostActions.length > 0 ? (
                  <>
                    {delayedHostAction ? <DropdownMenuSeparator /> : null}
                    <DropdownMenuGroup>
                      <DropdownMenuLabel>Agent</DropdownMenuLabel>
                      {agentHostActions.map(hostActionMenuItem)}
                    </DropdownMenuGroup>
                  </>
                ) : null}
                {otherHostActions.length > 0 ? (
                  <>
                    {delayedHostAction || agentHostActions.length > 0 ? <DropdownMenuSeparator /> : null}
                    <DropdownMenuGroup>{otherHostActions.map(hostActionMenuItem)}</DropdownMenuGroup>
                  </>
                ) : null}
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null}
          {stashedPromptsAction ? (
            <AppTooltip
              content={withShortcut(
                stashedPromptCount > 0
                  ? `${stashedPromptsAction.label} (${stashedPromptCount})`
                  : stashedPromptsAction.label,
                stashedPromptsAction.shortcut
              )}
            >
              {/* ghostex-chat-stash-control sizes the stack-push glyph up to
                  match its neighbours, exactly as in the chat footer. */}
              <span className='ghostex-chat-stash-control relative inline-flex'>
                <Button
                  aria-label={stashedPromptsAction.label}
                  className='ghostex-chat-footer-control rounded-full'
                  onClick={() => runHostAction(stashedPromptsAction)}
                  size='icon-sm'
                  variant='ghost'
                >
                  <IconStackPush aria-hidden='true' stroke={2} />
                </Button>
                {stashedPromptCount > 0 ? (
                  <span aria-hidden='true' className='ghostex-chat-stash-count-badge'>
                    {Math.min(stashedPromptCount, 9)}
                  </span>
                ) : null}
              </span>
            </AppTooltip>
          ) : null}
          {attachAction ? (
            <AppTooltip content={withShortcut(attachAction.label, attachAction.shortcut)}>
              <span className='inline-flex'>
                <Button
                  aria-label={attachAction.label}
                  className='ghostex-chat-footer-control rounded-full'
                  onClick={() => runHostAction(attachAction)}
                  size='icon-sm'
                  variant='ghost'
                >
                  <IconPaperclip aria-hidden='true' stroke={2} />
                </Button>
              </span>
            </AppTooltip>
          ) : null}
          <AppTooltip content={withShortcut('Chat View', hostActions.switchViewShortcut)}>
            <span className='inline-flex'>
              <Button
                aria-label='Chat View'
                className='ghostex-chat-footer-control rounded-full'
                onClick={hostActions.onSwitchToTerminal}
                size='icon-sm'
                variant='ghost'
              >
                <IconMessage aria-hidden='true' stroke={2} />
              </Button>
            </span>
          </AppTooltip>
          {/* Send's slot in the chat footer, filled here by Send's semantic
              twin: the Prompt Editor also injects input into the session, so a
              misclick from chat muscle memory is harmless — and the view toggle
              keeps the exact x it has in chat. */}
          {promptEditorAction ? (
            <AppTooltip content={withShortcut(promptEditorAction.label, promptEditorAction.shortcut)}>
              <span className='inline-flex'>
                <Button
                  aria-label={promptEditorAction.label}
                  className='ghostex-chat-send-button ghostex-terminal-action-bar-accent size-6'
                  onClick={() => runHostAction(promptEditorAction)}
                  size='icon'
                >
                  <IconEdit aria-hidden='true' className='size-[13.5px]' stroke={2} />
                </Button>
              </span>
            </AppTooltip>
          ) : null}
        </div>
      )}
    </div>
  );
}
