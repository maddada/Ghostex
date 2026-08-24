/*
CDXC:AgentHistorySearch 2026-08-20:
All Find-surface state in one hook, so gpui, web, and the mobile WebView render
the same behaviour from the same reducer and only differ in how they mount it.

Results are a window, not the whole list: gxserver holds ~25k prompts and ranks
them per keystroke, so the client asks for a page around the selection and pages
as the selection walks off the edge.
*/

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type {
  FindPromptAgent,
  FindPromptAgentFacet,
  FindPromptProjectFacet,
  FindPromptRow,
  ResolveAgentPromptLaunchResult,
} from '../../shared/agent-prompt-search';
import type { FindPromptsTransport } from './find-prompts-transport';

/** Rows fetched per page. Big enough that arrow-key walking rarely pages. */
export const FIND_PROMPTS_PAGE_SIZE = 120;
/** Keystroke settle time before re-querying; the server ranks in ~100ms. */
export const FIND_PROMPTS_QUERY_DEBOUNCE_MS = 120;

export type FindPromptsOverlay = 'agent' | 'fork' | 'project' | null;

export interface FindPromptsNotice {
  detail?: string;
  kind: 'error' | 'info';
  message: string;
}

export interface FindPromptsState {
  agentFacets: readonly FindPromptAgentFacet[];
  agents: ReadonlySet<FindPromptAgent>;
  expandedPrompt: boolean;
  fullscreenPreview: boolean;
  groupByDay: boolean;
  indexedAt: number;
  loading: boolean;
  matched: number;
  notice: FindPromptsNotice | null;
  overlay: FindPromptsOverlay;
  previewFocused: boolean;
  project: string | null;
  projectFacets: readonly FindPromptProjectFacet[];
  query: string;
  /** Absolute position of the selected row inside the matched list. */
  selection: number;
  /** Full text of the selected prompt once fetched; the row text until then. */
  selectedText: string | null;
  total: number;
  windowOffset: number;
  rows: readonly FindPromptRow[];
  wrapPreview: boolean;
}

export interface FindPromptsController extends FindPromptsState {
  cancelOverlay(): void;
  closeExpandedPrompt(): void;
  copySelected(): Promise<void>;
  forkSelected(agent: FindPromptAgent): Promise<void>;
  jumpDay(delta: -1 | 1): void;
  moveSelection(delta: number): void;
  openOverlay(overlay: Exclude<FindPromptsOverlay, null>): void;
  openExpandedPrompt(): void;
  refresh(): void;
  resumeSelected(): Promise<void>;
  selectRow(index: number): void;
  selectedRow: FindPromptRow | undefined;
  setGroupByDay(next: boolean): void;
  setProject(next: string | null): void;
  setQuery(next: string): void;
  toggleAgent(agent: FindPromptAgent): void;
  toggleFavorite(): Promise<void>;
  toggleFullscreenPreview(): void;
  togglePreviewFocus(): void;
  toggleWrapPreview(): void;
}

export interface UseFindPromptsOptions {
  /**
   * Overrides the daemon's Accept All policy for resumes and forks. Leave it
   * undefined — which is the normal case — and gxserver applies the same
   * setting `gx f` reads, so the two surfaces cannot disagree about whether a
   * resumed agent bypasses permissions.
   */
  acceptAll?: boolean;
  transport: FindPromptsTransport;
}

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return typeof error === 'string' && error ? error : 'Unknown error';
}

export function useFindPrompts({ acceptAll, transport }: UseFindPromptsOptions): FindPromptsController {
  const [query, setQueryState] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const [agents, setAgents] = useState<ReadonlySet<FindPromptAgent>>(() => new Set());
  const [project, setProjectState] = useState<string | null>(null);
  const [groupByDay, setGroupByDayState] = useState(false);
  const [rows, setRows] = useState<readonly FindPromptRow[]>([]);
  const [windowOffset, setWindowOffset] = useState(0);
  const [matched, setMatched] = useState(0);
  const [total, setTotal] = useState(0);
  const [indexedAt, setIndexedAt] = useState(0);
  const [projectFacets, setProjectFacets] = useState<readonly FindPromptProjectFacet[]>([]);
  const [agentFacets, setAgentFacets] = useState<readonly FindPromptAgentFacet[]>([]);
  const [selection, setSelection] = useState(0);
  const [selectedText, setSelectedText] = useState<string | null>(null);
  const [overlay, setOverlay] = useState<FindPromptsOverlay>(null);
  const [previewFocused, setPreviewFocused] = useState(false);
  const [wrapPreview, setWrapPreview] = useState(true);
  const [fullscreenPreview, setFullscreenPreview] = useState(false);
  const [expandedPrompt, setExpandedPrompt] = useState(false);
  const [loading, setLoading] = useState(true);
  const [notice, setNotice] = useState<FindPromptsNotice | null>(null);
  const [refreshToken, setRefreshToken] = useState(0);

  const requestSequence = useRef(0);
  const pendingOffset = useRef(0);

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedQuery(query), FIND_PROMPTS_QUERY_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [query]);

  const filterKey = useMemo(
    () => JSON.stringify([debouncedQuery, [...agents].sort(), project, groupByDay, refreshToken]),
    [agents, debouncedQuery, groupByDay, project, refreshToken]
  );

  // Changing the query or a filter restarts at the top, exactly like the
  // terminal picker's recompute.
  useEffect(() => {
    setSelection(0);
    pendingOffset.current = 0;
  }, [filterKey]);

  const runSearch = useCallback(
    async (offset: number, refresh: boolean) => {
      const sequence = ++requestSequence.current;
      setLoading(true);
      try {
        const result = await transport.search({
          agents: [...agents],
          groupByDay,
          includeFacets: true,
          limit: FIND_PROMPTS_PAGE_SIZE,
          offset,
          project: project ?? undefined,
          query: debouncedQuery,
          refresh,
        });
        if (sequence !== requestSequence.current) {
          return;
        }
        setRows(result.rows);
        setWindowOffset(result.offset);
        setMatched(result.matched);
        setTotal(result.total);
        setIndexedAt(result.indexedAt);
        if (result.projects) {
          setProjectFacets(result.projects);
        }
        if (result.agents) {
          setAgentFacets(result.agents);
        }
        setNotice(
          result.opencodeError
            ? {
                detail: result.opencodeError,
                kind: 'info',
                message: 'opencode history could not be read.',
              }
            : null
        );
      } catch (error) {
        if (sequence === requestSequence.current) {
          setNotice({ detail: errorMessage(error), kind: 'error', message: 'Search failed.' });
        }
      } finally {
        if (sequence === requestSequence.current) {
          setLoading(false);
        }
      }
    },
    [agents, debouncedQuery, groupByDay, project, transport]
  );

  useEffect(() => {
    void runSearch(pendingOffset.current, refreshToken > 0);
    // filterKey folds every input runSearch depends on, including refreshToken.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filterKey]);

  const selectedRow = useMemo(() => {
    const local = selection - windowOffset;
    return local >= 0 && local < rows.length ? rows[local] : undefined;
  }, [rows, selection, windowOffset]);

  // Page when the selection walks off the loaded window.
  useEffect(() => {
    if (matched === 0) {
      return;
    }
    const local = selection - windowOffset;
    if (local >= 0 && local < rows.length) {
      return;
    }
    const nextOffset = Math.max(0, Math.min(matched - 1, selection) - Math.floor(FIND_PROMPTS_PAGE_SIZE / 2));
    pendingOffset.current = nextOffset;
    void runSearch(nextOffset, false);
  }, [matched, rows.length, runSearch, selection, windowOffset]);

  // The row payload is capped, so pull the selected prompt in full for the
  // preview pane. Short prompts already arrived whole and skip the round trip.
  useEffect(() => {
    if (!selectedRow) {
      setSelectedText(null);
      return;
    }
    if (!selectedRow.truncated) {
      setSelectedText(selectedRow.text);
      return;
    }
    let cancelled = false;
    setSelectedText(selectedRow.text);
    void transport
      .readText({ key: selectedRow.key })
      .then((result) => {
        if (!cancelled) {
          setSelectedText(result.text);
        }
      })
      .catch(() => {
        // The capped row text is already showing; a failed top-up is not worth
        // replacing a usable preview with an error.
      });
    return () => {
      cancelled = true;
    };
  }, [selectedRow, transport]);

  const moveSelection = useCallback(
    (delta: number) => {
      setSelection((current) => {
        const next = Math.max(0, Math.min(matched - 1, current + delta));
        return Number.isFinite(next) && next >= 0 ? next : 0;
      });
    },
    [matched]
  );

  const selectRow = useCallback(
    (index: number) => {
      setSelection(Math.max(0, Math.min(Math.max(matched - 1, 0), index)));
    },
    [matched]
  );

  /*
  Day jumps scan the loaded window. When the next boundary is outside it, the
  selection lands on the window edge, which triggers a page load; pressing again
  continues from there. That keeps the jump instant without asking the server to
  compute day boundaries across 25k ranked rows.
  */
  const jumpDay = useCallback(
    (delta: -1 | 1) => {
      const local = selection - windowOffset;
      if (local < 0 || local >= rows.length) {
        moveSelection(delta * FIND_PROMPTS_PAGE_SIZE);
        return;
      }
      const currentDay = rows[local].dayKey;
      if (delta > 0) {
        for (let i = local + 1; i < rows.length; i += 1) {
          if (rows[i].dayKey !== currentDay) {
            selectRow(windowOffset + i);
            return;
          }
        }
        selectRow(windowOffset + rows.length - 1 + 1);
        return;
      }
      for (let i = local - 1; i >= 0; i -= 1) {
        if (rows[i].dayKey !== currentDay) {
          let start = i;
          while (start > 0 && rows[start - 1].dayKey === rows[i].dayKey) {
            start -= 1;
          }
          selectRow(windowOffset + start);
          return;
        }
      }
      selectRow(Math.max(0, windowOffset - 1));
    },
    [moveSelection, rows, selectRow, selection, windowOffset]
  );

  const setQuery = useCallback((next: string) => setQueryState(next), []);

  const setProject = useCallback((next: string | null) => {
    setProjectState(next);
  }, []);

  const setGroupByDay = useCallback((next: boolean) => setGroupByDayState(next), []);

  const toggleAgent = useCallback((agent: FindPromptAgent) => {
    setAgents((current) => {
      const next = new Set(current);
      if (next.has(agent)) {
        next.delete(agent);
      } else {
        next.add(agent);
      }
      return next;
    });
  }, []);

  const refresh = useCallback(() => setRefreshToken((value) => value + 1), []);

  const toggleFavorite = useCallback(async () => {
    if (!selectedRow) {
      return;
    }
    const key = selectedRow.key;
    const nextFavorite = !selectedRow.favorite;
    // Paint immediately; the list re-ranks on the next search because favorites
    // form a tier above every score.
    setRows((current) => current.map((row) => (row.key === key ? { ...row, favorite: nextFavorite } : row)));
    try {
      await transport.toggleFavorite({ favorite: nextFavorite, key });
    } catch (error) {
      setRows((current) => current.map((row) => (row.key === key ? { ...row, favorite: !nextFavorite } : row)));
      setNotice({
        detail: errorMessage(error),
        kind: 'error',
        message: 'Could not update the favorite.',
      });
    }
  }, [selectedRow, transport]);

  const applyLaunchPlan = useCallback(
    async (plan: ResolveAgentPromptLaunchResult) => {
      if (plan.mode === 'focus') {
        if (!transport.focusSession) {
          setNotice({
            kind: 'info',
            message: 'That conversation is already open in Ghostex.',
          });
          return;
        }
        await transport.focusSession({ projectId: plan.projectId, sessionId: plan.sessionId });
        transport.close?.();
        return;
      }
      if (!transport.launchSession) {
        setNotice({
          detail: plan.commandLine,
          kind: 'info',
          message: 'This surface cannot open sessions; run the command yourself.',
        });
        return;
      }
      await transport.launchSession(plan);
      transport.close?.();
    },
    [transport]
  );

  const resumeSelected = useCallback(async () => {
    if (!selectedRow) {
      return;
    }
    try {
      const plan = await transport.resolveLaunch({
        action: 'resume',
        key: selectedRow.key,
        ...(acceptAll === undefined ? {} : { acceptAll }),
      });
      await applyLaunchPlan(plan);
    } catch (error) {
      setNotice({ detail: errorMessage(error), kind: 'error', message: 'Could not resume.' });
    }
  }, [acceptAll, applyLaunchPlan, selectedRow, transport]);

  const forkSelected = useCallback(
    async (agent: FindPromptAgent) => {
      if (!selectedRow) {
        return;
      }
      setOverlay(null);
      try {
        const plan = await transport.resolveLaunch({
          action: 'fork',
          forkAgent: agent,
          key: selectedRow.key,
          ...(acceptAll === undefined ? {} : { acceptAll }),
        });
        await applyLaunchPlan(plan);
      } catch (error) {
        setNotice({ detail: errorMessage(error), kind: 'error', message: 'Could not fork.' });
      }
    },
    [acceptAll, applyLaunchPlan, selectedRow, transport]
  );

  const copySelected = useCallback(async () => {
    const text = selectedText ?? selectedRow?.text;
    if (!text) {
      return;
    }
    if (!transport.copyText) {
      setNotice({ kind: 'info', message: 'This surface has no clipboard access.' });
      return;
    }
    try {
      await transport.copyText(text);
      setNotice({ kind: 'info', message: 'Prompt copied to the clipboard.' });
    } catch (error) {
      setNotice({ detail: errorMessage(error), kind: 'error', message: 'Could not copy.' });
    }
  }, [selectedRow, selectedText, transport]);

  return {
    agentFacets,
    agents,
    cancelOverlay: useCallback(() => setOverlay(null), []),
    closeExpandedPrompt: useCallback(() => setExpandedPrompt(false), []),
    copySelected,
    expandedPrompt,
    forkSelected,
    fullscreenPreview,
    groupByDay,
    indexedAt,
    jumpDay,
    loading,
    matched,
    moveSelection,
    notice,
    openExpandedPrompt: useCallback(() => setExpandedPrompt(true), []),
    openOverlay: useCallback((next: Exclude<FindPromptsOverlay, null>) => setOverlay(next), []),
    overlay,
    previewFocused,
    project,
    projectFacets,
    query,
    refresh,
    resumeSelected,
    rows,
    selectRow,
    selectedRow,
    selectedText,
    selection,
    setGroupByDay,
    setProject,
    setQuery,
    toggleAgent,
    toggleFavorite,
    toggleFullscreenPreview: useCallback(() => setFullscreenPreview((value) => !value), []),
    togglePreviewFocus: useCallback(() => setPreviewFocused((value) => !value), []),
    toggleWrapPreview: useCallback(() => setWrapPreview((value) => !value), []),
    total,
    windowOffset,
    wrapPreview,
  };
}
