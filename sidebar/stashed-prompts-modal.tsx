import {
  IconCopy,
  IconFolder,
  IconInfoCircle,
  IconPencil,
  IconPlus,
  IconTrash,
} from '@tabler/icons-react';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '../components/ui/command';
import type { GxserverStashedPrompt } from '../shared/gxserver-protocol';
import { trimPromptEditorTrailingSpaces } from '../shared/prompt-editor-text';
import type { ExtensionToSidebarMessage } from '../shared/session-grid-contract';
import {
  normalizeDiscoveredProjectIconDataUrl,
  normalizeWorkspaceProjectIcon,
  resolveWorkspaceProjectIconDataUrl,
} from '../shared/workspace-project-appearance';
import { AppTooltip, TooltipProvider } from './app-tooltip';
import { SidebarCommandIconGlyph } from './sidebar-command-icon';
import { formatRelativeTime } from './relative-time';
import { QuickAccessHeader } from './quick-access-tabs';
import { TOOLTIP_DELAY_MS } from './tooltip-delay';
import type { WebviewApi } from './webview-api';

export type StashedPromptsModalProps = {
  isOpen: boolean;
  onClose: () => void;
  projectId?: string;
  sessionId?: string;
  stashHintTooltipDefaultOpen?: boolean;
  vscode: WebviewApi;
};

const TOOLTIP_LINE_COUNT = 30;
const STASH_PROMPT_HINT = "Press Option + S while you're using an agent to stash your prompt (Local only for now)";

type StashedPromptDayGroup = {
  dayLabel: string;
  prompts: GxserverStashedPrompt[];
};

/*
 * CDXC:StashedPrompts 2026-07-29:
 * Search matches on whitespace-collapsed prompt text plus the project name so
 * a query typed with single spaces still finds prompts whose original body
 * uses line breaks or indentation.
 */
function stashedPromptSearchText(prompt: GxserverStashedPrompt): string {
  return `${prompt.content} ${prompt.projectName ?? ''}`.toLowerCase().replace(/\s+/g, ' ').trim();
}

function stashedPromptTitle(prompt: GxserverStashedPrompt): string {
  return prompt.content.replace(/\s+/g, ' ').trim() || 'Untitled saved prompt';
}

function relativeTimeLabel(isoDate: string): string {
  const { suffix, value } = formatRelativeTime(isoDate, { allowJustNow: true });
  return suffix ? `${value} ${suffix}` : value;
}

function parseStashedPromptUpdatedAt(prompt: GxserverStashedPrompt): number {
  const timestamp = Date.parse(prompt.updatedAt);
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function groupStashedPromptsByDay(prompts: readonly GxserverStashedPrompt[]): StashedPromptDayGroup[] {
  const formatter = new Intl.DateTimeFormat(undefined, {
    day: 'numeric',
    month: 'long',
    weekday: 'long',
    year: 'numeric',
  });
  const promptsByDay = new Map<string, GxserverStashedPrompt[]>();
  const sortedPrompts = [...prompts].sort(
    (left, right) =>
      parseStashedPromptUpdatedAt(right) - parseStashedPromptUpdatedAt(left) ||
      left.promptId.localeCompare(right.promptId)
  );
  for (const prompt of sortedPrompts) {
    const timestamp = parseStashedPromptUpdatedAt(prompt);
    const dayLabel = timestamp === 0 ? 'Earlier' : formatter.format(new Date(timestamp));
    const grouped = promptsByDay.get(dayLabel);
    if (grouped) {
      grouped.push(prompt);
    } else {
      promptsByDay.set(dayLabel, [prompt]);
    }
  }
  return [...promptsByDay.entries()].map(([dayLabel, dayPrompts]) => ({
    dayLabel,
    prompts: dayPrompts,
  }));
}

export function StashedPromptsModal({
  isOpen,
  onClose,
  projectId,
  sessionId,
  stashHintTooltipDefaultOpen = false,
  vscode,
}: StashedPromptsModalProps) {
  const [prompts, setPrompts] = useState<GxserverStashedPrompt[]>();
  const [searchQuery, setSearchQuery] = useState('');
  const [isAddingPrompt, setIsAddingPrompt] = useState(false);
  const [editingPromptId, setEditingPromptId] = useState<string>();
  const [draftContent, setDraftContent] = useState('');
  const [isSavingPrompt, setIsSavingPrompt] = useState(false);
  const [saveError, setSaveError] = useState<string>();
  const [selectedPromptValue, setSelectedPromptValue] = useState('');
  const latestRequestIdRef = useRef<string | undefined>(undefined);
  const latestSaveRequestIdRef = useRef<string | undefined>(undefined);
  const requestCounterRef = useRef(0);
  const draftTextareaRef = useRef<HTMLTextAreaElement>(null);
  const promptListRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) {
      setPrompts(undefined);
      setSearchQuery('');
      setIsAddingPrompt(false);
      setEditingPromptId(undefined);
      setDraftContent('');
      setIsSavingPrompt(false);
      setSaveError(undefined);
      latestRequestIdRef.current = undefined;
      latestSaveRequestIdRef.current = undefined;
    }
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const handleMessage = (event: MessageEvent<ExtensionToSidebarMessage>) => {
      if (event.data?.type === 'saveStashedPromptResult') {
        if (event.data.requestId !== latestSaveRequestIdRef.current) {
          return;
        }
        setIsSavingPrompt(false);
        if (!event.data.ok || !event.data.prompt) {
          setSaveError(event.data.error ?? 'Could not save this prompt.');
          return;
        }
        const savedPrompt = event.data.prompt;
        setPrompts((current) => [
          savedPrompt,
          ...(current ?? []).filter((prompt) => prompt.promptId !== savedPrompt.promptId),
        ]);
        setDraftContent('');
        setSearchQuery('');
        setSaveError(undefined);
        setIsAddingPrompt(false);
        setEditingPromptId(undefined);
        latestSaveRequestIdRef.current = undefined;
        return;
      }
      if (event.data?.type !== 'stashedPromptsResult') {
        return;
      }
      if (event.data.requestId !== latestRequestIdRef.current) {
        return;
      }
      setPrompts(event.data.prompts);
    };
    window.addEventListener('message', handleMessage);
    return () => {
      window.removeEventListener('message', handleMessage);
    };
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen || !isAddingPrompt) {
      return;
    }
    const timeoutId = window.setTimeout(() => {
      draftTextareaRef.current?.focus();
    }, 0);
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      if (isSavingPrompt) {
        return;
      }
      setIsAddingPrompt(false);
      setEditingPromptId(undefined);
      setDraftContent('');
      setSaveError(undefined);
    };
    document.addEventListener('keydown', handleKeyDown, true);
    return () => {
      window.clearTimeout(timeoutId);
      document.removeEventListener('keydown', handleKeyDown, true);
    };
  }, [isAddingPrompt, isOpen, isSavingPrompt]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    requestCounterRef.current += 1;
    const requestId = `stashed-prompts-${Date.now()}-${requestCounterRef.current}`;
    latestRequestIdRef.current = requestId;
    setPrompts(undefined);
    vscode.postMessage({
      requestId,
      type: 'requestStashedPrompts',
    });
  }, [isOpen, vscode]);

  const visiblePrompts = useMemo(() => {
    if (!prompts) {
      return [];
    }
    const query = searchQuery.toLowerCase().replace(/\s+/g, ' ').trim();
    if (!query) {
      return prompts;
    }
    return prompts.filter((prompt) => stashedPromptSearchText(prompt).includes(query));
  }, [prompts, searchQuery]);
  const groupedVisiblePrompts = useMemo(() => groupStashedPromptsByDay(visiblePrompts), [visiblePrompts]);
  const normalizedSearchQuery = searchQuery.toLowerCase().replace(/\s+/g, ' ').trim();
  const showAddPrompt =
    normalizedSearchQuery.length === 0 || 'add saved prompt new prompt'.includes(normalizedSearchQuery);
  const topPromptValue = showAddPrompt ? 'add saved prompt new prompt' : visiblePrompts[0]?.promptId ?? '';

  useLayoutEffect(() => {
    if (!isOpen || isAddingPrompt) {
      return;
    }
    setSelectedPromptValue(topPromptValue);
    if (promptListRef.current) {
      promptListRef.current.scrollTop = 0;
    }
  }, [isAddingPrompt, isOpen, searchQuery, topPromptValue]);

  const insertPrompt = (prompt: GxserverStashedPrompt) => {
    vscode.postMessage({
      content: prompt.content,
      promptId: prompt.promptId,
      ...(sessionId ? { sessionId } : {}),
      type: 'insertStashedPrompt',
    });
    onClose();
  };

  const deletePrompt = (prompt: GxserverStashedPrompt) => {
    vscode.postMessage({ promptId: prompt.promptId, type: 'deleteStashedPrompt' });
    setPrompts((current) => current?.filter((candidate) => candidate.promptId !== prompt.promptId));
  };

  const savePrompt = () => {
    const content = trimPromptEditorTrailingSpaces(draftContent);
    if (!content.trim() || isSavingPrompt) {
      return;
    }
    requestCounterRef.current += 1;
    const requestId = `save-stashed-prompt-${Date.now()}-${requestCounterRef.current}`;
    latestSaveRequestIdRef.current = requestId;
    setIsSavingPrompt(true);
    setSaveError(undefined);
    vscode.postMessage({
      content,
      ...(editingPromptId ? { promptId: editingPromptId } : {}),
      ...(projectId ? { projectId } : {}),
      requestId,
      ...(sessionId ? { sessionId } : {}),
      type: 'saveStashedPrompt',
    });
  };

  return (
    <CommandDialog
      className='ghostex-settings-shadcn ghostex-command-palette-dialog ghostex-stashed-prompts-dialog top-1/2 -translate-y-1/2'
      description='Browse and add saved prompts.'
      open={isOpen}
      showCloseButton={false}
      title='Ghostex Quick Access'
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onClose();
        }
      }}
    >
      {/*
        CDXC:StashedPrompts 2026-07-29:
        Every prompt-editor save-and-close (Ctrl+G in a session, then Save)
        stashes the composed text in gxserver. This modal is the recall
        surface: the fourth Ghostex Quick Access tab, listing local prompts
        newest first. Selecting a row inserts the prompt into the launching
        session's active input surface without submitting it.
      */}
      <TooltipProvider delayDuration={TOOLTIP_DELAY_MS}>
        <Command
          className='quick-access-surface ghostex-stashed-prompts-command'
          shouldFilter={false}
          value={selectedPromptValue}
          onValueChange={setSelectedPromptValue}
        >
          <QuickAccessHeader activeTab='savedPrompts' />
          {isAddingPrompt ? (
            <div className='ghostex-stashed-prompt-editor'>
              <div className='ghostex-stashed-prompt-editor-heading'>
                {editingPromptId ? 'Edit Saved Prompt' : 'Add Saved Prompt'}
              </div>
              <textarea
                aria-label='Saved prompt content'
                className='ghostex-stashed-prompt-editor-textarea'
                disabled={isSavingPrompt}
                onChange={(event) => {
                  setDraftContent(event.target.value);
                }}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
                    event.preventDefault();
                    savePrompt();
                  }
                }}
                placeholder='Write a prompt you want to save...'
                ref={draftTextareaRef}
                spellCheck={false}
                value={draftContent}
              />
              {saveError ? (
                <div className='ghostex-stashed-prompt-editor-error' role='alert'>
                  {saveError}
                </div>
              ) : null}
              <div className='ghostex-stashed-prompt-editor-actions'>
                <button
                  className='ghostex-stashed-prompt-editor-button'
                  disabled={isSavingPrompt}
                  onClick={() => {
                    setIsAddingPrompt(false);
                    setEditingPromptId(undefined);
                    setDraftContent('');
                    setSaveError(undefined);
                  }}
                  type='button'
                >
                  Cancel
                </button>
                <button
                  className='ghostex-stashed-prompt-editor-button ghostex-stashed-prompt-editor-button-primary'
                  disabled={!draftContent.trim() || isSavingPrompt}
                  onClick={savePrompt}
                  type='button'
                >
                  {isSavingPrompt ? 'Saving...' : editingPromptId ? 'Save Changes' : 'Add Prompt'}
                </button>
              </div>
            </div>
          ) : (
            <>
              <CommandInput
                className='pl-3'
                clearOnEscape={false}
                clearLabel='Clear prompt search'
                onKeyDown={(event) => {
                  if (event.key !== 'Escape') {
                    return;
                  }
                  event.preventDefault();
                  event.stopPropagation();
                  onClose();
                }}
                placeholder='Search saved prompts...'
                value={searchQuery}
                onValueChange={setSearchQuery}
              />
              <CommandList
                className='ghostex-command-palette-list ghostex-stashed-prompts-list'
                ref={promptListRef}
              >
                {prompts !== undefined && !showAddPrompt && visiblePrompts.length === 0 ? (
                  <CommandEmpty>No saved prompts match this search.</CommandEmpty>
                ) : null}
                {showAddPrompt || visiblePrompts.length > 0 ? (
                  <CommandGroup>
                    {showAddPrompt ? (
                      <CommandItem
                        onSelect={() => {
                          setIsAddingPrompt(true);
                        }}
                        value='add saved prompt new prompt'
                      >
                        <IconPlus aria-hidden='true' />
                        <span className='ghostex-command-palette-copy'>
                          <span className='ghostex-command-palette-title'>Add Saved Prompt</span>
                        </span>
                      </CommandItem>
                    ) : null}
                    {prompts === undefined ? (
                      <div className='ghostex-stashed-prompts-empty'>Loading saved prompts…</div>
                    ) : (
                      groupedVisiblePrompts.map((group) => (
                        <section className='previous-sessions-day-group' key={group.dayLabel}>
                          <div className='previous-sessions-day-label'>{group.dayLabel}</div>
                          <div className='ghostex-stashed-prompt-day-list'>
                            {group.prompts.map((prompt) => (
                              <StashedPromptRow
                                key={prompt.promptId}
                                onDelete={() => {
                                  deletePrompt(prompt);
                                }}
                                onEdit={() => {
                                  setEditingPromptId(prompt.promptId);
                                  setDraftContent(prompt.content);
                                  setSaveError(undefined);
                                  setIsAddingPrompt(true);
                                }}
                                onSelect={() => {
                                  insertPrompt(prompt);
                                }}
                                prompt={prompt}
                              />
                            ))}
                          </div>
                        </section>
                      ))
                    )}
                  </CommandGroup>
                ) : null}
              </CommandList>
              <AppTooltip
                content={STASH_PROMPT_HINT}
                contentClassName='ghostex-stashed-prompts-stash-hint-tooltip'
                contentStyle={{ fontSize: '17px', lineHeight: '24px' }}
                defaultOpen={stashHintTooltipDefaultOpen}
                side='left'
                sideOffset={8}
              >
                <button aria-label={STASH_PROMPT_HINT} className='ghostex-stashed-prompts-stash-hint' type='button'>
                  <IconInfoCircle aria-hidden='true' size={16} stroke={1.8} />
                </button>
              </AppTooltip>
            </>
          )}
        </Command>
      </TooltipProvider>
    </CommandDialog>
  );
}

type StashedPromptRowProps = {
  onDelete: () => void;
  onEdit: () => void;
  onSelect: () => void;
  prompt: GxserverStashedPrompt;
};

/**
 * CDXC:StashedPrompts 2026-07-29:
 * Saved Prompt rows show the origin project with the sidebar's icon priority:
 * a user-selected image, the repository's discovered icon, a typed glyph,
 * then a folder fallback.
 */
function StashedPromptProjectIcon({ prompt }: { prompt: GxserverStashedPrompt }) {
  const iconSource = {
    icon: normalizeWorkspaceProjectIcon(prompt.projectIcon),
    iconDataUrl: prompt.projectIconDataUrl ?? undefined,
  };
  const iconDataUrl = resolveWorkspaceProjectIconDataUrl(iconSource);
  if (iconDataUrl) {
    return <img alt='' className='ghostex-stashed-prompt-project-icon-image' draggable={false} src={iconDataUrl} />;
  }
  const discoveredIconDataUrl = normalizeDiscoveredProjectIconDataUrl(prompt.projectDiscoveredIconDataUrl);
  if (discoveredIconDataUrl) {
    return (
      <img alt='' className='ghostex-stashed-prompt-project-icon-image' draggable={false} src={discoveredIconDataUrl} />
    );
  }
  if (iconSource.icon?.kind === 'tabler') {
    return <SidebarCommandIconGlyph color={iconSource.icon.color} icon={iconSource.icon.icon} size={13} stroke={1.8} />;
  }
  return <IconFolder aria-hidden='true' size={13} stroke={1.8} />;
}

function StashedPromptRow({ onDelete, onEdit, onSelect, prompt }: StashedPromptRowProps) {
  const lines = prompt.content.trim().split('\n');
  const tooltipLines = lines.slice(0, TOOLTIP_LINE_COUNT);
  const tooltipTruncated = lines.length > TOOLTIP_LINE_COUNT;

  return (
    <CommandItem className='ghostex-stashed-prompt-item' onSelect={onSelect} value={prompt.promptId}>
      <span className='ghostex-stashed-prompt-content'>
        <span className='ghostex-stashed-prompt-top-line'>
          <AppTooltip
            align='start'
            content={
              <div className='ghostex-stashed-prompt-tooltip-body'>
                {tooltipLines.join('\n')}
                {tooltipTruncated ? '\n…' : ''}
              </div>
            }
            contentStyle={{ width: 'min(560px, calc(100vw - 32px))' }}
            side='bottom'
            sideOffset={4}
          >
            <span className='ghostex-command-palette-copy'>
              <span className='ghostex-command-palette-title'>{stashedPromptTitle(prompt)}</span>
            </span>
          </AppTooltip>
          <span className='ghostex-stashed-prompt-actions'>
            <button
              aria-label='Copy prompt'
              className='ghostex-stashed-prompt-action copy-cursor'
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                void navigator.clipboard.writeText(prompt.content);
              }}
              type='button'
            >
              <IconCopy aria-hidden='true' size={14} stroke={1.9} />
            </button>
            <button
              aria-label='Edit prompt'
              className='ghostex-stashed-prompt-action'
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onEdit();
              }}
              type='button'
            >
              <IconPencil aria-hidden='true' size={14} stroke={1.9} />
            </button>
            <button
              aria-label='Delete prompt'
              className='ghostex-stashed-prompt-action'
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onDelete();
              }}
              type='button'
            >
              <IconTrash aria-hidden='true' size={14} stroke={1.9} />
            </button>
          </span>
        </span>
        <span className='ghostex-stashed-prompt-row-meta'>
          <span className='ghostex-stashed-prompt-project'>
            <span aria-hidden='true' className='ghostex-stashed-prompt-project-icon'>
              <StashedPromptProjectIcon prompt={prompt} />
            </span>
            <span className='ghostex-stashed-prompt-project-name'>{prompt.projectName ?? 'No project'}</span>
          </span>
          <span className='ghostex-stashed-prompt-time'>{relativeTimeLabel(prompt.updatedAt)}</span>
        </span>
      </span>
    </CommandItem>
  );
}
