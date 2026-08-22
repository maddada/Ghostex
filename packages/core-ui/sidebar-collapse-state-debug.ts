export const SIDEBAR_COLLAPSE_STATE_DEBUG_EVENT_PREFIX = "sidebar.collapseState.";

export function hashSidebarCollapseDebugId(value: string): string {
  /*
   * CDXC:SidebarCollapseDiagnostics 2026-06-02-23:52:
   * Collapse-state diagnostics need to correlate the same sidebar group across
   * read/write/toggle events without writing project names, workspace paths,
   * browser URLs, command text, or secrets into persistent support logs.
   */
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

export function summarizeSidebarCollapseDebugGroupIds(groupIds: readonly string[]): string[] {
  return groupIds.slice(0, 20).map(hashSidebarCollapseDebugId);
}
