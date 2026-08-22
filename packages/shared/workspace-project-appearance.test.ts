import { describe, expect, test } from "vitest";

import {
  normalizeDiscoveredProjectIconDataUrl,
  resolveWorkspaceProjectIconDataUrl,
} from "./workspace-project-appearance";

const pngDataUrl = "data:image/png;base64,cHJvamVjdC1pY29u";
const svgDataUrl = "data:image/svg+xml;base64,PHN2Zy8+";

describe("resolveWorkspaceProjectIconDataUrl", () => {
  test("prefers the typed image icon for shared React and native project chrome", () => {
    /**
     * CDXC:ProjectIcons 2026-05-11-01:50
     * macOS notification attachments and future React titlebar project UI must
     * consume the same validated project image data URL from workspace state.
     */
    expect(
      resolveWorkspaceProjectIconDataUrl({
        icon: { dataUrl: pngDataUrl, kind: "image" },
        iconDataUrl: svgDataUrl,
      }),
    ).toBe(pngDataUrl);
  });

  test("keeps legacy iconDataUrl available when no typed image icon exists", () => {
    expect(
      resolveWorkspaceProjectIconDataUrl({
        icon: { icon: "terminal", kind: "tabler" },
        iconDataUrl: svgDataUrl,
      }),
    ).toBe(svgDataUrl);
  });

  test("rejects invalid image data URLs", () => {
    expect(resolveWorkspaceProjectIconDataUrl({ iconDataUrl: "https://example.com/icon.png" })).toBe(
      undefined,
    );
  });
});

/**
 * CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
 * A discovered icon is whatever the repository ships, arriving from a daemon
 * that may be another machine, and it lands in an `<img src>`. So the accepted
 * set is wider than the picker's PNG/SVG — `favicon.ico` is the most common
 * real-world project icon there is — but everything that is not a base64 image
 * data URL is still refused.
 */
describe("normalizeDiscoveredProjectIconDataUrl", () => {
  test("accepts every format the discovery probe is allowed to read", () => {
    for (const mime of [
      "image/png",
      "image/svg+xml",
      "image/x-icon",
      "image/vnd.microsoft.icon",
      "image/jpeg",
      "image/webp",
      "image/gif",
    ]) {
      const dataUrl = `data:${mime};base64,cHJvamVjdC1pY29u`;
      expect(normalizeDiscoveredProjectIconDataUrl(dataUrl)).toBe(dataUrl);
    }
    expect(normalizeDiscoveredProjectIconDataUrl(`  ${pngDataUrl}  `)).toBe(pngDataUrl);
  });

  test("refuses anything that is not a base64 image data URL", () => {
    for (const value of [
      "https://example.com/icon.png",
      "/Users/madda/dev/Ghostex/favicon.png",
      "data:text/html;base64,PHNjcmlwdD4=",
      "data:image/png,not-base64",
      "javascript:alert(1)",
      "data:image/png;base64,<script>",
      "",
      undefined,
      42,
    ]) {
      expect(normalizeDiscoveredProjectIconDataUrl(value)).toBeUndefined();
    }
  });
});
