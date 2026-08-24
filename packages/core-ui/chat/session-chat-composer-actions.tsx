import {
  IconClockCheck,
  IconDots,
  IconEyeFilled,
  IconEyeOff,
  IconMaximize,
  IconMinimize,
  IconNote,
  IconPaperclip,
  IconStackPush,
} from '@tabler/icons-react';
import { Button } from '../../components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../../components/ui/dropdown-menu';
import { cn } from '@/packages/components/utils';
import { AppTooltip } from '../app-tooltip';

interface SessionChatComposerActionsProps {
  disabled: boolean;
  hasSendableDraft: boolean;
  maximized: boolean;
  onAttach?: () => void;
  onDelayedActions?: () => void;
  onSessionNote?: () => void;
  onShowStashedPrompts?: () => void;
  onStash?: () => void;
  onToggleMaximized: () => void;
  onToggleVerbose?: () => void;
  sessionNoteActive: boolean;
  stashedPromptCount: number;
  verboseMode: boolean;
}

export function SessionChatComposerActions({
  disabled,
  hasSendableDraft,
  maximized,
  onAttach,
  onDelayedActions,
  onSessionNote,
  onShowStashedPrompts,
  onStash,
  onToggleMaximized,
  onToggleVerbose,
  sessionNoteActive,
  stashedPromptCount,
  verboseMode,
}: SessionChatComposerActionsProps) {
  const stashOpensSavedPrompts = !hasSendableDraft && onShowStashedPrompts !== undefined;
  const stashLabel = stashOpensSavedPrompts
    ? `View stashed prompts${stashedPromptCount > 0 ? ` (${stashedPromptCount})` : ''}`
    : 'Stash prompt';
  const maximizeLabel = maximized ? 'Exit maximize' : 'Maximize';
  const verboseLabel = verboseMode ? 'Verbose mode on' : 'Verbose mode off';
  const VerboseIcon = verboseMode ? IconEyeFilled : IconEyeOff;

  return (
    <>
      <div className='ghostex-chat-composer-footer-actions-expanded items-center gap-1.5'>
        {onToggleVerbose ? (
          <AppTooltip content={verboseLabel}>
            <span className='ghostex-chat-verbose-wrapper inline-flex'>
              <Button
                aria-label={verboseLabel}
                aria-pressed={verboseMode}
                className={cn(
                  'ghostex-chat-footer-control ghostex-chat-verbose-control rounded-full',
                  verboseMode ? 'text-foreground' : 'text-muted-foreground'
                )}
                onClick={onToggleVerbose}
                size='icon-sm'
                variant={verboseMode ? 'secondary' : 'ghost'}
              >
                <VerboseIcon aria-hidden='true' stroke={2} />
              </Button>
            </span>
          </AppTooltip>
        ) : null}
        {onDelayedActions ? (
          <AppTooltip content='Delayed actions'>
            <span className='ghostex-chat-delayed-actions-control inline-flex'>
              <Button
                aria-label='Delayed actions'
                className='ghostex-chat-footer-control rounded-full'
                disabled={disabled}
                onClick={onDelayedActions}
                size='icon-sm'
                variant='ghost'
              >
                <IconClockCheck aria-hidden='true' stroke={2} />
              </Button>
            </span>
          </AppTooltip>
        ) : null}
        {onSessionNote ? (
          <AppTooltip content='Session note'>
            <span className='ghostex-chat-session-note-control inline-flex'>
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
            </span>
          </AppTooltip>
        ) : null}
        {onStash ? (
          <AppTooltip content={stashLabel}>
            <span className='ghostex-chat-stash-control relative inline-flex'>
              <Button
                aria-label={stashLabel}
                className='ghostex-chat-footer-control rounded-full'
                disabled={disabled || (!hasSendableDraft && onShowStashedPrompts === undefined)}
                onClick={stashOpensSavedPrompts ? onShowStashedPrompts : onStash}
                size='icon-sm'
                variant='ghost'
              >
                <IconStackPush aria-hidden='true' stroke={2} />
              </Button>
              {stashedPromptCount > 0 ? (
                <span aria-hidden='true' className='ghostex-chat-stash-count-badge'>
                  {stashedPromptCount > 99 ? '99+' : stashedPromptCount}
                </span>
              ) : null}
            </span>
          </AppTooltip>
        ) : null}
        {onAttach ? (
          <AppTooltip content='Attach an Image, File, or Folder'>
            <span className='inline-flex'>
              <Button
                aria-label='Attach an Image, File, or Folder'
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
      </div>

      <div className='ghostex-chat-composer-footer-actions-compact items-center'>
        <DropdownMenu>
          <AppTooltip content='Extra actions'>
            <DropdownMenuTrigger
              render={
                <Button
                  aria-label='Extra actions'
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
            <DropdownMenuGroup>
              {onToggleVerbose ? (
                <DropdownMenuCheckboxItem
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
              ) : null}
              {onDelayedActions ? (
                <DropdownMenuItem disabled={disabled} onClick={onDelayedActions}>
                  <IconClockCheck aria-hidden='true' />
                  Delayed actions
                </DropdownMenuItem>
              ) : null}
              {onSessionNote ? (
                <DropdownMenuCheckboxItem
                  checked={sessionNoteActive}
                  onCheckedChange={(checked: boolean) => {
                    if (checked !== sessionNoteActive) {
                      onSessionNote();
                    }
                  }}
                >
                  <IconNote aria-hidden='true' />
                  Session note
                </DropdownMenuCheckboxItem>
              ) : null}
              {onStash ? (
                <DropdownMenuItem
                  disabled={disabled || (!hasSendableDraft && onShowStashedPrompts === undefined)}
                  onClick={stashOpensSavedPrompts ? onShowStashedPrompts : onStash}
                >
                  <IconStackPush aria-hidden='true' />
                  {stashLabel}
                </DropdownMenuItem>
              ) : null}
              {onAttach ? (
                <DropdownMenuItem disabled={disabled} onClick={onAttach}>
                  <IconPaperclip aria-hidden='true' />
                  Attach image, file, or folder
                </DropdownMenuItem>
              ) : null}
              <DropdownMenuItem onClick={onToggleMaximized}>
                {maximized ? <IconMinimize aria-hidden='true' /> : <IconMaximize aria-hidden='true' />}
                {maximizeLabel}
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </>
  );
}
