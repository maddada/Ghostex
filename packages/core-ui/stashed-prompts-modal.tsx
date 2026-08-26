import {
  IconArrowUpRight,
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
import { Button } from '../components/ui/button';
import { Field, FieldGroup, FieldLabel } from '../components/ui/field';
import { Popover, PopoverContent, PopoverTrigger } from '../components/ui/popover';
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '../components/ui/select';
import { Textarea } from '../components/ui/textarea';
import { parseGxserverPresentationProjectSessionId } from '../shared/gxserver-presentation-sidebar-projection';
import type { GxserverStashedPrompt, GxserverStashedPromptTag } from '../shared/gxserver-protocol';
import { GXSERVER_FAVORITE_PROMPT_TAG_ID, GXSERVER_STASHED_PROMPT_TAG_ID } from '../shared/gxserver-protocol';
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
import { useSidebarStore } from './sidebar-store';
import { TOOLTIP_DELAY_MS } from './tooltip-delay';
import type { WebviewApi } from './webview-api';

/*
 * CDXC:StashedPromptSessionAssociation 2026-08-24:
 * Saved prompts are durably tied to the agent conversation they were stashed
 * from, so the library can be narrowed to the project or the conversation the
 * modal was opened for instead of only ever listing everything.
 */
export type StashedPromptsScope = 'all' | 'project' | 'session';

export type StashedPromptsModalProps = {
  /**
   * Scope the modal opens on. Optional: without it the modal picks its own
   * default — the session scope when it has session context and that scope is
   * not empty, otherwise all prompts.
   */
  initialScope?: StashedPromptsScope;
  isOpen: boolean;
  onClose: () => void;
  projectId?: string;
  /**
   * The launching session, as the sidebar's combined `combined-session:` key.
   * Insertion routes back through this id, while gxserver writes and stash-row
   * comparisons use the raw ids decoded out of it.
   */
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
type StashedPromptTagFilter = { kind: 'all' } | { kind: 'tag'; tagId: string } | { kind: 'untagged' };

const ALL_PROMPTS_FILTER: StashedPromptTagFilter = { kind: 'all' };
const ALL_PROJECTS_VALUE = 'scope:all';
const CURRENT_SESSION_VALUE = 'scope:session';
const NO_PROJECT_VALUE = 'project:none';
const ALL_TAGS_VALUE = 'tag:all';
const NO_TAG_VALUE = 'tag:none';

type SavedPromptProjectOption = {
  name: string;
  projectId: string;
};

function projectFilterValue(scope: StashedPromptsScope, projectId: string | undefined): string {
  if (scope === 'session') {
    return CURRENT_SESSION_VALUE;
  }
  return scope === 'project' && projectId ? `project:${projectId}` : ALL_PROJECTS_VALUE;
}

function tagFilterValue(filter: StashedPromptTagFilter): string {
  if (filter.kind === 'tag') {
    return `tag:${filter.tagId}`;
  }
  return filter.kind === 'untagged' ? NO_TAG_VALUE : ALL_TAGS_VALUE;
}

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

function promptLabelTagIds(prompt: GxserverStashedPrompt): readonly string[] {
  return promptTagIds(prompt).filter((tagId) => tagId !== GXSERVER_FAVORITE_PROMPT_TAG_ID);
}

type StashedPromptSessionContext = {
  agentSessionId: string | undefined;
  projectId: string | undefined;
  sessionId: string | undefined;
};

/*
 * CDXC:StashedPromptSessionAssociation 2026-08-24:
 * A prompt belongs to the conversation this modal was opened for when gxserver
 * stamped it with the same `agentSessionId` — that association is re-keyed
 * through provider compaction/resume rewrites, so it outlives the session row
 * the prompt was stashed from. Rows stashed before that column existed are
 * matched on the raw gxserver session ids instead.
 */
function promptBelongsToSession(prompt: GxserverStashedPrompt, context: StashedPromptSessionContext): boolean {
  if (context.agentSessionId && prompt.agentSessionId === context.agentSessionId) {
    return true;
  }
  if (!context.sessionId || prompt.sessionId !== context.sessionId) {
    return false;
  }
  return context.projectId === undefined || prompt.projectId === context.projectId;
}

function promptBelongsToProject(prompt: GxserverStashedPrompt, projectId: string | undefined): boolean {
  return projectId !== undefined && prompt.projectId === projectId;
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
  initialScope,
  isOpen,
  onClose,
  projectId,
  sessionId,
  stashHintTooltipDefaultOpen = false,
  vscode,
}: StashedPromptsModalProps) {
  const [prompts, setPrompts] = useState<GxserverStashedPrompt[]>();
  const [tags, setTags] = useState<GxserverStashedPromptTag[]>([]);
  const [scope, setScope] = useState<StashedPromptsScope>(initialScope ?? 'all');
  const [scopeProjectId, setScopeProjectId] = useState<string>();
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
  const [draftProjectId, setDraftProjectId] = useState(NO_PROJECT_VALUE);
  const [draftTagId, setDraftTagId] = useState(NO_TAG_VALUE);
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
  /*
   * CDXC:StashedPromptSessionAssociation 2026-08-24:
   * The "default to this session" decision needs the loaded rows, so it runs
   * once per open, on the first list result, and never again — otherwise a
   * later refresh would yank the scope back from under the user.
   */
  const hasResolvedDefaultScopeRef = useRef(false);

  /*
   * CDXC:StashedPromptSessionAssociation 2026-08-24:
   * The `sessionId` prop is the sidebar's combined presentation key, while
   * stash rows carry gxserver's raw ids. Decode once here so scope matching and
   * the save form both speak the daemon's id vocabulary.
   */
  const combinedSessionReference = useMemo(
    () => (sessionId ? parseGxserverPresentationProjectSessionId(sessionId) : undefined),
    [sessionId]
  );
  const rawSessionId = combinedSessionReference?.sessionId ?? sessionId;
  const rawProjectId = projectId ?? combinedSessionReference?.projectId;
  const currentAgentSessionId = useSidebarStore((state) =>
    sessionId ? state.sessionsById[sessionId]?.agentSessionId : undefined
  );
  const groupOrder = useSidebarStore((state) => state.groupOrder);
  const groupsById = useSidebarStore((state) => state.groupsById);
  const sessionContext = useMemo<StashedPromptSessionContext>(
    () => ({ agentSessionId: currentAgentSessionId, projectId: rawProjectId, sessionId: rawSessionId }),
    [currentAgentSessionId, rawProjectId, rawSessionId]
  );
  const hasSessionScope = Boolean(rawSessionId || currentAgentSessionId);
  const projectOptions = useMemo(() => {
    const options = new Map<string, SavedPromptProjectOption>();
    for (const groupId of groupOrder) {
      const group = groupsById[groupId];
      const groupProjectId = group?.projectContext?.editor.projectId;
      if (groupProjectId && !options.has(groupProjectId)) {
        options.set(groupProjectId, { name: group.title, projectId: groupProjectId });
      }
    }
    for (const prompt of prompts ?? []) {
      if (prompt.projectId && !options.has(prompt.projectId)) {
        options.set(prompt.projectId, {
          name: prompt.projectName?.trim() || 'Unnamed project',
          projectId: prompt.projectId,
        });
      }
    }
    if (rawProjectId && !options.has(rawProjectId)) {
      options.set(rawProjectId, { name: 'This project', projectId: rawProjectId });
    }
    return [...options.values()].sort((left, right) => left.name.localeCompare(right.name));
  }, [groupOrder, groupsById, prompts, rawProjectId]);
  /*
   * A scope whose segment is not on screen must not silently filter the list,
   * so an unavailable scope reads as "all" without rewriting the user's choice.
   */
  const effectiveScope: StashedPromptsScope =
    (scope === 'session' && !hasSessionScope) || (scope === 'project' && !scopeProjectId) ? 'all' : scope;

  useEffect(() => {
    if (!isOpen) {
      setPrompts(undefined);
      setTags([]);
      setScopeProjectId(undefined);
      setTagFilter(ALL_PROMPTS_FILTER);
      setTagMenuPromptId(undefined);
      setIsCreatingTag(false);
      setCreateTagName('');
      setTagError(undefined);
      setSearchQuery('');
      setIsAddingPrompt(false);
      setEditingPromptId(undefined);
      setDraftContent('');
      setDraftProjectId(NO_PROJECT_VALUE);
      setDraftTagId(NO_TAG_VALUE);
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
        setDraftProjectId(NO_PROJECT_VALUE);
        setDraftTagId(NO_TAG_VALUE);
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
      document.querySelector<HTMLInputElement>('.ghostex-stashed-prompts-dialog [data-slot="command-input"]')?.focus();
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
      setDraftProjectId(NO_PROJECT_VALUE);
      setDraftTagId(NO_TAG_VALUE);
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
    /*
     * CDXC:StashedPromptSessionAssociation 2026-08-24:
     * The whole library is loaded on every open and narrowed client-side, so
     * switching to the All scope never costs a round trip and the scope counts
     * describe the same set the list is drawn from.
     */
    hasResolvedDefaultScopeRef.current = false;
    setScope(initialScope ?? 'all');
    setScopeProjectId(rawProjectId);
    vscode.postMessage({
      requestId,
      type: 'requestStashedPrompts',
    });
  }, [initialScope, isOpen, rawProjectId, vscode]);

  /*
   * CDXC:StashedPromptSessionAssociation 2026-08-24:
   * Without a launcher-pinned scope the modal opens on this session when it has
   * session context and that scope actually has prompts in it. It never opens
   * on an empty filtered list.
   */
  useEffect(() => {
    if (!isOpen || prompts === undefined || hasResolvedDefaultScopeRef.current) {
      return;
    }
    hasResolvedDefaultScopeRef.current = true;
    if (initialScope !== undefined || !hasSessionScope) {
      return;
    }
    if (prompts.some((prompt) => promptBelongsToSession(prompt, sessionContext))) {
      setScope('session');
    }
  }, [hasSessionScope, initialScope, isOpen, prompts, sessionContext]);

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

  /*
   * CDXC:StashedPromptSessionAssociation 2026-08-24:
   * Scope narrows the searched set before the tag rail sees it, so the pill
   * counts keep describing what is actually on screen: search AND scope AND
   * tag, in that order.
   */
  const scopedPrompts = useMemo(() => {
    if (effectiveScope === 'all') {
      return searchedPrompts;
    }
    if (effectiveScope === 'project') {
      return searchedPrompts.filter((prompt) => promptBelongsToProject(prompt, scopeProjectId));
    }
    return searchedPrompts.filter((prompt) => promptBelongsToSession(prompt, sessionContext));
  }, [effectiveScope, scopeProjectId, searchedPrompts, sessionContext]);

  const visiblePrompts = useMemo(() => {
    if (tagFilter.kind === 'all') {
      return scopedPrompts;
    }
    if (tagFilter.kind === 'untagged') {
      return scopedPrompts.filter((prompt) => promptLabelTagIds(prompt).length === 0);
    }
    return scopedPrompts.filter((prompt) => promptTagIds(prompt).includes(tagFilter.tagId));
  }, [scopedPrompts, tagFilter]);

  const untaggedPromptCount = useMemo(
    () => scopedPrompts.filter((prompt) => promptLabelTagIds(prompt).length === 0).length,
    [scopedPrompts]
  );

  /*
   * CDXC:StashedPromptTags 2026-08-23:
   * Whether "No tag" exists is decided by the whole library, not the current
   * search: its count narrows with the query like every other pill, but the
   * pill itself must not blink in and out of the rail as the user types.
   */
  const hasTaggedPrompt = useMemo(
    () => (prompts ?? []).some((prompt) => promptLabelTagIds(prompt).length > 0),
    [prompts]
  );

  const promptCountByTagId = useMemo(() => {
    const counts = new Map<string, number>();
    for (const prompt of scopedPrompts) {
      for (const tagId of promptTagIds(prompt)) {
        counts.set(tagId, (counts.get(tagId) ?? 0) + 1);
      }
    }
    return counts;
  }, [scopedPrompts]);

  const tagsById = useMemo(() => new Map(tags.map((tag) => [tag.tagId, tag])), [tags]);

  const groupedVisiblePrompts = useMemo(() => groupStashedPromptsByDay(visiblePrompts), [visiblePrompts]);
  const topPromptValue = visiblePrompts[0]?.promptId ?? '';

  useLayoutEffect(() => {
    if (!isOpen || isAddingPrompt) {
      return;
    }
    setSelectedPromptValue(topPromptValue);
    if (promptListRef.current) {
      promptListRef.current.scrollTop = 0;
    }
  }, [isAddingPrompt, isOpen, searchQuery, topPromptValue]);

  const openAddPrompt = () => {
    const defaultProjectId = effectiveScope === 'project' ? scopeProjectId : rawProjectId;
    const defaultTagId =
      tagFilter.kind === 'tag' && tagFilter.tagId !== GXSERVER_FAVORITE_PROMPT_TAG_ID
        ? `tag:${tagFilter.tagId}`
        : NO_TAG_VALUE;
    setEditingPromptId(undefined);
    setDraftContent('');
    setDraftProjectId(defaultProjectId ? `project:${defaultProjectId}` : NO_PROJECT_VALUE);
    setDraftTagId(defaultTagId);
    setSaveError(undefined);
    setIsAddingPrompt(true);
  };

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
    const favoriteTagIds = current.includes(GXSERVER_FAVORITE_PROMPT_TAG_ID) ? [GXSERVER_FAVORITE_PROMPT_TAG_ID] : [];
    if (tagId === GXSERVER_FAVORITE_PROMPT_TAG_ID) {
      const labelTagId = current.find((candidate) => candidate !== GXSERVER_FAVORITE_PROMPT_TAG_ID);
      setPromptTags(prompt, [
        ...(favoriteTagIds.length > 0 ? [] : [GXSERVER_FAVORITE_PROMPT_TAG_ID]),
        ...(labelTagId ? [labelTagId] : []),
      ]);
      return;
    }
    setPromptTags(prompt, current.includes(tagId) ? favoriteTagIds : [...favoriteTagIds, tagId]);
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
      setPromptTags(prompt, [
        ...(promptTagIds(prompt).includes(GXSERVER_FAVORITE_PROMPT_TAG_ID) ? [GXSERVER_FAVORITE_PROMPT_TAG_ID] : []),
        createdTag.tagId,
      ]);
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
    const selectedProjectId = draftProjectId === NO_PROJECT_VALUE ? undefined : draftProjectId.slice('project:'.length);
    const selectedTagIds = draftTagId === NO_TAG_VALUE ? [] : [draftTagId.slice('tag:'.length)];
    /*
     * CDXC:StashedPromptSessionAssociation 2026-08-24:
     * Post the raw gxserver ids decoded out of the combined presentation key.
     * This form used to store the combined key verbatim, which made its rows
     * name a session gxserver has never heard of; the daemon normalizes stored
     * ids as of migration 0026, and writing them raw keeps the two in step.
     */
    vscode.postMessage({
      content,
      ...(editingPromptId ? { promptId: editingPromptId } : {}),
      ...(!editingPromptId && selectedProjectId ? { projectId: selectedProjectId } : {}),
      requestId,
      ...(!editingPromptId && selectedProjectId === rawProjectId && rawSessionId ? { sessionId: rawSessionId } : {}),
      ...(!editingPromptId ? { tagIds: selectedTagIds } : {}),
      type: 'saveStashedPrompt',
    });
  };

  const jumpToPromptSession = (prompt: GxserverStashedPrompt) => {
    vscode.postMessage({
      ...(prompt.agentSessionId ? { agentSessionId: prompt.agentSessionId } : {}),
      ...(prompt.projectId ? { projectId: prompt.projectId } : {}),
      ...(prompt.sessionId ? { sessionId: prompt.sessionId } : {}),
      type: 'jumpToStashedPromptSession',
    });
    onClose();
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
            <div className='ghostex-stashed-prompt-editor' data-editing={String(Boolean(editingPromptId))}>
              <div className='ghostex-stashed-prompt-editor-heading'>
                {editingPromptId ? 'Edit Saved Prompt' : 'Add Saved Prompt'}
              </div>
              {!editingPromptId ? (
                <FieldGroup className='ghostex-stashed-prompt-editor-metadata'>
                  <Field>
                    <FieldLabel className='sr-only'>Project</FieldLabel>
                    <Select value={draftProjectId} onValueChange={setDraftProjectId}>
                      <SelectTrigger aria-label='Project for saved prompt' size='sm'>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent align='start' alignItemWithTrigger={false}>
                        <SelectGroup>
                          <SelectItem value={NO_PROJECT_VALUE}>No project</SelectItem>
                          {projectOptions.map((project) => (
                            <SelectItem key={project.projectId} value={`project:${project.projectId}`}>
                              {project.name}
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                  </Field>
                  <Field>
                    <FieldLabel className='sr-only'>Tag</FieldLabel>
                    <Select value={draftTagId} onValueChange={setDraftTagId}>
                      <SelectTrigger aria-label='Tag for saved prompt' size='sm'>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent align='start' alignItemWithTrigger={false}>
                        <SelectGroup>
                          <SelectItem value={NO_TAG_VALUE}>No tag</SelectItem>
                          {tags
                            .filter((tag) => tag.tagId !== GXSERVER_FAVORITE_PROMPT_TAG_ID)
                            .map((tag) => (
                              <SelectItem key={tag.tagId} value={`tag:${tag.tagId}`}>
                                {tag.name}
                              </SelectItem>
                            ))}
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                  </Field>
                </FieldGroup>
              ) : null}
              <Textarea
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
                <Button
                  disabled={isSavingPrompt}
                  onClick={() => {
                    setIsAddingPrompt(false);
                    setEditingPromptId(undefined);
                    setDraftContent('');
                    setDraftProjectId(NO_PROJECT_VALUE);
                    setDraftTagId(NO_TAG_VALUE);
                    setSaveError(undefined);
                  }}
                  size='sm'
                  type='button'
                  variant='outline'
                >
                  Cancel
                </Button>
                <Button disabled={!draftContent.trim() || isSavingPrompt} onClick={savePrompt} size='sm' type='button'>
                  {isSavingPrompt ? 'Saving...' : editingPromptId ? 'Save Changes' : 'Add Prompt'}
                </Button>
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
              <StashedPromptFiltersToolbar
                onAddPrompt={openAddPrompt}
                onProjectFilterChange={(value) => {
                  if (value === CURRENT_SESSION_VALUE) {
                    setScope('session');
                    return;
                  }
                  if (value === ALL_PROJECTS_VALUE) {
                    setScope('all');
                    return;
                  }
                  setScopeProjectId(value.slice('project:'.length));
                  setScope('project');
                }}
                projectFilterValue={projectFilterValue(effectiveScope, scopeProjectId)}
                projectOptions={projectOptions}
                showSessionFilter={hasSessionScope}
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
                promptCount={scopedPrompts.length}
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
              <CommandList className='ghostex-command-palette-list ghostex-stashed-prompts-list' ref={promptListRef}>
                {prompts !== undefined && visiblePrompts.length === 0 ? (
                  <CommandEmpty>
                    {tagFilter.kind === 'tag'
                      ? 'No saved prompts carry this tag yet.'
                      : tagFilter.kind === 'untagged'
                        ? 'Every saved prompt here already carries a tag.'
                        : effectiveScope === 'session'
                          ? 'No saved prompts came from this session.'
                          : effectiveScope === 'project'
                            ? 'No saved prompts came from this project.'
                            : 'No saved prompts match this search.'}
                  </CommandEmpty>
                ) : null}
                {prompts === undefined || visiblePrompts.length > 0 ? (
                  <CommandGroup>
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
                                onJumpToSession={() => {
                                  jumpToPromptSession(prompt);
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

type StashedPromptFiltersToolbarProps = {
  createTagColor: string;
  createTagName: string;
  isCreatingTag: boolean;
  onAddPrompt: () => void;
  onCommitCreateTag: () => void;
  onCreateTagColorChange: (color: string) => void;
  onCreateTagNameChange: (name: string) => void;
  onCreateTagOpenChange: (nextOpen: boolean) => void;
  onDeleteTag: (tag: GxserverStashedPromptTag) => void;
  onProjectFilterChange: (value: string) => void;
  onSelectFilter: (filter: StashedPromptTagFilter) => void;
  projectFilterValue: string;
  projectOptions: readonly SavedPromptProjectOption[];
  promptCount: number;
  promptCountByTagId: Map<string, number>;
  showSessionFilter: boolean;
  showUntaggedFilter: boolean;
  tagFilter: StashedPromptTagFilter;
  tags: readonly GxserverStashedPromptTag[];
  untaggedPromptCount: number;
};

/*
 * CDXC:StashedPromptTags 2026-08-23:
 * Project and tag filters share one compact toolbar. Dropdowns keep the whole
 * vocabulary reachable without spending two rows, the new-tag action sits
 * immediately after the tag selector, and Add Prompt remains pinned right.
 */
function StashedPromptFiltersToolbar({
  createTagColor,
  createTagName,
  isCreatingTag,
  onAddPrompt,
  onCommitCreateTag,
  onCreateTagColorChange,
  onCreateTagNameChange,
  onCreateTagOpenChange,
  onDeleteTag,
  onProjectFilterChange,
  onSelectFilter,
  projectFilterValue: selectedProjectFilter,
  projectOptions,
  promptCount,
  promptCountByTagId,
  showSessionFilter,
  showUntaggedFilter,
  tagFilter,
  tags,
  untaggedPromptCount,
}: StashedPromptFiltersToolbarProps) {
  return (
    <div className='ghostex-stashed-prompt-toolbar'>
      <Select value={selectedProjectFilter} onValueChange={onProjectFilterChange}>
        <SelectTrigger
          aria-label='Filter saved prompts by project'
          className='ghostex-stashed-prompt-project-filter'
          size='sm'
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent align='start' alignItemWithTrigger={false}>
          <SelectGroup>
            <SelectItem value={ALL_PROJECTS_VALUE}>All projects</SelectItem>
            {showSessionFilter ? <SelectItem value={CURRENT_SESSION_VALUE}>This session</SelectItem> : null}
            {projectOptions.map((project) => (
              <SelectItem key={project.projectId} value={`project:${project.projectId}`}>
                {project.name}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <Select
        value={tagFilterValue(tagFilter)}
        onValueChange={(value) => {
          if (value === ALL_TAGS_VALUE) {
            onSelectFilter(ALL_PROMPTS_FILTER);
          } else if (value === NO_TAG_VALUE) {
            onSelectFilter({ kind: 'untagged' });
          } else {
            onSelectFilter({ kind: 'tag', tagId: value.slice('tag:'.length) });
          }
        }}
      >
        <SelectTrigger aria-label='Filter saved prompts by tag' className='ghostex-stashed-prompt-tag-filter' size='sm'>
          <SelectValue />
        </SelectTrigger>
        <SelectContent align='start' alignItemWithTrigger={false}>
          <SelectGroup>
            <SelectItem value={ALL_TAGS_VALUE}>All tags ({promptCount})</SelectItem>
            {tags.map((tag) => (
              <SelectItem
                key={tag.tagId}
                onContextMenu={(event) => {
                  if (!tag.isBuiltin) {
                    event.preventDefault();
                    onDeleteTag(tag);
                  }
                }}
                title={tag.isBuiltin ? tag.name : `${tag.name} — right-click to delete this tag`}
                value={`tag:${tag.tagId}`}
              >
                <span
                  aria-hidden='true'
                  className='ghostex-stashed-prompt-select-tag-dot'
                  style={{ '--ghostex-tag-color': tag.color } as React.CSSProperties}
                />
                {tag.name} ({promptCountByTagId.get(tag.tagId) ?? 0})
              </SelectItem>
            ))}
            {showUntaggedFilter || tagFilter.kind === 'untagged' ? (
              <SelectItem value={NO_TAG_VALUE}>No tag ({untaggedPromptCount})</SelectItem>
            ) : null}
          </SelectGroup>
        </SelectContent>
      </Select>
      <Popover open={isCreatingTag} onOpenChange={onCreateTagOpenChange}>
        <PopoverTrigger
          render={
            <Button aria-label='New tag' size='icon-sm' title='New tag' type='button' variant='outline'>
              <IconPlus aria-hidden='true' />
            </Button>
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
      <Button
        className='ghostex-stashed-prompt-add-button'
        onClick={onAddPrompt}
        size='default'
        type='button'
        variant='outline'
      >
        <IconPlus aria-hidden='true' data-icon='inline-start' />
        Add Prompt
      </Button>
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
  onJumpToSession: () => void;
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
  onJumpToSession,
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
  const labelTags = tagIds
    .filter((tagId) => tagId !== GXSERVER_FAVORITE_PROMPT_TAG_ID)
    .map((tagId) => tagsById.get(tagId))
    .filter((tag): tag is GxserverStashedPromptTag => tag !== undefined);
  const visibleRowTags = labelTags.filter((tag) => tag.tagId !== GXSERVER_STASHED_PROMPT_TAG_ID);
  /*
   * CDXC:StashedPromptTags 2026-08-23:
   * The row's left edge carries its first non-Favorites tag color. That stripe
   * is what separates one prompt from the next now that there are no rules
   * between rows: a repeating vertical mark the eye can group by, instead of a
   * hairline that competes with the hover and selection fills.
   */
  const stripeColor = labelTags[0]?.color;
  /*
   * CDXC:StashedPromptSessionAssociation 2026-08-24:
   * A prompt is jumpable while gxserver can still name where it came from: the
   * conversation id survives the session row, so either id is enough to open
   * something — waking, restoring, or resuming as needed.
   */
  const canJumpToSession = Boolean(prompt.agentSessionId || prompt.sessionId);

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
            {canJumpToSession ? (
              <AppTooltip content='Go to session' side='top' sideOffset={6}>
                <button
                  aria-label='Go to session'
                  className='ghostex-stashed-prompt-action'
                  onClick={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    onJumpToSession();
                  }}
                  type='button'
                >
                  <IconArrowUpRight aria-hidden='true' size={14} stroke={1.9} />
                </button>
              </AppTooltip>
            ) : null}
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
                  {tags
                    .filter((tag) => tag.tagId !== GXSERVER_FAVORITE_PROMPT_TAG_ID)
                    .map((tag) => (
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
                        <span aria-hidden='true' className='ghostex-stashed-prompt-tag-dot' />
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
            {/*
              CDXC:StashedPromptSessionAssociation 2026-08-24:
              The origin conversation's current title, not the one it had when
              the prompt was stashed: gxserver resolves it through the
              conversation id, so a renamed or resumed session still reads as
              the place this prompt came from.
            */}
            {prompt.sessionTitle ? (
              <span
                className='ghostex-stashed-prompt-chip ghostex-stashed-prompt-session-chip'
                title={prompt.sessionTitle}
              >
                <span className='ghostex-stashed-prompt-session-chip-label'>{prompt.sessionTitle}</span>
              </span>
            ) : null}
            {visibleRowTags.length > 0 ? (
              <span className='ghostex-stashed-prompt-chips'>
                {visibleRowTags.map((tag) => (
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
