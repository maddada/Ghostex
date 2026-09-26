import {
  MANAGE_COMMENT_ANNOTATION_COLOR,
  MANAGE_MEO_CODE_BLOCK_BACKGROUND,
  MANAGE_MEO_CODE_COLOR,
  MANAGE_MEO_HEADING_COLOR,
  MANAGE_REDLINE_ANNOTATION_COLOR,
  MANAGE_SIDEBAR_DEFAULT_WIDTH,
} from './constants';
import { quickLabelColor } from './annotation-store';
import nativeFloatingPanelShadow from '@/packages/core-ui/assets/native-floating-panel-shadow.png';

export const MANAGE_STYLES = `
  :root {
    color-scheme: dark;
    /*
     * CDXC:Docs 2026-09-07 WHY:
     * The shared menu's default level 300 paints underneath the floating files sidebar at 650, making right-click menus invisible and unclickable.
     * Keep the existing menu and its dismissal layer above Docs chrome and below app dialogs.
     */
    --sidebar-context-menu-backdrop-z-index: 749;
    --sidebar-context-menu-z-index: 750;
    --sidebar-context-menu-submenu-z-index: 751;
    --manage-bg: var(--app-background, light-dark(#f7f7f8, #0e0e0e));
    --manage-panel: var(--app-background, light-dark(#f7f7f8, #0e0e0e));
    --manage-panel-strong: light-dark(#ffffff, #161616);
    --manage-panel-raised: light-dark(#f2f2f3, #1d1d1d);
    --manage-border: light-dark(rgba(0, 0, 0, 0.11), rgba(255, 255, 255, 0.11));
    --manage-border-strong: light-dark(rgba(0, 0, 0, 0.12), rgba(255, 255, 255, 0.12));
    --manage-text: light-dark(#27272a, #e5e5e5);
    --manage-muted: light-dark(#626269, #a3a3a3);
    --manage-subtle: light-dark(#71717a, #808080);
    --manage-accent: light-dark(#315d88, #9bbce0);
    --manage-accent-muted: light-dark(rgba(0, 0, 0, 0.055), rgba(255, 255, 255, 0.055));
    --manage-row-surface: light-dark(#e9e9eb, #202020);
    /*
     * CDXC:Docs 2026-08-23:
     * One control height for every free-standing in-row control (inputs,
     * buttons, dropdowns that share a row), matching the Kanban/Automate
     * 32px convention. Header bars keep their own 35px row height.
     */
    --manage-control-height: 32px;
    --manage-header-button-size: 27px;
    --manage-header-button-width: 32px;
    --manage-header-button-radius: 7px;
    --manage-header-button-gap: 2px;
    --manage-header-edge-padding: 9px;
    --manage-toolbar-icon-color: light-dark(#3f3f46, #a3a3a3);
    --manage-toolbar-disabled-color: light-dark(#b5b5bd, #55555d);
    --manage-sidebar-edge-button-width: var(--manage-header-button-width);
    --manage-green: light-dark(#376c51, #9db6aa);
    --manage-red: light-dark(#be123c, #fda4af);
    --manage-yellow: light-dark(#85632f, #c6ad80);
    background: var(--manage-bg);
  }

  * {
    box-sizing: border-box;
  }

  html,
  body,
  #root {
    background: var(--manage-bg);
    height: 100%;
    margin: 0;
    overflow: hidden;
    width: 100%;
  }

  /*
   * CDXC:Docs 2026-08-18:
   * Manage pulls in the shared sidebar theme for its tooltip/app tokens, and
   * that stylesheet also carries the sidebar app's own shell layout
   * ('#root { display: grid; grid-template-rows: auto minmax(0, 1fr) }', a
   * titlebar row plus a content row). Manage has no titlebar row: it renders a
   * single '.manage-shell' child that must own the whole document. Under the
   * sidebar grid that child lands in the content-sized 'auto' row, so Docs
   * stopped at its content height and left the rest of the pane empty. Manage
   * declares its own root layout so the document root stays a plain full-height
   * block box regardless of which shared theme sheets are loaded.
   */
  #root {
    display: block;
  }

  /* The native Docs view's browser area (embed.tsx): one HTML file or drawing fills the page. */
  .manage-embed {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .manage-embed > * {
    flex: 1 1 auto;
    min-height: 0;
  }

  body {
    color: var(--manage-text);
    font-family: "Inter Variable", Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif;
  }

  button,
  input,
  textarea {
    font: inherit;
  }

  .manage-shell {
    background: var(--manage-bg);
    display: grid;
    grid-template-columns: var(--manage-sidebar-width) minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    position: relative;
    /*
     * CDXC:Docs 2026-09-12 DECISION:
     * User: the files sidebar slides in with the same speed and style as the app's floating sidebar reveal.
     * That reveal moves the panel across its own width in 220ms on an ease-out cubic curve, so both surfaces share the duration and the curve.
     * The panel travels one panel width from the shell edge it lives on, and the document root already clips at the viewport, so no extra clipping is needed.
     * SEE-ALSO: apps/desktop/native/macos/GpuiSidebarReveal.m (animateTo:), apps/desktop/views/manage/constants.ts.
     */
    --manage-sidebar-reveal-duration: 220ms;
    --manage-sidebar-reveal-easing: cubic-bezier(0.33, 1, 0.68, 1);
    --manage-sidebar-reveal-offset: -100%;
    /*
     * CDXC:Docs 2026-09-19 DECISION:
     * User: the Docs files list is not resizable and keeps one width, so the shell owns that width instead of a persisted per-user value.
     */
    --manage-sidebar-width: ${MANAGE_SIDEBAR_DEFAULT_WIDTH}px;
    width: 100%;
  }

  .manage-shell[data-sidebar-side="right"] {
    grid-template-columns: minmax(0, 1fr) var(--manage-sidebar-width);
    --manage-sidebar-reveal-offset: 100%;
  }

  @keyframes manage-sidebar-reveal-in {
    from {
      transform: translateX(var(--manage-sidebar-reveal-offset));
    }

    to {
      transform: translateX(0);
    }
  }

  @keyframes manage-sidebar-reveal-out {
    from {
      transform: translateX(0);
    }

    to {
      transform: translateX(var(--manage-sidebar-reveal-offset));
    }
  }

  @media (prefers-reduced-motion: no-preference) {
    .manage-shell[data-sidebar-motion="in"] .manage-sidebar {
      animation: manage-sidebar-reveal-in var(--manage-sidebar-reveal-duration) var(--manage-sidebar-reveal-easing);
      will-change: transform;
    }

    /* The leaving panel holds its final offset until React unmounts it, and stops taking clicks so the document is usable the instant it is dismissed. */
    .manage-shell[data-sidebar-motion="out"] .manage-sidebar {
      animation: manage-sidebar-reveal-out var(--manage-sidebar-reveal-duration) var(--manage-sidebar-reveal-easing)
        forwards;
      pointer-events: none;
      will-change: transform;
    }

    /* Pinning a peek leaves the panel exactly where it is and only drops its raised shadow, so fade that out over the same curve instead of snapping it. */
    .manage-sidebar::before {
      transition: opacity var(--manage-sidebar-reveal-duration) var(--manage-sidebar-reveal-easing);
    }

    .manage-shell[data-sidebar-motion] .manage-sidebar::before {
      transition: none;
    }
  }

  .manage-shell[data-sidebar-hidden="true"] {
    grid-template-columns: minmax(0, 1fr);
  }

  .manage-shell[data-sidebar-hidden="true"] .manage-preview {
    grid-column: 1;
    grid-row: 1;
  }

  /*
   * CDXC:Docs 2026-06-30-13:45:
   * Floating mode keeps the preview full-width and preserves the sidebar side preference for which edge the panel opens from. The shell width breakpoint is owned by MANAGE_FLOATING_SIDEBAR_MAX_WIDTH.
   *
   * CDXC:Docs 2026-06-30-21:52:
   * The floating Docs sidebar must paint above the copied Meo Markdown toolbar, whose z-index is 500, so the file tree covers the entire editor chrome instead of starting visually below the toolbar. Cast the floating shadow from the sidebar edge that overlaps the Markdown editor so the panel reads as a raised sheet.
   */
  .manage-shell[data-sidebar-floating="true"],
  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] {
    grid-template-columns: minmax(0, 1fr);
  }

  .manage-shell[data-sidebar-floating="true"] .manage-preview {
    grid-column: 1;
    grid-row: 1;
  }

  /*
   * CDXC:Docs 2026-09-05 DECISION:
   * User: make the files list match the existing app sidebar, including its lightweight text, neutral row states, and context menu.
   */
  /*
   * CDXC:Docs 2026-09-15 DECISION:
   * User: in light mode the Docs files list, its search row, and the document and formatting toolbars use #f4f4f5, and the search row's bottom border is #e5e5e5.
   * The 6px resizer gap that used to sit beside that border was removed on 2026-09-19, leaving the border as the only line between the list and the content.
   * Dark mode keeps the 2026-09-07 decision (files sidebar, search row, and header rows use #0b0b0b) until the user shares dark colours.
   */
  .manage-sidebar {
    background: var(--app-chrome-background, light-dark(#f4f4f5, #0b0b0b));
    color: var(--app-foreground);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    grid-column: 1;
    grid-row: 1;
    min-height: 0;
    min-width: 0;
    padding: 0 0 7px;
    position: relative;
  }

  /*
   * CDXC:Docs 2026-09-15 DECISION:
   * User: the floating Docs file list shadow must exactly match the app's floating sidebar shadow.
   * The sidebar uses AppKit's window shadow, so use a 2x capture of the same borderless, nonactivating NSPanel instead of approximating it with CSS blur layers.
   * The capture has 19/23/27/23px outer margins; nine-slicing preserves 23px of each inner corner, and the clip removes the captured panel so the current theme paints its own content.
   * SEE-ALSO: apps/desktop/native/macos/GpuiSidebarReveal.m, packages/core-ui/assets/native-floating-panel-shadow.png.
   */
  .manage-sidebar::before {
    border: solid transparent;
    border-width: 42px 46px 50px;
    border-image: url("${nativeFloatingPanelShadow}") 84 92 100 92 stretch;
    clip-path: polygon(evenodd,
      0 0, 100% 0, 100% 100%, 0 100%, 0 0,
      23px 19px, calc(100% - 23px) 19px,
      calc(100% - 23px) calc(100% - 27px), 23px calc(100% - 27px), 23px 19px);
    content: "";
    inset: -19px -23px -27px;
    opacity: 0;
    pointer-events: none;
    position: absolute;
  }

  .manage-shell[data-sidebar-floating="true"] .manage-sidebar::before {
    opacity: 1;
  }

  .manage-shell[data-sidebar-side="right"] .manage-sidebar {
    grid-column: 2;
    grid-row: 1;
  }

  .manage-shell[data-sidebar-floating="true"] .manage-sidebar {
    border-right: 1px solid var(--manage-border);
    bottom: 0;
    grid-column: 1;
    grid-row: 1;
    left: 0;
    max-width: calc(100% - 34px);
    position: absolute;
    top: 0;
    width: min(var(--manage-sidebar-width), calc(100% - 34px));
    z-index: 650;
  }

  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-sidebar {
    border-left: 1px solid var(--manage-border);
    border-right: 0;
    left: auto;
    right: 0;
  }

  /*
   * CDXC:Workarea 2026-09-19 DECISION:
   * User: the Docs files list shows the same divider as the rest of the app, a single 1px line with no gap, and it is not resizable, so the line is the docked list's own edge border instead of a draggable rail.
   * This supersedes the 6px resizer column and the hover line that came with it.
   * SEE-ALSO: apps/desktop/src/app/render/resize_rail.rs (the GPUI rails this matches).
   */
  .manage-shell[data-sidebar-floating="false"] .manage-sidebar {
    border-right: 1px solid light-dark(#e5e5e5, #212121);
  }

  .manage-shell[data-sidebar-floating="false"][data-sidebar-side="right"] .manage-sidebar {
    border-left: 1px solid light-dark(#e5e5e5, #212121);
    border-right: 0;
  }

  .manage-preview {
    grid-column: 2;
    grid-row: 1;
  }

  .manage-shell[data-sidebar-side="right"] .manage-preview {
    grid-column: 1;
    grid-row: 1;
  }

  .manage-sidebar-header {
    align-items: center;
    border-bottom: 1px solid var(--manage-border);
    box-sizing: border-box;
    display: flex;
    gap: var(--manage-header-button-gap);
    height: 35px;
    justify-content: flex-end;
    max-height: 35px;
    min-height: 35px;
    overflow: visible;
    padding: 0 var(--manage-header-edge-padding);
  }

  .manage-sidebar-header[data-root-drop-target="true"] {
    background: color-mix(in srgb, var(--manage-text) 8%, transparent);
  }

  .manage-sidebar-actions {
    align-items: center;
    align-self: stretch;
    display: inline-flex;
    flex: 0 0 auto;
    gap: var(--manage-header-button-gap);
    height: 100%;
    position: relative;
  }

  .manage-icon-button {
    align-items: center;
    background: transparent;
    border: 0;
    color: color-mix(in srgb, var(--manage-text) 88%, var(--manage-subtle) 12%);
    display: inline-flex;
    height: 26px;
    justify-content: center;
    padding: 0;
    width: 26px;
  }

  .manage-icon-button:hover,
  .manage-icon-button:focus-visible {
    background: color-mix(in srgb, var(--manage-text) 10%, transparent);
    color: var(--manage-text);
    outline: none;
  }

  .manage-icon-button:disabled {
    color: var(--manage-subtle);
  }

  .manage-sidebar-header .manage-icon-button,
  .manage-sidebar-restore-button {
    background: transparent;
    border: 0;
    border-radius: var(--manage-header-button-radius);
    box-shadow: none;
    box-sizing: border-box;
    color: var(--manage-toolbar-icon-color);
    height: var(--manage-header-button-size);
    max-height: var(--manage-header-button-size);
    min-height: var(--manage-header-button-size);
    padding: 0;
    width: var(--manage-header-button-width);
  }

  /*
   * CDXC:Docs 2026-09-21 DECISION:
   * User: make the buttons on the top right of the Docs view match the look, gap, and right-edge alignment of the native view tab strip buttons above them.
   * The document and file-sidebar header actions use the metrics of the strip's panel toggles, which sit directly above them: 32px by 27px tiles, 7px radius, 2px gap, 18px stroke-2 icons, and a 9px right inset (TITLEBAR_CONTROL_HEIGHT, TITLEBAR_BUTTON_HORIZONTAL_PADDING, TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE, TITLEBAR_BUTTON_RADIUS, WORKAREA_HEADER_EDGE_PADDING); the 26px/14px size of the strip's smaller icon buttons was tried first and the user rejected it as smaller than the buttons above. The native Browser address bar uses the same numbers (BROWSER_TOOLBAR_BUTTON_WIDTH). This supersedes the 2026-09-14 28px/1px-gap toolbar reference and the hidden sidebar's 41px expand button. Darker enabled icons in light mode still keep disabled controls obvious.
   */
  .manage-sidebar-header .manage-icon-button {
    flex: 0 0 var(--manage-header-button-width);
    width: var(--manage-header-button-width);
  }

  .manage-sidebar-restore-button {
    flex: 0 0 var(--manage-sidebar-edge-button-width);
    width: var(--manage-sidebar-edge-button-width);
  }

  .manage-sidebar-header .manage-icon-button:not(:disabled):hover,
  .manage-sidebar-header .manage-icon-button:not(:disabled):focus-visible,
  .manage-sidebar-header .manage-icon-button[aria-expanded="true"],
  .manage-sidebar-restore-button:not(:disabled):hover,
  .manage-sidebar-restore-button:not(:disabled):focus-visible {
    background: light-dark(rgba(0, 0, 0, 0.08), rgba(255, 255, 255, 0.08));
    color: var(--manage-text);
    outline: none;
  }

  .manage-sidebar-header .manage-icon-button:disabled {
    background: transparent;
    color: var(--manage-toolbar-disabled-color);
    cursor: default;
    opacity: 1;
  }

  .manage-sidebar-header .manage-icon-button svg,
  .manage-sidebar-restore-button svg {
    height: 18px;
    width: 18px;
    stroke-width: 2;
  }

  .manage-sidebar-tree-toggle svg {
    transform: rotate(90deg);
  }

  /*
   * CDXC:AppModal 2026-08-26:
   * Docs dropdown panels follow the app's rounded menu language — 8px panels
   * with 6px item rows — the same radii the shared modal tokens use for menu
   * surfaces (--gx-modal-radius-control / --gx-modal-radius-menu).
   */
  .manage-sidebar-menu {
    backdrop-filter: blur(18px);
    background: var(--app-dropdown-background);
    border: 1px solid var(--ghostex-tooltip-border);
    border-radius: 8px;
    box-shadow:
      0 18px 42px rgba(0, 0, 0, 0.38),
      0 4px 12px rgba(0, 0, 0, 0.28),
      inset 0 1px 0 light-dark(rgba(0, 0, 0, 0.08), rgba(255, 255, 255, 0.08));
    display: grid;
    gap: 3px;
    min-width: 190px;
    padding: 6px;
    position: absolute;
    right: 6px;
    top: calc(100% + 7px);
    z-index: 30;
  }

  .manage-create-menu {
    min-width: 182px;
  }

  .manage-sidebar-menu-item {
    align-items: center;
    background: transparent;
    border: 0;
    border-radius: 6px;
    color: light-dark(rgba(24, 24, 27, 0.88), rgba(244, 244, 245, 0.88));
    display: flex;
    font-size: 12.5px;
    font-weight: 400;
    gap: 9px;
    line-height: 16px;
    min-height: 34px;
    padding: 8px 10px 8px 9px;
    position: relative;
    text-align: left;
    white-space: nowrap;
    width: 100%;
    z-index: 1;
  }

  .manage-sidebar-menu-item svg {
    color: light-dark(rgba(24, 24, 27, 0.72), rgba(244, 244, 245, 0.72));
    flex: 0 0 auto;
    height: 15px;
    width: 15px;
  }

  .manage-sidebar-menu-item:hover,
  .manage-sidebar-menu-item:focus-visible {
    background: light-dark(rgba(0, 0, 0, 0.105), rgba(255, 255, 255, 0.105));
    box-shadow: inset 0 0 0 1px light-dark(rgba(0, 0, 0, 0.045), rgba(255, 255, 255, 0.045));
    color: light-dark(rgba(24, 24, 27, 0.98), rgba(250, 250, 250, 0.98));
    outline: none;
  }

  .manage-sidebar-menu-item:hover svg,
  .manage-sidebar-menu-item:focus-visible svg {
    color: light-dark(rgba(24, 24, 27, 0.92), rgba(250, 250, 250, 0.92));
  }

  .manage-sidebar-menu-item:disabled {
    color: var(--manage-subtle);
    cursor: not-allowed;
  }

  .manage-sidebar-menu-item:disabled svg {
    color: color-mix(in srgb, var(--manage-subtle) 72%, transparent);
  }

  .manage-sidebar-menu-item:disabled:hover {
    background: transparent;
    box-shadow: none;
  }

  .manage-sidebar-restore-button {
    left: var(--manage-header-edge-padding);
    position: absolute;
    top: calc((35px - var(--manage-header-button-size)) / 2);
    z-index: 5;
  }

  .manage-shell[data-sidebar-side="right"] .manage-sidebar-restore-button {
    left: auto;
    right: var(--manage-header-edge-padding);
  }

  .manage-shell[data-sidebar-hidden="true"] .manage-preview-header,
  .manage-shell[data-sidebar-floating="true"] .manage-preview-header {
    padding-left: calc(var(--manage-header-edge-padding) + var(--manage-sidebar-edge-button-width) + 13px);
  }

  .manage-shell[data-sidebar-hidden="true"][data-sidebar-side="right"] .manage-preview-header,
  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-preview-header {
    padding-left: 16px;
    padding-right: calc(var(--manage-header-edge-padding) + var(--manage-sidebar-edge-button-width) + 13px);
  }

  /*
   * CDXC:Docs 2026-06-30-22:58:
   * Markdown Docs can collapse header action labels at narrow widths, making
   * the annotations/comments button the last visible header action before the
   * right-side restore control. Reserve only the restore button's real width so
   * the comments button does not leave an empty gutter to its right.
   *
   * CDXC:Docs 2026-06-30-23:52:
   * Floating sidebars hide and show above the same preview grid, so header
   * action geometry must not depend on whether the floating sidebar is currently
   * visible. Apply the same right-edge reservation in floating mode to prevent
   * the Markdown toolbar buttons from shifting during hide/show.
   */
  .manage-shell[data-sidebar-hidden="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="markdown"] .manage-preview-header,
  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="markdown"] .manage-preview-header,
  .manage-shell[data-sidebar-hidden="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="html"] .manage-preview-header,
  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="html"] .manage-preview-header {
    padding-right: calc(var(--manage-header-edge-padding) + var(--manage-sidebar-edge-button-width) + var(--manage-header-button-gap));
  }

  /*
   * CDXC:Docs 2026-09-12 DECISION:
   * User: keep the document search off the right edge of the Docs view when the files sidebar is hidden.
   * The panel hung 14px past the editor toolbar it drops from, which is the view's own edge in every state except a docked right sidebar, so it read as glued to the frame and bled over the files list in that one remaining state.
   * Pulling it inside the toolbar instead lines its right edge up with the last toolbar button above it, and this stays scoped to Docs so the editor app's own copy of the panel is untouched.
   */
  .manage-shell .find-panel {
    bottom: calc(100% + 8px);
    right: 0;
    top: auto;
  }

  .manage-search {
    /* CDXC:Docs 2026-09-06 DECISION: User: make the Docs file search bar 3px taller. */
    align-items: center;
    background: var(--app-chrome-background, light-dark(#f4f4f5, #0b0b0b));
    border: 0;
    border-bottom: 1px solid light-dark(#e5e5e5, #292929);
    box-sizing: border-box;
    display: flex;
    gap: 11px;
    height: calc(var(--manage-control-height) + 3px);
    margin: 0 0 4px;
    padding: 7px 10px;
    width: 100%;
  }

  /*
   * CDXC:Docs 2026-06-30-11:11:
   * Docs file search needs an inline X button that appears only while text is present; clicking it or pressing Escape clears the filter and keeps keyboard focus in the search field.
   */
  .manage-search:focus-within {
    background: color-mix(in srgb, var(--manage-text) 8%, transparent);
  }

  .manage-search > svg {
    color: var(--manage-text);
    flex: 0 0 auto;
    pointer-events: none;
  }

  .manage-search input {
    background: transparent;
    border: 0;
    color: var(--manage-text);
    flex: 1 1 auto;
    font-size: 15.55px;
    font-weight: 300;
    line-height: 20px;
    min-width: 0;
    outline: 0;
    padding: 0;
    width: 100%;
  }

  .manage-search input::placeholder {
    color: color-mix(in srgb, var(--manage-text) 52%, transparent);
  }

  .manage-search-clear-button {
    align-items: center;
    background: transparent;
    border: 0;
    color: color-mix(in srgb, var(--manage-text) 58%, transparent);
    display: inline-flex;
    flex: 0 0 auto;
    height: 20px;
    justify-content: center;
    margin-right: -3px;
    padding: 0;
    width: 20px;
  }

  .manage-search-clear-button:hover,
  .manage-search-clear-button:focus-visible {
    color: var(--manage-text);
    outline: none;
  }

  /*
   * CDXC:Docs 2026-09-15 DECISION:
   * User: the open-files list sits under Search and is as tall as the number of open files; it only scrolls once it would take more than a third of the sidebar.
   * Rows share the tree's type and hover language. The close control appears on hover, and an unsaved row shows a dot in its place until hovered.
   */
  .manage-open-files {
    border-bottom: 1px solid light-dark(#e5e5e5, #292929);
    display: flex;
    flex: 0 0 auto;
    flex-direction: column;
    max-height: 34%;
    overflow: auto;
    padding: 0 0 4px;
    scrollbar-width: thin;
  }

  /*
   * CDXC:Docs 2026-09-16 DECISION:
   * User: label the list under Search "Open Files" and the tree below it "Project Docs" so the two lists read as different things.
   */
  .manage-sidebar-section-label {
    color: var(--manage-subtle);
    flex: 0 0 auto;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.01em;
    line-height: 14px;
    padding: 8px 14px 3px;
    user-select: none;
    white-space: nowrap;
  }

  .manage-open-file-row {
    align-items: center;
    color: light-dark(#52525b, #b4b8c0);
    cursor: default;
    display: grid;
    flex: 0 0 auto;
    gap: 9px;
    grid-template-columns: 16px minmax(0, 1fr) 20px;
    height: 28px;
    padding: 0 7px 0 14px;
  }

  .manage-open-file-row:hover,
  .manage-open-file-row:focus-visible {
    background: var(--app-context-menu-hover-background);
    color: light-dark(#3f3f46, #d8d8d8);
    outline: none;
  }

  .manage-open-file-row[data-selected="true"] {
    background: var(--manage-row-surface);
    color: light-dark(#3f3f46, #d8d8d8);
  }

  .manage-open-file-name {
    font-size: 15.55px;
    font-weight: 300;
    line-height: 20px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-open-file-close {
    align-items: center;
    background: transparent;
    border: 0;
    border-radius: 4px;
    color: inherit;
    display: inline-flex;
    height: 20px;
    justify-content: center;
    padding: 0;
    position: relative;
    width: 20px;
  }

  .manage-open-file-close svg {
    opacity: 0;
  }

  .manage-open-file-row:hover .manage-open-file-close svg,
  .manage-open-file-row:focus-within .manage-open-file-close svg {
    opacity: 1;
  }

  .manage-open-file-close:hover,
  .manage-open-file-close:focus-visible {
    background: light-dark(rgba(0, 0, 0, 0.08), rgba(255, 255, 255, 0.1));
    outline: none;
  }

  .manage-open-file-dot {
    background: currentColor;
    border-radius: 999px;
    display: none;
    height: 8px;
    position: absolute;
    width: 8px;
  }

  .manage-open-file-row[data-dirty="true"]:not(:hover):not(:focus-within) .manage-open-file-dot {
    display: block;
  }

  .manage-preview-title-unsaved {
    background: currentColor;
    border-radius: 999px;
    flex: 0 0 auto;
    height: 9px;
    margin: 0 3px;
    width: 9px;
  }

  .manage-index-status {
    padding: 8px 12px;
    color: var(--muted-foreground);
    font-size: 11px;
    flex-shrink: 0;
  }

  .manage-file-list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 0 10px;
    position: relative;
    scrollbar-color: transparent transparent;
    scrollbar-width: thin;
  }

  .manage-file-list:hover,
  .manage-file-list:focus-within {
    scrollbar-color: light-dark(rgba(0, 0, 0, 0.38), rgba(255, 255, 255, 0.38)) transparent;
  }

  .manage-file-list::-webkit-scrollbar {
    height: 2px;
    width: 2px;
  }

  .manage-file-list::-webkit-scrollbar-track {
    background: transparent;
  }

  .manage-file-list::-webkit-scrollbar-thumb {
    background: transparent;
  }

  .manage-file-list:hover::-webkit-scrollbar-thumb,
  .manage-file-list:focus-within::-webkit-scrollbar-thumb {
    background: light-dark(rgba(0, 0, 0, 0.38), rgba(255, 255, 255, 0.38));
  }

  .manage-file-list::-webkit-scrollbar-thumb:hover {
    background: light-dark(rgba(0, 0, 0, 0.54), rgba(255, 255, 255, 0.54));
  }

  .manage-file-list::before {
    background: light-dark(#52525b, #c8cdd5);
    box-shadow:
      0 0 0 1px rgba(200, 205, 213, 0.22),
      0 0 14px rgba(200, 205, 213, 0.24);
    content: "";
    height: 3px;
    left: 12px;
    opacity: 0;
    pointer-events: none;
    position: absolute;
    right: 12px;
    top: 0;
    transition: opacity 120ms ease;
    z-index: 3;
  }

  .manage-file-list[data-root-drop-target="true"]::before {
    opacity: 1;
  }

  /*
   * CDXC:Docs 2026-06-30-03:20:
   * The Docs file tree should sit 5px closer to the sidebar's left edge while the Search field keeps its current padding and icon alignment.
   */
  .manage-file-row {
    --depth: 0;
    align-items: center;
    background: transparent;
    border: 0;
    box-sizing: border-box;
    color: light-dark(#52525b, #b4b8c0);
    display: grid;
    gap: 9px;
    grid-template-columns: 14px 16px minmax(0, 1fr) auto;
    min-height: 34px;
    padding: 7px 7px 7px calc(9px + (var(--depth) * 18px));
    position: relative;
    text-align: left;
    width: 100%;
  }

  .manage-file-row:hover,
  .manage-file-row:focus-visible {
    background: var(--app-context-menu-hover-background);
    color: light-dark(#3f3f46, #d8d8d8);
    outline: none;
  }

  .manage-file-row[data-kind="directory"] {
    color: var(--manage-muted);
    font-weight: 300;
  }

  .manage-file-row[data-kind="directory"][data-active-descendant="true"] {
    color: light-dark(#18181b, #ffffff);
  }

  .manage-file-row[data-selected="true"] {
    background: var(--manage-row-surface);
    color: light-dark(#3f3f46, #d8d8d8);
  }

  .manage-file-row[data-context-menu-open="true"] {
    background: var(--manage-row-surface);
    color: var(--manage-text);
  }

  .manage-file-row[data-dragging="true"] {
    opacity: 0.18;
  }

  .manage-file-row[data-drop-target="true"] {
    background: var(--manage-row-surface);
    color: var(--manage-text);
  }

  .manage-file-disclosure {
    align-items: center;
    color: var(--manage-subtle);
    display: inline-flex;
    height: 14px;
    justify-content: center;
    width: 14px;
  }

  .manage-file-disclosure[data-visible="false"] {
    opacity: 0;
  }

  .manage-file-disclosure svg {
    transition: transform 120ms ease;
  }

  .manage-file-row[aria-expanded="true"] .manage-file-disclosure svg {
    transform: rotate(90deg);
  }

  .manage-file-row[data-active-descendant="true"] .manage-file-disclosure {
    color: currentColor;
  }

  .manage-file-icon {
    color: currentColor;
    opacity: 0.75;
  }

  .manage-file-name {
    font-size: 15.55px;
    font-weight: 300;
    line-height: 20px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-file-badges {
    align-items: center;
    display: flex;
    gap: 5px;
    min-width: 0;
  }

  .manage-count-badge {
    align-items: center;
    background: var(--manage-panel-raised);
    border: 1px solid var(--manage-border-strong);
    border-radius: 4px;
    color: var(--manage-muted);
    display: inline-flex;
    font-size: 10px;
    font-weight: 500;
    height: 17px;
    justify-content: center;
    min-width: 17px;
    padding: 0 5px;
  }

  .manage-file-context-menu {
    color: var(--app-foreground);
    font-size: 12px;
    font-weight: 400;
  }

  /* The tooltip positioner owns stacking; its popup cannot escape a level below the Markdown toolbar. */
  .tooltip-positioner {
    z-index: var(--ghostex-tooltip-z-index, 1400);
  }

  .manage-file-context-menu-item {
    line-height: 16px;
  }

  .manage-file-context-menu-item svg {
    color: currentColor;
  }

  .manage-file-context-menu-spacer {
    flex: 1 1 auto;
    min-width: 10px;
  }

  .manage-file-context-menu-item:disabled {
    cursor: wait;
    opacity: 0.42;
  }

  .manage-file-context-menu-item-danger[data-confirming="true"] {
    background: color-mix(in srgb, #ff7b72 18%, transparent);
  }

  /*
   * CDXC:AppModal 2026-08-26:
   * Docs modals are AppModalShell dialogs, so their surface, radius, hairlines,
   * control height, typography, and footer pills all come from the .gx-app-modal
   * rules in packages/core-ui/styles/modals.css. Only two things stay here.
   *
   * 1. Stacking. The dialog primitive portals its backdrop and popup to
   *    <body> at z-index 50, while the Docs shell paints a floating file
   *    sidebar (650), the annotation dropdown (700), and the comment composer
   *    (710) in the same root stacking context, so an un-raised dialog would
   *    open underneath them with an undimmed sidebar on top of its scrim. Lift
   *    the dialog layer above every Docs overlay, matching the 1200 app-modal
   *    convention. Docs renders exactly one dialog stack, so scoping by slot is
   *    precise enough and cannot leak to another surface's sheet.
   * 2. The rename error line, which is layout unique to this modal.
   */
  [data-slot="dialog-overlay"] {
    z-index: 1200;
  }

  [data-slot="dialog-content"] {
    z-index: 1201;
  }

  .manage-rename-modal [data-slot="field-error"] {
    font-size: 13px;
    line-height: 1.45;
  }

  .manage-empty {
    align-items: center;
    color: var(--manage-subtle);
    display: flex;
    font-size: 12px;
    gap: 8px;
    justify-content: center;
    min-height: 72px;
    padding: 14px;
  }

  .manage-preview {
    background: var(--manage-bg);
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }

  .manage-preview-content {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .manage-preview-content[data-compact-header="true"] {
    grid-template-rows: auto minmax(0, 1fr);
  }

  .manage-preview-header {
    font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif;
    align-items: center;
    background: var(--app-chrome-background, light-dark(#f4f4f5, #0b0b0b));
    border-bottom: 1px solid var(--manage-border);
    box-sizing: border-box;
    display: flex;
    gap: 8px;
    height: 35px;
    max-height: 35px;
    min-height: 35px;
    min-width: 0;
    overflow: visible;
    padding: 0 var(--manage-header-edge-padding) 0 13px;
  }

  /*
   * CDXC:Theming 2026-09-23 DECISION:
   * User: "the titlebar of docs ... doesn't match the style of the rest when i have the glass effect more transparent". Under window glass the page is one solid card below the see-through tab strip, so its header row takes the page's own tone, kept apart by its hairline, instead of the chrome tone that only lines up with an opaque tab strip.
   */
  :root[data-window-glass="true"] .manage-preview-header {
    background: var(--manage-bg);
  }

  .manage-preview-content[data-kind="drawing"] .manage-preview-header {
    padding-right: 13px;
  }

  .manage-preview-title {
    /*
     * CDXC:Docs 2026-07-01-00:11:
     * Long project-relative Docs filenames should truncate before they can
     * displace metadata or header action buttons. Use a zero flex basis and
     * hidden overflow so the title yields width first while keeping the file
     * icon anchored at the left edge.
     */
    align-items: center;
    background: transparent;
    border: 0;
    border-radius: 0;
    color: inherit;
    padding: 0;
    text-align: left;
    display: flex;
    flex: 1 1 0;
    font-size: 12px;
    font-weight: 500;
    gap: 7px;
    line-height: 35px;
    min-width: 0;
    overflow: hidden;
  }

  .manage-preview-title svg {
    flex: 0 0 auto;
    height: 15px;
    width: 15px;
  }

  .manage-preview-title:hover {
    color: light-dark(#18181b, #ffffff);
  }

  .manage-preview-title:focus-visible {
    outline: 1px solid var(--manage-muted);
    outline-offset: -1px;
  }

  /*
   * CDXC:Docs 2026-09-15 DECISION:
   * User: truncate long Docs top-bar titles from the start so the buttons on the right stay on screen.
   * The inner bdi preserves the title's reading order while the outer RTL box places the ellipsis on the left.
   */
  .manage-preview-title > span {
    direction: rtl;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-preview-meta {
    align-items: center;
    color: var(--manage-subtle);
    display: flex;
    flex: 0 0 auto;
    font-size: 10.5px;
    font-weight: 400;
    gap: 9px;
    line-height: 35px;
    min-width: max-content;
  }

  .manage-preview-header-actions {
    align-items: center;
    align-self: stretch;
    display: inline-flex;
    flex: 0 0 auto;
    gap: var(--manage-header-button-gap);
    height: 100%;
    min-width: 0;
  }

  .manage-annotation-dropdown-shell {
    display: inline-flex;
    margin-right: 7px;
    position: relative;
  }

  .manage-preview-content[data-kind="markdown"] .manage-annotation-dropdown-shell {
    margin-right: 0;
  }

  /*
   * CDXC:Docs 2026-06-30-22:58:
   * When the right-side Docs sidebar is hidden, the restore button already owns the titlebar edge spacing. Remove the annotation dropdown shell's extra right margin so no empty strip appears between the comments/count button and the restore control.
   */
  .manage-shell[data-sidebar-hidden="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="markdown"] .manage-annotation-dropdown-shell,
  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="markdown"] .manage-annotation-dropdown-shell {
    margin-right: 0;
  }

  .manage-preview-path {
    border-bottom: 1px solid light-dark(rgba(0, 0, 0, 0.055), rgba(255, 255, 255, 0.055));
    color: var(--manage-subtle);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    font-size: 11px;
    overflow: hidden;
    padding: 8px 18px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-text-editor {
    background: var(--manage-bg);
    border: 0;
    color: light-dark(rgba(24, 24, 27, 0.88), rgba(248, 250, 252, 0.88));
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    font-size: 12px;
    height: 100%;
    line-height: 1.55;
    margin: 0;
    min-height: 0;
    outline: 0;
    overflow: auto;
    padding: 16px 18px 28px;
    resize: none;
    tab-size: 2;
    white-space: pre;
    width: 100%;
  }

  /*
   * CDXC:Docs 2026-06-29-17:25:
   * Rendered HTML Docs should give the artifact an isolated browser-like viewport. Do not apply Ghostex typography, padding, link colors, or dark background to the iframe because the HTML document's own CSS must decide how the page looks.
   *
   * CDXC:Docs 2026-06-30-04:41:
   * The iframe element itself should not paint a white scrollbar gutter around dark HTML documents. Keep it transparent over the Manage background while the loaded document still owns its actual page background.
   */
  .manage-html-render-view {
    background: transparent;
    border: 0;
    color-scheme: dark;
    display: block;
    height: 100%;
    min-height: 0;
    min-width: 0;
    width: 100%;
  }

  .manage-markdown-review {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    min-height: 0;
    min-width: 0;
    overflow: hidden;
    width: 100%;
  }

  .manage-markdown-meo-review {
    background: var(--manage-bg);
  }

  .manage-markdown-review-main {
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    min-height: 0;
    min-width: 0;
    overflow: hidden;
    width: 100%;
  }

  .manage-preview-header-actions button:where(:not(.manage-review-menu button)),
  .manage-comment-popover-actions button,
  .manage-markdown-selection-toolbar button {
    align-items: center;
    display: inline-flex;
    font-size: 11px;
    font-weight: 500;
    gap: 5px;
    justify-content: center;
    min-width: 0;
  }

  .manage-preview-header-actions button:where(:not(.manage-review-menu button)),
  .manage-comment-popover-actions button {
    background: light-dark(rgba(0, 0, 0, 0.04), rgba(255, 255, 255, 0.04));
    border: 1px solid var(--manage-border);
    color: var(--manage-muted);
    height: var(--manage-control-height);
    padding: 0 8px;
  }

  .manage-preview-header-actions button:where(:not(.manage-review-menu button)) {
    background: transparent;
    border: 0;
    border-radius: var(--manage-header-button-radius);
    box-shadow: none;
    box-sizing: border-box;
    color: var(--manage-toolbar-icon-color);
    font-size: 10.5px;
    font-weight: 500;
    height: var(--manage-header-button-size);
    line-height: var(--manage-header-button-size);
    max-height: var(--manage-header-button-size);
    min-height: var(--manage-header-button-size);
    min-width: var(--manage-header-button-width);
    padding: 0 7px;
  }

  .manage-preview-header-actions button:where(:not(.manage-review-menu button)):not(:disabled):hover,
  .manage-preview-header-actions button:where(:not(.manage-review-menu button)):not(:disabled):focus-visible,
  .manage-comment-popover-actions button:not(:disabled):hover,
  .manage-comment-popover-actions button:not(:disabled):focus-visible {
    background: rgba(125, 211, 252, 0.12);
    border-color: rgba(125, 211, 252, 0.32);
    color: var(--manage-text);
    outline: none;
  }

  .manage-preview-header-actions button:where(:not(.manage-review-menu button)):not(:disabled):hover,
  .manage-preview-header-actions button:where(:not(.manage-review-menu button)):not(:disabled):focus-visible,
  .manage-preview-header-actions button:where(:not(.manage-review-menu button))[aria-expanded="true"],
  .manage-preview-header-actions .manage-annotation-toggle[aria-pressed="true"] {
    background: light-dark(rgba(0, 0, 0, 0.08), rgba(255, 255, 255, 0.08));
    border-color: light-dark(#d4d4d8, #252525);
    color: var(--manage-text);
    outline: none;
  }

  .manage-preview-header-actions button:where(:not(.manage-review-menu button)):disabled,
  .manage-comment-popover-actions button:disabled {
    color: var(--manage-subtle);
  }

  .manage-preview-header-actions button:where(:not(.manage-review-menu button)):disabled {
    background: transparent;
    color: var(--manage-toolbar-disabled-color);
    cursor: default;
    opacity: 1;
  }

  .manage-preview-header-actions button:where(:not(.manage-review-menu button)):disabled:hover {
    background: transparent;
    color: var(--manage-toolbar-disabled-color);
  }

  .manage-preview-header-actions .manage-annotation-toggle[aria-pressed="true"] {
    border-left-color: light-dark(#d4d4d8, #252525);
  }

  .manage-preview-header-actions .manage-clear-annotations-button[data-confirming="true"] {
    background: rgba(244, 63, 94, 0.13);
    border-color: rgba(244, 63, 94, 0.34);
    color: light-dark(#be123c, #fda4af);
  }

  .manage-preview-header-actions .manage-clear-annotations-button[data-confirming="true"]:not(:disabled):hover,
  .manage-preview-header-actions .manage-clear-annotations-button[data-confirming="true"]:not(:disabled):focus-visible {
    background: rgba(244, 63, 94, 0.18);
    border-color: rgba(244, 63, 94, 0.46);
    color: light-dark(#9f1239, #fecdd3);
  }

  .manage-preview-header-actions .manage-annotation-dropdown-trigger {
    flex: 0 0 auto;
    padding: 0 7px;
    width: auto;
  }

  .manage-preview-header-actions .manage-add-global-comment-button,
  .manage-preview-header-actions .manage-copy-feedback-button,
  .manage-preview-header-actions .manage-clear-annotations-button,
  .manage-preview-header-actions .manage-file-reload-button {
    flex: 0 0 var(--manage-header-button-width);
    padding: 0;
    width: var(--manage-header-button-width);
  }

  .manage-preview-header-actions .manage-add-global-comment-button > span,
  .manage-preview-header-actions .manage-copy-feedback-button > span,
  .manage-preview-header-actions .manage-clear-annotations-button > span {
    display: none;
  }

  .manage-preview-header-actions .manage-count-badge {
    height: 17px;
    min-width: 17px;
    padding: 0 4px;
  }

  /*
   * CDXC:Docs 2026-09-15 WHY:
   * The review-menu exclusion is wrapped in :where() so these generic rules keep the specificity of a plain descendant selector.
   * Without it the base padding (0 7px) outranked the icon-only buttons' padding: 0, the 28px tile left 14px of content, and the flex-item icon shrank to 14px wide while the files-list header icons stayed 18px.
   */
  .manage-preview-header-actions button:where(:not(.manage-review-menu button)) svg {
    flex: 0 0 auto;
    height: 18px;
    width: 18px;
    stroke-width: 2;
  }

  .manage-preview-header-actions .manage-file-reload-button {
    position: relative;
  }

  .manage-file-change-indicator {
    background: #fbbf24;
    border: 1px solid light-dark(#f4f4f5, #0e0e0e);
    border-radius: 999px;
    box-shadow: 0 0 0 1px rgba(251, 191, 36, 0.18);
    height: 7px;
    pointer-events: none;
    position: absolute;
    right: 6px;
    top: 6px;
    width: 7px;
  }

  .manage-meo-markdown-editor {
    background: var(--manage-bg);
    box-sizing: border-box;
    color: light-dark(rgba(24, 24, 27, 0.9), rgba(248, 250, 252, 0.9));
    inline-size: 100%;
    max-inline-size: 100%;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }

  /*
   * CDXC:Docs 2026-06-30-13:45:
   * The embedded Meo editor must keep both its toolbar and CodeMirror surface owned by the Manage preview column after heading formatting changes remeasure live Markdown content.
   * Keep Meo's single-row toolbar layout, measure before hiding the three secondary right-side utility buttons, and use one Live/Source toggle button instead of a two-option segmented control.
   */
  /*
   * CDXC:Docs 2026-09-16 DECISION:
   * User: give the Docs find-and-replace pane the same look and background as the floating formatting bar in light and dark modes so it stands out from the document.
   */
  .manage-shell .find-panel,
  .manage-meo-markdown-editor .mode-toolbar {
    background: var(--app-chrome-background, light-dark(#f4f4f5, #0b0b0b));
    border: 1px solid var(--manage-border);
    border-radius: 10px;
    box-shadow:
      0 8px 24px light-dark(rgba(0, 0, 0, 0.1), rgba(0, 0, 0, 0.45)),
      0 1px 2px light-dark(rgba(0, 0, 0, 0.06), rgba(0, 0, 0, 0.3));
  }

  /*
   * CDXC:Docs 2026-09-15 DECISION:
   * User: the formatting bar is a floating, rounded bar near the bottom of the document instead of a second header row, and it can collapse to one pill.
   * The bar sits at its content width, centred over the editor, and the editor keeps extra bottom padding so the last lines can scroll out from under it. The inset is MANAGE_FORMATTING_BAR_INSET in constants.ts.
   */
  .manage-meo-markdown-editor .mode-toolbar {
    bottom: 14px;
    box-sizing: border-box;
    display: flex;
    gap: 8px;
    height: 37px;
    left: 50%;
    max-width: calc(100% - 28px);
    min-width: 0;
    overflow: visible;
    padding: 3px;
    position: absolute;
    transform: translateX(-50%);
    width: max-content;
    z-index: 500;
  }

  .manage-meo-markdown-editor .mode-toolbar .format-group {
    margin-left: 0;
  }

  /* Meo pads the bar's left edge to line up with the gutter; a centred floating bar keeps its own 3px. Outranks Meo's :has() rule for hidden line numbers. */
  .manage-shell .manage-preview .manage-meo-markdown-editor.editor-root .mode-toolbar {
    padding-left: 3px;
  }

  .manage-meo-markdown-editor .mode-toolbar .manage-formatting-bar-toggle {
    border-radius: 4px;
    color: var(--manage-toolbar-icon-color);
    flex: 0 0 var(--manage-header-button-size);
    height: var(--manage-header-button-size);
    width: var(--manage-header-button-size);
  }

  .manage-meo-markdown-editor .mode-toolbar .manage-formatting-bar-toggle:hover,
  .manage-meo-markdown-editor .mode-toolbar .manage-formatting-bar-toggle:focus-visible {
    background: light-dark(rgba(0, 0, 0, 0.08), rgba(255, 255, 255, 0.08));
    color: var(--manage-text);
  }

  .manage-meo-markdown-editor .mode-toolbar[data-collapsed="true"] > :is(.format-group, .right-group, .mode-group) {
    display: none;
  }

  .manage-meo-markdown-editor .mode-toolbar[data-collapsed="true"] {
    gap: 0;
  }

  .manage-meo-markdown-editor .mode-toolbar .format-group,
  .manage-meo-markdown-editor .mode-toolbar .right-group {
    gap: 1px;
  }

  .manage-meo-markdown-editor .mode-toolbar > :is(.format-group, .right-group) .format-button,
  .manage-meo-markdown-editor .mode-toolbar .heading-dropdown-option {
    border-radius: 4px;
    color: var(--manage-toolbar-icon-color);
    flex: 0 0 var(--manage-header-button-size);
    height: var(--manage-header-button-size);
    width: var(--manage-header-button-size);
  }

  .manage-meo-markdown-editor .mode-toolbar > :is(.format-group, .right-group) .format-button svg,
  .manage-meo-markdown-editor .mode-toolbar .heading-dropdown-option svg {
    height: 18px;
    width: 18px;
    stroke-width: 1.7;
  }

  /* An unselected toggle is still enabled; Meo's default half opacity made it look disabled. */
  .manage-meo-markdown-editor .mode-toolbar > :is(.format-group, .right-group) .format-button.toggle-button:not(.is-active) {
    color: var(--manage-toolbar-icon-color);
    opacity: 1;
  }

  .manage-meo-markdown-editor .mode-toolbar > :is(.format-group, .right-group) .format-button:not(:disabled):hover,
  .manage-meo-markdown-editor .mode-toolbar > :is(.format-group, .right-group) .format-button:not(:disabled):focus-visible,
  .manage-meo-markdown-editor .mode-toolbar .heading-dropdown-option:not(:disabled):hover,
  .manage-meo-markdown-editor .mode-toolbar .heading-dropdown-option:not(:disabled):focus-visible {
    background: light-dark(rgba(0, 0, 0, 0.08), rgba(255, 255, 255, 0.08));
    color: var(--manage-text);
  }

  .manage-meo-markdown-editor .mode-toolbar > :is(.format-group, .right-group) .format-button.is-active {
    background: var(--manage-row-surface);
    color: var(--manage-text);
  }

  .manage-meo-markdown-editor .mode-toolbar > :is(.format-group, .right-group) .format-button:disabled,
  .manage-meo-markdown-editor .mode-toolbar > :is(.format-group, .right-group) .format-button.toggle-button:disabled,
  .manage-meo-markdown-editor .mode-toolbar .heading-dropdown-option:disabled {
    background: transparent;
    color: var(--manage-toolbar-disabled-color);
    cursor: default;
    opacity: 1;
  }

  .manage-meo-markdown-editor .format-group {
    /*
     * CDXC:Docs 2026-07-01-00:11:
     * The left formatting group must not push the persistent right-side toolbar
     * controls outside narrow Docs panes. Let it shrink from zero-basis and
     * clip lower-priority formatting buttons before search, display toggles, or
     * the Live/Source mode control leave the visible toolbar.
     */
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
  }

  .manage-meo-markdown-editor .right-group,
  .manage-meo-markdown-editor .mode-group {
    flex: 0 0 auto;
  }

  .manage-meo-markdown-editor .right-group {
    margin-left: auto;
    margin-right: 0;
    min-width: 0;
  }

  .manage-meo-markdown-editor .mode-group {
    background: transparent;
    border: 0;
    border-radius: 4px;
    gap: 1px;
    padding: 0;
  }

  .manage-meo-markdown-editor .mode-button {
    border-radius: 4px;
    color: var(--manage-toolbar-icon-color);
    height: var(--manage-header-button-size);
    min-width: 64px;
  }

  .manage-meo-markdown-editor .manage-mode-toggle {
    min-width: 76px;
  }

  .manage-meo-markdown-editor .mode-button[aria-selected="true"],
  .manage-meo-markdown-editor .mode-button.is-active {
    background: light-dark(#e9e9eb, #242424);
    box-shadow: inset 0 0 0 1px var(--manage-border-strong);
    color: var(--manage-text);
  }

  .manage-meo-markdown-editor .mode-button[aria-selected="false"]:hover,
  .manage-meo-markdown-editor .mode-button:not(.is-active):hover {
    background: light-dark(rgba(0, 0, 0, 0.07), rgba(255, 255, 255, 0.07));
    color: var(--manage-text);
  }

  .manage-meo-markdown-editor .table-grid-cell {
    appearance: none;
    padding: 0;
  }

  .manage-selection-inline-mode-button {
    color: light-dark(#1f2937, ${MANAGE_MEO_HEADING_COLOR});
  }

  .manage-meo-markdown-editor .cm-line:not(.meo-md-code-block):not(.meo-src-code-block):not(.meo-mermaid-block) .meo-md-inline-code,
  .manage-meo-markdown-editor .cm-line:not(.meo-md-code-block):not(.meo-src-code-block):not(.meo-mermaid-block) .meo-md-inline-code * {
    background: light-dark(#f3f4f6, ${MANAGE_MEO_CODE_BLOCK_BACKGROUND}) !important;
    color: light-dark(#374151, ${MANAGE_MEO_CODE_COLOR}) !important;
    -webkit-text-fill-color: light-dark(#374151, ${MANAGE_MEO_CODE_COLOR}) !important;
  }

  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block),
  .manage-meo-markdown-editor .cm-line.meo-md-alert:is(.meo-md-code-block, .meo-src-code-block) {
    background: light-dark(#f3f4f6, ${MANAGE_MEO_CODE_BLOCK_BACKGROUND}) !important;
  }

  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block) {
    --manage-code-block-top-border: transparent;
    --manage-code-block-bottom-border: transparent;
    box-shadow:
      inset 1px 0 0 var(--manage-border-strong),
      inset -1px 0 0 var(--manage-border-strong),
      inset 0 1px 0 var(--manage-code-block-top-border),
      inset 0 -1px 0 var(--manage-code-block-bottom-border);
  }

  .manage-meo-markdown-editor .cm-line:not(.meo-md-code-block):not(.meo-src-code-block) + .cm-line:is(.meo-md-code-block, .meo-src-code-block),
  .manage-meo-markdown-editor .cm-content > .cm-line:is(.meo-md-code-block, .meo-src-code-block):first-child {
    --manage-code-block-top-border: var(--manage-border-strong);
    border-radius: 8px 8px 0 0;
  }

  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block):has(+ .cm-line:not(.meo-md-code-block):not(.meo-src-code-block)),
  .manage-meo-markdown-editor .cm-content > .cm-line:is(.meo-md-code-block, .meo-src-code-block):last-child {
    --manage-code-block-bottom-border: var(--manage-border-strong);
    border-radius: 0 0 8px 8px;
  }

  .manage-meo-markdown-editor .meo-search-match {
    background: rgba(155, 188, 224, 0.18);
    color: var(--manage-text);
    outline-color: rgba(155, 188, 224, 0.4);
  }

  .manage-meo-markdown-editor .meo-search-match-active {
    background: rgba(155, 188, 224, 0.32);
    outline-color: var(--manage-accent);
  }

  .manage-meo-markdown-editor .editor-wrapper,
  .manage-meo-markdown-editor .editor-host,
  .manage-meo-markdown-editor .cm-editor,
  .manage-meo-markdown-editor .cm-scroller,
  .manage-meo-markdown-editor .cm-content,
  .manage-meo-markdown-editor .cm-line {
    box-sizing: border-box;
    inline-size: 100%;
    max-inline-size: 100%;
    min-height: 0;
    min-width: 0;
  }

  .manage-meo-markdown-editor .cm-editor {
    background: var(--manage-bg);
    height: 100%;
  }

  .manage-meo-markdown-editor .cm-scroller {
    scrollbar-color: light-dark(rgba(0, 0, 0, 0.28), rgba(255, 255, 255, 0.28)) transparent;
  }

  .manage-meo-markdown-editor .cm-gutters {
    max-width: 47px;
    min-width: 47px;
  }

  .manage-meo-markdown-editor .cm-gutter.meo-md-fold-gutter {
    max-width: 16px;
    min-width: 16px;
    width: 16px;
  }

  .manage-meo-markdown-editor .cm-gutter.cm-lineNumbers,
  .manage-meo-markdown-editor .cm-lineNumbers .cm-gutterElement {
    max-width: 28px;
    min-width: 28px;
    width: 28px;
  }

  .manage-meo-markdown-editor .cm-lineNumbers .cm-gutterElement {
    align-items: flex-start;
    padding: 0 4px 0 0;
  }

  .manage-meo-markdown-editor .cm-content {
    margin-left: 0;
    margin-right: 0;
    padding-bottom: 72px;
    padding-right: 12px;
  }

  .manage-markdown-document {
    color: light-dark(rgba(24, 24, 27, 0.9), rgba(248, 250, 252, 0.9));
    font-size: 15px;
    line-height: 1.625;
    min-height: 0;
    overflow: auto;
    padding: 24px 32px 48px;
  }

  .manage-markdown-document > :first-child {
    margin-top: 0;
  }

  .manage-markdown-document h1,
  .manage-markdown-document h2,
  .manage-markdown-document h3,
  .manage-markdown-document h4,
  .manage-markdown-document h5,
  .manage-markdown-document h6 {
    color: light-dark(#1f2937, ${MANAGE_MEO_HEADING_COLOR});
    letter-spacing: 0;
    line-height: 1.22;
  }

  .manage-markdown-document h1 {
    font-size: 24px;
    font-weight: 750;
    margin: 24px 0 16px;
  }

  .manage-markdown-document h2 {
    font-size: 20px;
    font-weight: 700;
    margin: 32px 0 12px;
  }

  .manage-markdown-document h3 {
    font-size: 16px;
    font-weight: 700;
    margin: 24px 0 8px;
  }

  .manage-markdown-document h4,
  .manage-markdown-document h5,
  .manage-markdown-document h6 {
    font-size: 15px;
    font-weight: 700;
    margin: 18px 0 8px;
  }

  .manage-markdown-document p {
    margin: 0 0 16px;
  }

  .manage-markdown-document a {
    color: var(--manage-accent);
    text-decoration: none;
  }

  .manage-markdown-document a:hover,
  .manage-markdown-document a:focus-visible {
    text-decoration: underline;
  }

  .manage-markdown-document blockquote {
    border-left: 2px solid rgba(125, 211, 252, 0.48);
    color: var(--manage-muted);
    font-style: italic;
    margin: 16px 0;
    padding-left: 16px;
  }

  .manage-markdown-document blockquote p:last-child,
  .manage-md-alert p:last-child,
  .manage-md-directive p:last-child {
    margin-bottom: 0;
  }

  .manage-md-empty {
    color: var(--manage-subtle);
  }

  .manage-md-inline-code {
    background: light-dark(rgba(0, 0, 0, 0.07), rgba(255, 255, 255, 0.07));
    border: 1px solid light-dark(rgba(0, 0, 0, 0.08), rgba(255, 255, 255, 0.08));
    border-radius: 4px;
    color: light-dark(rgba(24, 24, 27, 0.92), rgba(248, 250, 252, 0.92));
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    font-size: 0.9em;
    padding: 1px 4px;
  }

  .manage-md-inline-image {
    border: 1px solid var(--manage-border);
    display: block;
    margin: 12px 0;
    max-width: 100%;
  }

  .manage-md-list-item {
    align-items: flex-start;
    display: flex;
    gap: 12px;
    margin: 6px 0 6px calc(var(--manage-md-list-level, 0) * 20px);
  }

  .manage-md-list-marker {
    color: var(--manage-muted);
    flex: 0 0 22px;
    font-size: 13px;
    line-height: 1.625;
    text-align: right;
  }

  .manage-md-list-marker input {
    height: 13px;
    margin: 4px 0 0;
    width: 13px;
  }

  .manage-md-list-text {
    color: light-dark(rgba(24, 24, 27, 0.9), rgba(248, 250, 252, 0.9));
    min-width: 0;
  }

  .manage-md-list-text.is-checked {
    color: var(--manage-muted);
    text-decoration: line-through;
  }

  .manage-md-code-block {
    margin: 20px 0;
    position: relative;
  }

  .manage-md-code-block button {
    align-items: center;
    background: light-dark(rgba(0, 0, 0, 0.08), rgba(255, 255, 255, 0.08));
    border: 1px solid var(--manage-border);
    color: var(--manage-muted);
    display: inline-flex;
    height: 28px;
    justify-content: center;
    opacity: 0;
    padding: 0;
    position: absolute;
    right: 8px;
    top: 8px;
    transition: opacity 120ms ease;
    width: 28px;
  }

  .manage-md-code-block:hover button,
  .manage-md-code-block button:focus-visible {
    opacity: 1;
  }

  .manage-md-code-block pre {
    background: light-dark(rgba(0, 0, 0, 0.045), rgba(255, 255, 255, 0.045));
    border: 1px solid light-dark(rgba(0, 0, 0, 0.09), rgba(255, 255, 255, 0.09));
    border-radius: 8px;
    color: light-dark(rgba(24, 24, 27, 0.88), rgba(248, 250, 252, 0.88));
    font-size: 13px;
    line-height: 1.6;
    margin: 0;
    overflow-x: auto;
    padding: 16px;
  }

  .manage-md-code-block code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  }

  .manage-md-table-wrap {
    margin: 16px 0;
    overflow-x: auto;
  }

  .manage-md-table-wrap table {
    border-collapse: collapse;
    min-width: 100%;
  }

  .manage-md-table-wrap th,
  .manage-md-table-wrap td {
    border-bottom: 1px solid var(--manage-border);
    font-size: 14px;
    padding: 8px 12px;
    text-align: left;
    vertical-align: top;
  }

  .manage-md-table-wrap th {
    background: light-dark(rgba(0, 0, 0, 0.045), rgba(255, 255, 255, 0.045));
    color: light-dark(rgba(24, 24, 27, 0.9), rgba(248, 250, 252, 0.9));
    font-weight: 700;
  }

  .manage-md-table-wrap td {
    color: light-dark(rgba(24, 24, 27, 0.8), rgba(248, 250, 252, 0.8));
  }

  .manage-md-alert,
  .manage-md-directive {
    border: 1px solid rgba(125, 211, 252, 0.26);
    border-left: 3px solid rgba(125, 211, 252, 0.72);
    margin: 16px 0;
    padding: 12px 14px;
  }

  .manage-md-alert-title {
    color: var(--manage-accent);
    font-size: 11px;
    font-weight: 780;
    margin-bottom: 6px;
    text-transform: uppercase;
  }

  .manage-md-alert[data-kind="warning"],
  .manage-md-alert[data-kind="caution"] {
    border-color: rgba(253, 230, 138, 0.3);
    border-left-color: rgba(253, 230, 138, 0.72);
  }

  .manage-md-html-block {
    color: light-dark(rgba(24, 24, 27, 0.9), rgba(248, 250, 252, 0.9));
    font-size: 15px;
    line-height: 1.625;
    margin: 16px 0;
  }

  .annotation-highlight,
  .manage-annotation-highlight {
    --manage-annotation-color: ${MANAGE_COMMENT_ANNOTATION_COLOR};
    background: color-mix(in srgb, var(--manage-annotation-color) 28%, transparent);
    color: inherit;
    padding: 0 2px;
  }

  .annotation-highlight.comment,
  .manage-annotation-highlight[data-type="comment"] {
    --manage-annotation-color: ${MANAGE_COMMENT_ANNOTATION_COLOR};
  }

  .annotation-highlight[data-label-id="clarify"],
  .manage-annotation-highlight[data-label-id="clarify"] {
    --manage-annotation-color: ${quickLabelColor('clarify')};
  }

  .annotation-highlight[data-label-id="needs-tests"],
  .manage-annotation-highlight[data-label-id="needs-tests"] {
    --manage-annotation-color: ${quickLabelColor('needs-tests')};
  }

  .annotation-highlight[data-label-id="looks-good"],
  .manage-annotation-highlight[data-label-id="looks-good"] {
    --manage-annotation-color: ${quickLabelColor('looks-good')};
  }

  .annotation-highlight.deletion,
  .manage-annotation-highlight[data-type="redline"] {
    --manage-annotation-color: ${MANAGE_REDLINE_ANNOTATION_COLOR};
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--manage-annotation-color) 82%, transparent);
    text-decoration-thickness: 2px;
  }

  .manage-annotation-dropdown {
    background: color-mix(in srgb, var(--manage-panel-raised) 94%, #000 6%);
    border: 1px solid var(--manage-border-strong);
    border-radius: 5px;
    box-shadow: 0 18px 52px rgba(0, 0, 0, 0.36);
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    max-height: min(520px, calc(100vh - 76px));
    min-height: 0;
    overflow: hidden;
    position: absolute;
    right: 0;
    top: calc(100% + 8px);
    width: min(360px, calc(100vw - 28px));
    z-index: 700;
  }

  .manage-annotation-dropdown header {
    align-items: center;
    border-bottom: 1px solid var(--manage-border);
    color: var(--manage-muted);
    display: flex;
    font-size: 12px;
    font-weight: 500;
    justify-content: space-between;
    min-height: 40px;
    padding: 0 12px;
  }

  .manage-annotation-dropdown-list {
    align-content: start;
    display: grid;
    gap: 8px;
    grid-auto-rows: max-content;
    min-height: 0;
    overflow: auto;
    padding: 10px;
  }

  .manage-preview-header-actions .manage-send-feedback-button {
    color: light-dark(#0f766e, #8ed3f3);
    flex: 0 0 auto;
    gap: 5px;
    padding: 0 8px;
    white-space: nowrap;
    width: auto;
  }

  .manage-preview-header-actions .manage-send-feedback-button:not(:disabled):hover,
  .manage-preview-header-actions .manage-send-feedback-button:not(:disabled):focus-visible {
    background: light-dark(rgba(45, 212, 191, 0.12), rgba(142, 211, 243, 0.12));
    color: light-dark(#0f766e, #b9e4f8);
  }

  .manage-preview-header-actions .manage-send-feedback-button[data-state="sent"] {
    color: light-dark(#15803d, #86efac);
  }

  .manage-preview-header-actions .manage-send-feedback-button[data-state="error"] {
    color: light-dark(#be123c, #fda4af);
  }

  .manage-preview-header-actions .manage-send-feedback-button[data-state="notice"],
  .manage-preview-header-actions .manage-send-feedback-button[data-state="sending"] {
    color: var(--manage-muted);
  }

  .manage-review-menu-shell {
    display: inline-flex;
    position: relative;
  }

  .manage-preview-header-actions .manage-review-menu-trigger {
    flex: 0 0 var(--manage-header-button-width);
    padding: 0;
    width: var(--manage-header-button-width);
  }

  /*
   * The menu lives inside the header actions, whose generic button rules size
   * every button as an icon tile and hide its text in the compact header, so
   * those rules exclude the menu's buttons and the rows style themselves.
   */
  .manage-review-menu {
    width: min(320px, calc(100vw - 28px));
  }

  .manage-review-menu-list {
    display: grid;
    gap: 2px;
    padding: 6px;
  }

  .manage-review-menu-row {
    align-items: center;
    background: transparent;
    border: 0;
    border-radius: 4px;
    color: var(--manage-text);
    cursor: pointer;
    display: grid;
    font: inherit;
    font-size: 12px;
    gap: 8px;
    grid-template-columns: 16px minmax(0, 1fr) auto;
    min-height: 30px;
    padding: 0 8px;
    text-align: left;
  }

  .manage-review-menu-row svg {
    color: var(--manage-muted);
  }

  .manage-review-menu-row:not(:disabled):hover,
  .manage-review-menu-row:not(:disabled):focus-visible {
    background: light-dark(rgba(0, 0, 0, 0.06), rgba(255, 255, 255, 0.06));
    outline: none;
  }

  .manage-review-menu-row:disabled {
    color: var(--manage-subtle);
    cursor: default;
  }

  .manage-review-menu-row:disabled svg {
    color: var(--manage-subtle);
  }

  .manage-review-menu-row-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-review-menu-row-description {
    color: var(--manage-muted);
    font-size: 10.5px;
    white-space: nowrap;
  }

  .manage-review-menu-row:disabled .manage-review-menu-row-description {
    color: var(--manage-subtle);
  }

  .manage-annotation-sent-pill {
    align-items: center;
    border: 1px solid color-mix(in srgb, var(--manage-annotation-color) 45%, transparent);
    border-radius: 999px;
    color: var(--manage-muted);
    display: inline-flex;
    font-size: 9.5px;
    font-weight: 500;
    gap: 3px;
    line-height: 1;
    margin-left: 6px;
    padding: 2px 6px;
    vertical-align: middle;
  }

  .manage-annotation-card[data-sent="true"] {
    opacity: 0.78;
  }

  .manage-annotation-highlight[data-sent="true"] {
    background: color-mix(in srgb, var(--manage-annotation-color) 14%, transparent);
    box-shadow: inset 0 -1px 0 color-mix(in srgb, var(--manage-annotation-color) 55%, transparent);
  }

  .manage-attachment-strip {
    display: grid;
    gap: 6px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .manage-attachment-chip {
    align-items: center;
    background: light-dark(rgba(0, 0, 0, 0.04), rgba(255, 255, 255, 0.04));
    border: 1px solid var(--manage-border);
    border-radius: 6px;
    display: grid;
    gap: 6px;
    grid-template-columns: 34px minmax(0, 1fr) 20px;
    margin: 0;
    min-width: 0;
    padding: 5px;
  }

  .manage-attachment-chip img,
  .manage-annotation-attachments img {
    background: light-dark(rgba(0, 0, 0, 0.06), rgba(255, 255, 255, 0.06));
    border-radius: 4px;
    height: 34px;
    object-fit: cover;
    width: 34px;
  }

  .manage-attachment-chip figcaption,
  .manage-annotation-attachments span {
    color: var(--manage-muted);
    font-size: 10px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-attachment-chip button {
    align-items: center;
    background: transparent;
    border: 0;
    color: var(--manage-muted);
    display: inline-flex;
    height: 20px;
    justify-content: center;
    padding: 0;
    width: 20px;
  }

  .manage-attachment-error {
    color: var(--manage-red);
    font-size: 11px;
    line-height: 1.35;
  }

  .manage-annotation-empty {
    color: var(--manage-subtle);
    font-size: 12px;
    padding: 12px 2px;
  }

  .manage-annotation-card {
    --manage-annotation-color: ${MANAGE_COMMENT_ANNOTATION_COLOR};
    align-self: start;
    background: color-mix(in srgb, var(--manage-panel) 96%, var(--manage-annotation-color) 4%);
    border: 1px solid color-mix(in srgb, var(--manage-annotation-color) 24%, var(--manage-border));
    border-radius: 4px;
    display: grid;
    gap: 7px;
    height: max-content;
    min-width: 0;
    padding: 9px 33px 9px 9px;
    position: relative;
  }

  .manage-annotation-card[data-type="redline"] {
    border-color: color-mix(in srgb, var(--manage-annotation-color) 28%, var(--manage-border));
  }

  .manage-annotation-card-header {
    align-items: center;
    color: var(--manage-muted);
    display: flex;
    font-size: 11px;
    font-weight: 500;
    justify-content: space-between;
  }

  .manage-annotation-card-header span {
    color: color-mix(in srgb, var(--manage-annotation-color) 72%, var(--manage-text));
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-preview-header-actions .manage-annotation-remove-button,
  .manage-annotation-remove-button,
  .manage-annotation-edit-button {
    background: transparent;
    border: 0;
    border-left: 0;
    border-radius: 3px;
    box-shadow: none;
    color: color-mix(in srgb, var(--manage-annotation-color) 48%, var(--manage-muted));
    height: 22px;
    padding: 0;
    position: absolute;
    right: 7px;
    top: 7px;
    transition: background 120ms ease, color 120ms ease;
    width: 22px;
  }

  .manage-preview-header-actions .manage-annotation-remove-button:not(:disabled):hover,
  .manage-preview-header-actions .manage-annotation-remove-button:not(:disabled):focus-visible,
  .manage-annotation-remove-button:hover,
  .manage-annotation-remove-button:focus-visible {
    background: transparent;
    border: 0;
    border-left: 0;
    color: color-mix(in srgb, var(--manage-annotation-color) 70%, var(--manage-text));
  }

  .manage-annotation-edit-button {
    right: 31px;
  }

  .manage-annotation-edit-button:hover,
  .manage-annotation-edit-button:focus-visible {
    color: color-mix(in srgb, var(--manage-annotation-color) 70%, var(--manage-text));
  }

  .manage-annotation-card blockquote {
    border-left: 2px solid color-mix(in srgb, var(--manage-annotation-color) 62%, transparent);
    color: light-dark(rgba(24, 24, 27, 0.86), rgba(248, 250, 252, 0.86));
    font-size: 12px;
    line-height: 1.45;
    margin: 0;
    max-height: 96px;
    overflow: auto;
    padding-left: 8px;
    scrollbar-color: transparent transparent;
    scrollbar-width: thin;
  }

  .manage-annotation-card blockquote::-webkit-scrollbar {
    height: 2px;
    width: 2px;
  }

  .manage-annotation-card blockquote::-webkit-scrollbar-track,
  .manage-annotation-card blockquote::-webkit-scrollbar-track-piece,
  .manage-annotation-card blockquote::-webkit-scrollbar-corner {
    background: transparent;
  }

  .manage-annotation-card blockquote::-webkit-scrollbar-thumb {
    background: transparent;
  }

  .manage-annotation-card:hover blockquote,
  .manage-annotation-card:focus-within blockquote {
    scrollbar-color: color-mix(in srgb, var(--manage-annotation-color) 58%, transparent) transparent;
  }

  .manage-annotation-card:hover blockquote::-webkit-scrollbar-thumb,
  .manage-annotation-card:focus-within blockquote::-webkit-scrollbar-thumb {
    background: color-mix(in srgb, var(--manage-annotation-color) 58%, transparent);
  }

  .manage-annotation-card[data-type="redline"] blockquote {
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--manage-annotation-color) 82%, transparent);
    text-decoration-thickness: 2px;
  }

  .manage-annotation-card p {
    color: color-mix(in srgb, var(--manage-text) 72%, var(--manage-muted));
    font-size: 12px;
    line-height: 1.45;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .manage-annotation-attachments {
    display: grid;
    gap: 6px;
  }

  .manage-annotation-attachments a {
    align-items: center;
    color: inherit;
    display: grid;
    gap: 6px;
    grid-template-columns: 34px minmax(0, 1fr);
    text-decoration: none;
  }

  .manage-markdown-selection-toolbar {
    align-items: center;
    background: var(--manage-panel-raised);
    border: 1px solid var(--manage-border-strong);
    border-radius: 8px;
    box-shadow: 0 8px 24px light-dark(rgba(0, 0, 0, 0.12), rgba(0, 0, 0, 0.34));
    display: flex;
    gap: 5px;
    max-width: calc(100vw - 36px);
    overflow: visible;
    padding: 5px;
    position: fixed;
    transform: translateX(-50%);
    z-index: 10;
  }

  .manage-markdown-selection-toolbar button {
    --manage-toolbar-action-color: var(--manage-muted);
    align-items: center;
    background: transparent;
    border: 0;
    border-radius: 6px;
    color: var(--manage-toolbar-action-color);
    display: inline-flex;
    height: var(--manage-control-height);
    justify-content: center;
    padding: 0;
    position: relative;
    width: var(--manage-control-height);
  }

  .manage-markdown-selection-toolbar button svg {
    color: currentColor;
  }

  .manage-markdown-selection-toolbar button:hover,
  .manage-markdown-selection-toolbar button:focus-visible {
    background: color-mix(in srgb, var(--manage-toolbar-action-color) 16%, transparent);
    color: var(--manage-toolbar-action-color);
    outline: none;
  }

  .manage-annotation-preview-card {
    --manage-annotation-color: ${MANAGE_COMMENT_ANNOTATION_COLOR};
    background: color-mix(in srgb, var(--manage-panel-raised) 96%, var(--manage-annotation-color) 4%);
    border: 1px solid color-mix(in srgb, var(--manage-annotation-color) 28%, var(--manage-border-strong));
    border-radius: 8px;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.42);
    color: var(--manage-text);
    display: grid;
    gap: 6px;
    max-width: calc(100vw - 24px);
    padding: 10px 36px 10px 12px;
    pointer-events: none;
    position: fixed;
    z-index: 39;
  }

  .manage-annotation-preview-card header {
    align-items: center;
    display: flex;
    font-size: 10px;
    font-weight: 500;
    justify-content: space-between;
    letter-spacing: 0;
    line-height: 1.1;
    text-transform: uppercase;
  }

  .manage-annotation-preview-card header span:first-child {
    color: color-mix(in srgb, var(--manage-annotation-color) 76%, var(--manage-text));
  }

  .manage-annotation-preview-card header span:last-child {
    color: var(--manage-muted);
    font-weight: 500;
    text-transform: none;
  }

  .manage-annotation-preview-remove-button {
    background: transparent;
    border: 0;
    box-shadow: none;
    color: color-mix(in srgb, var(--manage-annotation-color) 48%, var(--manage-muted));
    pointer-events: auto;
    position: absolute;
    right: 7px;
    top: 7px;
    transition: background 120ms ease, color 120ms ease;
  }

  .manage-annotation-preview-remove-button:hover,
  .manage-annotation-preview-remove-button:focus-visible {
    background: transparent;
    color: color-mix(in srgb, var(--manage-annotation-color) 70%, var(--manage-text));
  }

  .manage-annotation-preview-card p {
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    color: color-mix(in srgb, var(--manage-text) 88%, var(--manage-muted));
    display: -webkit-box;
    font-size: 12px;
    line-height: 1.4;
    margin: 0;
    overflow: hidden;
  }

  .manage-comment-popover {
    background: var(--manage-panel-strong);
    border: 1px solid var(--manage-border-strong);
    border-radius: 10px;
    box-shadow:
      0 12px 32px light-dark(rgba(0, 0, 0, 0.12), rgba(0, 0, 0, 0.4)),
      0 2px 6px light-dark(rgba(0, 0, 0, 0.06), rgba(0, 0, 0, 0.2));
    color: var(--manage-text);
    display: grid;
    gap: 10px;
    max-height: calc(100vh - 24px);
    overflow: auto;
    padding: 34px 12px 12px;
    position: fixed;
    z-index: 710;
  }

  .manage-comment-popover-close {
    border-radius: 4px;
    color: var(--manage-muted);
    height: 24px;
    position: absolute;
    right: 8px;
    top: 8px;
    width: 24px;
  }

  .manage-comment-popover-close:hover,
  .manage-comment-popover-close:focus-visible {
    background: light-dark(rgba(0, 0, 0, 0.075), rgba(255, 255, 255, 0.075));
    color: var(--manage-text);
    outline: none;
  }

  .manage-comment-popover textarea {
    background: var(--manage-panel);
    border: 1px solid var(--manage-border-strong);
    border-radius: 8px;
    color: var(--manage-text);
    font-size: 12px;
    height: 116px;
    line-height: 1.45;
    min-height: 80px;
    outline: 0;
    padding: 10px;
    resize: vertical;
  }

  .manage-comment-popover textarea::placeholder {
    color: var(--manage-subtle);
    opacity: 1;
  }

  .manage-comment-popover textarea:focus {
    border-color: var(--ring);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ring) 20%, transparent);
  }

  .manage-comment-popover-actions {
    align-items: center;
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .manage-comment-popover-actions .manage-comment-popover-submit-chord {
    background: light-dark(rgba(0, 0, 0, 0.055), rgba(255, 255, 255, 0.07));
    border: 1px solid var(--manage-border);
    border-radius: 4px;
    color: var(--manage-muted);
    font: inherit;
    font-size: 11px;
    line-height: 1;
    padding: 3px 5px;
  }

  .manage-comment-popover-actions button {
    border-radius: 7px;
    color: var(--manage-text);
    height: var(--manage-control-height);
    padding: 0 11px;
  }

  .manage-comment-popover-actions .manage-comment-popover-image-button {
    background: light-dark(rgba(0, 0, 0, 0.055), rgba(255, 255, 255, 0.055));
    border-color: var(--manage-border-strong);
  }

  .manage-comment-popover-actions .manage-comment-popover-submit {
    background: light-dark(rgba(0, 0, 0, 0.055), rgba(255, 255, 255, 0.055));
    border-color: var(--manage-border-strong);
  }

  .manage-comment-popover-actions .manage-comment-popover-submit:disabled {
    background: var(--manage-panel-raised);
    border-color: var(--manage-border);
    color: var(--manage-subtle);
  }

  .manage-hidden-file-input {
    display: none;
  }

  .manage-drawing-editor {
    background: light-dark(#fafafa, #101112);
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    min-height: 0;
    position: relative;
  }

  .manage-drawing-editor .excalidraw {
    min-height: 0;
  }

  .manage-drawing-error {
    align-items: center;
    background: rgba(253, 164, 175, 0.12);
    border: 1px solid rgba(253, 164, 175, 0.3);
    color: var(--manage-red);
    display: flex;
    font-size: 12px;
    gap: 7px;
    left: 12px;
    max-width: calc(100% - 24px);
    padding: 7px 9px;
    position: absolute;
    top: 12px;
    z-index: 3;
  }

  .manage-drawing-source {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
  }

  .manage-preview-message {
    align-items: center;
    color: var(--manage-muted);
    display: flex;
    gap: 10px;
    height: 100%;
    justify-content: center;
    min-height: 140px;
    padding: 24px;
  }

  .manage-preview-message span {
    font-size: 13px;
    font-weight: 500;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  /*
   * CDXC:Docs 2026-09-16 WHY:
   * This block used to reset the header padding below 960px, which outranked the room the header keeps for the sidebar corner button and let the file icon slide under it.
   * The header no longer changes shape at narrow widths, so only the label collapse remains here.
   * Send keeps "Send 5" / "Copy 5" until 560px; other header labels still collapse here.
   */
  @media (max-width: 960px) {
    .manage-preview-content[data-kind="markdown"] .manage-preview-header-actions button:where(:not(.manage-review-menu button):not(.manage-send-feedback-button)) span:not(.manage-count-badge):not(.manage-file-change-indicator) {
      display: none;
    }

    .manage-markdown-review {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  @media (max-width: 560px) {
    .manage-preview-header-actions .manage-send-feedback-button {
      padding: 0;
      width: var(--manage-header-button-width);
    }

    .manage-preview-header-actions .manage-send-feedback-button > span {
      display: none;
    }
  }

  @media (max-width: 760px) {
    .manage-shell:not([data-sidebar-hidden="true"]):not([data-sidebar-floating="true"]) {
      grid-template-columns: min(var(--manage-sidebar-width), 42%) minmax(0, 1fr);
    }

    .manage-shell:not([data-sidebar-hidden="true"]):not([data-sidebar-floating="true"])[data-sidebar-side="right"] {
      grid-template-columns: minmax(0, 1fr) min(var(--manage-sidebar-width), 42%);
    }

    .manage-shell[data-sidebar-hidden="true"],
    .manage-shell[data-sidebar-floating="true"],
    .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] {
      grid-template-columns: minmax(0, 1fr);
    }

    .manage-preview-path,
    .manage-text-editor,
    .manage-markdown-document {
      padding-left: 14px;
      padding-right: 14px;
    }
  }
`;
