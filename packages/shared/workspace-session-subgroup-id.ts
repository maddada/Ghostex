/**
 * The sidebar id of a user-made session group inside a project: `gpui-wsg:<encoded project id>:<group id>`.
 *
 * The Rust store builds the same ids (packages/gx-core/src/keys.rs, `encode_workspace_subgroup_id`
 * and `parse_workspace_subgroup_id`); this copy is for the TypeScript readers of the sidebar's
 * shape (apps/session-tui). It moved here from the desktop's deleted
 * `apps/desktop/sidebar/workspace-session-groups.ts` unchanged.
 */
const GPUI_WORKSPACE_SESSION_SUBGROUP_ID_PREFIX = 'gpui-wsg:';

export function createGpuiWorkspaceSessionSubgroupId(projectId: string, groupId: string): string {
  return `${GPUI_WORKSPACE_SESSION_SUBGROUP_ID_PREFIX}${encodeURIComponent(projectId)}:${groupId}`;
}

export function parseGpuiWorkspaceSessionSubgroupId(value: string): { groupId: string; projectId: string } | undefined {
  if (!value.startsWith(GPUI_WORKSPACE_SESSION_SUBGROUP_ID_PREFIX)) {
    return undefined;
  }
  const rest = value.slice(GPUI_WORKSPACE_SESSION_SUBGROUP_ID_PREFIX.length);
  const separator = rest.indexOf(':');
  if (separator <= 0 || separator === rest.length - 1) {
    return undefined;
  }
  try {
    return {
      groupId: rest.slice(separator + 1),
      projectId: decodeURIComponent(rest.slice(0, separator)),
    };
  } catch {
    return undefined;
  }
}
