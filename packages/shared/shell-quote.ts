/**
 * CDXC:SessionRestore 2026-05-20-09:00:
 * Agent resume and fork commands must wrap titles and session ids in double quotes
 * so spaces and shell metacharacters do not break restore. Ghostex echoes the same
 * quoted form before running restore commands so users see the exact argument shape.
 */
export function quoteShellDoubleArg(value: string): string {
  return `"${value
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"')
    .replace(/\$/g, "\\$")
    .replace(/`/g, "\\`")}"`;
}
