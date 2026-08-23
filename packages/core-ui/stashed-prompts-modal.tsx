import {
  IconCheck,
  IconCopy,
  IconFolder,
  IconInfoCircle,
  IconPencil,
  IconPlus,
  IconStar,
  IconStarFilled,
  IconTag,
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
import { Popover, PopoverContent, PopoverTrigger } from '../components/ui/popover';
import type {
  GxserverStashedPrompt,
  GxserverStashedPromptTag,
} from '../shared/gxserver-protocol';
import { GXSERVER_FAVORITE_PROMPT_TAG_ID } from '../shared/gxserver-protocol';
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

/*
 * CDXC:StashedPromptTags 2026-08-23:
 * New tags pick their color from this palette rather than a color input: eight
 * hues that stay legible as a 7px dot, an 18px chip, and a 3px row stripe on
 * the modal's background, which a free-form picker cannot guarantee.
 */
const STASHED_PROMPT_TAG_COLORS = [
  '#e3b341',
  '#7f9cf5',
  '#86d1a4',
  '#e3796b',
  '#c99bdd',
  '#7ec7f5',
  '#e0a3c8',
  '#9aa4b2',
] as const;

const MAX_TAG_NAME_LENGTH = 40;

/*
 * CDXC:StashedPromptTags 2026-08-23:
 * The rail filters on three distinct things, so it is a union rather than a
 * nullable tagId: "untagged" is a real selection, not the absence of one, and a
 * sentinel string mixed into the tagId space could one day collide with a tag
 * the daemon mints.
 */
type StashedPromptTagFilter =
  | { kind: 'all' }
  | { kind: 'tag'; tagId: string }
  | { kind: 'untagged' };

const ALL_PROMPTS_FILTER: StashedPromptTagFilter = { kind: 'all' };

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

function promptTagIds(prompt: GxserverStashedPrompt): readonly string[] {
  return prompt.tagIds ?? [];
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
  const [tags, setTags] = useState<GxserverStashedPromptTag[]>([]);
  const [tagFilter, setTagFilter] = useState<StashedPromptTagFilter>(ALL_PROMPTS_FILTER);
  const [tagMenuPromptId, setTagMenuPromptId] = useState<string>();
  const [isCreatingTag, setIsCreatingTag] = useState(false);
  const [createTagName, setCreateTagName] = useState('');
  const [createTagColor, setCreateTagColor] = useState<string>(STASHED_PROMPT_TAG_COLORS[1]);
  const [tagError, setTagError] = useState<string>();
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
  /*
   * CDXC:StashedPromptTags 2026-08-23:
   * The tag menu that opened the create form: 'row' applies the new tag to that
   * prompt on creation, 'rail' switches the filter to it instead.
   */
  const createTagOriginRef = useRef<'rail' | 'row'>('rail');
  const createTagPromptIdRef = useRef<string | undefined>(undefined);
  /*
   * The daemon owns tag ids, so a tag created from a row's menu cannot be
   * applied in the same message. Remember what to file once the refreshed
   * catalogue comes back naming it.
   */
  const pendingTagApplicationRef = useRef<{ name: string; promptId: string | undefined }>(undefined);

  useEffect(() => {
    if (!isOpen) {
      setPrompts(undefined);
      setTags([]);
      setTagFilter(ALL_PROMPTS_FILTER);
      setTagMenuPromptId(undefined);
      setIsCreatingTag(false);
      setCreateTagName('');
      setTagError(undefined);
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
      /*
       * CDXC:StashedPromptTags 2026-08-23:
       * Tag mutations answer with the whole refreshed catalogue. A delete also
       * names the tag it removed so the rows this modal is still holding drop
       * that assignment without a second round trip for the prompt list.
       */
      if (event.data?.type === 'stashedPromptTagsResult') {
        if (!event.data.ok) {
          setTagError(event.data.error ?? 'Could not update tags.');
          return;
        }
        setTagError(undefined);
        setTags(event.data.tags);
        const deletedTagId = event.data.deletedTagId;
        if (deletedTagId) {
          setPrompts((current) =>
            current?.map((prompt) =>
              promptTagIds(prompt).includes(deletedTagId)
                ? { ...prompt, tagIds: promptTagIds(prompt).filter((tagId) => tagId !== deletedTagId) }
                : prompt
            )
          );
          setTagFilter((current) =>
            current.kind === 'tag' && current.tagId === deletedTagId ? ALL_PROMPTS_FILTER : current
          );
        }
        return;
      }
      if (event.data?.type === 'setStashedPromptTagsResult') {
        if (!event.data.ok || !event.data.prompt) {
          setTagError(event.data.error ?? "Could not update this prompt's tags.");
          return;
        }
        setTagError(undefined);
        const taggedPrompt = event.data.prompt;
        setPrompts((current) =>
          current?.map((prompt) => (prompt.promptId === taggedPrompt.promptId ? taggedPrompt : prompt))
        );
        return;
      }
      if (event.data?.type !== 'stashedPromptsResult') {
        return;
      }
      if (event.data.requestId !== latestRequestIdRef.current) {
        return;
      }
      setPrompts(event.data.prompts);
      setTags(event.data.tags ?? []);
    };
    window.addEventListener('message', handleMessage);
    return () => {
      window.removeEventListener('message', handleMessage);
    };
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen || isAddingPrompt) {
      return;
    }
    const timeoutId = window.setTimeout(() => {
      document
        .querySelector<HTMLInputElement>(
          '.ghostex-stashed-prompts-dialog [data-slot="command-input"]'
        )
        ?.focus();
    }, 0);
    return () => window.clearTimeout(timeoutId);
  }, [isAddingPrompt, isOpen]);

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

  /*
   * CDXC:StashedPromptTags 2026-08-23:
   * The rail refines the current search rather than replacing it, so pill
   * counts describe the searched set: "3 of what you are looking at is tagged
   * Release", not a standing total that contradicts the visible list.
   */
  const searchedPrompts = useMemo(() => {
    if (!prompts) {
      return [];
    }
    const query = searchQuery.toLowerCase().replace(/\s+/g, ' ').trim();
    if (!query) {
      return prompts;
    }
    return prompts.filter((prompt) => stashedPromptSearchText(prompt).includes(query));
  }, [prompts, searchQuery]);

  const visiblePrompts = useMemo(() => {
    if (tagFilter.kind === 'all') {
      return searchedPrompts;
    }
    if (tagFilter.kind === 'untagged') {
      return searchedPrompts.filter((prompt) => promptTagIds(prompt).length === 0);
    }
    return searchedPrompts.filter((prompt) => promptTagIds(prompt).includes(tagFilter.tagId));
  }, [searchedPrompts, tagFilter]);

  const untaggedPromptCount = useMemo(
    () => searchedPrompts.filter((prompt) => promptTagIds(prompt).length === 0).length,
    [searchedPrompts]
  );

  /*
   * CDXC:StashedPromptTags 2026-08-23:
   * Whether "No tag" exists is decided by the whole library, not the current
   * search: its count narrows with the query like every other pill, but the
   * pill itself must not blink in and out of the rail as the user types.
   */
  const hasTaggedPrompt = useMemo(
    () => (prompts ?? []).some((prompt) => promptTagIds(prompt).length > 0),
    [prompts]
  );

  const promptCountByTagId = useMemo(() => {
    const counts = new Map<string, number>();
    for (const prompt of searchedPrompts) {
      for (const tagId of promptTagIds(prompt)) {
        counts.set(tagId, (counts.get(tagId) ?? 0) + 1);
      }
    }
    return counts;
  }, [searchedPrompts]);

  const tagsById = useMemo(() => new Map(tags.map((tag) => [tag.tagId, tag])), [tags]);

  const groupedVisiblePrompts = useMemo(() => groupStashedPromptsByDay(visiblePrompts), [visiblePrompts]);
  const normalizedSearchQuery = searchQuery.toLowerCase().replace(/\s+/g, ' ').trim();
  const showAddPrompt =
    tagFilter.kind === 'all' &&
    (normalizedSearchQuery.length === 0 || 'add saved prompt new prompt'.includes(normalizedSearchQuery));
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

  const nextTagRequestId = (kind: string) => {
    requestCounterRef.current += 1;
    return `${kind}-${Date.now()}-${requestCounterRef.current}`;
  };

  /*
   * CDXC:StashedPromptTags 2026-08-23:
   * Tag toggles paint immediately and are confirmed by the daemon's echo. The
   * star is a one-click control on a list the user is scanning, so waiting a
   * round trip before it fills in reads as a dropped click.
   */
  const setPromptTags = (prompt: GxserverStashedPrompt, tagIds: readonly string[]) => {
    const nextTagIds = [...tagIds];
    setPrompts((current) =>
      current?.map((candidate) =>
        candidate.promptId === prompt.promptId ? { ...candidate, tagIds: nextTagIds } : candidate
      )
    );
    vscode.postMessage({
      promptId: prompt.promptId,
      requestId: nextTagRequestId('set-stashed-prompt-tags'),
      tagIds: nextTagIds,
      type: 'setStashedPromptTags',
    });
  };

  const togglePromptTag = (prompt: GxserverStashedPrompt, tagId: string) => {
    const current = promptTagIds(prompt);
    setPromptTags(
      prompt,
      current.includes(tagId) ? current.filter((candidate) => candidate !== tagId) : [...current, tagId]
    );
  };

  const openCreateTag = (origin: 'rail' | 'row', promptId?: string) => {
    createTagOriginRef.current = origin;
    createTagPromptIdRef.current = promptId;
    setCreateTagName('');
    setCreateTagColor(STASHED_PROMPT_TAG_COLORS[tags.length % STASHED_PROMPT_TAG_COLORS.length]);
    setTagError(undefined);
    setIsCreatingTag(true);
  };

  const commitCreateTag = () => {
    const name = createTagName.trim().replace(/\s+/g, ' ');
    if (!name) {
      return;
    }
    pendingTagApplicationRef.current =
      createTagOriginRef.current === 'row' && createTagPromptIdRef.current
        ? { name: name.toLowerCase(), promptId: createTagPromptIdRef.current }
        : { name: name.toLowerCase(), promptId: undefined };
    vscode.postMessage({
      color: createTagColor,
      name,
      requestId: nextTagRequestId('save-stashed-prompt-tag'),
      type: 'saveStashedPromptTag',
    });
    setIsCreatingTag(false);
    setCreateTagName('');
  };

  /*
   * CDXC:StashedPromptTags 2026-08-23:
   * Resolve a just-created tag once the refreshed catalogue arrives: file it on
   * the prompt whose menu created it, or make it the active rail filter when it
   * was created from the rail's own "+".
   */
  useEffect(() => {
    const pending = pendingTagApplicationRef.current;
    if (!pending) {
      return;
    }
    const createdTag = tags.find((tag) => tag.name.toLowerCase() === pending.name);
    if (!createdTag) {
      return;
    }
    pendingTagApplicationRef.current = undefined;
    if (!pending.promptId) {
      setTagFilter({ kind: 'tag', tagId: createdTag.tagId });
      return;
    }
    const prompt = prompts?.find((candidate) => candidate.promptId === pending.promptId);
    if (prompt && !promptTagIds(prompt).includes(createdTag.tagId)) {
      setPromptTags(prompt, [...promptTagIds(prompt), createdTag.tagId]);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prompts, tags]);

  const deleteTag = (tag: GxserverStashedPromptTag) => {
    if (tag.isBuiltin) {
      return;
    }
    vscode.postMessage({
      requestId: nextTagRequestId('delete-stashed-prompt-tag'),
      tagId: tag.tagId,
      type: 'deleteStashedPromptTag',
    });
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
              <StashedPromptTagRail
                onSelectFilter={setTagFilter}
                tagFilter={tagFilter}
                createTagColor={createTagColor}
                createTagName={createTagName}
                isCreatingTag={isCreatingTag && createTagOriginRef.current === 'rail'}
                onCommitCreateTag={commitCreateTag}
                onCreateTagColorChange={setCreateTagColor}
                onCreateTagNameChange={setCreateTagName}
                onCreateTagOpenChange={(nextOpen) => {
                  if (nextOpen) {
                    openCreateTag('rail');
                  } else {
                    setIsCreatingTag(false);
                  }
                }}
                onDeleteTag={deleteTag}
                promptCount={searchedPrompts.length}
                showUntaggedFilter={hasTaggedPrompt}
                untaggedPromptCount={untaggedPromptCount}
                promptCountByTagId={promptCountByTagId}
                tags={tags}
              />
              {tagError ? (
                <div className='ghostex-stashed-prompt-tag-error' role='alert'>
                  {tagError}
                </div>
              ) : null}
              <CommandList
                className='ghostex-command-palette-list ghostex-stashed-prompts-list'
                ref={promptListRef}
              >
                {prompts !== undefined && !showAddPrompt && visiblePrompts.length === 0 ? (
                  <CommandEmpty>
                    {tagFilter.kind === 'tag'
                      ? 'No saved prompts carry this tag yet.'
                      : tagFilter.kind === 'untagged'
                        ? 'Every saved prompt here already carries a tag.'
                        : 'No saved prompts match this search.'}
                  </CommandEmpty>
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
                                createTagColor={createTagColor}
                                createTagName={createTagName}
                                isCreatingTag={
                                  isCreatingTag &&
                                  createTagOriginRef.current === 'row' &&
                                  createTagPromptIdRef.current === prompt.promptId
                                }
                                isTagMenuOpen={tagMenuPromptId === prompt.promptId}
                                key={prompt.promptId}
                                onCommitCreateTag={commitCreateTag}
                                onCreateTagColorChange={setCreateTagColor}
                                onCreateTagNameChange={setCreateTagName}
                                onCreateTagOpenChange={(nextOpen) => {
                                  if (nextOpen) {
                                    openCreateTag('row', prompt.promptId);
                                  } else {
                                    setIsCreatingTag(false);
                                  }
                                }}
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
                                onTagMenuOpenChange={(nextOpen) => {
                                  setTagMenuPromptId(nextOpen ? prompt.promptId : undefined);
                                }}
                                onToggleTag={(tagId) => {
                                  togglePromptTag(prompt, tagId);
                                }}
                                prompt={prompt}
                                tags={tags}
                                tagsById={tagsById}
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

type StashedPromptTagRailProps = {
  createTagColor: string;
  createTagName: string;
  isCreatingTag: boolean;
  onCommitCreateTag: () => void;
  onCreateTagColorChange: (color: string) => void;
  onCreateTagNameChange: (name: string) => void;
  onCreateTagOpenChange: (nextOpen: boolean) => void;
  onDeleteTag: (tag: GxserverStashedPromptTag) => void;
  onSelectFilter: (filter: StashedPromptTagFilter) => void;
  promptCount: number;
  promptCountByTagId: Map<string, number>;
  showUntaggedFilter: boolean;
  tagFilter: StashedPromptTagFilter;
  tags: readonly GxserverStashedPromptTag[];
  untaggedPromptCount: number;
};

/*
 * CDXC:StashedPromptTags 2026-08-23:
 * The rail is the tag surface: one horizontally scrollable row of pills above
 * the list, so picking a tag never costs a menu round trip and the whole
 * vocabulary stays visible while scanning. It is deliberately a filter strip
 * and not a sidebar — the modal is 654px wide and a column would halve the
 * space prompt text has to be readable in.
 */
function StashedPromptTagRail({
  createTagColor,
  createTagName,
  isCreatingTag,
  onCommitCreateTag,
  onCreateTagColorChange,
  onCreateTagNameChange,
  onCreateTagOpenChange,
  onDeleteTag,
  onSelectFilter,
  promptCount,
  promptCountByTagId,
  showUntaggedFilter,
  tagFilter,
  tags,
  untaggedPromptCount,
}: StashedPromptTagRailProps) {
  return (
    <div className='ghostex-stashed-prompt-tag-rail-wrap'>
      <div className='ghostex-stashed-prompt-tag-rail' role='group' aria-label='Filter saved prompts by tag'>
        <button
          aria-pressed={tagFilter.kind === 'all'}
          className='ghostex-stashed-prompt-tag-pill'
          data-active={String(tagFilter.kind === 'all')}
          onClick={() => onSelectFilter(ALL_PROMPTS_FILTER)}
          style={{ '--ghostex-tag-color': '#9a9aa4' } as React.CSSProperties}
          type='button'
        >
          All
          <span className='ghostex-stashed-prompt-tag-count'>{promptCount}</span>
        </button>
        <span aria-hidden='true' className='ghostex-stashed-prompt-tag-rail-separator' />
        {tags.map((tag) => (
          <button
            aria-pressed={tagFilter.kind === 'tag' && tagFilter.tagId === tag.tagId}
            className='ghostex-stashed-prompt-tag-pill'
            data-active={String(tagFilter.kind === 'tag' && tagFilter.tagId === tag.tagId)}
            key={tag.tagId}
            onClick={() =>
              onSelectFilter(
                tagFilter.kind === 'tag' && tagFilter.tagId === tag.tagId
                  ? ALL_PROMPTS_FILTER
                  : { kind: 'tag', tagId: tag.tagId }
              )
            }
            /*
             * Removing a tag lives on its own pill rather than in a settings
             * screen, so the place you file prompts is the place you unfile a
             * mistake. Builtin Favorites has no delete affordance at all.
             */
            onContextMenu={(event) => {
              if (tag.isBuiltin) {
                return;
              }
              event.preventDefault();
              onDeleteTag(tag);
            }}
            style={{ '--ghostex-tag-color': tag.color } as React.CSSProperties}
            title={tag.isBuiltin ? tag.name : `${tag.name} — right-click to delete this tag`}
            type='button'
          >
            {tag.isBuiltin ? (
              <IconStarFilled aria-hidden='true' className='ghostex-stashed-prompt-tag-star' size={11} />
            ) : (
              <span aria-hidden='true' className='ghostex-stashed-prompt-tag-dot' />
            )}
            {tag.name}
            <span className='ghostex-stashed-prompt-tag-count'>{promptCountByTagId.get(tag.tagId) ?? 0}</span>
          </button>
        ))}
        {/*
          CDXC:StashedPromptTags 2026-08-23:
          "No tag" closes the rail so the pills partition the list completely —
          without it, prompts nobody has filed are reachable only through All.
          It stays out of the rail until something is tagged, because until then
          it selects the same set as All. It also stays put while it is the
          active filter, so tagging the last loose prompt cannot leave the list
          filtered by a pill that is no longer on screen.
        */}
        {showUntaggedFilter || tagFilter.kind === 'untagged' ? (
          <button
            aria-pressed={tagFilter.kind === 'untagged'}
            className='ghostex-stashed-prompt-tag-pill ghostex-stashed-prompt-tag-pill-untagged'
            data-active={String(tagFilter.kind === 'untagged')}
            onClick={() =>
              onSelectFilter(tagFilter.kind === 'untagged' ? ALL_PROMPTS_FILTER : { kind: 'untagged' })
            }
            title='Saved prompts with no tag'
            type='button'
          >
            <span aria-hidden='true' className='ghostex-stashed-prompt-tag-dot' />
            No tag
            <span className='ghostex-stashed-prompt-tag-count'>{untaggedPromptCount}</span>
          </button>
        ) : null}
      </div>
      {/*
        The "+" lives outside the scrolling strip: once there are more tags than
        fit, a New-tag control that scrolled away with them would be the one
        thing you cannot reach when you most need it.
      */}
      <Popover open={isCreatingTag} onOpenChange={onCreateTagOpenChange}>
        <PopoverTrigger
          render={
            <button
              aria-label='New tag'
              className='ghostex-stashed-prompt-tag-pill ghostex-stashed-prompt-tag-pill-add'
              title='New tag'
              type='button'
            >
              <IconPlus aria-hidden='true' size={13} stroke={2.2} />
            </button>
          }
        />
        <StashedPromptCreateTagPopover
          align='end'
          color={createTagColor}
          name={createTagName}
          onColorChange={onCreateTagColorChange}
          onCommit={onCommitCreateTag}
          onNameChange={onCreateTagNameChange}
        />
      </Popover>
    </div>
  );
}

type StashedPromptCreateTagPopoverProps = {
  align: 'center' | 'end' | 'start';
  color: string;
  name: string;
  onColorChange: (color: string) => void;
  onCommit: () => void;
  onNameChange: (name: string) => void;
};

function StashedPromptCreateTagPopover({
  align,
  color,
  name,
  onColorChange,
  onCommit,
  onNameChange,
}: StashedPromptCreateTagPopoverProps) {
  return (
    <PopoverContent align={align} className='ghostex-stashed-prompt-tag-popover' sideOffset={6}>
      <div className='ghostex-stashed-prompt-tag-popover-title'>New tag</div>
      <input
        aria-label='Tag name'
        autoFocus
        className='ghostex-stashed-prompt-tag-popover-input'
        maxLength={MAX_TAG_NAME_LENGTH}
        onChange={(event) => onNameChange(event.target.value)}
        /*
         * The rail lives inside cmdk, which reads arrow keys and Enter as list
         * navigation. This field owns those keys while it is open.
         */
        onKeyDown={(event) => {
          event.stopPropagation();
          if (event.key === 'Enter') {
            event.preventDefault();
            onCommit();
          }
        }}
        placeholder='Tag name'
        spellCheck={false}
        value={name}
      />
      <div className='ghostex-stashed-prompt-tag-swatches'>
        {STASHED_PROMPT_TAG_COLORS.map((swatch) => (
          <button
            aria-label={`Use color ${swatch}`}
            className='ghostex-stashed-prompt-tag-swatch'
            data-active={String(swatch === color)}
            key={swatch}
            onClick={() => onColorChange(swatch)}
            style={{ '--ghostex-tag-color': swatch } as React.CSSProperties}
            type='button'
          />
        ))}
      </div>
      <div className='ghostex-stashed-prompt-tag-popover-actions'>
        <button
          className='ghostex-stashed-prompt-editor-button ghostex-stashed-prompt-editor-button-primary'
          disabled={!name.trim()}
          onClick={onCommit}
          type='button'
        >
          Create tag
        </button>
      </div>
    </PopoverContent>
  );
}

type StashedPromptRowProps = {
  createTagColor: string;
  createTagName: string;
  isCreatingTag: boolean;
  isTagMenuOpen: boolean;
  onCommitCreateTag: () => void;
  onCreateTagColorChange: (color: string) => void;
  onCreateTagNameChange: (name: string) => void;
  onCreateTagOpenChange: (nextOpen: boolean) => void;
  onDelete: () => void;
  onEdit: () => void;
  onSelect: () => void;
  onTagMenuOpenChange: (nextOpen: boolean) => void;
  onToggleTag: (tagId: string) => void;
  prompt: GxserverStashedPrompt;
  tags: readonly GxserverStashedPromptTag[];
  tagsById: Map<string, GxserverStashedPromptTag>;
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

function StashedPromptRow({
  createTagColor,
  createTagName,
  isCreatingTag,
  isTagMenuOpen,
  onCommitCreateTag,
  onCreateTagColorChange,
  onCreateTagNameChange,
  onCreateTagOpenChange,
  onDelete,
  onEdit,
  onSelect,
  onTagMenuOpenChange,
  onToggleTag,
  prompt,
  tags,
  tagsById,
}: StashedPromptRowProps) {
  const lines = prompt.content.trim().split('\n');
  const tooltipLines = lines.slice(0, TOOLTIP_LINE_COUNT);
  const tooltipTruncated = lines.length > TOOLTIP_LINE_COUNT;
  const tagIds = promptTagIds(prompt);
  const isFavorite = tagIds.includes(GXSERVER_FAVORITE_PROMPT_TAG_ID);
  const rowTags = tagIds
    .filter((tagId) => tagId !== GXSERVER_FAVORITE_PROMPT_TAG_ID)
    .map((tagId) => tagsById.get(tagId))
    .filter((tag): tag is GxserverStashedPromptTag => tag !== undefined);
  /*
   * CDXC:StashedPromptTags 2026-08-23:
   * The row's left edge carries its first non-Favorites tag color. That stripe
   * is what separates one prompt from the next now that there are no rules
   * between rows: a repeating vertical mark the eye can group by, instead of a
   * hairline that competes with the hover and selection fills.
   */
  const stripeColor = rowTags[0]?.color;

  return (
    <CommandItem
      className='ghostex-stashed-prompt-item'
      data-favorite={String(isFavorite)}
      /*
       * CDXC:StashedPromptTags 2026-08-23:
       * The tag menu is portalled out of this row, so moving the pointer into
       * it ends the row's :hover and empties its :focus-within. Without this
       * flag the action cluster would collapse to display:none, the open
       * popover's anchor would measure 0x0, and the menu would jump to the top
       * left of the window mid-hover. Pin the cluster open for as long as the
       * menu it launched is open.
       */
      data-tag-menu-open={String(isTagMenuOpen)}
      onSelect={onSelect}
      style={stripeColor ? ({ '--ghostex-stashed-prompt-stripe': stripeColor } as React.CSSProperties) : undefined}
      value={prompt.promptId}
    >
      <span aria-hidden='true' className='ghostex-stashed-prompt-stripe' />
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
          {/*
            The persistent star marks a favorite while scanning, and gives way
            to the action cluster on hover so the two never stack in the same
            corner.
          */}
          {isFavorite ? (
            <span aria-label='Favorite' className='ghostex-stashed-prompt-favorite-mark'>
              <IconStarFilled aria-hidden='true' size={13} />
            </span>
          ) : null}
          <span className='ghostex-stashed-prompt-actions'>
            <button
              aria-label={isFavorite ? 'Remove from favorites' : 'Add to favorites'}
              aria-pressed={isFavorite}
              className='ghostex-stashed-prompt-action ghostex-stashed-prompt-action-favorite'
              data-active={String(isFavorite)}
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onToggleTag(GXSERVER_FAVORITE_PROMPT_TAG_ID);
              }}
              type='button'
            >
              {isFavorite ? (
                <IconStarFilled aria-hidden='true' size={14} />
              ) : (
                <IconStar aria-hidden='true' size={14} stroke={1.9} />
              )}
            </button>
            <Popover open={isTagMenuOpen} onOpenChange={onTagMenuOpenChange}>
              <PopoverTrigger
                render={
                  <button
                    aria-label='Tags'
                    className='ghostex-stashed-prompt-action'
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                    }}
                    type='button'
                  >
                    <IconTag aria-hidden='true' size={14} stroke={1.9} />
                  </button>
                }
              />
              <PopoverContent
                align='end'
                className='ghostex-stashed-prompt-tag-popover'
                onKeyDown={(event) => event.stopPropagation()}
                sideOffset={6}
              >
                <div className='ghostex-stashed-prompt-tag-popover-title'>Tags</div>
                <div className='ghostex-stashed-prompt-tag-menu-list'>
                  {tags.map((tag) => (
                    <button
                      className='ghostex-stashed-prompt-tag-menu-item'
                      data-active={String(tagIds.includes(tag.tagId))}
                      key={tag.tagId}
                      onClick={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        onToggleTag(tag.tagId);
                      }}
                      style={{ '--ghostex-tag-color': tag.color } as React.CSSProperties}
                      type='button'
                    >
                      {tag.isBuiltin ? (
                        <IconStarFilled aria-hidden='true' className='ghostex-stashed-prompt-tag-star' size={11} />
                      ) : (
                        <span aria-hidden='true' className='ghostex-stashed-prompt-tag-dot' />
                      )}
                      <span className='ghostex-stashed-prompt-tag-menu-name'>{tag.name}</span>
                      <IconCheck
                        aria-hidden='true'
                        className='ghostex-stashed-prompt-tag-menu-check'
                        size={13}
                        stroke={2.4}
                      />
                    </button>
                  ))}
                </div>
                <Popover open={isCreatingTag} onOpenChange={onCreateTagOpenChange}>
                  <PopoverTrigger
                    render={
                      <button className='ghostex-stashed-prompt-tag-menu-item' type='button'>
                        <IconPlus aria-hidden='true' size={13} stroke={2.2} />
                        <span className='ghostex-stashed-prompt-tag-menu-name'>New tag…</span>
                      </button>
                    }
                  />
                  <StashedPromptCreateTagPopover
                    align='end'
                    color={createTagColor}
                    name={createTagName}
                    onColorChange={onCreateTagColorChange}
                    onCommit={onCommitCreateTag}
                    onNameChange={onCreateTagNameChange}
                  />
                </Popover>
              </PopoverContent>
            </Popover>
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
            {rowTags.length > 0 ? (
              <span className='ghostex-stashed-prompt-chips'>
                {rowTags.map((tag) => (
                  <span
                    className='ghostex-stashed-prompt-chip'
                    key={tag.tagId}
                    style={{ '--ghostex-tag-color': tag.color } as React.CSSProperties}
                  >
                    <span aria-hidden='true' className='ghostex-stashed-prompt-chip-dot' />
                    {tag.name}
                  </span>
                ))}
              </span>
            ) : null}
          </span>
          <span className='ghostex-stashed-prompt-time'>{relativeTimeLabel(prompt.updatedAt)}</span>
        </span>
      </span>
    </CommandItem>
  );
}
