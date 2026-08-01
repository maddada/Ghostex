import { describe, expect, test } from "vite-plus/test";
import {
  createSidebarCommandButtons,
  getFirstBrowserSidebarCommandUrl,
  getSidebarCommandPreviewLabel,
  normalizeStoredSidebarCommandOrder,
  normalizeStoredSidebarCommands,
  SIDEBAR_UNCONFIGURED_TERMINAL_COMMAND_LABEL,
} from "./sidebar-commands";

describe("createSidebarCommandButtons", () => {
  test("should expose the default terminal action slots when no actions are configured", () => {
    expect(createSidebarCommandButtons([])).toEqual([
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "dev",
        isDefault: true,
        name: "Dev",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "build",
        isDefault: true,
        name: "Build",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "test",
        isDefault: true,
        name: "Test",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "setup",
        isDefault: true,
        name: "Setup",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
    ]);
  });

  test("should merge configured defaults and append custom terminal and browser actions", () => {
    expect(
      createSidebarCommandButtons([
        {
          actionType: "terminal",
          closeTerminalOnExit: false,
          command: "vp dev",
          commandId: "dev",
          isDefault: true,
          name: "App",
          playCompletionSound: true,
          showOnProjectRow: false,
        },
        {
          actionType: "browser",
          closeTerminalOnExit: false,
          commandId: "custom-docs",
          isDefault: false,
          name: "Docs",
          playCompletionSound: false,
          showOnProjectRow: false,
          url: "https://example.com/docs",
        },
      ]),
    ).toEqual([
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: "vp dev",
        commandId: "dev",
        isDefault: true,
        name: "App",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "build",
        isDefault: true,
        name: "Build",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "test",
        isDefault: true,
        name: "Test",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "setup",
        isDefault: true,
        name: "Setup",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
      {
        actionType: "browser",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "custom-docs",
        isDefault: false,
        name: "Docs",
        playCompletionSound: false,
        showOnProjectRow: false,
        url: "https://example.com/docs",
      },
    ]);
  });

  test("should preserve configured icon metadata for custom actions", () => {
    expect(
      createSidebarCommandButtons([
        {
          actionType: "terminal",
          closeTerminalOnExit: false,
          command: "pnpm test",
          commandId: "custom-tests",
          icon: "bug",
          iconColor: "#92b4ff",
          isDefault: false,
          name: "",
          playCompletionSound: true,
          showOnProjectRow: false,
        },
      ]),
    ).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          commandId: "custom-tests",
          icon: "bug",
          name: "",
          playCompletionSound: true,
          showOnProjectRow: false,
        }),
      ]),
    );
  });

  test("should respect a stored action order for defaults and custom actions", () => {
    expect(
      createSidebarCommandButtons(
        [
          {
            actionType: "browser",
            closeTerminalOnExit: false,
            commandId: "custom-docs",
            isDefault: false,
            name: "Docs",
            playCompletionSound: false,
            showOnProjectRow: false,
            url: "https://example.com/docs",
          },
        ],
        ["test", "custom-docs", "dev"],
      ),
    ).toEqual([
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "test",
        isDefault: true,
        name: "Test",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
      {
        actionType: "browser",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "custom-docs",
        isDefault: false,
        name: "Docs",
        playCompletionSound: false,
        showOnProjectRow: false,
        url: "https://example.com/docs",
      },
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "dev",
        isDefault: true,
        name: "Dev",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "build",
        isDefault: true,
        name: "Build",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "setup",
        isDefault: true,
        name: "Setup",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
    ]);
  });

  test("should hide deleted default actions", () => {
    expect(createSidebarCommandButtons([], [], ["build", "test"])).toEqual([
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "dev",
        isDefault: true,
        name: "Dev",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: undefined,
        commandId: "setup",
        isDefault: true,
        name: "Setup",
        playCompletionSound: true,
        showOnProjectRow: false,
        url: undefined,
      },
    ]);
  });
});

describe("getFirstBrowserSidebarCommandUrl", () => {
  test("should return the first browser action url in the current action order", () => {
    const commands = createSidebarCommandButtons(
      [
        {
          actionType: "browser",
          closeTerminalOnExit: false,
          commandId: "custom-docs",
          isDefault: false,
          name: "Docs",
          playCompletionSound: false,
          showOnProjectRow: false,
          url: "https://example.com/docs",
        },
        {
          actionType: "browser",
          closeTerminalOnExit: false,
          commandId: "custom-app",
          isDefault: false,
          name: "App",
          playCompletionSound: false,
          showOnProjectRow: false,
          url: "https://example.com/app",
        },
      ],
      ["custom-app", "custom-docs"],
    );

    expect(getFirstBrowserSidebarCommandUrl(commands)).toBe("https://example.com/app");
  });

  test("should return undefined when no browser actions exist", () => {
    expect(getFirstBrowserSidebarCommandUrl(createSidebarCommandButtons([]))).toBeUndefined();
  });
});

describe("normalizeStoredSidebarCommands", () => {
  test("should normalize legacy terminal actions and trim valid values", () => {
    expect(
      normalizeStoredSidebarCommands([
        {
          command: "  vp dev  ",
          commandId: " dev ",
          isDefault: true,
          name: "  Dev server ",
        },
      ]),
    ).toEqual([
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: "vp dev",
        commandId: "dev",
        isDefault: true,
        name: "Dev server",
        playCompletionSound: true,
        showOnProjectRow: false,
      },
    ]);
  });

  test("should keep saved project-row visibility and default legacy records to hidden", () => {
    expect(
      normalizeStoredSidebarCommands([
        {
          command: "lazygit",
          commandId: "custom-lazygit",
          isDefault: false,
          name: "Lazygit",
          showOnProjectRow: true,
        },
        {
          commandId: "custom-docs",
          isDefault: false,
          name: "Docs",
          showOnProjectRow: "yes",
          url: "https://example.com/docs",
        },
        {
          command: "vp test",
          commandId: "custom-legacy",
          isDefault: false,
          name: "Legacy",
        },
      ]),
    ).toEqual([
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: "lazygit",
        commandId: "custom-lazygit",
        isDefault: false,
        name: "Lazygit",
        playCompletionSound: true,
        showOnProjectRow: true,
      },
      {
        actionType: "browser",
        closeTerminalOnExit: false,
        commandId: "custom-docs",
        isDefault: false,
        name: "Docs",
        playCompletionSound: false,
        showOnProjectRow: false,
        url: "https://example.com/docs",
      },
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: "vp test",
        commandId: "custom-legacy",
        isDefault: false,
        name: "Legacy",
        playCompletionSound: true,
        showOnProjectRow: false,
      },
    ]);
  });

  test("should infer legacy browser actions from saved urls and reject invalid values", () => {
    expect(
      normalizeStoredSidebarCommands([
        {
          commandId: " docs ",
          isDefault: false,
          name: " Docs ",
          playCompletionSound: true,
          showOnProjectRow: false,
          url: " https://example.com/docs ",
        },
        {
          actionType: "browser",
          commandId: "missing-url",
          isDefault: false,
          name: "Broken",
          playCompletionSound: true,
          showOnProjectRow: false,
        },
      ]),
    ).toEqual([
      {
        actionType: "browser",
        closeTerminalOnExit: false,
        commandId: "docs",
        isDefault: false,
        name: "Docs",
        playCompletionSound: false,
        showOnProjectRow: false,
        url: "https://example.com/docs",
      },
    ]);
  });

  test("should normalize command icons and strip legacy icon colors", () => {
    /*
     * CDXC:ProjectActions 2026-06-17-07:40:
     * Existing users may have saved per-action icon colors from older builds.
     * Updating the Mac app must keep the Action command while stripping the
     * removed color field so titlebar action glyphs inherit chrome color.
     */
    expect(
      normalizeStoredSidebarCommands([
        {
          closeTerminalOnExit: false,
          command: "pnpm dev",
          commandId: "devtools",
          icon: "terminal",
          iconColor: "not-a-color",
          isDefault: false,
          name: "",
          playCompletionSound: true,
          showOnProjectRow: false,
        },
      ]),
    ).toEqual([
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: "pnpm dev",
        commandId: "devtools",
        icon: "terminal",
        isDefault: false,
        name: "",
        playCompletionSound: true,
        showOnProjectRow: false,
      },
    ]);
  });

  test("should keep runnable actions with empty labels", () => {
    expect(
      normalizeStoredSidebarCommands([
        {
          command: "npm run ci",
          commandId: "ci",
          isDefault: false,
          name: "",
        },
      ]),
    ).toEqual([
      {
        actionType: "terminal",
        closeTerminalOnExit: false,
        command: "npm run ci",
        commandId: "ci",
        isDefault: false,
        name: "",
        playCompletionSound: true,
        showOnProjectRow: false,
      },
    ]);
  });

});

describe("getSidebarCommandPreviewLabel", () => {
  test("should show the placeholder when a terminal action has no command", () => {
    expect(
      getSidebarCommandPreviewLabel({
        actionType: "terminal",
        closeTerminalOnExit: false,
        commandId: "dev",
        isDefault: true,
        name: "Dev",
        playCompletionSound: true,
        showOnProjectRow: false,
      }),
    ).toBe(SIDEBAR_UNCONFIGURED_TERMINAL_COMMAND_LABEL);
  });
});

describe("normalizeStoredSidebarCommandOrder", () => {
  test("should ignore invalid ids, trim values, and dedupe entries", () => {
    expect(normalizeStoredSidebarCommandOrder([" test ", "", "dev", "test", 42, null])).toEqual([
      "test",
      "dev",
    ]);
  });
});
