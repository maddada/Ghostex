/*
CDXC:AgentHistorySearch 2026-08-20:
The Find surface — a GUI for `gx f`. Item placement follows the terminal picker
(query and hint strip on top, two-line results in the middle, the selected
prompt and its metadata at the bottom), while the type, color, spacing, and
radii come from the Session Chat surface's tokens.

The query input keeps DOM focus for the whole session so every hotkey resolves
through one handler; rows and overlays select on mousedown with the default
prevented rather than taking focus.
*/

import { IconLoader2 } from "@tabler/icons-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  FindPromptAgent,
  FindPromptRow,
} from "../../shared/agent-prompt-search";
import { cn } from "@/packages/components/utils";
import { FIND_PROMPT_AGENTS } from "../../shared/agent-prompt-search";
import { FindPromptResultRow } from "./find-prompt-row";
import {
  formatDayHeader,
  formatLastActiveCompact,
  formatLastActiveFull,
  formatPromptMetaLine,
} from "./find-prompts-format";
import {
  FIND_PROMPTS_HINTS,
  resolveFindPromptsAction,
  type FindPromptsAction,
  type FindPromptsHintAction,
  type FindPromptsMode,
} from "./find-prompts-hotkeys";
import {
  FindAgentFilterOverlay,
  FindForkOverlay,
  FindProjectFilterOverlay,
  filterProjectFacets,
  useOverlayCursor,
} from "./find-prompts-overlays";
import type { FindPromptsTransport } from "./find-prompts-transport";
import { useFindPrompts } from "./use-find-prompts";

export interface FindPromptsViewProps {
  /**
   * Overrides the daemon's Accept All policy. Hosts normally omit it and let
   * gxserver apply the same setting `gx f` reads.
   */
  acceptAll?: boolean;
  /** Rendered at the top-right, for host chrome such as a close button. */
  hostActions?: React.ReactNode;
  transport: FindPromptsTransport;
}

type ViewRow =
  /*
  `position` is carried on day headers only to key them. Toggling `^d` flips
  grouping a render before the re-sorted rows arrive, so for one frame headers
  are derived from ungrouped rows and the same day can appear twice — with a
  bare `day-<dayKey>` key that is a duplicate-key collision, and React answers
  those by dropping and duplicating siblings, which corrupted the list for good.
  */
  | { dayKey: number; position: number; type: "day" }
  | { position: number; row: FindPromptRow; type: "row" };

function buildViewRows(
  rows: readonly FindPromptRow[],
  windowOffset: number,
  groupByDay: boolean,
): ViewRow[] {
  if (!groupByDay) {
    return rows.map((row, position) => ({ position: windowOffset + position, row, type: "row" }));
  }
  const out: ViewRow[] = [];
  let lastDay: number | null = null;
  rows.forEach((row, position) => {
    if (lastDay === null || lastDay !== row.dayKey) {
      out.push({ dayKey: row.dayKey, position: windowOffset + position, type: "day" });
      lastDay = row.dayKey;
    }
    out.push({ position: windowOffset + position, row, type: "row" });
  });
  return out;
}

export function FindPromptsView({ acceptAll, hostActions, transport }: FindPromptsViewProps) {
  const find = useFindPrompts({ acceptAll, transport });
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const selectedRef = useRef<HTMLDivElement | null>(null);
  const [projectFilter, setProjectFilter] = useState("");
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  // Relative labels ("6m ago") go stale while the surface sits open.
  useEffect(() => {
    const timer = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 30_000);
    return () => clearInterval(timer);
  }, []);

  const agentColors = useMemo(() => {
    const colors: Record<string, string> = {};
    for (const facet of find.agentFacets) {
      colors[facet.agent] = facet.color;
    }
    return colors;
  }, [find.agentFacets]);

  const visibleProjects = useMemo(
    () => filterProjectFacets(find.projectFacets, projectFilter),
    [find.projectFacets, projectFilter],
  );

  const agentCursor = useOverlayCursor(FIND_PROMPT_AGENTS.length);
  const projectCursor = useOverlayCursor(visibleProjects.length);

  const mode: FindPromptsMode = find.overlay
    ? find.overlay === "agent"
      ? "agentPicker"
      : find.overlay === "project"
        ? "projectPicker"
        : "forkPicker"
    : find.previewFocused
      ? "preview"
      : "list";

  useEffect(() => {
    if (find.overlay === null) {
      setProjectFilter("");
      inputRef.current?.focus();
    }
  }, [find.overlay]);

  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: "nearest" });
  }, [find.selection, find.rows]);

  const editQuery = useCallback(
    (transform: (value: string, caret: number) => { caret: number; value: string }) => {
      const input = inputRef.current;
      const caret = input?.selectionStart ?? find.query.length;
      const next = transform(find.query, caret);
      find.setQuery(next.value);
      requestAnimationFrame(() => {
        inputRef.current?.setSelectionRange(next.caret, next.caret);
      });
    },
    [find],
  );

  const runAction = useCallback(
    (action: FindPromptsAction) => {
      switch (action.type) {
        case "move":
          if (mode === "agentPicker") {
            agentCursor.move(action.delta);
          } else if (mode === "projectPicker") {
            projectCursor.move(action.delta);
          } else {
            find.moveSelection(action.delta);
          }
          break;
        case "jumpDay":
          find.jumpDay(action.delta);
          break;
        case "scrollPreview": {
          const pane = document.querySelector<HTMLElement>("[data-find-preview]");
          pane?.scrollBy({ top: action.delta * pane.clientHeight * 0.9 });
          break;
        }
        case "resumePrompt":
          void find.resumeSelected();
          break;
        case "close":
          transport.close?.();
          break;
        case "toggleDayGrouping":
          find.setGroupByDay(!find.groupByDay);
          break;
        case "openAgentPicker":
          if (find.overlay === "agent") {
            find.cancelOverlay();
          } else {
            find.openOverlay("agent");
          }
          break;
        case "openProjectPicker":
          if (find.overlay === "project") {
            find.cancelOverlay();
          } else {
            find.openOverlay("project");
          }
          break;
        case "toggleFavorite":
          void find.toggleFavorite();
          break;
        case "viewPrompt":
          if (find.expandedPrompt) {
            find.closeExpandedPrompt();
          } else {
            find.openExpandedPrompt();
          }
          break;
        case "copyPrompt":
          void find.copySelected();
          break;
        case "forkPicker":
          if (find.overlay === "fork") {
            find.cancelOverlay();
          } else if (find.selectedRow) {
            find.openOverlay("fork");
          }
          break;
        case "togglePreviewFocus":
          find.togglePreviewFocus();
          break;
        case "toggleWrap":
          find.toggleWrapPreview();
          break;
        case "toggleFullscreenPreview":
          find.toggleFullscreenPreview();
          break;
        case "cancelOverlay":
          find.cancelOverlay();
          break;
        case "togglePickerSelection":
          if (mode === "agentPicker") {
            find.toggleAgent(FIND_PROMPT_AGENTS[agentCursor.cursor]);
          } else if (mode === "projectPicker") {
            const facet = visibleProjects[projectCursor.cursor];
            find.setProject(facet && find.project !== facet.path ? facet.path : null);
            find.cancelOverlay();
          }
          break;
        case "pickIndex":
          if (mode === "forkPicker") {
            void find.forkSelected(FIND_PROMPT_AGENTS[action.index]);
          } else if (mode === "agentPicker") {
            agentCursor.setCursor(action.index);
            find.toggleAgent(FIND_PROMPT_AGENTS[action.index]);
          }
          break;
        case "killToEnd":
          editQuery((value, caret) => ({ caret, value: value.slice(0, caret) }));
          break;
        case "killToStart":
          editQuery((value, caret) => ({ caret: 0, value: value.slice(caret) }));
          break;
        case "deleteWordBackward":
          editQuery((value, caret) => {
            const head = value.slice(0, caret).replace(/[^\p{L}\p{N}_]*[\p{L}\p{N}_]*$/u, "");
            return { caret: head.length, value: head + value.slice(caret) };
          });
          break;
        case "deleteWordForward":
          editQuery((value, caret) => {
            const tail = value.slice(caret).replace(/^[^\p{L}\p{N}_]*[\p{L}\p{N}_]*/u, "");
            return { caret, value: value.slice(0, caret) + tail };
          });
          break;
        default:
          break;
      }
    },
    [agentCursor, editQuery, find, mode, projectCursor, transport, visibleProjects],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (find.expandedPrompt) {
        if (event.key === "Escape") {
          event.preventDefault();
          find.closeExpandedPrompt();
        }
        return;
      }
      const action = resolveFindPromptsAction(event, mode);
      if (!action) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      runAction(action);
    },
    [find, mode, runAction],
  );

  const viewRows = useMemo(
    () => buildViewRows(find.rows, find.windowOffset, find.groupByDay),
    [find.groupByDay, find.rows, find.windowOffset],
  );

  const selectedRow = find.selectedRow;
  const previewText = find.selectedText ?? selectedRow?.text ?? "";
  const metaLine = selectedRow ? formatPromptMetaLine(selectedRow.meta) : "";

  const hintState = useCallback(
    (action: FindPromptsHintAction) => {
      switch (action) {
        case "toggleDayGrouping":
          return { active: find.groupByDay, disabled: false };
        case "openAgentPicker":
          return { active: find.overlay === "agent" || find.agents.size > 0, disabled: false };
        case "openProjectPicker":
          return { active: find.overlay === "project" || find.project !== null, disabled: false };
        case "toggleFavorite":
          return { active: selectedRow?.favorite === true, disabled: !selectedRow };
        case "viewPrompt":
          return { active: find.expandedPrompt, disabled: !selectedRow };
        case "forkPicker":
          return { active: find.overlay === "fork", disabled: !selectedRow };
        case "copyPrompt":
          return { active: false, disabled: !selectedRow };
      }
    },
    [find, selectedRow],
  );

  return (
    <div
      className="ghostex-find-scope relative flex h-full min-h-0 flex-col bg-background text-foreground [--radius:0.625rem]"
      onKeyDown={handleKeyDown}
    >
      {/* Query row: input on the left, counter and hint keys on the right. */}
      <div className="flex shrink-0 items-center gap-3 border-b border-border/60 px-3 py-2">
        <span aria-hidden="true" className="text-[13px] text-primary">
          ❯
        </span>
        <input
          aria-label="Search previous prompts"
          autoFocus
          className="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-muted-foreground"
          onChange={(event) => find.setQuery(event.target.value)}
          placeholder="Search every prompt you have sent to an agent"
          ref={inputRef}
          spellCheck={false}
          type="text"
          value={find.query}
        />
        {find.loading ? (
          <IconLoader2 aria-label="Searching" className="size-3.5 animate-spin text-muted-foreground" />
        ) : null}
        <span className="shrink-0 tabular-nums text-[11px] text-muted-foreground">
          {find.matched}/{find.total}
        </span>
        <div className="hidden shrink-0 items-center gap-0.5 text-[11px] md:flex">
          {FIND_PROMPTS_HINTS.map((hint) => {
            const state = hintState(hint.action);
            return (
              <button
                aria-pressed={state.active}
                className={cn(
                  "rounded-md px-1.5 py-1 text-muted-foreground transition-colors",
                  "hover:bg-accent/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                  state.active && "bg-accent text-foreground",
                  state.disabled && "pointer-events-none opacity-40",
                )}
                disabled={state.disabled}
                key={hint.key}
                onClick={() => runAction({ type: hint.action })}
                onKeyDown={(event) => event.stopPropagation()}
                onMouseDown={(event) => event.preventDefault()}
                title={`${hint.label} (${hint.key})`}
                type="button"
              >
                <span className="font-medium text-foreground/70">{hint.key}</span> {hint.label}
              </button>
            );
          })}
        </div>
        {hostActions}
      </div>

      {/* Results */}
      <div
        className={cn(
          "min-h-0 flex-1 overflow-y-auto scrollbar-thin px-2 py-1.5",
          find.fullscreenPreview && "hidden",
        )}
        ref={listRef}
        role="listbox"
        tabIndex={-1}
      >
        {viewRows.length === 0 && !find.loading ? (
          <div className="px-2 py-6 text-center text-[13px] text-muted-foreground">
            {find.total === 0
              ? "No agent prompt history was found on this machine."
              : "No prompts match this search."}
          </div>
        ) : null}
        {viewRows.map((viewRow) =>
          viewRow.type === "day" ? (
            <div
              className="px-2 pb-1 pt-3 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground"
              key={`day-${viewRow.position}-${viewRow.dayKey}`}
            >
              {formatDayHeader(viewRow.dayKey, now)}
            </div>
          ) : (
            <div
              key={`${viewRow.row.key}-${viewRow.position}`}
              ref={viewRow.position === find.selection ? selectedRef : undefined}
            >
              <FindPromptResultRow
                onSelect={() => find.selectRow(viewRow.position)}
                row={viewRow.row}
                selected={viewRow.position === find.selection}
                timeLabel={formatLastActiveCompact(viewRow.row.ts, now)}
              />
            </div>
          ),
        )}
      </div>

      {/* Bottom pane: overlays take it over, otherwise the selected prompt. */}
      <div
        className={cn(
          "flex shrink-0 flex-col border-t border-border/60",
          find.fullscreenPreview ? "min-h-0 flex-1" : "h-56",
        )}
      >
        {find.overlay === "agent" ? (
          <FindAgentFilterOverlay
            colors={agentColors}
            cursor={agentCursor.cursor}
            onToggle={find.toggleAgent}
            selected={find.agents}
          />
        ) : find.overlay === "project" ? (
          <FindProjectFilterOverlay
            cursor={projectCursor.cursor}
            filter={projectFilter}
            onFilterChange={setProjectFilter}
            onSelect={(path) => {
              find.setProject(path);
              find.cancelOverlay();
            }}
            projects={visibleProjects}
            selected={find.project}
          />
        ) : find.overlay === "fork" ? (
          <FindForkOverlay colors={agentColors} onPick={(agent) => void find.forkSelected(agent)} />
        ) : (
          <>
            <div className="flex shrink-0 items-baseline gap-2 px-3 pb-1 pt-2 text-[11px] text-muted-foreground">
              <span className="min-w-0 flex-1 truncate">
                {selectedRow?.project || "—"}
              </span>
              <span className="shrink-0 tabular-nums">
                {find.matched === 0 ? 0 : find.selection + 1}/{find.matched}
              </span>
              {find.agents.size > 0 ? (
                <span className="shrink-0">agents: {[...find.agents].join(",")}</span>
              ) : null}
              {find.project ? <span className="shrink-0">project filter on</span> : null}
            </div>
            <div
              className={cn(
                "min-h-0 flex-1 overflow-auto scrollbar-thin px-3 text-[13px] leading-5",
                find.wrapPreview ? "whitespace-pre-wrap break-words" : "whitespace-pre",
                find.previewFocused && "ring-1 ring-inset ring-border/70",
              )}
              data-find-preview="true"
            >
              {previewText}
            </div>
            <div className="flex shrink-0 items-baseline gap-2 px-3 pb-2 pt-1 text-[11px] text-muted-foreground">
              <span className="truncate">
                {selectedRow ? formatLastActiveFull(selectedRow.ts) : ""}
                {metaLine ? ` ${metaLine}` : ""}
              </span>
            </div>
          </>
        )}
      </div>

      {find.notice ? (
        <div
          className={cn(
            "shrink-0 border-t px-3 py-1.5 text-[11px]",
            find.notice.kind === "error"
              ? "border-destructive/40 bg-destructive/10 text-destructive-foreground"
              : "border-border/60 bg-accent/30 text-muted-foreground",
          )}
          role="status"
        >
          {find.notice.message}
          {find.notice.detail ? <span className="opacity-70"> {find.notice.detail}</span> : null}
        </div>
      ) : null}

      {/* `^e` — the whole prompt, scrollable and selectable. */}
      {find.expandedPrompt && selectedRow ? (
        <div
          className="absolute inset-0 z-20 flex flex-col bg-background/95 backdrop-blur-sm"
          role="dialog"
        >
          <div className="flex shrink-0 items-center gap-2 border-b border-border/60 px-3 py-2 text-[11px] text-muted-foreground">
            <span
              className="font-medium"
              style={{ color: selectedRow.agentColor }}
            >
              {selectedRow.agent}
            </span>
            <span className="min-w-0 flex-1 truncate">{selectedRow.title}</span>
            <button
              className="rounded-md px-2 py-0.5 hover:bg-accent/60"
              onMouseDown={(event) => {
                event.preventDefault();
                find.closeExpandedPrompt();
              }}
              type="button"
            >
              Close
            </button>
          </div>
          <div className="min-h-0 flex-1 select-text overflow-auto scrollbar-thin whitespace-pre-wrap break-words px-4 py-3 text-[13px] leading-6">
            {previewText}
          </div>
        </div>
      ) : null}
    </div>
  );
}

export type { FindPromptAgent };
