import {
  IconFolder,
  IconFolderOpen,
  IconGitBranch,
  IconMessageCircle,
  IconTerminal2,
  IconWorld,
} from "@tabler/icons-react";
import type { CSSProperties } from "react";
import type { SidebarSessionItem } from "../../shared/session-grid-contract";
import {
  normalizeDiscoveredProjectIconDataUrl,
  resolveWorkspaceProjectIconDataUrl,
  type WorkspaceProjectIcon,
} from "../../shared/workspace-project-appearance";
import { AGENT_LOGOS, COLORED_AGENT_LOGOS } from "../agent-logos";
import { SidebarCommandIconGlyph } from "../sidebar-command-icon";
import { AppTooltip } from "../app-tooltip";

/*
 * CDXC:SidebarV2 2026-07-29:
 * V2 renders its own leading icons rather than reusing the V1 session-card
 * icon stack. The V1 component is a positioned overlay glued to the card's
 * hover/close/timer chrome; V2 needs a plain inline 16px glyph in normal flow.
 * The ASSETS are shared (`agent-logos`), so agent identity can never drift
 * between the two sidebars even though the boxes differ.
 */

type SidebarV2AgentLogoStyle = CSSProperties & {
  "--session-agent-logo": string;
  "--session-agent-logo-colored": string;
};

export type SidebarV2SessionIconProps = {
  agentIcon: SidebarSessionItem["agentIcon"];
  faviconDataUrl?: string;
  isBrowser: boolean;
  /** Mirrors the Session Cards "colored agent icons" setting. */
  useColoredAgentIcons: boolean;
};

export function SidebarV2SessionIcon({
  agentIcon,
  faviconDataUrl,
  isBrowser,
  useColoredAgentIcons,
}: SidebarV2SessionIconProps) {
  if (isBrowser || agentIcon === "browser") {
    if (faviconDataUrl) {
      return (
        <img
          alt=""
          aria-hidden="true"
          className="sidebar-v2-session-icon"
          data-icon-variant="favicon"
          src={faviconDataUrl}
        />
      );
    }
    return (
      <IconWorld
        aria-hidden="true"
        className="sidebar-v2-session-icon"
        data-icon-variant="glyph"
        size={16}
        stroke={1.8}
      />
    );
  }

  if (!agentIcon) {
    return (
      <IconTerminal2
        aria-hidden="true"
        className="sidebar-v2-session-icon"
        data-icon-variant="glyph"
        size={16}
        stroke={1.8}
      />
    );
  }

  const logoStyle: SidebarV2AgentLogoStyle = {
    "--session-agent-logo": `url("${AGENT_LOGOS[agentIcon]}")`,
    "--session-agent-logo-colored": `url("${COLORED_AGENT_LOGOS[agentIcon]}")`,
  };
  return (
    <span
      aria-hidden="true"
      className="sidebar-v2-session-icon"
      data-agent-icon={agentIcon}
      data-icon-variant={useColoredAgentIcons ? "logo-colored" : "logo"}
      style={logoStyle}
    />
  );
}

/*
 * CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
 * The one place a project's icon is resolved for every V2 surface — the card's
 * project line, the group headers, the scope menu, the Browser rows. The order
 * is:
 *
 *   1. a user-attached IMAGE (`icon.kind === "image"` or the legacy
 *      `iconDataUrl`): somebody deliberately uploaded a picture for this
 *      project, and no automatic guess should override that;
 *   2. the icon the project's OWN repository ships, discovered by gxserver;
 *   3. a typed Tabler glyph;
 *   4. the surface-specific fallback: worktree, open folder, or closed folder.
 *
 * The discovered icon deliberately outranks the TYPED glyph, which is the one
 * place this chain departs from `RecentProjectIcon`. A typed glyph is almost
 * never a considered choice on a session row: V1 does not render typed glyphs on
 * session-tree project rows only gained typed glyph support when this resolver
 * became shared, so in practice many are legacy values migrated forward from
 * the deprecated macOS app's picker, which the gpui app no longer exposes. A
 * repository's real favicon is a better answer than an inherited `archive`
 * glyph. Uploaded images stay on top because they carry deliberate user intent;
 * and the glyph is still the fallback whenever a project ships no icon of its
 * own, so nothing that used to render disappears.
 */
export type SidebarV2ProjectIconProps = {
  /**
   * CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
   * The icon the project's OWN repository ships, discovered by gxserver from
   * the checkout (its favicon or the icon its HTML
   * entry point declares) and carried as a data URL. Ranks below a user-attached
   * image and above the typed glyph — see the chain above.
   */
  discoveredIconDataUrl?: string;
  fallback?: "folder" | "folder-open" | "worktree";
  icon?: WorkspaceProjectIcon;
  iconDataUrl?: string;
  title: string;
};

export function SidebarV2ProjectIcon({
  discoveredIconDataUrl,
  fallback = "folder",
  icon,
  iconDataUrl,
  title,
}: SidebarV2ProjectIconProps) {
  const imageDataUrl = resolveWorkspaceProjectIconDataUrl({ icon, iconDataUrl });
  if (imageDataUrl) {
    return (
      <AppTooltip content={title}>
        <img
          alt=""
          aria-hidden="true"
          className="sidebar-v2-project-icon"
          data-icon-variant="image"
          src={imageDataUrl}
        />
      </AppTooltip>
    );
  }
  /*
   * The project's own icon, rendered in the same 16px rounded box as the user's
   * image variant (the shared `.sidebar-v2-project-icon` rule owns the radius
   * and `object-fit`), so a discovered favicon reads exactly like the rounded
   * favicons the browser rows already show. Its own variant marker keeps the
   * two distinguishable in tests and in the DOM without duplicating any style.
   */
  const discovered = normalizeDiscoveredProjectIconDataUrl(discoveredIconDataUrl);
  if (discovered) {
    return (
      <AppTooltip content={title}>
        <img
          alt=""
          aria-hidden="true"
          className="sidebar-v2-project-icon"
          data-icon-variant="discovered"
          src={discovered}
        />
      </AppTooltip>
    );
  }
  if (icon?.kind === "tabler") {
    /*
     * The glyph is wrapped rather than styled directly because the shared
     * V1 glyph component owns its own svg attributes: the wrapper keeps the
     * 16px box identical to the image and folder variants and carries the
     * state hook, without teaching a V1 component about V2's markup.
    */
    return (
      <AppTooltip content={title}>
        <span
          aria-hidden="true"
          className="sidebar-v2-project-icon"
          data-icon-variant="tabler"
        >
          <SidebarCommandIconGlyph color={icon.color} icon={icon.icon} size={16} stroke={1.8} />
        </span>
      </AppTooltip>
    );
  }
  const FallbackIcon =
    fallback === "worktree"
      ? IconGitBranch
      : fallback === "folder-open"
        ? IconFolderOpen
        : IconFolder;
  return (
    <FallbackIcon
      aria-hidden="true"
      className="sidebar-v2-project-icon"
      data-fallback-kind={fallback}
      data-icon-variant="glyph"
      size={16}
      stroke={1.8}
    />
  );
}
