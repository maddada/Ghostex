const MAX_SIDEBAR_BUTTON_GRID_COLUMNS = 5;

export function getSidebarButtonGridColumnCount(buttonCount: number): number {
  return Math.min(Math.max(buttonCount, 1), MAX_SIDEBAR_BUTTON_GRID_COLUMNS);
}
