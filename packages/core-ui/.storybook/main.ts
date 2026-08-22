import path from "node:path";
import fs from "node:fs";
import os from "node:os";
import { fileURLToPath } from "node:url";
import type { StorybookConfig } from "@storybook/react-vite";

const storybookDir = path.dirname(fileURLToPath(import.meta.url));

const config: StorybookConfig = {
  framework: "@storybook/react-vite",
  /**
   * CDXC:NativeOnlyCleanup 2026-05-05-02:22
   * Storybook now covers the native/shared sidebar UI only. The old VS Code
   * workspace webview was removed with the unused extension terminal backend.
   *
   * CDXC:RepoOrganization 2026-06-09-09:31
   * Storybook config lives under packages/core-ui/.storybook so the repo root has fewer folders while config ownership stays with the sidebar UI surface.
   * Paths must resolve from this config directory because package scripts pass `-c packages/core-ui/.storybook` instead of relying on the default root .storybook folder.
   */
  stories: ["../**/*.stories.@(ts|tsx)"],
  viteFinal: async (config) => {
    const existingPlugins = config.plugins ?? [];
    config.resolve = {
      ...config.resolve,
      alias: {
        ...(Array.isArray(config.resolve?.alias) ? {} : config.resolve?.alias),
        "@": path.resolve(storybookDir, "../../.."),
      },
    };
    config.plugins = [
      ...existingPlugins,
      {
        name: "ghostex-current-sidebar-settings",
        configureServer(server) {
          server.middlewares.use("/__ghostex-current-sidebar-settings", (_request, response) => {
            const settingsPath = path.join(
              os.homedir(),
              ".ghostex",
              "state",
              "native-sidebar-settings.json",
            );
            response.setHeader("Content-Type", "application/json; charset=utf-8");
            try {
              /**
               * CDXC:StorybookSettings 2026-05-08-16:45
               * Local sidebar scenarios must reproduce the user's running ghostex
               * chrome before visual regression checks. Storybook serves the
               * shared native settings snapshot read-only so fixtures can use
               * the same sidebar mode, width, theme, and visibility settings
               * as the app instead of stale hard-coded harness defaults.
               */
              response.end(fs.readFileSync(settingsPath, "utf8"));
            } catch {
              response.end("{}");
            }
          });
          server.middlewares.use("/__ghostex-current-sidebar-projects", (_request, response) => {
            const projectsPath = path.join(
              os.homedir(),
              ".ghostex",
              "state",
              "native-sidebar-projects.json",
            );
            response.setHeader("Content-Type", "application/json; charset=utf-8");
            try {
              /**
               * CDXC:SidebarScroll 2026-05-20-08:08:
               * Scroll regressions depend on the user's real project/session
               * count, especially when the zmux project is expanded. Storybook
               * serves the native project snapshot read-only so the regression
               * story can reproduce that local sidebar shape without committing
               * private project data into source fixtures.
               */
              response.end(fs.readFileSync(projectsPath, "utf8"));
            } catch {
              response.end("{}");
            }
          });
        },
      },
    ];

    return config;
  },
};

export default config;
