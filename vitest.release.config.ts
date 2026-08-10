import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "bun:test": "vitest",
      "vite-plus/test": "vitest",
    },
  },
  test: {
    /*
     * CDXC:ReleaseAutomation 2026-06-14-09:07:
     * Release verification must exercise Ghostex-owned Vitest suites without
     * walking imported, generated, packaged dependency, or alternate-runner
     * trees. The broad default Vitest discovery can pick up node:test files
     * under gxserver and Jest/effect-package tests under code-server,
     * then fail for reasons unrelated to the release candidate.
     */
    exclude: [
      "**/node_modules/**",
      "**/.git/**",
      "**/dist/**",
      "**/build/**",
      "**/out/**",
      "**/coverage/**",
      "**/.cache/**",
      "**/.turbo/**",
      "**/.vite/**",
      "**/.zig-cache/**",
      "**/zig-out/**",
      "**/DerivedData/**",
      "**/target/**",
      ".dependencies/**",
      "ghostty/**",
      "tui/vendor/**",
      "tui/target/**",
      "mobile/android/.gradle/**",
      "mobile/android/**/build/**",
      "code-server/lib/**",
      "code-server/test/**",
      "gxserver/test/**",
      "storybook-static/**",
      "tmp/**",
    ],
  },
});
