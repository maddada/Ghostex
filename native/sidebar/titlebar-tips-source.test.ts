import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const titlebarHostSource = readFileSync(new URL("./titlebar-host.tsx", import.meta.url), "utf8");

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("native titlebar Tips & Tricks source", () => {
  test("does not render right-aligned section counts", () => {
    /*
     * CDXC:TipsAndTricks 2026-06-12-23:28:
     * macOS Tips & Tricks section headers should show labels only; the previous
     * right-side count looked like noisy chrome beside Read and Unread headings.
     */
    const sectionSource = sourceBetween(
      titlebarHostSource,
      "function TitlebarTipsSection",
      "function TitlebarNoticeRow",
    );

    expect(titlebarHostSource).toContain("headers read as labels only");
    expect(sectionSource).toContain("count > 0 ? children");
    expect(titlebarHostSource).not.toContain("titlebar-tips-section-count");
  });
});
