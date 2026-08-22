export function logSidebarDebug(
  enabled: boolean | undefined,
  _event: string,
  _payload?: unknown,
): void {
  if (!enabled) {
    return;
  }
}
