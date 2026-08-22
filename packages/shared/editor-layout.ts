import type { TerminalViewMode } from "./session-grid-contract";

export type EditorLayoutOrientation = 0 | 1;

export type EditorLayoutGroup = {
  groups?: EditorLayoutGroup[];
  orientation?: EditorLayoutOrientation;
  size?: number;
};

export type EditorLayout = {
  groups: EditorLayoutGroup[];
  orientation: EditorLayoutOrientation;
};

export type EditorLayoutPlan = {
  layout: EditorLayout;
  rowLengths: number[];
};

export function createEditorLayoutPlan(
  visibleCount: number,
  viewMode: TerminalViewMode,
): EditorLayoutPlan {
  const normalizedCount = clampVisibleCount(visibleCount);

  if (viewMode === "horizontal") {
    return {
      layout: {
        groups: createLeafGroups(normalizedCount),
        orientation: 0,
      },
      rowLengths: Array.from({ length: normalizedCount }, () => 1),
    };
  }

  if (viewMode === "vertical") {
    return {
      layout: {
        groups: createLeafGroups(normalizedCount),
        orientation: 1,
      },
      rowLengths: [normalizedCount],
    };
  }

  const rowLengths = createGridRowLengths(normalizedCount);
  if (rowLengths.length === 1) {
    return {
      layout: {
        groups: createLeafGroups(rowLengths[0]),
        orientation: 0,
      },
      rowLengths,
    };
  }

  return {
    layout: {
      groups: rowLengths.map((rowLength) => ({
        groups: createLeafGroups(rowLength),
        orientation: 0,
      })),
      orientation: 1,
    },
    rowLengths,
  };
}

function clampVisibleCount(value: number): number {
  /**
   * CDXC:EditorLayout 2026-05-11-17:14
   * Editor/workspace layout planning must accept every visible session count.
   * The old fixed pane cap was removed from workspace panes, so this helper only
   * normalizes invalid counts to a positive integer.
   */
  return Number.isFinite(value) ? Math.max(1, Math.floor(value)) : 1;
}

function createGridRowLengths(visibleCount: number): number[] {
  if (visibleCount === 3) {
    return [2, 1];
  }

  const rowCount = Math.ceil(visibleCount / 3);
  const baseRowLength = Math.floor(visibleCount / rowCount);
  const remainder = visibleCount % rowCount;

  return Array.from({ length: rowCount }, (_, index) => {
    return baseRowLength + (index < remainder ? 1 : 0);
  });
}

function createLeafGroups(count: number): EditorLayoutGroup[] {
  return Array.from({ length: count }, () => ({}));
}
