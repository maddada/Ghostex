import { describe, expect, test } from "vitest";
import { createAgentsHubExternalEditorCommand } from "./agents-hub-editor-command";

describe("createAgentsHubExternalEditorCommand", () => {
  test("opens VS Code-compatible editors on the containing folder with the file focused", () => {
    /**
     * CDXC:AgentsHub 2026-05-16-07:23:
     * The Agents Hub Code button should launch VS Code-style editors with the file's folder as the workspace and the selected file focused via --goto.
     */
    expect(
      createAgentsHubExternalEditorCommand({
        defaultEditorCommand: "code",
        editorCommand: "code",
        filePath: "/Users/madda/.agents/main.md",
      }),
    ).toBe("code --reuse-window '/Users/madda/.agents' --goto '/Users/madda/.agents/main.md:1:1'");
    expect(
      createAgentsHubExternalEditorCommand({
        defaultEditorCommand: "other",
        editorCommand: "'/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code'",
        filePath: "/Users/madda/.agents/main.md",
      }),
    ).toBe(
      "'/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code' --reuse-window '/Users/madda/.agents' --goto '/Users/madda/.agents/main.md:1:1'",
    );
  });

  test("opens Zed on the containing folder with the file focused", () => {
    expect(
      createAgentsHubExternalEditorCommand({
        defaultEditorCommand: "zed",
        editorCommand: "zed",
        filePath: "/Users/madda/agents/skills/example/SKILL.md",
      }),
    ).toBe(
      "zed --existing '/Users/madda/agents/skills/example' '/Users/madda/agents/skills/example/SKILL.md:1:1'",
    );
    expect(
      createAgentsHubExternalEditorCommand({
        defaultEditorCommand: "zeditor",
        editorCommand: "zeditor",
        filePath: "/Users/madda/.agents/main.md",
      }),
    ).toBe("zeditor --existing '/Users/madda/.agents' '/Users/madda/.agents/main.md:1:1'");
  });

  test("passes both folder and file to other editor commands", () => {
    expect(
      createAgentsHubExternalEditorCommand({
        defaultEditorCommand: "subl",
        editorCommand: "subl",
        filePath: "/Users/madda/agents/skills/example/SKILL.md",
      }),
    ).toBe("subl '/Users/madda/agents/skills/example' '/Users/madda/agents/skills/example/SKILL.md'");
  });
});
