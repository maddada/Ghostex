import ampCliLogo from "../src/assets/amp-cli.svg" with { type: "text" };
import antigravityCliLogo from "../src/assets/antigravity-cli.svg" with { type: "text" };
import browserLogo from "../src/assets/browser.svg" with { type: "text" };
import claudeLogo from "../src/assets/claude.svg" with { type: "text" };
import codebuddyLogo from "../src/assets/codebuddy.svg" with { type: "text" };
import cursorCliLogo from "../src/assets/cursor-cli.svg" with { type: "text" };
import codexLogo from "../src/assets/codex.svg" with { type: "text" };
import copilotLogo from "../src/assets/copilot.svg" with { type: "text" };
import factoryDroidLogo from "../src/assets/factory-droid.svg" with { type: "text" };
import geminiLogo from "../src/assets/gemini.svg" with { type: "text" };
import grokBuildLogo from "../src/assets/grok-build.svg" with { type: "text" };
import hermesAgentLogo from "../src/assets/hermes-agent.svg" with { type: "text" };
import kiroLogo from "../src/assets/kiro.svg" with { type: "text" };
import ompLogo from "../src/assets/omp.svg" with { type: "text" };
import opencodeLogo from "../src/assets/opencode.svg" with { type: "text" };
import piLogo from "../src/assets/pi.svg" with { type: "text" };
import qoderLogo from "../src/assets/qoder.svg" with { type: "text" };
import rovoDevLogo from "../src/assets/rovo-dev.svg" with { type: "text" };
import type { SidebarAgentIcon } from "../shared/sidebar-agents";
import type { CSSProperties } from "react";

/**
 * CDXC:AgentDetection 2026-04-27-07:07
 * Sidebar card agent icons render as CSS masks. Native WKWebView can create
 * the span correctly while failing to paint a relative-file SVG mask, so agent
 * logos must be inline data URLs shared by masks and regular image sources.
 *
 * CDXC:AgentsHub 2026-05-13-08:08
 * Hub profile chips reuse these same mask data URLs. Storybook may resolve
 * text imports to data URLs, while the native Bun build reads raw SVG text, so
 * keep both forms valid instead of using bundler-specific import query syntax.
 */
function svgTextToDataUrl(svgText: string): string {
  if (svgText.startsWith("data:image/svg+xml,")) {
    return svgText;
  }
  return `data:image/svg+xml,${encodeURIComponent(svgText)}`;
}

function svgTextFromMaybeDataUrl(svgText: string): string {
  if (!svgText.startsWith("data:image/svg+xml,")) {
    return svgText;
  }

  const commaIndex = svgText.indexOf(",");
  if (commaIndex === -1) {
    return svgText;
  }

  try {
    return decodeURIComponent(svgText.slice(commaIndex + 1));
  } catch {
    return svgText;
  }
}

function svgTextToColorizedDataUrl(svgText: string, color: string): string {
  const rawSvgText = svgTextFromMaybeDataUrl(svgText);
  const withRootColor = rawSvgText.replace(/<svg\b([^>]*)>/i, (_match, attributes: string) => {
    const colorAttribute = /\scolor=/i.test(attributes) ? "" : ` color="${color}"`;
    const fillAttribute = /\sfill=/i.test(attributes) ? "" : ` fill="${color}"`;
    return `<svg${attributes}${colorAttribute}${fillAttribute}>`;
  });
  const colorizedSvgText = withRootColor
    .replace(/currentColor/g, color)
    .replace(/fill=(["'])#(?:000|000000)\1/gi, `fill="${color}"`)
    .replace(/fill:\s*#(?:000|000000)\b/gi, `fill:${color}`)
    .replace(/fill=(["'])rgb\(\s*(?:0\s*,\s*0\s*,\s*0|16\s*,\s*24\s*,\s*32)\s*\)\1/gi, `fill="${color}"`)
    .replace(/fill:\s*rgb\(\s*(?:0\s*,\s*0\s*,\s*0|16\s*,\s*24\s*,\s*32)\s*\)/gi, `fill:${color}`);

  return `data:image/svg+xml,${encodeURIComponent(colorizedSvgText)}`;
}

export const AGENT_LOGOS: Record<SidebarAgentIcon, string> = {
  "amp-cli": svgTextToDataUrl(ampCliLogo),
  "antigravity-cli": svgTextToDataUrl(antigravityCliLogo),
  browser: svgTextToDataUrl(browserLogo),
  claude: svgTextToDataUrl(claudeLogo),
  codebuddy: svgTextToDataUrl(codebuddyLogo),
  "cursor-cli": svgTextToDataUrl(cursorCliLogo),
  codex: svgTextToDataUrl(codexLogo),
  copilot: svgTextToDataUrl(copilotLogo),
  "factory-droid": svgTextToDataUrl(factoryDroidLogo),
  gemini: svgTextToDataUrl(geminiLogo),
  "grok-build": svgTextToDataUrl(grokBuildLogo),
  "hermes-agent": svgTextToDataUrl(hermesAgentLogo),
  kiro: svgTextToDataUrl(kiroLogo),
  omp: svgTextToDataUrl(ompLogo),
  opencode: svgTextToDataUrl(opencodeLogo),
  pi: svgTextToDataUrl(piLogo),
  qoder: svgTextToDataUrl(qoderLogo),
  "rovo-dev": svgTextToDataUrl(rovoDevLogo),
};

/**
 * CDXC:NativePaneReorder 2026-05-03-04:59
 * Sidebar agent SVGs are mask assets, so their visible color comes from CSS,
 * not the SVG fill. Native title bars and drag ghosts receive this same color
 * map with the data URL so AppKit can tint the template image to match the
 * session card.
 */
export const AGENT_LOGO_COLORS: Record<SidebarAgentIcon, string> = {
  "amp-cli": "#ffffff",
  "antigravity-cli": "#749bff",
  browser: "#82b7ff",
  claude: "#d97757",
  codebuddy: "#72d6ff",
  "cursor-cli": "#edecec",
  codex: "#ffffff",
  copilot: "#ffffff",
  "factory-droid": "#ff7a1a",
  gemini: "#8b9aff",
  "grok-build": "#ffffff",
  "hermes-agent": "#f3c46b",
  kiro: "#a6e3ff",
  omp: "#a663ed",
  opencode: "#6d96c0",
  pi: "#c8ff62",
  qoder: "#a991ff",
  "rovo-dev": "#4fc3a1",
};

/**
 * CDXC:SidebarSessionAgentIcons 2026-06-29-23:58:
 * Colored session-card agent icons render the SVGs as image backgrounds, not
 * CSS masks. Patch currentColor, inherited-fill, and black-only source logos
 * to the existing brand color map so colored mode does not turn dark logos
 * invisible on the dark macOS sidebar.
 */
export const COLORED_AGENT_LOGOS: Record<SidebarAgentIcon, string> = {
  "amp-cli": svgTextToColorizedDataUrl(ampCliLogo, AGENT_LOGO_COLORS["amp-cli"]),
  "antigravity-cli": svgTextToColorizedDataUrl(
    antigravityCliLogo,
    AGENT_LOGO_COLORS["antigravity-cli"],
  ),
  browser: svgTextToColorizedDataUrl(browserLogo, AGENT_LOGO_COLORS.browser),
  claude: svgTextToColorizedDataUrl(claudeLogo, AGENT_LOGO_COLORS.claude),
  codebuddy: svgTextToColorizedDataUrl(codebuddyLogo, AGENT_LOGO_COLORS.codebuddy),
  "cursor-cli": svgTextToColorizedDataUrl(cursorCliLogo, AGENT_LOGO_COLORS["cursor-cli"]),
  codex: svgTextToColorizedDataUrl(codexLogo, AGENT_LOGO_COLORS.codex),
  copilot: svgTextToColorizedDataUrl(copilotLogo, AGENT_LOGO_COLORS.copilot),
  "factory-droid": svgTextToColorizedDataUrl(
    factoryDroidLogo,
    AGENT_LOGO_COLORS["factory-droid"],
  ),
  gemini: svgTextToColorizedDataUrl(geminiLogo, AGENT_LOGO_COLORS.gemini),
  "grok-build": svgTextToColorizedDataUrl(grokBuildLogo, AGENT_LOGO_COLORS["grok-build"]),
  "hermes-agent": svgTextToColorizedDataUrl(
    hermesAgentLogo,
    AGENT_LOGO_COLORS["hermes-agent"],
  ),
  kiro: svgTextToColorizedDataUrl(kiroLogo, AGENT_LOGO_COLORS.kiro),
  omp: svgTextToColorizedDataUrl(ompLogo, AGENT_LOGO_COLORS.omp),
  opencode: svgTextToColorizedDataUrl(opencodeLogo, AGENT_LOGO_COLORS.opencode),
  pi: svgTextToColorizedDataUrl(piLogo, AGENT_LOGO_COLORS.pi),
  qoder: svgTextToColorizedDataUrl(qoderLogo, AGENT_LOGO_COLORS.qoder),
  "rovo-dev": svgTextToColorizedDataUrl(rovoDevLogo, AGENT_LOGO_COLORS["rovo-dev"]),
};

/**
 * Brand-colored picker and settings icons normally use a monochrome SVG mask
 * tinted with the provider color. OMP's supplied logo is intentionally
 * multicolor, so render its artwork directly instead of flattening it to the
 * legacy lime mask color.
 */
export function getBrandAgentLogoStyle(icon: SidebarAgentIcon): CSSProperties {
  if (icon === "omp") {
    return {
      backgroundColor: "transparent",
      backgroundImage: `url("${AGENT_LOGOS[icon]}")`,
      backgroundPosition: "center",
      backgroundRepeat: "no-repeat",
      backgroundSize: "contain",
    };
  }

  return {
    backgroundColor: AGENT_LOGO_COLORS[icon],
    maskImage: `url("${AGENT_LOGOS[icon]}")`,
    WebkitMaskImage: `url("${AGENT_LOGOS[icon]}")`,
  };
}
