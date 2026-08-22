import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

/*
 * CDXC:RepoRestructure 2026-08-22: source trees moved under packages/ and apps/,
 * and cross-tree imports are written as repo-root `@/...` specifiers. Vitest needs
 * the same `@` -> repository-root alias every vite/esbuild/Storybook config already
 * declares, otherwise suites that pull in shared UI fail to resolve those modules.
 *
 * CDXC:ReleaseAutomation 2026-08-23: this config lives at
 * scripts/release-gpui/vitest.release.config.ts, not the repo root, so `root`
 * must be set explicitly. Vite/Vitest default `root` to the config file's own
 * directory, and every test/exclude glob below is written relative to the
 * repository root (to select the exact same suite the config selected before
 * the move), so leaving `root` unset would make Vitest resolve those globs
 * against scripts/release-gpui/ instead and discover the wrong (empty) set.
 */
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

export default defineConfig({
  root: repoRoot,
  resolve: {
    alias: {
      "@": repoRoot,
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
      "apps/mobile/app/android/.gradle/**",
      "apps/mobile/app/android/**/build/**",
      ".dependencies/code-server/lib/**",
      ".dependencies/code-server/test/**",
      "storybook-static/**",
      "tmp/**",
    ],
  },
});
