/*
 * CDXC:ProjectBoard 2026-08-24:
 * The Kanban "View" menu lets the user hide every card detail except the
 * title. The preference is app-wide (one look for every project's board) and
 * survives restarts, so it lives in the CEF app-UI profile's localStorage
 * rather than in per-project board state.
 */
export type BoardCardViewOptions = {
  showId: boolean;
  showAssignee: boolean;
  showPriority: boolean;
  showDescription: boolean;
  showLabels: boolean;
  showDetails: boolean;
  showLinks: boolean;
};

export const BOARD_CARD_VIEW_DEFAULTS: BoardCardViewOptions = {
  showId: true,
  showAssignee: true,
  showPriority: true,
  /* Descriptions are opt-in: cards stay short by default. */
  showDescription: false,
  showLabels: true,
  showDetails: true,
  showLinks: true,
};

export const BOARD_CARD_VIEW_FIELDS: Array<{
  key: keyof BoardCardViewOptions;
  label: string;
}> = [
  { key: 'showId', label: 'Ticket ID' },
  { key: 'showPriority', label: 'Priority' },
  { key: 'showAssignee', label: 'Assignee' },
  { key: 'showDescription', label: 'Description' },
  { key: 'showLabels', label: 'Labels' },
  { key: 'showDetails', label: 'Details' },
  { key: 'showLinks', label: 'Conversation links' },
];

export const BOARD_CARD_VIEW_STORAGE_KEY = 'ghostexProjectBoardCardView.v1';

export function loadBoardCardViewOptions(): BoardCardViewOptions {
  try {
    const raw = window.localStorage.getItem(BOARD_CARD_VIEW_STORAGE_KEY);
    if (!raw) {
      return { ...BOARD_CARD_VIEW_DEFAULTS };
    }
    const parsed = JSON.parse(raw) as Partial<BoardCardViewOptions>;
    const merged = { ...BOARD_CARD_VIEW_DEFAULTS };
    for (const field of BOARD_CARD_VIEW_FIELDS) {
      if (typeof parsed[field.key] === 'boolean') {
        merged[field.key] = parsed[field.key] as boolean;
      }
    }
    return merged;
  } catch {
    return { ...BOARD_CARD_VIEW_DEFAULTS };
  }
}

export function saveBoardCardViewOptions(options: BoardCardViewOptions): void {
  try {
    window.localStorage.setItem(BOARD_CARD_VIEW_STORAGE_KEY, JSON.stringify(options));
  } catch {
    // Persistence is best-effort; the in-memory state still applies.
  }
}
