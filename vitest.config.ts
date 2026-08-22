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
     * Root test discovery is limited to Ghostex-owned suites. Imported apps,
     * vendored sources, generated output, dependencies, and build/cache trees
     * have their own test runners and must not be collected by root Vitest.
     */
    exclude: [
      "**/node_modules/**",
      "**/bower_components/**",
      "**/vendor/**",
      "**/.git/**",
      "**/.hg/**",
      "**/.svn/**",
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
      "storybook-static/**",
      "tmp/**",
    ],
  },
});
