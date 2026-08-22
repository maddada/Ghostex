import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const discoverModalSource = readFileSync(
  new URL("./discover-ghostex-modal.tsx", import.meta.url),
  "utf8",
);
const sidebarStylesSource = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const featureScreenshotPaths = [
  "../../media/readme/ghostex-rich-prompt-editor-ctrl-g.png",
  "../../media/readme/ghostex-chromium-design-mode.png",
  "../../media/readme/ghostex-embedded-vscode-editor.png",
  "../../media/readme/ghostex-kanban-beads-board.png",
  "../../media/readme/ghostex-agents-terminal-splits.png",
] as const;
const pngSignature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

function readPngSize(buffer: Buffer): { height: number; width: number } {
  expect([...buffer.subarray(0, pngSignature.length)], "asset should be PNG data").toEqual(
    pngSignature,
  );
  return {
    width: buffer.readUInt32BE(16),
    height: buffer.readUInt32BE(20),
  };
}

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

function expectSourceOrder(source: string, orderedNeedles: readonly string[]): void {
  let previousIndex = -1;
  for (const needle of orderedNeedles) {
    const nextIndex = source.indexOf(needle);
    expect(nextIndex, `${needle} should exist`).toBeGreaterThanOrEqual(0);
    expect(nextIndex, `${needle} should follow the previous feature`).toBeGreaterThan(previousIndex);
    previousIndex = nextIndex;
  }
}

describe("discover ghostex modal source", () => {
  test("uses the authored highlighted-feature order, screenshots, and copy", () => {
    /*
    CDXC:HighlightedFeatures 2026-06-16-11:24:
    The Highlighted Features tour should follow the authored content order:
    page title, subtitle, and image for Rich Prompt Editor, Browser & Design
    Mode, Full Embedded Editor, Kanban Board & Beads, and Full Layout Freedom.
    */
    for (const screenshotPath of featureScreenshotPaths) {
      expect(discoverModalSource).toContain(screenshotPath);
    }
    expectSourceOrder(discoverModalSource, [
      'title: "Rich Prompt Editor with Ctrl + G"',
      "Edit your agent prompts with full hotkeys support and even image previews!",
      'title: "Chromium Browser with Design Mode"',
      "Comes with Devtools, Agent Browser Control, and Profiles mgmt. Your agent can control it with the /ghostex-embedded-browser-use skill.",
      'title: "Full VS Code Based Editor Built-in"',
      "Great for working with markdown, reviewing code, and checking PRs (Github Extension is great!)",
      'title: "Manage Your Project on a Kanban board"',
      "Store your ideas here then let an orchestrator agent hand them off to other agents (use the /ghostex-agent-orchestration skill)",
      'title: "Full Layout Freedom"',
      "Split your agent terminals anyway you like. Use the same hotkeys from ghostty to navigate the UI with keyboard only.",
    ]);
    expect(discoverModalSource).toContain("Highlighted Features");
    expect(discoverModalSource).not.toContain("<DialogTitle id={titleId}>Discover Ghostex</DialogTitle>");
    expect(discoverModalSource).not.toContain("Image showing the feature");
    expect(discoverModalSource).not.toContain("Placeholder until screenshots are added");
  });

  test("keeps featured README screenshot assets visible", () => {
    /*
    CDXC:HighlightedFeatures 2026-06-16-08:41:
    The Highlighted Features modal should keep real screenshot files behind the
    imported README image paths. A tiny placeholder asset makes the native modal
    look like images failed to load even when the bundler resolves the import.

    CDXC:HighlightedFeatures 2026-06-16-14:33:
    Checked-in README media is sanitized before release so repository images do
    not expose private workspace content. Verify PNG identity, full visual
    dimensions, and enough encoded detail to reject abstract skeleton PNGs that
    have the right canvas size but not real product pixels.
    */
    for (const imagePath of featureScreenshotPaths) {
      const imageUrl = new URL(imagePath, import.meta.url);
      const imageData = readFileSync(imageUrl);
      const { height, width } = readPngSize(imageData);
      expect(imageData.byteLength, `${imagePath} should contain real screenshot detail`).toBeGreaterThan(
        100_000,
      );
      expect(width, `${imagePath} should keep full screenshot width`).toBeGreaterThanOrEqual(2000);
      expect(height, `${imagePath} should keep full screenshot height`).toBeGreaterThanOrEqual(
        1200,
      );
    }
  });

  test("keeps feature copy above a full-width contained image", () => {
    /*
    CDXC:HighlightedFeatures 2026-06-16-12:35:
    The modal should put title and subtitle in one unrestricted text stack, fit
    the complete screenshot, use quiet outlines inside the modal, and use the
    same slight roundness tokens as Settings.

    CDXC:HighlightedFeatures 2026-06-16-14:08:
    Carousel arrows should sit beside the screenshot, not on top of it, and the
    screenshot outline should render evenly around rounded corners.

    CDXC:HighlightedFeatures 2026-06-16-18:27:
    Feature screenshot frames must stay transparent so PNG alpha corners do not
    reveal an artificial background behind the authored screenshot.

    CDXC:HighlightedFeatures 2026-06-16-18:48:
    The header should not render a feature icon; title and subtitle own the full
    copy row.

    CDXC:HighlightedFeatures 2026-07-29-05:09:
    GPUI child-window chrome owns the close control, so this surface should not
    render a second in-content X button.
    */
    expect(discoverModalSource).toContain("discover-ghostex-feature-heading");
    expect(discoverModalSource).toContain("disablePointerDismissal");
    expect(discoverModalSource).toContain("showCloseButton={false}");
    expect(discoverModalSource).toContain("IconChevronLeft");
    expect(discoverModalSource).toContain("IconChevronRight");
    expect(discoverModalSource).toContain("activateRelativeFeature(-1)");
    expect(discoverModalSource).toContain("activateRelativeFeature(1)");
    expect(discoverModalSource).toContain("Math.min(");
    expect(discoverModalSource).toContain("Math.max(0, activeFeatureIndex + offset)");
    expect(discoverModalSource).toContain('event.key === "ArrowLeft"');
    expect(discoverModalSource).toContain('event.key === "ArrowRight"');
    expect(discoverModalSource).toContain("disabled={!canActivatePreviousFeature}");
    expect(discoverModalSource).toContain("disabled={!canActivateNextFeature}");
    expect(discoverModalSource).not.toContain("%\n      DISCOVER_GHOSTEX_FEATURES.length");
    expect(discoverModalSource).not.toContain("discover-ghostex-thumbnail-icon");
    expect(discoverModalSource).not.toContain("discover-ghostex-feature-icon");
    const featureHeadingMarkup = sourceBetween(
      discoverModalSource,
      '<div className="discover-ghostex-feature-heading">',
      "</div>\n            </div>",
    );
    expect(featureHeadingMarkup).toContain("discover-ghostex-feature-title");
    expect(featureHeadingMarkup).toContain("discover-ghostex-feature-description");

    const featureStageStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .discover-ghostex-feature-stage {",
      ".ghostex-settings-shadcn .discover-ghostex-feature-copy {",
    );
    expect(featureStageStyles).toContain("grid-template-rows: auto minmax(0, 1fr);");
    expect(featureStageStyles).not.toContain("grid-template-columns:");

    const featureCopyStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .discover-ghostex-feature-copy {",
      ".ghostex-settings-shadcn .discover-ghostex-feature-heading {",
    );
    expect(featureCopyStyles).toContain("gap: 0.625rem;");
    expect(featureCopyStyles).toContain("grid-template-columns: minmax(0, 1fr);");
    expect(featureCopyStyles).toContain("justify-content: stretch;");

    const featureHeadingStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .discover-ghostex-feature-heading {",
      ".ghostex-settings-shadcn .discover-ghostex-feature-title {",
    );
    expect(featureHeadingStyles).toContain("gap: 0.25rem;");
    expect(featureHeadingStyles).not.toContain("grid-template-columns:");
    expect(featureHeadingStyles).not.toMatch(/(^|\n)\s*width:/);
    expect(sidebarStylesSource).not.toContain(".discover-ghostex-feature-icon");

    const featureTitleStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .discover-ghostex-feature-title {",
      ".ghostex-settings-shadcn .discover-ghostex-feature-description {",
    );
    expect(featureTitleStyles).toContain("font-size: 1.22rem;");
    expect(featureTitleStyles).toContain("max-width: none;");

    const featureDescriptionStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .discover-ghostex-feature-description {",
      ".ghostex-settings-shadcn .discover-ghostex-feature-visual {",
    );
    expect(featureDescriptionStyles).toContain(
      "color: color-mix(in srgb, var(--foreground) 70%, #8b8f98 30%);",
    );
    expect(featureDescriptionStyles).toContain("font-weight: 400;");
    expect(featureDescriptionStyles).toContain("max-width: none;");
    expect(featureDescriptionStyles).toContain("text-align: left;");

    const featureVisualStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .discover-ghostex-feature-visual {",
      ".ghostex-settings-shadcn .discover-ghostex-feature-image {",
    );
    expect(featureVisualStyles).toContain("background: transparent;");
    expect(featureVisualStyles).toContain("border: 0;");
    expect(featureVisualStyles).toContain("display: grid;");
    expect(featureVisualStyles).toContain("grid-template-columns: 2.25rem minmax(0, 1fr) 2.25rem;");
    expect(featureVisualStyles).toContain("width: 100%;");

    const featureImageStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .discover-ghostex-feature-image {",
      ".ghostex-settings-shadcn .discover-ghostex-feature-nav-button {",
    );
    expect(featureImageStyles).toContain("background: transparent;");
    expect(featureImageStyles).toContain("border: 0;");
    expect(featureImageStyles).toContain("box-shadow: 0 0 0 0.5px");
    expect(featureImageStyles).toContain("var(--foreground) 18%");
    expect(featureImageStyles).toContain("border-radius: var(--settings-radius-section);");
    expect(featureImageStyles).toContain("margin: 0.5px;");
    expect(featureImageStyles).toContain("max-height: calc(100% - 1px);");
    expect(featureImageStyles).toContain("max-width: calc(100% - 1px);");
    expect(featureImageStyles).toContain("object-fit: contain;");
    expect(featureImageStyles).toContain("object-position: center;");

    const featureNavButtonStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .discover-ghostex-feature-nav-button {",
      ".ghostex-settings-shadcn .discover-ghostex-feature-nav-button-left {",
    );
    expect(featureNavButtonStyles).toContain("position: relative;");
    expect(featureNavButtonStyles).not.toContain("top: 50%;");
    expect(featureNavButtonStyles).toContain("border: 0.5px solid");
    expect(sidebarStylesSource).toContain(".ghostex-settings-shadcn .discover-ghostex-feature-nav-button:disabled");

    const featureNavLeftStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .discover-ghostex-feature-nav-button-left {",
      ".ghostex-settings-shadcn .discover-ghostex-feature-nav-button-right {",
    );
    expect(featureNavLeftStyles).toContain("justify-self: end;");

    const featureNavRightStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .discover-ghostex-feature-nav-button-right {",
      ".ghostex-settings-shadcn .discover-ghostex-feature-nav-button:hover {",
    );
    expect(featureNavRightStyles).toContain("justify-self: start;");
    expect(sidebarStylesSource).not.toMatch(/discover-ghostex[\s\S]{0,320}border:\s*2px/);
  });

  test("does not render the removed bottom thumbnail strip", () => {
    /*
    CDXC:HighlightedFeatures 2026-06-17-12:45:
    Highlighted Features should not render the old bottom thumbnail strip. Keep
    the feature pages reachable through the main image arrow buttons only.
    */
    expect(discoverModalSource).not.toContain("thumbnailTitle");
    expect(discoverModalSource).not.toContain("discover-ghostex-feature-strip");
    expect(discoverModalSource).not.toContain("discover-ghostex-thumbnail");
    expect(discoverModalSource).not.toContain('role="tablist"');
    expect(discoverModalSource).not.toContain('role="tab"');
    expect(sidebarStylesSource).not.toMatch(/\.ghostex-settings-shadcn \.discover-ghostex-feature-strip/);
    expect(sidebarStylesSource).not.toMatch(/\.ghostex-settings-shadcn \.discover-ghostex-thumbnail/);
  });
});
