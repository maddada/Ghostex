import type { SidebarPreviousSessionItem } from "../shared/session-grid-contract";

export function getSessionHistoryCardTitle(
  session: Pick<SidebarPreviousSessionItem, "alias" | "displayTitle" | "primaryTitle" | "terminalTitle">,
): string {
  const displayTitle = session.displayTitle?.trim();
  if (displayTitle) {
    return displayTitle;
  }

  const primaryTitle = session.primaryTitle?.trim();
  if (primaryTitle) {
    return primaryTitle;
  }

  const terminalTitle = session.terminalTitle?.trim();
  if (terminalTitle) {
    return terminalTitle;
  }

  return session.alias;
}
