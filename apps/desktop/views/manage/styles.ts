import {
  MANAGE_COMMENT_ANNOTATION_COLOR,
  MANAGE_MEO_CODE_BLOCK_BACKGROUND,
  MANAGE_MEO_CODE_COLOR,
  MANAGE_MEO_HEADING_COLOR,
  MANAGE_MEO_VARIABLE_COLOR,
  MANAGE_REDLINE_ANNOTATION_COLOR,
} from './constants';
import { quickLabelColor } from './annotation-store';

export const MANAGE_STYLES = `
  :root {
    color-scheme: dark;
    --manage-bg: #0e0e0e;
    --manage-panel: #0e0e0e;
    --manage-panel-strong: #181818;
    --manage-panel-raised: #202020;
    --manage-border: color-mix(in srgb, #ffffff 11%, transparent);
    --manage-border-strong: rgba(255, 255, 255, 0.12);
    --manage-text: #c8cdd5;
    --manage-muted: #a6adb6;
    --manage-subtle: #747b85;
    --manage-accent: #95d7f6;
    --manage-accent-muted: rgba(255, 255, 255, 0.055);
    --manage-row-surface: #202020;
    /*
     * CDXC:Docs 2026-08-23:
     * One control height for every free-standing in-row control (inputs,
     * buttons, dropdowns that share a row), matching the Kanban/Automate
     * 32px convention. Header bars keep their own 35px full-height segments.
     */
    --manage-control-height: 32px;
    --manage-green: #86efac;
    --manage-red: #fda4af;
    --manage-yellow: #fde68a;
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

  body {
    color: var(--manage-text);
    font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif;
  }

  button,
  input,
  textarea {
    font: inherit;
  }

  .manage-shell {
    background: var(--manage-bg);
    display: grid;
    grid-template-columns: var(--manage-sidebar-width, 292px) 5px minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    position: relative;
    width: 100%;
  }

  .manage-shell[data-sidebar-side="right"] {
    grid-template-columns: minmax(0, 1fr) 5px var(--manage-sidebar-width, 292px);
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
   * When the Manage page is below 690px wide, Docs should use a floating sidebar above the full-width preview instead of squeezing the preview into a second grid column. Keep the sidebar side preference for which edge the floating panel opens from, and let outside clicks hide it.
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

  .manage-sidebar {
    background: var(--manage-panel);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    grid-column: 1;
    grid-row: 1;
    min-height: 0;
    min-width: 0;
    padding: 0 0 7px;
  }

  .manage-shell[data-sidebar-side="right"] .manage-sidebar {
    grid-column: 3;
    grid-row: 1;
  }

  .manage-shell[data-sidebar-floating="true"] .manage-sidebar {
    border-right: 1px solid var(--manage-border);
    bottom: 0;
    box-shadow:
      16px 0 36px rgba(0, 0, 0, 0.42),
      4px 0 14px rgba(0, 0, 0, 0.26);
    grid-column: 1;
    grid-row: 1;
    left: 0;
    max-width: calc(100% - 34px);
    position: absolute;
    top: 0;
    width: min(var(--manage-sidebar-width, 292px), calc(100% - 34px));
    z-index: 650;
  }

  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-sidebar {
    border-left: 1px solid var(--manage-border);
    border-right: 0;
    box-shadow:
      -16px 0 36px rgba(0, 0, 0, 0.42),
      -4px 0 14px rgba(0, 0, 0, 0.26);
    left: auto;
    right: 0;
  }

  .manage-sidebar-resizer {
    background: var(--manage-bg);
    cursor: ew-resize;
    grid-column: 2;
    grid-row: 1;
    min-width: 5px;
    outline: none;
    position: relative;
    touch-action: none;
  }

  .manage-sidebar-resizer::before {
    background: #212121;
    content: "";
    bottom: 0;
    position: absolute;
    right: 0;
    top: 0;
    width: 1px;
  }

  .manage-shell[data-sidebar-side="right"] .manage-sidebar-resizer::before {
    left: 0;
    right: auto;
  }

  .manage-sidebar-resizer::after {
    background: #ffffff;
    bottom: 0;
    content: "";
    opacity: 0;
    position: absolute;
    right: 0;
    top: 0;
    transition: opacity 180ms ease-out 50ms;
    width: 3px;
  }

  .manage-shell[data-sidebar-side="right"] .manage-sidebar-resizer::after {
    left: 0;
    right: auto;
  }

  .manage-sidebar-resizer:hover::after,
  .manage-sidebar-resizer:focus-visible::after {
    opacity: 1;
  }

  .manage-shell[data-sidebar-floating="true"] .manage-sidebar-resizer {
    display: none;
  }

  .manage-preview {
    grid-column: 3;
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
    gap: 8px;
    height: 35px;
    justify-content: flex-end;
    max-height: 35px;
    min-height: 35px;
    overflow: visible;
    padding: 0;
  }

  .manage-sidebar-header[data-root-drop-target="true"] {
    background: color-mix(in srgb, var(--manage-text) 8%, transparent);
  }

  .manage-sidebar-actions {
    align-items: center;
    align-self: stretch;
    display: inline-flex;
    flex: 0 0 auto;
    gap: 0;
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
    border-radius: 0;
    box-shadow: none;
    box-sizing: border-box;
    color: rgba(255, 255, 255, 0.84);
    height: 35px;
    max-height: 35px;
    min-height: 35px;
    padding: 0;
    width: 38px;
  }

  .manage-sidebar-header .manage-icon-button {
    border-left: 1px solid #252525;
    width: 42px;
  }

  /*
   * CDXC:Docs 2026-06-30-04:55:
   * Docs sidebar header actions should now use the same 42px width, including the rightmost action, so the top control strip stays evenly spaced.
   */
  .manage-sidebar-header .manage-icon-button:last-child {
    width: 42px;
  }

  .manage-sidebar-restore-button {
    border-right: 1px solid #252525;
  }

  .manage-sidebar-header .manage-icon-button:not(:disabled):hover,
  .manage-sidebar-header .manage-icon-button:not(:disabled):focus-visible,
  .manage-sidebar-header .manage-icon-button[aria-expanded="true"],
  .manage-sidebar-restore-button:not(:disabled):hover,
  .manage-sidebar-restore-button:not(:disabled):focus-visible {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.96);
    outline: none;
  }

  .manage-sidebar-header .manage-icon-button:disabled {
    background: transparent;
    color: rgba(255, 255, 255, 0.34);
    cursor: default;
  }

  .manage-sidebar-header .manage-icon-button svg,
  .manage-sidebar-restore-button svg {
    height: 16px;
    width: 16px;
  }

  .manage-sidebar-tree-toggle svg {
    height: 14px;
    transform: rotate(90deg);
    width: 14px;
  }

  /*
   * CDXC:AppModal 2026-08-26:
   * Docs dropdown panels follow the app's rounded menu language — 8px panels
   * with 6px item rows — the same radii the shared modal tokens use for menu
   * surfaces (--gx-modal-radius-control / --gx-modal-radius-menu).
   */
  .manage-sidebar-menu {
    backdrop-filter: blur(18px);
    background: #0e0e0e;
    border: 1px solid #595959;
    border-radius: 8px;
    box-shadow:
      0 18px 42px rgba(0, 0, 0, 0.38),
      0 4px 12px rgba(0, 0, 0, 0.28),
      inset 0 1px 0 rgba(255, 255, 255, 0.08);
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
    color: rgba(244, 244, 245, 0.88);
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
    color: rgba(244, 244, 245, 0.72);
    flex: 0 0 auto;
    height: 15px;
    width: 15px;
  }

  .manage-sidebar-menu-item:hover,
  .manage-sidebar-menu-item:focus-visible {
    background: rgba(255, 255, 255, 0.105);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.045);
    color: rgba(250, 250, 250, 0.98);
    outline: none;
  }

  .manage-sidebar-menu-item:hover svg,
  .manage-sidebar-menu-item:focus-visible svg {
    color: rgba(250, 250, 250, 0.92);
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
    left: 0;
    position: absolute;
    top: 0;
    z-index: 5;
  }

  .manage-shell[data-sidebar-side="right"] .manage-sidebar-restore-button {
    border-left: 1px solid #252525;
    border-right: 0;
    left: auto;
    right: 0;
  }

  .manage-shell[data-sidebar-hidden="true"] .manage-preview-header,
  .manage-shell[data-sidebar-floating="true"] .manage-preview-header {
    padding-left: 51px;
  }

  .manage-shell[data-sidebar-hidden="true"][data-sidebar-side="right"] .manage-preview-header,
  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-preview-header {
    padding-left: 16px;
    padding-right: 51px;
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
    padding-right: 38px;
  }

  .manage-search {
    align-items: center;
    background: transparent;
    border: 0;
    border-bottom: 1px solid #292929;
    box-sizing: border-box;
    display: flex;
    gap: 11px;
    height: var(--manage-control-height);
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
    line-height: 18px;
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

  .manage-file-list {
    min-height: 0;
    overflow: auto;
    padding: 0 0 10px;
    position: relative;
    scrollbar-color: transparent transparent;
    scrollbar-width: thin;
  }

  .manage-file-list:hover,
  .manage-file-list:focus-within {
    scrollbar-color: rgba(255, 255, 255, 0.38) transparent;
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
    background: rgba(255, 255, 255, 0.38);
  }

  .manage-file-list::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.54);
  }

  .manage-file-list::before {
    background: #c8cdd5;
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
    color: var(--manage-muted);
    display: grid;
    gap: 9px;
    grid-template-columns: 14px 16px minmax(0, 1fr) auto;
    min-height: 29px;
    padding: 4px 7px 4px calc(9px + (var(--depth) * 18px));
    position: relative;
    text-align: left;
    width: 100%;
  }

  .manage-file-row:hover,
  .manage-file-row:focus-visible {
    background: color-mix(in srgb, var(--manage-text) 8%, transparent);
    color: var(--manage-text);
    outline: none;
  }

  .manage-file-row[data-kind="directory"] {
    color: var(--manage-muted);
    font-weight: 300;
  }

  .manage-file-row[data-kind="directory"][data-active-descendant="true"] {
    color: #ffffff;
  }

  .manage-file-row[data-selected="true"] {
    background: color-mix(in srgb, var(--manage-row-surface) 72%, transparent);
    color: #ffffff;
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
  }

  .manage-file-name {
    font-size: 15.55px;
    font-weight: 300;
    line-height: 18px;
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
    background: rgba(253, 230, 138, 0.14);
    border: 1px solid rgba(253, 230, 138, 0.32);
    border-radius: 4px;
    color: var(--manage-yellow);
    display: inline-flex;
    font-size: 10px;
    font-weight: 500;
    height: 17px;
    justify-content: center;
    min-width: 17px;
    padding: 0 5px;
  }

  /*
   * CDXC:Docs 2026-08-08:
   * Docs uses the shared sidebar context-menu stylesheet and class contract.
   * Keep only Docs-specific tokens and nested-row layout here so its menu
   * surface, spacing, square corners, hover, dividers, and danger rows cannot
   * drift from the GPUI sidebar menu again.
   */
  .sidebar-context-menu-backdrop {
    background: transparent;
    border: 0;
    cursor: default;
    inset: 0;
    margin: 0;
    padding: 0;
    position: fixed;
    z-index: 60;
  }

  .manage-file-context-menu {
    --app-border: var(--manage-border);
    --app-card: var(--manage-panel);
    --app-context-menu-hover-background: var(--manage-row-surface);
    /*
     * CDXC:AppModal 2026-08-26:
     * Docs-scoped only. The shared sidebar-context-menu sheet stays square for
     * every other surface that uses it; rounding is applied through the Docs
     * class so this page's menus match the Docs dropdown language.
     */
    border-radius: 8px;
    color: var(--manage-text);
    font-size: 12px;
    font-weight: 400;
  }

  .manage-file-context-menu-item {
    border-radius: 6px;
    line-height: 16px;
  }

  .manage-file-context-menu-item svg {
    color: currentColor;
  }

  .manage-file-context-menu-nested {
    display: grid;
    gap: 2px;
  }

  .manage-file-context-menu-subitem {
    padding-left: 28px;
  }

  .manage-file-context-menu-spacer {
    flex: 1 1 auto;
    min-width: 10px;
  }

  .manage-file-context-menu-item .manage-file-context-menu-chevron {
    height: 13px;
    transform: rotate(0deg);
    transition: transform 120ms ease;
    width: 13px;
  }

  .manage-file-context-menu-item .manage-file-context-menu-chevron[data-open="true"] {
    transform: rotate(90deg);
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
    grid-template-rows: auto auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .manage-preview-content[data-compact-header="true"] {
    grid-template-rows: auto minmax(0, 1fr);
  }

  .manage-preview-header {
    align-items: center;
    border-bottom: 1px solid var(--manage-border);
    box-sizing: border-box;
    display: flex;
    gap: 8px;
    height: 35px;
    max-height: 35px;
    min-height: 35px;
    overflow: visible;
    padding: 0 0 0 13px;
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

  .manage-preview-title span {
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
    align-items: stretch;
    align-self: stretch;
    display: inline-flex;
    flex: 0 0 auto;
    gap: 0;
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
    border-bottom: 1px solid rgba(255, 255, 255, 0.055);
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
    color: rgba(248, 250, 252, 0.88);
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

  .manage-preview-header-actions button,
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

  .manage-preview-header-actions button,
  .manage-comment-popover-actions button {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--manage-border);
    color: var(--manage-muted);
    height: var(--manage-control-height);
    padding: 0 8px;
  }

  .manage-preview-header-actions button {
    background: transparent;
    border: 0;
    border-left: 1px solid #252525;
    border-radius: 0;
    box-shadow: none;
    box-sizing: border-box;
    color: rgba(255, 255, 255, 0.84);
    font-size: 10.5px;
    font-weight: 500;
    height: 35px;
    line-height: 35px;
    max-height: 35px;
    min-height: 35px;
    min-width: 38px;
    padding: 0 10px;
  }

  .manage-preview-header-actions button:not(:disabled):hover,
  .manage-preview-header-actions button:not(:disabled):focus-visible,
  .manage-comment-popover-actions button:not(:disabled):hover,
  .manage-comment-popover-actions button:not(:disabled):focus-visible {
    background: rgba(125, 211, 252, 0.12);
    border-color: rgba(125, 211, 252, 0.32);
    color: var(--manage-text);
    outline: none;
  }

  .manage-preview-header-actions button:not(:disabled):hover,
  .manage-preview-header-actions button:not(:disabled):focus-visible,
  .manage-preview-header-actions button[aria-expanded="true"],
  .manage-preview-header-actions .manage-annotation-toggle[aria-pressed="true"] {
    background: rgba(255, 255, 255, 0.08);
    border-color: #252525;
    color: rgba(255, 255, 255, 0.96);
    outline: none;
  }

  .manage-preview-header-actions button:disabled,
  .manage-comment-popover-actions button:disabled {
    color: var(--manage-subtle);
  }

  .manage-preview-header-actions button:disabled {
    background: transparent;
    color: rgba(255, 255, 255, 0.34);
    cursor: default;
  }

  .manage-preview-header-actions button:disabled:hover {
    background: transparent;
    color: rgba(255, 255, 255, 0.34);
  }

  .manage-preview-header-actions .manage-annotation-toggle[aria-pressed="true"] {
    border-left-color: #252525;
  }

  .manage-preview-header-actions .manage-clear-annotations-button[data-confirming="true"] {
    background: rgba(244, 63, 94, 0.13);
    border-color: rgba(244, 63, 94, 0.34);
    color: #fda4af;
  }

  .manage-preview-header-actions .manage-clear-annotations-button[data-confirming="true"]:not(:disabled):hover,
  .manage-preview-header-actions .manage-clear-annotations-button[data-confirming="true"]:not(:disabled):focus-visible {
    background: rgba(244, 63, 94, 0.18);
    border-color: rgba(244, 63, 94, 0.46);
    color: #fecdd3;
  }

  .manage-preview-header-actions .manage-annotation-dropdown-trigger {
    padding: 0 9px;
  }

  .manage-preview-header-actions .manage-count-badge {
    height: 17px;
    min-width: 17px;
    padding: 0 4px;
  }

  .manage-preview-header-actions button svg {
    height: 16px;
    width: 16px;
  }

  .manage-preview-header-actions .manage-file-reload-button {
    padding: 0 10px;
    position: relative;
  }

  .manage-file-change-indicator {
    background: #fbbf24;
    border: 1px solid #0e0e0e;
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
    background: #101112;
    box-sizing: border-box;
    color: rgba(248, 250, 252, 0.9);
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
  .manage-meo-markdown-editor .mode-toolbar {
    box-sizing: border-box;
    display: flex;
    flex: 0 0 auto;
    gap: 8px;
    inline-size: 100%;
    max-inline-size: 100%;
    min-width: 0;
    overflow: visible;
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
    background: rgba(255, 255, 255, 0.025);
    border-color: rgba(255, 255, 255, 0.16);
    border-radius: 9px;
    gap: 2px;
  }

  .manage-meo-markdown-editor .mode-button {
    color: var(--manage-muted);
    min-width: 64px;
  }

  .manage-meo-markdown-editor .manage-mode-toggle {
    min-width: 76px;
  }

  .manage-meo-markdown-editor .mode-button[aria-selected="true"],
  .manage-meo-markdown-editor .mode-button.is-active {
    background: rgba(125, 211, 252, 0.18);
    box-shadow: inset 0 0 0 1px rgba(125, 211, 252, 0.34);
    color: var(--manage-text);
  }

  .manage-meo-markdown-editor .mode-button[aria-selected="false"]:hover,
  .manage-meo-markdown-editor .mode-button:not(.is-active):hover {
    background: rgba(255, 255, 255, 0.07);
    color: var(--manage-text);
  }

  .manage-meo-markdown-editor .table-grid-cell {
    appearance: none;
    padding: 0;
  }

  .manage-selection-inline-mode-button {
    color: ${MANAGE_MEO_HEADING_COLOR};
  }

  .manage-meo-markdown-editor .cm-line:not(.meo-md-code-block):not(.meo-src-code-block):not(.meo-mermaid-block) .meo-md-inline-code,
  .manage-meo-markdown-editor .cm-line:not(.meo-md-code-block):not(.meo-src-code-block):not(.meo-mermaid-block) .meo-md-inline-code * {
    background: ${MANAGE_MEO_CODE_BLOCK_BACKGROUND} !important;
    color: ${MANAGE_MEO_CODE_COLOR} !important;
    -webkit-text-fill-color: ${MANAGE_MEO_CODE_COLOR} !important;
  }

  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block),
  .manage-meo-markdown-editor .cm-line.meo-md-alert:is(.meo-md-code-block, .meo-src-code-block) {
    background: ${MANAGE_MEO_CODE_BLOCK_BACKGROUND} !important;
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
  }

  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block):has(+ .cm-line:not(.meo-md-code-block):not(.meo-src-code-block)),
  .manage-meo-markdown-editor .cm-content > .cm-line:is(.meo-md-code-block, .meo-src-code-block):last-child {
    --manage-code-block-bottom-border: var(--manage-border-strong);
  }

  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block) [style*="#fde68a" i],
  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block) [style*="rgb(253, 230, 138)" i],
  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block) [style*="#c084fc" i],
  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block) [style*="rgb(192, 132, 252)" i] {
    color: ${MANAGE_MEO_VARIABLE_COLOR} !important;
    -webkit-text-fill-color: ${MANAGE_MEO_VARIABLE_COLOR} !important;
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
    background: #101112;
    height: 100%;
  }

  .manage-meo-markdown-editor .cm-scroller {
    scrollbar-color: rgba(148, 163, 184, 0.35) transparent;
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
    padding-right: 12px;
  }

  .manage-markdown-document {
    color: rgba(248, 250, 252, 0.9);
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
    color: ${MANAGE_MEO_HEADING_COLOR};
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
    background: rgba(255, 255, 255, 0.07);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 4px;
    color: rgba(248, 250, 252, 0.92);
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
    color: rgba(248, 250, 252, 0.9);
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
    background: rgba(255, 255, 255, 0.08);
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
    background: rgba(255, 255, 255, 0.045);
    border: 1px solid rgba(255, 255, 255, 0.09);
    border-radius: 8px;
    color: rgba(248, 250, 252, 0.88);
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
    background: rgba(255, 255, 255, 0.045);
    color: rgba(248, 250, 252, 0.9);
    font-weight: 700;
  }

  .manage-md-table-wrap td {
    color: rgba(248, 250, 252, 0.8);
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
    color: rgba(248, 250, 252, 0.9);
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

  .manage-attachment-strip {
    display: grid;
    gap: 6px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .manage-attachment-chip {
    align-items: center;
    background: rgba(255, 255, 255, 0.04);
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
    background: rgba(255, 255, 255, 0.06);
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
  .manage-annotation-remove-button {
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

  .manage-annotation-card blockquote {
    border-left: 2px solid color-mix(in srgb, var(--manage-annotation-color) 62%, transparent);
    color: rgba(248, 250, 252, 0.86);
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
    box-shadow: 0 14px 40px rgba(0, 0, 0, 0.34);
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
    background: color-mix(in srgb, var(--manage-panel-raised) 76%, #000 24%);
    border: 1px solid color-mix(in srgb, var(--manage-border-strong) 74%, #000 26%);
    border-radius: 10px;
    box-shadow: 0 20px 54px rgba(0, 0, 0, 0.44);
    display: grid;
    gap: 10px;
    max-height: calc(100vh - 24px);
    overflow: auto;
    padding: 34px 12px 12px;
    position: fixed;
    z-index: 710;
  }

  .manage-comment-popover-close {
    color: var(--manage-muted);
    height: 24px;
    position: absolute;
    right: 8px;
    top: 8px;
    width: 24px;
  }

  .manage-comment-popover-close:hover,
  .manage-comment-popover-close:focus-visible {
    background: rgba(255, 255, 255, 0.075);
    color: var(--manage-text);
    outline: none;
  }

  .manage-comment-popover textarea {
    background: color-mix(in srgb, var(--manage-panel) 72%, #000 28%);
    border: 1px solid var(--manage-border-strong);
    border-radius: 8px;
    color: var(--manage-text);
    font-size: 12px;
    height: 116px;
    line-height: 1.45;
    outline: 0;
    padding: 10px;
    resize: vertical;
  }

  .manage-comment-popover textarea:focus {
    border-color: rgba(125, 211, 252, 0.46);
    box-shadow: 0 0 0 1px rgba(125, 211, 252, 0.16);
  }

  .manage-comment-popover-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .manage-comment-popover-actions button {
    border-radius: 7px;
    color: var(--manage-text);
    height: var(--manage-control-height);
    padding: 0 11px;
  }

  .manage-comment-popover-actions .manage-comment-popover-image-button {
    background: rgba(255, 255, 255, 0.055);
    border-color: var(--manage-border-strong);
  }

  .manage-comment-popover-actions .manage-comment-popover-submit {
    background: rgba(34, 197, 94, 0.18);
    border-color: rgba(74, 222, 128, 0.48);
    color: #bbf7d0;
  }

  .manage-comment-popover-actions .manage-comment-popover-submit:not(:disabled):hover,
  .manage-comment-popover-actions .manage-comment-popover-submit:not(:disabled):focus-visible {
    background: rgba(34, 197, 94, 0.26);
    border-color: rgba(74, 222, 128, 0.66);
    color: #dcfce7;
  }

  .manage-comment-popover-actions .manage-comment-popover-submit:disabled {
    background: rgba(34, 197, 94, 0.08);
    border-color: rgba(74, 222, 128, 0.2);
    color: rgba(187, 247, 208, 0.42);
  }

  .manage-hidden-file-input {
    display: none;
  }

  .manage-drawing-editor {
    background: #101112;
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

  @media (max-width: 960px) {
    .manage-preview-header {
      align-items: center;
      flex-direction: row;
      gap: 8px;
      height: 35px;
      max-height: 35px;
      min-height: 35px;
      padding: 0 0 0 13px;
    }

    .manage-preview-meta {
      align-self: auto;
    }

    .manage-preview-content[data-compact-header="true"] .manage-preview-header {
      align-items: center;
      flex-direction: row;
      gap: 8px;
      height: 35px;
      max-height: 35px;
      min-height: 35px;
      padding: 0 0 0 13px;
    }

    .manage-preview-content[data-compact-header="true"] .manage-preview-meta {
      align-self: auto;
    }

    .manage-preview-content[data-kind="markdown"] .manage-preview-header-actions button span:not(.manage-count-badge):not(.manage-file-change-indicator) {
      display: none;
    }

    .manage-markdown-review {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  @media (max-width: 760px) {
    .manage-shell:not([data-sidebar-hidden="true"]):not([data-sidebar-floating="true"]) {
      grid-template-columns: minmax(190px, 42%) 5px minmax(0, 1fr);
    }

    .manage-shell:not([data-sidebar-hidden="true"]):not([data-sidebar-floating="true"])[data-sidebar-side="right"] {
      grid-template-columns: minmax(0, 1fr) 5px minmax(190px, 42%);
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
