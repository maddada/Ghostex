import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const titlebarHostSource = readFileSync(new URL("./titlebar-host.tsx", import.meta.url), "utf8");

describe("titlebar host motion import source", () => {
  test("does not import motion/react", () => {
    /*
     * CDXC:ModeSwitcher 2026-06-15-20:07:
     * The titlebar host must not pull the Motion runtime into the titlebar
     * bundle. The animated mode pill was replaced by an instant active state,
     * so keep this bundle-weight guard even though the mode switcher itself is
     * gone from this host.
     */
    expect(titlebarHostSource).not.toMatch(/^import \{ motion \} from "motion\/react";$/m);
    expect(titlebarHostSource).not.toContain('from "motion/react"');
  });
});
