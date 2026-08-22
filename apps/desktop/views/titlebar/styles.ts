export const TITLEBAR_STYLES = `
  @keyframes titlebar-git-spin {
    to {
      transform: rotate(360deg);
    }
  }
  .titlebar-open-menu {
    /**
     * CDXC:TitlebarMenus 2026-05-28-13:52:
     * Titlebar dropdown surfaces should match the unified app overlay
     * background instead of using the older #181818 menu shell.
     *
     * CDXC:SidebarTheme 2026-06-15-01:43:
     * Titlebar dropdowns follow --app-dropdown-background so Dark 1 uses
     * #191919, Dark 2 preserves #0e0e0e, and Light uses a light overlay.
     */
    background: var(--app-dropdown-background) !important;
    background-color: var(--app-dropdown-background) !important;
    border: 1px solid rgba(255,255,255,0.14);
    box-shadow: 0 18px 42px rgba(0,0,0,0.44);
  }
  /*
   * CDXC:ReactTitlebar 2026-06-11-13:22:
   * Native child-window dropdowns reuse the existing web menu components, but
   * their document is the panel itself rather than Radix portal content inside
   * the titlebar WKWebView. Remove portal-era viewport offsets so the Swift
   * child window owns placement.
   *
   * CDXC:ReactTitlebar 2026-06-12-02:50:
   * Native panels are still sized before they open, but compact dropdown height
   * now comes from the rendered option count while Tips/Resources keep their
   * larger reading surfaces. The React panel fills the child WebView exactly
   * without ResizeObserver-driven native resize messages after open.
   */
  .titlebar-dropdown-panel-root {
    background: var(--app-dropdown-background);
    color: var(--foreground);
    display: block;
    height: 100vh;
    min-height: 1px;
    overflow: hidden;
    width: 100vw;
  }
  .titlebar-dropdown-panel-root .titlebar-open-menu {
    box-sizing: border-box;
    box-shadow: none;
    height: 100%;
    max-height: none;
    max-width: none;
    min-height: 0;
    min-width: 0 !important;
    overflow: auto;
    position: static;
    width: 100% !important;
  }
  .titlebar-dropdown-panel-root .titlebar-tips-menu,
  .titlebar-dropdown-panel-root .titlebar-resources-menu {
    width: 100% !important;
  }
  .titlebar-dropdown-panel-root .titlebar-tips-panel,
  .titlebar-dropdown-panel-root .titlebar-resources-panel {
    height: 100%;
    max-height: none;
    min-height: 0;
  }
  .titlebar-dropdown-panel-root .titlebar-tips-scroll,
  .titlebar-dropdown-panel-root .titlebar-resources-scroll {
    max-height: none;
    min-height: 0;
    overflow: auto;
  }
  .titlebar-dropdown-panel-root .titlebar-tips-scroll::-webkit-scrollbar,
  .titlebar-dropdown-panel-root .titlebar-resources-scroll::-webkit-scrollbar {
    width: 2px;
  }
  .titlebar-tips-menu {
    /**
     * CDXC:TipsAndTricks 2026-05-30-08:31:
     * Tips should use the same maximum dropdown height as Resources and keep
     * the authored array order on screen. The menu is a reading surface, not an
     * editor, so it stays dense and square like the Resources manager.
     *
     * CDXC:TipsAndTricks 2026-06-12-08:56:
     * The macOS Tips & Tricks child panel is 100px narrower than the Resources
     * reading panel so the guide occupies less horizontal space.
     */
    background: var(--app-dropdown-background) !important;
    background-color: var(--app-dropdown-background) !important;
    width: min(556px, calc(100vw - 24px));
    max-height: min(760px, calc(100vh - 46px));
    overflow: hidden;
  }
  .titlebar-tips-panel {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    max-height: min(760px, calc(100vh - 46px));
    overflow: hidden;
  }
  .titlebar-tips-header {
    align-items: stretch;
    border-bottom: 1px solid rgba(255,255,255,0.12);
    display: flex;
    gap: 12px;
    justify-content: space-between;
    min-height: 47px;
    padding: 0 0 0 12px;
  }
  .titlebar-tips-title,
  .titlebar-tips-actions,
  .titlebar-tips-section-heading,
  .titlebar-tip-read-button,
  .titlebar-tip-read-state {
    align-items: center;
    display: inline-flex;
  }
  .titlebar-tips-title {
    color: rgba(255,255,255,0.96);
    font: 750 14px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 8px;
    min-width: 0;
  }
  .titlebar-tips-actions {
    /*
     * CDXC:TipsAndTricks 2026-06-16-10:04:
     * Tips & Tricks header actions should use matching button widths, point to
     * Features and Setup, and omit the previous unread
     * text summary from the top-right action row.
     *
     * CDXC:TipsAndTricks 2026-06-16-19:42:
     * Add the release-updates action as the rightmost equal-width header action
     * so release notes are available without changing the existing titlebar Tips
     * layout model.
     *
     * CDXC:TipsAndTricks 2026-06-18-04:53:
     * Add Docs as a fourth equal-width action and keep the labels short enough
     * that all actions fit in the native titlebar dropdown.
     *
     * CDXC:TipsAndTricks 2026-06-30-01:38:
     * The Tips header action buttons should fill the header height and touch side by side. Use left/right borders as the only separators so the row reads as connected titlebar chrome instead of separate inset buttons.
     *
     * CDXC:TipsAndTricks 2026-06-30-03:22:
     * The rightmost Tips header action should sit flush with the panel edge, the idle buttons should have no fill, and every action should share the widest button's width with only 15px of side padding.
     *
     * CDXC:TipsAndTricks 2026-06-30-04:28:
     * The visible Tips action labels should stay compact: Video opens the tutorial video, and Updates opens the releases changelog. Short labels keep the equal-width action columns from widening the dropdown header.
     */
    align-self: stretch;
    align-items: stretch;
    display: grid;
    gap: 0;
    grid-template-columns: repeat(4, minmax(max-content, 1fr));
    margin-left: auto;
    width: max-content;
  }
  .titlebar-tips-action-button {
    align-items: center;
    background: transparent;
    border: 0;
    border-left: 1px solid rgba(255,255,255,0.12);
    border-radius: 0;
    box-sizing: border-box;
    color: rgba(255,255,255,0.78);
    display: inline-flex;
    gap: 6px;
    font: 750 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 100%;
    justify-content: center;
    padding: 0 15px;
    white-space: nowrap;
    width: 100%;
  }
  .titlebar-tips-action-button:last-child {
    border-right: 1px solid rgba(255,255,255,0.12);
  }
  .titlebar-tips-panel button:not(:disabled),
  .titlebar-tips-panel [role="button"]:not([aria-disabled="true"]) {
    /*
     * CDXC:TipsAndTricks 2026-06-16-10:04:
     * Every actionable control inside the Tips & Tricks panel should expose the
     * pointer cursor so clickable rows and buttons advertise their interaction.
     */
    cursor: pointer;
  }
  .titlebar-tips-action-button:not(:disabled):hover {
    background: rgba(255,255,255,0.14);
    color: rgba(255,255,255,0.94);
  }
  .titlebar-tips-action-button:disabled {
    color: rgba(255,255,255,0.3);
    cursor: default;
  }
  .titlebar-tips-scroll {
    display: grid;
    gap: 0;
    max-height: min(700px, calc(100vh - 104px));
    overflow: auto;
    padding: 8px 10px 10px;
  }
  .titlebar-tips-section + .titlebar-tips-section {
    margin-top: 10px;
  }
  .titlebar-tips-section-heading {
    align-items: center;
    color: rgba(255,255,255,0.62);
    display: flex;
    font: 750 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 6px;
    justify-content: space-between;
    letter-spacing: 0.08em;
    padding: 4px 2px 7px;
    text-transform: uppercase;
    width: 100%;
  }
  .titlebar-tips-list {
    display: grid;
    gap: 7px;
  }
  .titlebar-tip-row {
    align-items: start;
    background: rgba(255,255,255,0.025);
    border: 1px solid rgba(255,255,255,0.1);
    display: grid;
    gap: 10px;
    grid-template-columns: minmax(0, 1fr) 28px;
    min-height: 72px;
    overflow: hidden;
    padding: 9px 8px;
    transition: background 120ms ease, border-color 120ms ease;
  }
  .titlebar-tip-row[data-read="true"] {
    opacity: 0.72;
  }
  .titlebar-tip-row[data-actionable="true"]:hover {
    /*
     * CDXC:TipsAndTricks 2026-06-28-08:00:
     * Action-backed tips should read like clickable detail rows without making
     * the per-row read check part of the navigation target.
     */
    background: rgba(255,255,255,0.05);
    border-color: rgba(255,255,255,0.18);
  }
  .titlebar-tip-row-notice {
    cursor: pointer;
    grid-template-columns: 28px minmax(0, 1fr);
    text-align: left;
    transition: background 120ms ease, border-color 120ms ease;
    width: 100%;
  }
  .titlebar-tip-row-notice:hover {
    background: rgba(245,158,11,0.06);
    border-color: rgba(245,158,11,0.34);
  }
  .titlebar-tip-row-notice .titlebar-tip-icon {
    background: rgba(245,158,11,0.14);
    color: rgba(251,191,36,0.95);
  }
  .titlebar-tip-row-notice .titlebar-tip-body {
    /**
     * CDXC:CliInstall 2026-06-07-15:26:
     * Runtime notices can describe an action plus a short benefit list, but
     * Tips & Tricks should remain dense. Clamp notice descriptions to three
     * lines so the CLI accessibility warning cannot dominate the dropdown.
     */
    -webkit-line-clamp: 3;
  }
  .titlebar-tip-icon {
    align-items: center;
    align-self: start;
    background: rgba(255,255,255,0.1);
    color: rgba(255,255,255,0.84);
    display: inline-flex;
    height: 28px;
    justify-content: center;
    width: 28px;
  }
  .titlebar-tip-detail {
    align-items: start;
    display: grid;
    gap: 10px;
    grid-template-columns: 28px minmax(0, 1fr);
    min-width: 0;
    text-align: left;
  }
  .titlebar-tip-detail-button {
    appearance: none;
    background: transparent;
    border: 0;
    color: inherit;
    font: inherit;
    padding: 0;
    width: 100%;
  }
  .titlebar-tip-copy {
    display: grid;
    gap: 7px;
    min-width: 0;
  }
  .titlebar-tip-title {
    color: rgba(255,255,255,0.94);
    display: block;
    font: 700 13px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-tip-body {
    color: rgba(255,255,255,0.58);
    display: -webkit-box;
    font: 500 12px/1.35 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
  }
  .titlebar-tip-read-button,
  .titlebar-tip-read-state {
    align-self: end;
    justify-self: end;
    justify-content: center;
  }
  .titlebar-tip-read-button {
    background: rgba(255,255,255,0.14);
    border: 1px solid rgba(255,255,255,0.16);
    border-radius: 0;
    color: rgba(255,255,255,0.9);
    height: 24px;
    padding: 0;
    transition: background 120ms ease, color 120ms ease;
    width: 24px;
  }
  .titlebar-tip-read-button:hover {
    background: rgba(255,255,255,0.2);
    color: rgba(255,255,255,0.96);
  }
  .titlebar-tip-read-state {
    color: rgba(255,255,255,0.46);
    height: 24px;
    width: 24px;
  }
  .titlebar-tips-empty {
    color: rgba(255,255,255,0.54);
    font: 500 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    padding: 10px 4px;
  }
  .titlebar-resources-menu {
    /**
     * CDXC:TitlebarResources 2026-05-28-13:22:
     * The Resources manager background must match the titlebar dropdown family
     * while adjacent titlebar dropdowns keep the existing titlebar menu color.
     *
     * CDXC:SidebarTheme 2026-06-15-01:43:
     * Resources uses the dropdown token so the large child panel switches with
     * Dark 1, Dark 2, and Light.
     */
    background: var(--app-dropdown-background) !important;
    background-color: var(--app-dropdown-background) !important;
    width: min(656px, calc(100vw - 24px));
    max-height: min(760px, calc(100vh - 46px));
    overflow: hidden;
  }
  .titlebar-resources-panel {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    max-height: min(760px, calc(100vh - 46px));
    overflow: hidden;
  }
  .titlebar-resources-panel button:not(:disabled) {
    /*
     * CDXC:TitlebarResources 2026-06-16-10:36:
     * Resources should show the pointer cursor only over real button controls.
     * CPU/RAM metric chips are read-only status, so they override expandable row
     * pointer inheritance back to the default cursor below.
     *
     * CDXC:TitlebarResources 2026-06-16-12:34:
     * The Resources modal should not show a hand cursor over expandable row
     * chrome in the macOS titlebar. Keep expansion clickable through the row
     * handler, but reserve pointer cursor feedback for explicit buttons only.
     */
    cursor: pointer;
  }
  .titlebar-resources-panel button:disabled {
    cursor: default;
  }
  .titlebar-resources-header {
    align-items: center;
    border-bottom: 1px solid rgba(255,255,255,0.12);
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 11px 12px;
    position: relative;
    z-index: 2;
  }
  .titlebar-resources-title,
  .titlebar-resources-actions,
  .titlebar-resources-summary,
  .titlebar-resource-section-summary,
  .titlebar-resource-section-summary span,
  .titlebar-resources-summary span {
    align-items: center;
    display: inline-flex;
  }
  .titlebar-resources-title {
    gap: 8px;
    /*
     * CDXC:TitlebarResources 2026-06-16-00:19:
     * The Resources dropdown should use the same lighter text treatment as the
     * titlebar action menus. Keep labels, metrics, daemon status, and controls
     * visually consistent instead of mixing heavy font weights across the panel.
     */
    font: 400 14px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    min-width: 0;
  }
  .titlebar-resource-tooltip {
    background: var(--ghostex-tooltip-background, rgba(24,24,24,0.98));
    border: 1px solid var(--ghostex-tooltip-border, rgba(255,255,255,0.12));
    box-shadow: var(--ghostex-tooltip-shadow, 0 12px 30px rgba(0,0,0,0.35));
    color: var(--ghostex-tooltip-foreground, rgba(255,255,255,0.78));
    display: grid;
    font: var(--ghostex-tooltip-font, 500 12px/1.35 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif);
    gap: 3px;
    max-width: 292px;
    padding: 8px 9px;
  }
  .titlebar-resource-tooltip-title {
    color: var(--ghostex-tooltip-strong-foreground, rgba(255,255,255,0.94));
    font-weight: 760;
  }
  .titlebar-resources-actions {
    gap: 10px;
    margin-left: auto;
  }
  .titlebar-resources-info-control {
    display: inline-flex;
  }
  .titlebar-resources-info-button {
    align-items: center;
    appearance: none;
    background: rgba(255,255,255,0.08);
    border: 1px solid rgba(255,255,255,0.12);
    border-radius: 0;
    color: rgba(255,255,255,0.78);
    display: inline-flex;
    height: 24px;
    justify-content: center;
    padding: 0;
    transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
    width: 24px;
  }
  .titlebar-resources-info-button:hover,
  .titlebar-resources-info-button:focus-visible,
  .titlebar-resources-info-button[aria-expanded="true"] {
    background: rgba(255,255,255,0.14);
    border-color: rgba(255,255,255,0.18);
    color: rgba(255,255,255,0.94);
    outline: none;
  }
  .titlebar-resources-info-popover {
    background: color-mix(in srgb, var(--app-dropdown-background) 82%, #ffffff 18%) !important;
    border: 1px solid rgba(255,255,255,0.14);
    box-shadow: 0 14px 36px rgba(0,0,0,0.36);
    box-sizing: border-box;
    color: rgba(255,255,255,0.72);
    padding: 10px;
    position: absolute;
    right: 12px;
    top: calc(100% + 8px);
    width: min(620px, calc(100% - 24px));
    z-index: 5;
  }
  .titlebar-resources-collapse-all-button {
    /*
     * CDXC:TitlebarResources 2026-06-12-20:20:
     * Keep the Resources bulk section toggle visible at rest. Sleep actions
     * intentionally fade in only after header interaction, but this Resources
     * affordance is the user's fixed control immediately to their left.
     */
    align-items: center;
    appearance: none;
    background: rgba(255,255,255,0.12);
    border: 1px solid rgba(255,255,255,0.18);
    border-radius: 0;
    color: rgba(255,255,255,0.82);
    display: inline-flex;
    flex: 0 0 24px;
    height: 24px;
    justify-content: center;
    padding: 0;
    width: 24px;
  }
  .titlebar-resources-collapse-all-button:hover,
  .titlebar-resources-collapse-all-button:focus-visible {
    background: rgba(255,255,255,0.2);
    color: rgba(255,255,255,0.96);
    outline: none;
  }
  .titlebar-resources-collapse-all-button:disabled {
    cursor: default;
    opacity: 0.45;
  }
  .titlebar-resources-action-button {
    /*
     * CDXC:TitlebarResources 2026-06-12-23:37:
     * Header Sleep buttons are ordinary controls. Keep them visible and
     * hit-testable at rest; use only standard hover/disabled selectors for
     * interaction feedback.
     */
    align-items: center;
    appearance: none;
    background: rgba(255,255,255,0.08);
    border: 1px solid rgba(255,255,255,0.12);
    border-radius: 0;
    color: rgba(255,255,255,0.78);
    display: inline-flex;
    gap: 6px;
    font: 400 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 24px;
    justify-content: center;
    padding: 0 8px;
    transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
    white-space: nowrap;
  }
  .titlebar-resources-action-button[data-variant="quit"] {
    background: rgba(220,38,38,0.18);
    border-color: rgba(248,113,113,0.28);
    color: rgba(255,255,255,0.86);
  }
  .titlebar-resources-action-button:disabled {
    color: rgba(255,255,255,0.3);
    cursor: default;
    opacity: 0.55;
  }
  .titlebar-resources-action-button[data-variant="sleep"]:not(:disabled):hover {
    background: rgba(255,255,255,0.14);
    color: rgba(255,255,255,0.92);
  }
  .titlebar-resources-action-button[data-variant="quit"]:not(:disabled):hover {
    background: rgba(220,38,38,0.28);
    color: rgba(255,255,255,0.96);
  }
  .titlebar-resource-section-quit-button {
    align-items: center;
    appearance: none;
    background: rgba(220,38,38,0.18);
    border: 1px solid rgba(248,113,113,0.28);
    border-radius: 0;
    color: rgba(255,255,255,0.86);
    display: inline-flex;
    font: 400 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 24px;
    justify-content: center;
    opacity: 0;
    padding: 0 8px;
    pointer-events: none;
    transition: opacity 120ms ease, background 120ms ease, color 120ms ease;
    white-space: nowrap;
  }
  .titlebar-resource-section-quit-button[data-action="sleep"],
  .titlebar-resource-section-quit-button[data-action="stop"] {
    background: rgba(255,255,255,0.08);
    border-color: rgba(255,255,255,0.13);
  }
  .titlebar-resource-section-heading:hover .titlebar-resource-section-quit-button,
  .titlebar-resource-section-heading:focus-within .titlebar-resource-section-quit-button {
    /*
     * CDXC:TitlebarResources 2026-05-21-16:58:
     * Resource-manager Quit controls should stay available without crowding the
     * header or section chrome. Reveal destructive buttons only while the row is
     * hovered or keyboard-focused.
     *
     * CDXC:TitlebarResources 2026-05-26-13:11:
     * Sleep Project is a non-destructive project-group action, but it should
     * use the same hover reveal slot as section Quit so resource metrics remain
     * stable until the user targets the group action area.
     */
    opacity: 1;
    pointer-events: auto;
  }
  .titlebar-resource-section-quit-button[data-action="sleep"]:hover,
  .titlebar-resource-section-quit-button[data-action="stop"]:hover {
    background: rgba(255,255,255,0.14);
    color: rgba(255,255,255,0.92);
  }
  .titlebar-resource-section-quit-button[data-action="quit"]:hover {
    background: rgba(220,38,38,0.28);
    color: rgba(255,255,255,0.96);
  }
  .titlebar-resources-summary {
    color: rgba(255,255,255,0.72);
    gap: 12px;
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
  }
  .titlebar-resources-summary span {
    gap: 5px;
  }
  .titlebar-resources-scroll {
    /*
     * CDXC:TitlebarResources 2026-06-16-09:49:
     * Resources sections must stay stacked at the top of the fixed-height child
     * panel when few rows are visible. Keep implicit grid rows content-sized
     * and align the grid content to the start so spare height remains after the
     * final section instead of expanding gaps between sections.
     */
    align-content: start;
    display: grid;
    gap: 0;
    grid-auto-rows: max-content;
    max-height: min(700px, calc(100vh - 104px));
    overflow: auto;
    padding: 8px 10px 10px;
  }
  .titlebar-resources-scroll[data-loading="true"] {
    grid-template-rows: auto minmax(260px, 1fr);
  }
  .titlebar-resources-loading {
    align-items: center;
    color: rgba(255,255,255,0.58);
    display: flex;
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 8px;
    justify-content: center;
    min-height: 260px;
  }
  .titlebar-resources-loading-icon {
    animation: titlebar-git-spin 1s linear infinite;
    flex: 0 0 auto;
  }
  .titlebar-resources-info-note {
    /*
     * CDXC:TitlebarResources 2026-05-21-16:58:
     * Keep explanatory copy out of the crowded titlebar. Put the general
     * resource-usage note in the scroll body above the resource sections.
     *
     * CDXC:TitlebarResources 2026-06-16-01:08:
     * The note now appears only inside the click-triggered info dropdown next
     * to the bulk expand/collapse control, with paragraph spacing instead of
     * inline line breaks.
     *
     * CDXC:TitlebarResources 2026-06-16-01:54:
     * The popover shell owns the only card background and border. Keep this
     * inner text wrapper visually transparent so the note is not a card inside
     * another boxed surface.
     */
    color: rgba(255,255,255,0.62);
    font: 400 12px/1.35 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
  }
  .titlebar-resources-info-note p {
    margin: 0;
  }
  .titlebar-resources-info-note p + p {
    margin-top: 10px;
  }
  .titlebar-gxserver-daemon {
    /*
     * CDXC:TitlebarResources 2026-05-31-03:56:
     * The Resources dropdown must expose gxserver daemon status, version, stop/restart controls, and a small Always start checkbox without changing the sidebar session restore list.
     *
     * CDXC:TitlebarResources 2026-06-12-11:30:
     * The gxserver status headline should show the live status message (for example "gxserver is running and uses the expected protocol.") beside the state dot instead of a generic "Daemon" label, with the state/version line directly underneath.
     *
     * CDXC:TitlebarResources 2026-06-16-00:56:
     * Hide the gxserver daemon status strip in the Resources dropdown with CSS
     * only. Keep the component mounted so the surrounding daemon controls and
     * status plumbing do not need a separate conditional path.
     */
    align-items: center;
    background: rgba(255,255,255,0.045);
    border: 1px solid rgba(255,255,255,0.1);
    color: rgba(255,255,255,0.72);
    display: none;
    gap: 6px 10px;
    grid-template-columns: minmax(0, 1fr) auto;
    margin-bottom: 8px;
    min-width: 0;
    padding: 8px 10px;
  }
  .titlebar-gxserver-daemon-main,
  .titlebar-gxserver-daemon-controls {
    align-items: center;
    display: inline-flex;
    min-width: 0;
  }
  .titlebar-gxserver-daemon-main {
    gap: 8px;
  }
  .titlebar-gxserver-daemon-dot {
    background: rgba(255,255,255,0.35);
    border-radius: 999px;
    box-shadow: 0 0 0 3px rgba(255,255,255,0.05);
    flex: 0 0 auto;
    height: 7px;
    width: 7px;
  }
  .titlebar-gxserver-daemon-dot[data-state="running"] {
    background: #4ade80;
    box-shadow: 0 0 0 3px rgba(74,222,128,0.14);
  }
  .titlebar-gxserver-daemon-dot[data-state="starting"] {
    background: #facc15;
    box-shadow: 0 0 0 3px rgba(250,204,21,0.16);
  }
  .titlebar-gxserver-daemon-dot[data-state="error"],
  .titlebar-gxserver-daemon-dot[data-state="nodeUnavailable"],
  .titlebar-gxserver-daemon-dot[data-state="runtimeUnavailable"],
  .titlebar-gxserver-daemon-dot[data-state="startFailed"] {
    background: #fb7185;
    box-shadow: 0 0 0 3px rgba(251,113,133,0.16);
  }
  .titlebar-gxserver-daemon-copy {
    display: grid;
    font: 400 11px/1.25 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 1px;
    min-width: 0;
  }
  .titlebar-gxserver-daemon-copy span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-gxserver-daemon-copy span:first-child {
    color: rgba(255,255,255,0.92);
    font-weight: 400;
  }
  .titlebar-gxserver-daemon-controls {
    gap: 6px;
  }
  .titlebar-gxserver-daemon-icon-button {
    align-items: center;
    background: rgba(255,255,255,0.08);
    border: 1px solid rgba(255,255,255,0.12);
    color: rgba(255,255,255,0.78);
    display: inline-flex;
    height: 24px;
    justify-content: center;
    width: 24px;
  }
  .titlebar-gxserver-daemon-icon-button:disabled {
    color: rgba(255,255,255,0.28);
  }
  .titlebar-gxserver-daemon-icon-button:not(:disabled):hover {
    background: rgba(255,255,255,0.14);
    color: rgba(255,255,255,0.94);
  }
  .titlebar-resource-section + .titlebar-resource-section {
    margin-top: 8px;
    padding-top: 0;
  }
  .titlebar-resource-section-heading {
    align-items: center;
    color: rgba(255,255,255,0.62);
    display: flex;
    font: 400 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 6px;
    letter-spacing: 0.08em;
    padding: 4px 2px 7px;
    position: relative;
    text-transform: uppercase;
    width: 100%;
  }
  .titlebar-resource-section-label {
    align-items: center;
    color: inherit;
    display: inline-flex;
    flex: 1;
    font: inherit;
    gap: 6px;
    letter-spacing: inherit;
    min-width: 0;
    padding: 0;
    text-transform: inherit;
  }
  .titlebar-resource-section-quit-button {
    height: 22px;
    position: absolute;
    right: 2px;
    top: 2px;
  }
  .titlebar-resource-section-heading:hover .titlebar-resource-section-summary,
  .titlebar-resource-section-heading:focus-within .titlebar-resource-section-summary {
    /*
     * CDXC:TitlebarResources 2026-05-22-23:21:
     * Section-level Quit actions should replace the CPU/RAM/count metrics on
     * hover, matching resource session rows where destructive controls occupy
     * the metrics area instead of adding another right-edge control.
     */
    opacity: 0;
  }
  .titlebar-resource-collapse-button svg[data-collapsed="true"] {
    transform: rotate(-90deg);
  }
  .titlebar-resource-section-count {
    color: rgba(255,255,255,0.38);
  }
  .titlebar-resource-section-summary {
    color: rgba(255,255,255,0.52);
    gap: 10px;
    margin-left: auto;
    text-transform: none;
    transition: opacity 120ms ease;
  }
  .titlebar-resource-section-summary span {
    gap: 4px;
    letter-spacing: 0;
  }
  .titlebar-resource-section-body {
    /*
     * CDXC:TitlebarResources 2026-05-28-10:17:
     * Expanded project sections need a small gutter below the project header so
     * the hover-revealed Sleep Project button does not visually touch the first
     * resource row.
     */
    display: grid;
    gap: 7px;
    margin-top: 5px;
  }
  .titlebar-resource-bundle {
    border: 1px solid rgba(255,255,255,0.1);
    border-radius: 0;
    overflow: hidden;
    background: rgba(255,255,255,0.025);
  }
  .titlebar-resource-bundle[data-quitting="true"] {
    opacity: 0.3;
  }
  .titlebar-resource-row {
    /*
     * CDXC:TitlebarResources 2026-05-16-20:07:
     * Long session titles must not shift row controls. Keep identity controls in
     * fixed grid tracks and let only the text track shrink.
     *
     * CDXC:TitlebarResources 2026-06-13-00:56:
     * Per-item Focus and Sleep/Close buttons are fixed visible columns. Do not
     * overlay them on hover or hide metrics to reveal
     * actions; normal hover on the buttons is the only interaction treatment.
     *
     * CDXC:TitlebarResources 2026-06-13-02:07:
     * CPU and RAM should read as one usage cluster. Keep the text and action
     * tracks stable so values do not drift into the action area.
     *
     * CDXC:TitlebarResources 2026-06-16-01:10:
     * CPU and RAM must always occupy the far-right row area. Focus and
     * Sleep/Close sit immediately to the left of the metrics so usage values
     * stay aligned at the panel edge across all resource rows.
     *
     * CDXC:TitlebarResources 2026-06-16-07:37:
     * Resource row action buttons must stay on the same line as the CPU/RAM
     * cards. Explicitly pin every row item to grid row 1 so reordered or
     * conditionally missing controls cannot create a second implicit row.
     *
     * CDXC:TitlebarResources 2026-06-16-07:37:
     * CPU and RAM cards should keep the smaller collapsed-row dimensions at
     * every hierarchy level. Use one fixed metrics cluster for parent rows and
     * expanded child-process rows instead of allowing parent rows to stretch.
     */
    align-items: center;
    display: grid;
    gap: 8px;
    grid-template-columns: minmax(0, 1fr) 24px 24px 200px;
    min-height: 44px;
    overflow: hidden;
    padding: 7px 8px;
    position: relative;
  }
  .titlebar-resource-main {
    align-items: center;
    display: grid;
    gap: 8px;
    grid-column: 1;
    grid-row: 1;
    grid-template-columns: 20px 28px minmax(0, 1fr);
    min-width: 0;
  }
  .titlebar-resource-collapse-button {
    align-items: center;
    background: transparent;
    border: 0;
    color: rgba(255,255,255,0.55);
    display: inline-flex;
    height: 20px;
    justify-content: center;
    padding: 0;
    width: 20px;
  }
  .titlebar-resource-collapse-spacer {
    display: block;
    width: 20px;
  }
  .titlebar-resource-avatar {
    align-items: center;
    background: rgba(255,255,255,0.1);
    border-radius: 0;
    color: rgba(255,255,255,0.84);
    display: inline-flex;
    flex: 0 0 auto;
    font: 400 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 28px;
    justify-content: center;
    width: 28px;
  }
  .titlebar-resource-avatar svg {
    color: rgba(255,255,255,0.82);
  }
  .titlebar-resource-avatar-logo {
    /*
     * CDXC:TitlebarResources 2026-05-26-13:24:
     * Resource avatars use the Agents Hub mask-logo rendering path, so rows get
     * recognizable agent icons without changing the fixed avatar column size.
     */
    display: block;
    height: 15px;
    mask-position: center;
    mask-repeat: no-repeat;
    mask-size: contain;
    width: 15px;
    -webkit-mask-position: center;
    -webkit-mask-repeat: no-repeat;
    -webkit-mask-size: contain;
  }
  .titlebar-resource-text {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .titlebar-resource-name {
    color: rgba(255,255,255,0.94);
    font: 400 13px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-resource-main-link {
    text-decoration: none;
  }
  .titlebar-resource-main-link:hover {
    color: rgba(157, 215, 246, 0.98);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .titlebar-resource-meta {
    align-items: center;
    color: rgba(255,255,255,0.58);
    display: flex;
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }
  .titlebar-resource-meta-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .titlebar-resource-child-name {
    color: rgba(255,255,255,0.58);
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-resource-portless-action {
    background: rgba(157, 215, 246, 0.12);
    border: 1px solid rgba(157, 215, 246, 0.26);
    border-radius: 4px;
    color: rgba(201, 232, 248, 0.94);
    flex: 0 0 auto;
    font: 500 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 19px;
    padding: 0 6px;
  }
  .titlebar-resource-portless-action:hover {
    background: rgba(157, 215, 246, 0.18);
  }
  .titlebar-resource-metrics,
  .titlebar-resource-child-metrics {
    align-items: center;
    cursor: default;
    display: grid;
    gap: 8px;
    grid-template-columns: 86px 106px;
    justify-self: end;
    max-width: 200px;
    min-width: 200px;
    width: 200px;
  }
  .titlebar-resource-metrics {
    grid-column: 4;
    grid-row: 1;
  }
  .titlebar-resource-child-metrics {
    grid-column: 2;
  }
  .titlebar-resource-metric {
    align-items: center;
    background: rgba(255,255,255,0.055);
    border: 1px solid rgba(255,255,255,0.105);
    box-sizing: border-box;
    color: rgba(255,255,255,0.88);
    cursor: default;
    display: inline-flex;
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    font-variant-numeric: tabular-nums;
    gap: 6px;
    height: 24px;
    justify-content: center;
    min-width: 0;
    padding: 0 8px;
    white-space: nowrap;
    width: 100%;
  }
  .titlebar-resource-metric svg {
    color: rgba(255,255,255,0.62);
  }
  .titlebar-resource-focus-button,
  .titlebar-resource-kill-button {
    align-items: center;
    appearance: none;
    background: rgba(255,255,255,0.14);
    border: 1px solid transparent;
    border-radius: 0;
    color: rgba(255,255,255,0.9);
    display: inline-flex;
    height: 22px;
    justify-content: center;
    padding: 0;
    transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
    width: 22px;
  }
  .titlebar-resource-focus-button {
    /*
     * CDXC:TitlebarResources 2026-05-28-10:39:
     * Keep row Focus directly left of Sleep/Close in a stable action column so
     * the session label and process totals never shift.
     */
    border-color: rgba(255,255,255,0.16);
    grid-column: 2;
    grid-row: 1;
    justify-self: center;
  }
  .titlebar-resource-focus-button:hover,
  .titlebar-resource-focus-button:focus-visible {
    background: rgba(255,255,255,0.2);
    color: rgba(255,255,255,0.96);
    outline: none;
  }
  .titlebar-resource-kill-button {
    /*
     * CDXC:TitlebarResources 2026-06-14-16:50:
     * Row-level Close should carry the same neutral background, border, and
     * icon color as Sleep. The Resources modal still distinguishes the action
     * by the X icon and aria label without using a destructive red palette.
     */
    background: rgba(255,255,255,0.14);
    border-color: rgba(255,255,255,0.16);
    color: rgba(255,255,255,0.9);
    grid-column: 3;
    grid-row: 1;
    justify-self: center;
  }
  .titlebar-resource-kill-button[data-action="sleep"],
  .titlebar-resource-kill-button[data-action="stop"] {
    background: rgba(255,255,255,0.14);
    border-color: rgba(255,255,255,0.16);
    color: rgba(255,255,255,0.9);
  }
  .titlebar-resource-kill-button[data-action="sleep"]:hover,
  .titlebar-resource-kill-button[data-action="sleep"]:focus-visible,
  .titlebar-resource-kill-button[data-action="stop"]:hover,
  .titlebar-resource-kill-button[data-action="stop"]:focus-visible,
  .titlebar-resource-kill-button[data-action="quit"]:hover,
  .titlebar-resource-kill-button[data-action="quit"]:focus-visible {
    background: rgba(255,255,255,0.2);
    color: rgba(255,255,255,0.96);
    outline: none;
  }
  .titlebar-resource-children {
    display: grid;
    padding: 0 8px 8px 64px;
  }
  .titlebar-resource-child-row {
    align-items: center;
    display: grid;
    gap: 8px;
    grid-template-columns: minmax(0, 1fr) 200px;
    min-height: 24px;
  }
  .titlebar-resources-empty {
    color: rgba(255,255,255,0.54);
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    padding: 10px 4px;
  }
  /*
   * CDXC:TooltipLifecycle 2026-06-13-02:30:
   * Titlebar native pointer-out may hide currently visible tooltip surfaces,
   * but it must not reset all hover styling or stay false until a click. The
   * main titlebar document restores this flag on DOM pointer movement so hover
   * tooltips can appear again immediately.
   */
  body[data-native-pointer-inside="false"] [data-slot="tooltip-content"],
  body[data-native-pointer-inside="false"] .titlebar-resource-tooltip {
    opacity: 0 !important;
    pointer-events: none !important;
    visibility: hidden !important;
  }
`;
