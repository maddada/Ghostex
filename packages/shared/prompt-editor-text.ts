/**
 * CDXC:PromptEditor 2026-05-28-07:47:
 * Opening the Monaco rich prompt editor and pasting plain text into it should
 * remove spaces at line ends so prompt buffers do not accumulate invisible
 * trailing whitespace from shell capture or clipboard content.
 *
 * CDXC:PromptEditor 2026-05-28-07:47:
 * Prompt-like text surfaces should share the same trailing-space rule for
 * pasted and submitted text: remove spaces and tabs at line ends while
 * preserving indentation, internal spaces, blank lines, and line endings.
 */
export function trimPromptEditorTrailingSpaces(text: string): string {
  return text.replace(/[ \t]+(?=\r\n|\n|\r|$)/g, "");
}
