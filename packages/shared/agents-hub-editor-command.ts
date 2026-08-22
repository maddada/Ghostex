import type { DefaultEditorCommand } from "./ghostex-settings";

const vscodeCompatibleEditorCommands = new Set(["code", "code-insiders", "codium", "cursor", "windsurf"]);
const zedCompatibleEditorCommands = new Set(["zed", "zeditor"]);

export function createAgentsHubExternalEditorCommand({
  defaultEditorCommand,
  editorCommand,
  filePath,
}: {
  defaultEditorCommand?: DefaultEditorCommand;
  editorCommand: string;
  filePath: string;
}): string {
  const normalizedFilePath = filePath.trim();
  const folderPath = dirnameAgentsHubPath(normalizedFilePath);
  const editorCliName = getEditorCliName(editorCommand);

  if (isEditorCommandMatch(defaultEditorCommand, editorCliName, vscodeCompatibleEditorCommands)) {
    /**
     * CDXC:AgentsHub 2026-05-16-07:23:
     * The Hub's editor button should open the selected file's containing folder as the VS Code-style workspace and focus the selected file when the CLI supports it.
     * Pass both the folder and --goto file target so the file tree is rooted at the instructions/config folder instead of opening only a single loose file window.
     */
    return [
      editorCommand,
      "--reuse-window",
      quoteAgentsHubShellArg(folderPath),
      "--goto",
      quoteAgentsHubShellArg(`${normalizedFilePath}:1:1`),
    ].join(" ");
  }

  if (isEditorCommandMatch(defaultEditorCommand, editorCliName, zedCompatibleEditorCommands)) {
    /**
     * CDXC:AgentsHub 2026-05-16-07:26:
     * The Hub's editor button must respect the default IDE selected in Settings, including Zed and its zeditor alias.
     * Zed focuses files via path:line:column arguments, so open the containing folder as the workspace path and the selected file as a positioned path in the existing Zed window when possible.
     */
    return [
      editorCommand,
      "--existing",
      quoteAgentsHubShellArg(folderPath),
      quoteAgentsHubShellArg(`${normalizedFilePath}:1:1`),
    ].join(" ");
  }

  return [
    editorCommand,
    quoteAgentsHubShellArg(folderPath),
    quoteAgentsHubShellArg(normalizedFilePath),
  ].join(" ");
}

function isEditorCommandMatch(
  defaultEditorCommand: DefaultEditorCommand | undefined,
  editorCliName: string | undefined,
  compatibleCommands: Set<string>,
): boolean {
  return Boolean(
    (defaultEditorCommand && compatibleCommands.has(defaultEditorCommand)) ||
      (editorCliName && compatibleCommands.has(editorCliName)),
  );
}

function dirnameAgentsHubPath(path: string): string {
  const index = path.lastIndexOf("/");
  return index > 0 ? path.slice(0, index) : ".";
}

function getEditorCliName(editorCommand: string): string | undefined {
  const executable = editorCommand.trim().match(/^(?:"([^"]+)"|'([^']+)'|(\S+))/)?.slice(1).find(Boolean);
  return executable?.split("/").filter(Boolean).at(-1);
}

function quoteAgentsHubShellArg(value: string): string {
  return `'${value.replace(/'/g, "'\\''")}'`;
}
