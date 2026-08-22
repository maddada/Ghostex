export const PROJECT_BOARD_STYLES = `
  :root {
    color-scheme: dark;
    background: var(--app-background, #191919);
    color: #f4f4f5;
    font-family: Inter Variable, -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    --background: var(--app-background, #191919);
    --foreground: oklch(0.985 0 0);
    --card: #171717;
    --card-foreground: oklch(0.985 0 0);
    --popover: #171717;
    --popover-foreground: oklch(0.985 0 0);
    --primary: oklch(0.922 0 0);
    --primary-foreground: oklch(0.205 0 0);
    --secondary: #242424;
    --secondary-foreground: oklch(0.985 0 0);
    --muted: #242424;
    --muted-foreground: oklch(0.708 0 0);
    --accent: #242424;
    --accent-foreground: oklch(0.985 0 0);
    --destructive: oklch(0.704 0.191 22.216);
    --border: oklch(1 0 0 / 10%);
    --input: oklch(1 0 0 / 15%);
    --ring: oklch(0.556 0 0);
    --radius: 6px;
    --project-board-bg: var(--app-background, #191919);
    --project-board-panel: #171717;
    --project-board-panel-hover: #1d1d1d;
    /*
     * CDXC:ProjectBoardCards 2026-06-19-09:14:
     * Kanban card surfaces need a brighter resting background than their lane panels so cards stand out in the macOS Project board.
     * Keep hover one step brighter than the resting card color so hover feedback remains visible after raising the base card tone.
     */
    --project-board-card: #242424;
    --project-board-card-hover: #2b2b2b;
    --project-board-border: rgba(255, 255, 255, 0.1);
    --project-board-border-strong: rgba(255, 255, 255, 0.16);
    --project-board-control-height: 36px;
    --project-board-scrollbar: rgba(255, 255, 255, 0.28);
    /*
     * CDXC:ProjectBoardRoundness 2026-06-29-20:55:
     * The Kanban ticket dialog and board cards/controls should adopt the Settings surface roundness instead of the global square theme.
     * Small chips/labels use the compact radius, interactive controls/cards/inputs use the control radius, and the dialog plus dropdown popups use the section/control radius. Field focus reuses a neutral dimmed border like Settings rather than a saturated focus ring.
     */
    --project-board-radius-compact: 4px;
    --project-board-radius-control: 6px;
    --project-board-radius-section: 10px;
    --project-board-focus-border: color-mix(in srgb, #f4f4f5 58%, var(--project-board-border) 42%);
  }

  * { box-sizing: border-box; }

  body {
    background: var(--project-board-bg);
    margin: 0;
    min-height: 100vh;
    overflow: hidden;
  }

  /*
   * CDXC:ProjectBoard 2026-06-13-13:37:
   * Kanban bead context menus should feel like Ghostex sidebar menus while staying owned by the standalone Project board bundle.
   * Use a transparent fixed backdrop to dismiss the menu and fixed menu coordinates so right-click placement is independent of lane scroll positions.
   */
  .project-board-context-menu-backdrop {
    background: transparent;
    border: 0;
    cursor: default;
    inset: 0;
    margin: 0;
    padding: 0;
    position: fixed;
    z-index: 1190;
  }

  .project-board-ticket-context-menu {
    background: color-mix(in srgb, var(--project-board-panel) 92%, #000 8%);
    border: 1px solid rgba(255, 255, 255, 0.13);
    box-shadow:
      0 14px 28px rgba(0, 0, 0, 0.32),
      0 0 0 1px rgba(255, 255, 255, 0.04);
    display: grid;
    gap: 2px;
    min-width: 164px;
    padding: 6px;
    position: fixed;
    z-index: 1200;
  }

  .project-board-ticket-context-menu-item {
    align-items: center;
    background: transparent;
    border: 0;
    color: rgba(244, 244, 245, 0.88);
    display: flex;
    font: inherit;
    font-size: 12px;
    font-weight: 620;
    gap: 8px;
    min-height: 32px;
    padding: 8px 10px;
    text-align: left;
    white-space: nowrap;
    width: 100%;
  }

  .project-board-ticket-context-menu-item svg {
    flex: 0 0 auto;
    height: 14px;
    width: 14px;
  }

  .project-board-ticket-context-menu-item:hover,
  .project-board-ticket-context-menu-item:focus-visible {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(250, 250, 250, 0.96);
    outline: none;
  }

  .project-board-ticket-context-menu-item:disabled {
    color: rgba(244, 244, 245, 0.34);
    cursor: not-allowed;
  }

  .project-board-ticket-context-menu-item:disabled:hover {
    background: transparent;
  }

  .project-board-ticket-context-menu-item-danger {
    color: rgba(255, 158, 158, 0.92);
  }

  .project-board-ticket-context-menu-item-danger:hover,
  .project-board-ticket-context-menu-item-danger:focus-visible {
    background: rgba(235, 87, 87, 0.16);
    color: #ffd2d2;
  }

  .project-board-shell {
    background: var(--project-board-bg);
    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100vh;
    min-height: 0;
    overflow: hidden;
    padding: 22px 24px 24px;
  }

  .project-board-shell * {
    border-radius: 0 !important;
  }

  /*
   * CDXC:ProjectBoardRoundness 2026-06-29-20:55:
   * Match the Settings surface look: round the Kanban ticket dialog plus the board's interactive controls and bead cards while large swimlane panels stay square so adjacent lanes keep one shared separator line.
   * Board opt-ins need higher specificity than the .project-board-shell square reset, so they reassert the radius with !important; the ticket dialog and portaled dropdown popups live outside the shell and round without it.
   */
  .project-board-card,
  .project-board-card-conversation,
  .project-board-card [data-slot="button"],
  .project-automation-card,
  .project-automation-run-card,
  .project-automation-card [data-slot="button"],
  .project-automation-run-card [data-slot="button"],
  .project-automation-detail-actions [data-slot="button"],
  .project-automation-detail-section pre,
  .project-automation-detail-run-stack div,
  .project-automation-empty-state-icon,
  .project-automation-empty-action,
  .project-automation-coming-soon-panel,
  .project-automation-coming-soon-icon,
  .project-automation-tab,
  .project-automation-tabs,
  .project-automation-toolbar-button,
  .project-board-toolbar-actions [data-slot="button"],
  .project-board-lane-header-action,
  .project-board-search input,
  .project-board-filter-select {
    border-radius: var(--project-board-radius-control) !important;
  }

  .project-automation-panel,
  .project-automation-split,
  .project-automation-coming-soon {
    border-radius: var(--project-board-radius-section) !important;
  }

  .project-board-card-label {
    border-radius: var(--project-board-radius-compact) !important;
  }

  [data-slot="select-content"],
  [data-slot="popover-content"] {
    border-radius: var(--project-board-radius-control) !important;
  }

  .project-ticket-dialog .rounded-none,
  .project-ticket-dialog [data-slot="input"],
  .project-ticket-dialog [data-slot="textarea"],
  .project-ticket-dialog [data-slot="select-trigger"],
  .project-ticket-dialog [data-slot="button"],
  .project-ticket-dialog [data-slot="dialog-close"],
  .project-ticket-dialog .project-ticket-image-thumb,
  .project-ticket-dialog .project-ticket-image-remove,
  .project-ticket-dialog .project-ticket-comment-list,
  .project-ticket-dialog .project-ticket-comment,
  .project-ticket-dialog .project-ticket-conversation-row {
    border-radius: var(--project-board-radius-control);
  }

  .project-ticket-dialog .project-ticket-label-chip {
    border-radius: var(--project-board-radius-compact);
  }

  /*
   * CDXC:ProjectBoardRoundness 2026-06-29-20:55:
   * Give Kanban form controls the Settings field treatment: a subtle translucent fill, a visible neutral border (select triggers ship transparent borders by default), and a dimmed neutral focus border without the saturated shadcn focus ring.
   */
  .project-ticket-dialog [data-slot="input"],
  .project-ticket-dialog [data-slot="textarea"],
  .project-ticket-dialog [data-slot="select-trigger"],
  .project-board-shell .project-board-search input,
  .project-board-shell .project-board-filter-select {
    background: color-mix(in srgb, var(--input) 30%, transparent);
    border: 1px solid var(--input);
  }

  .project-ticket-dialog [data-slot="input"]:is(:focus, :focus-visible),
  .project-ticket-dialog [data-slot="textarea"]:is(:focus, :focus-visible),
  .project-ticket-dialog [data-slot="select-trigger"]:is(:focus, :focus-visible),
  .project-board-shell .project-board-search input:is(:focus, :focus-visible),
  .project-board-shell .project-board-filter-select:is(:focus, :focus-visible) {
    border-color: var(--project-board-focus-border);
    box-shadow: none;
    outline: none;
  }

  .project-ticket-comment-list [data-slot="scroll-area-viewport"] {
    scrollbar-color: transparent transparent;
    scrollbar-width: none;
  }

  .project-ticket-comment-list:hover [data-slot="scroll-area-viewport"],
  .project-ticket-comment-list:focus-within [data-slot="scroll-area-viewport"] {
    scrollbar-color: var(--project-board-scrollbar) transparent;
  }

  .project-ticket-comment-list [data-slot="scroll-area-viewport"]::-webkit-scrollbar {
    height: 2px;
    width: 2px;
  }

  /*
   * CDXC:BoardScrollbars 2026-08-07:
   * The board strip and every lane body keep the browser's own scrollbar so the
   * bar stays clickable and draggable instead of wheel-only. Chromium paints
   * ::-webkit-scrollbar geometry only while the scroller keeps scrollbar-width
   * at auto and leaves scrollbar-color unset; either one hands rendering to the
   * standard scrollbar and collapses the gutter to 0px, which is why these two
   * scrollers stay out of the hidden-scrollbar rules above. The 8px box is the
   * mouse target and the thumb's transparent borders keep the painted rail at
   * the board's 2px width.
   *
   * CDXC:DialogScrollbar 2026-08-07:
   * The ticket dialog body sat in the hidden-scrollbar rules above, and
   * measuring it in Chromium showed the same wheel-only failure the board had:
   * a 0px gutter, and no scroll from a track click or a thumb drag at any x
   * offset along its right edge. It joins the real-scrollbar rules here. The
   * comment list stays hidden above because its Radix ScrollArea paints its own
   * interactable bar.
   */
  .project-board-lanes,
  .project-board-lane-scroll,
  .project-ticket-dialog-body {
    scrollbar-width: auto;
  }

  .project-board-lanes::-webkit-scrollbar,
  .project-board-lane-scroll::-webkit-scrollbar,
  .project-ticket-dialog-body::-webkit-scrollbar {
    background: transparent;
    display: block;
    height: 8px;
    width: 8px;
  }

  .project-board-lanes::-webkit-scrollbar-track,
  .project-board-lane-scroll::-webkit-scrollbar-track,
  .project-ticket-dialog-body::-webkit-scrollbar-track,
  .project-ticket-comment-list [data-slot="scroll-area-viewport"]::-webkit-scrollbar-track {
    background: transparent;
  }

  .project-board-lanes::-webkit-scrollbar-thumb,
  .project-board-lane-scroll::-webkit-scrollbar-thumb,
  .project-ticket-dialog-body::-webkit-scrollbar-thumb,
  .project-ticket-comment-list [data-slot="scroll-area-viewport"]::-webkit-scrollbar-thumb {
    background: transparent;
  }

  .project-ticket-comment-list:hover [data-slot="scroll-area-viewport"]::-webkit-scrollbar-thumb,
  .project-ticket-comment-list:focus-within [data-slot="scroll-area-viewport"]::-webkit-scrollbar-thumb {
    background: var(--project-board-scrollbar);
  }

  .project-board-lanes::-webkit-scrollbar-thumb {
    background-clip: content-box;
    border-bottom: 3px solid transparent;
    border-top: 3px solid transparent;
  }

  .project-board-lane-scroll::-webkit-scrollbar-thumb,
  .project-ticket-dialog-body::-webkit-scrollbar-thumb {
    background-clip: content-box;
    border-left: 3px solid transparent;
    border-right: 3px solid transparent;
  }

  .project-board-lanes:hover::-webkit-scrollbar-thumb,
  .project-board-lanes:focus-within::-webkit-scrollbar-thumb,
  .project-board-lane:hover .project-board-lane-scroll::-webkit-scrollbar-thumb,
  .project-board-lane:focus-within .project-board-lane-scroll::-webkit-scrollbar-thumb,
  .project-ticket-dialog-body:hover::-webkit-scrollbar-thumb,
  .project-ticket-dialog-body:focus-within::-webkit-scrollbar-thumb {
    background-color: var(--project-board-scrollbar);
  }

  .project-ticket-comment-list [data-slot="scroll-area-scrollbar"] {
    opacity: 0;
    transition: opacity 120ms ease;
    width: 5px;
  }

  .project-ticket-comment-list:hover [data-slot="scroll-area-scrollbar"],
  .project-ticket-comment-list:focus-within [data-slot="scroll-area-scrollbar"] {
    opacity: 1;
  }

  .project-ticket-comment-list [data-slot="scroll-area-thumb"] {
    background: var(--project-board-scrollbar);
  }

  .project-board-toolbar {
    align-items: center;
    display: grid;
    flex: 0 0 auto;
    gap: 12px;
    grid-template-columns: minmax(0, 1fr) auto;
    min-height: 40px;
  }

  .project-board-toolbar[data-surface="automations"] {
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  }

  .project-board-toolbar-heading {
    display: grid;
    gap: 4px;
    justify-self: start;
    min-width: 0;
  }

  .project-automation-eyebrow {
    color: rgba(244, 244, 245, 0.48);
    font-size: 10px;
    font-weight: 750;
    letter-spacing: 0.08em;
    line-height: 1;
    text-transform: uppercase;
  }

  .project-board-toolbar-title {
    color: rgba(250, 250, 250, 0.96);
    font-size: 21px;
    font-weight: 650;
    line-height: 1.15;
    margin: 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-board-toolbar-actions {
    align-items: center;
    display: flex;
    gap: 8px;
    justify-self: end;
  }

  /*
   * CDXC:Automations 2026-06-29-15:55:
   * The first shipped Automation page uses a compact local nav for gxserver
   * definitions, run history, and triage while keeping the Kanban board header
   * unchanged.
   *
   * CDXC:Automations 2026-06-30-10:55:
   * The Automation page tabs should read as a flat Kanban segmented control, not a gradient-backed strip. The Create automation and + Automation actions should use the same height and radius as Kanban's new-ticket action buttons.
   *
   * CDXC:Automations 2026-06-30-21:10:
   * Automations, Runs, and Triage tabs must share the widest tab width so the segmented control feels stable while still sizing from its labels instead of a hard-coded pixel width.
   */
  .project-automation-tabs {
    align-items: center;
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    display: inline-grid;
    flex: 0 0 auto;
    gap: 3px;
    grid-auto-columns: 1fr;
    grid-auto-flow: column;
    height: var(--project-board-control-height);
    justify-self: center;
    padding: 3px;
    width: fit-content;
  }

  .project-automation-tab {
    align-items: center;
    background: transparent;
    border: 1px solid transparent;
    color: rgba(250, 250, 250, 0.68);
    cursor: pointer;
    display: inline-flex;
    font-size: 12px;
    font-weight: 650;
    height: 28px;
    justify-content: center;
    line-height: 1;
    padding: 0 12px;
    white-space: nowrap;
  }

  .project-automation-tab:hover {
    background: var(--project-board-panel-hover);
    border-color: rgba(255, 255, 255, 0.1);
    color: rgba(250, 250, 250, 0.88);
  }

  .project-automation-tab[data-active="true"] {
    background: var(--project-board-card);
    border-color: var(--project-board-border-strong);
    color: rgba(250, 250, 250, 0.94);
  }

  .project-automation-empty-action,
  .project-automation-toolbar-button {
    height: var(--project-board-control-height);
    min-height: var(--project-board-control-height);
  }

  /*
   * CDXC:ProjectAutomations 2026-06-09-18:40:
   * Automation views use one connected shell: a darker list sidebar on the left and a detail pane on the right with no gutter between them. Both columns share the same height so empty states stay vertically centered together.
   *
   * CDXC:ProjectAutomations 2026-06-09-15:40:
   * Automation split views need centered empty states with icon, title, helper copy, and optional create action so blank Automations/Triage/Runs panels do not look like misaligned top-left placeholders.
   *
   * CDXC:Automations 2026-06-30-10:50:
   * Automation pages should share Kanban's rounded card/control language instead of inheriting the shell's square reset. Use flat Project Board panel/card colors and explicit radius opt-ins, with no gradient backgrounds.
   */
  .project-automation-split {
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    display: grid;
    flex: 1 1 auto;
    gap: 0;
    grid-template-columns: minmax(280px, 0.9fr) minmax(320px, 1.1fr);
    grid-template-rows: minmax(0, 1fr);
    min-height: 0;
    overflow: hidden;
  }

  .project-automation-panel {
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    display: flex;
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
  }

  /*
   * CDXC:Automations 2026-07-01-03:24:
   * Automations Overview and project Automate are openable discovery pages, but
   * their real content must stay covered until Enable Experimental Features is
   * on. Use an opaque first-party panel instead of a transparent overlay so
   * disabled users cannot inspect automation definitions, runs, or triage data.
   */
  .project-automation-coming-soon {
    align-items: center;
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    display: flex;
    flex: 1 1 auto;
    justify-content: center;
    min-height: 0;
    overflow: hidden;
    padding: 28px;
  }

  .project-automation-coming-soon-panel {
    align-items: center;
    background: color-mix(in srgb, var(--project-board-panel) 92%, #fff 8%);
    border: 1px solid var(--project-board-border-strong);
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 420px;
    padding: 28px;
    text-align: center;
  }

  .project-automation-coming-soon-icon {
    align-items: center;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: rgba(244, 244, 245, 0.52);
    display: flex;
    height: 52px;
    justify-content: center;
    width: 52px;
  }

  .project-automation-coming-soon-icon svg {
    height: 26px;
    width: 26px;
  }

  .project-automation-coming-soon-panel span {
    color: rgba(244, 244, 245, 0.48);
    font-size: 10px;
    font-weight: 750;
    letter-spacing: 0.08em;
    line-height: 1;
    text-transform: uppercase;
  }

  .project-automation-coming-soon-panel h2 {
    color: rgba(250, 250, 250, 0.96);
    font-size: 20px;
    font-weight: 650;
    line-height: 1.2;
    margin: 0;
  }

  .project-automation-coming-soon-panel p {
    color: rgba(244, 244, 245, 0.58);
    font-size: 13px;
    line-height: 1.5;
    margin: 0;
    max-width: 340px;
  }

  .project-automation-split > * {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }

  .project-automation-split > :first-child {
    background: var(--project-board-panel);
    border-right: 1px solid var(--project-board-border);
  }

  .project-automation-split > :last-child {
    background: color-mix(in srgb, var(--project-board-panel) 94%, #fff 6%);
  }

  .project-automation-empty-state {
    align-items: center;
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 10px;
    justify-content: center;
    height: 100%;
    min-height: 0;
    padding: 36px 28px;
    text-align: center;
  }

  .project-automation-split > .project-automation-empty-state {
    background: transparent;
    border: none;
  }

  .project-automation-empty-state[data-variant="detail"] {
    padding: 24px;
  }

  .project-automation-empty-state-icon {
    align-items: center;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 12px;
    color: rgba(244, 244, 245, 0.46);
    display: flex;
    height: 52px;
    justify-content: center;
    margin-bottom: 4px;
    width: 52px;
  }

  .project-automation-empty-state-icon svg {
    height: 26px;
    width: 26px;
  }

  .project-automation-empty-state strong {
    color: rgba(250, 250, 250, 0.94);
    font-size: 15px;
    font-weight: 650;
    line-height: 1.25;
  }

  .project-automation-empty-state p {
    color: rgba(244, 244, 245, 0.54);
    font-size: 13px;
    line-height: 1.5;
    margin: 0;
    max-width: 300px;
  }

  .project-automation-split .project-automation-detail {
    background: transparent;
    border: none;
    flex: 1 1 auto;
    min-height: 0;
  }

  .project-automation-split .project-automation-detail:not(.project-automation-detail--empty) {
    --edge-fade-distance: 16px;
    overflow: auto;
    padding: 16px;
  }

  .project-automation-detail--empty {
    align-items: center;
    display: flex;
    flex: 1 1 auto;
    height: 100%;
    justify-content: center;
    min-height: 0;
    padding: 0;
  }

  .project-automation-list,
  .project-automation-run-list {
    --edge-fade-distance: 16px;
    display: grid;
    flex: 1 1 auto;
    gap: 10px;
    grid-auto-rows: min-content;
    min-height: 0;
    overflow: auto;
    padding: 12px;
  }

  .project-automation-card,
  .project-automation-run-card {
    background: var(--project-board-card);
    border-color: var(--project-board-border);
  }

  .project-automation-card:hover,
  .project-automation-run-card:hover {
    background: var(--project-board-card-hover);
  }

  .project-automation-card[data-selected="true"],
  .project-automation-run-card[data-selected="true"] {
    border-color: rgba(244, 244, 245, 0.32);
    box-shadow: inset 0 0 0 1px rgba(244, 244, 245, 0.18);
  }

  .project-automation-card [data-slot="card-content"],
  .project-automation-run-card [data-slot="card-content"] {
    align-items: flex-start;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
  }

  .project-automation-card-main,
  .project-automation-run-main {
    display: grid;
    gap: 6px;
    min-width: 0;
    width: 100%;
  }

  .project-automation-card-title,
  .project-automation-run-heading {
    align-items: center;
    display: flex;
    gap: 8px;
    min-width: 0;
  }

  .project-automation-card-title strong,
  .project-automation-run-heading strong {
    color: rgba(250, 250, 250, 0.96);
    font-size: 14px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-automation-card-title span,
  .project-automation-run-heading span {
    border-radius: 999px;
    color: rgba(250, 250, 250, 0.86);
    flex: 0 0 auto;
    font-size: 11px;
    font-weight: 700;
    padding: 3px 7px;
    text-transform: capitalize;
  }

  .project-automation-card-title span[data-enabled="true"],
  .project-automation-run-heading span[data-status="findings"] {
    background: rgba(111, 207, 151, 0.18);
    color: #8ee4ad;
  }

  .project-automation-card-title span[data-enabled="false"],
  .project-automation-run-heading span[data-status="failed"],
  .project-automation-run-heading span[data-status="needs_attention"] {
    background: rgba(235, 87, 87, 0.18);
    color: #ff9a9a;
  }

  .project-automation-card-main p,
  .project-automation-run-main p,
  .project-automation-card-meta,
  .project-automation-run-meta {
    color: rgba(244, 244, 245, 0.58);
    font-size: 12px;
    margin: 0;
  }

  .project-automation-card-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 2px;
  }

  .project-automation-card-tags span,
  .project-automation-card-meta span,
  .project-automation-run-meta span {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 999px;
    color: rgba(244, 244, 245, 0.68);
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
  }

  .project-automation-card-meta span[data-unread="true"] {
    background: rgba(111, 207, 151, 0.14);
    border-color: rgba(111, 207, 151, 0.24);
    color: #8ee4ad;
  }

  .project-automation-card-agent {
    align-items: center;
    color: rgba(244, 244, 245, 0.72);
    display: inline-flex;
    font-size: 12px;
    gap: 6px;
    margin-top: 4px;
  }

  .project-automation-card-meta,
  .project-automation-run-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 4px;
  }

  .project-automation-card-actions {
    align-items: center;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    display: flex;
    flex: 0 0 auto;
    gap: 6px;
    justify-content: flex-end;
    padding-top: 10px;
    width: 100%;
  }

  .project-automation-run-actions {
    align-items: center;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    display: flex;
    flex: 0 0 auto;
    gap: 6px;
    justify-content: flex-end;
    padding-top: 10px;
    width: 100%;
  }

  .project-automation-detail {
    display: grid;
    gap: 14px;
    grid-auto-rows: min-content;
    min-height: 0;
  }

  .project-automation-detail:not(.project-automation-detail--empty) {
    --edge-fade-distance: 16px;
    overflow: auto;
  }

  .project-automation-detail-header {
    align-items: flex-start;
    display: flex;
    gap: 12px;
    justify-content: space-between;
    min-width: 0;
  }

  .project-automation-detail-header h2 {
    color: rgba(250, 250, 250, 0.96);
    font-size: 18px;
    line-height: 1.2;
    margin: 6px 0 0;
  }

  .project-automation-detail-header span,
  .project-automation-detail-run-stack span {
    border-radius: 999px;
    color: rgba(250, 250, 250, 0.86);
    display: inline-flex;
    font-size: 11px;
    font-weight: 700;
    padding: 3px 7px;
    text-transform: capitalize;
  }

  .project-automation-detail-header span[data-enabled="true"],
  .project-automation-detail-header span[data-status="findings"],
  .project-automation-detail-run-stack span[data-status="findings"] {
    background: rgba(111, 207, 151, 0.18);
    color: #8ee4ad;
  }

  .project-automation-detail-header span[data-enabled="false"],
  .project-automation-detail-header span[data-status="failed"],
  .project-automation-detail-header span[data-status="needs_attention"],
  .project-automation-detail-run-stack span[data-status="failed"],
  .project-automation-detail-run-stack span[data-status="needs_attention"] {
    background: rgba(235, 87, 87, 0.18);
    color: #ff9a9a;
  }

  .project-automation-detail-actions {
    align-items: center;
    display: flex;
    flex: 0 0 auto;
    gap: 6px;
  }

  .project-automation-detail-grid {
    display: grid;
    gap: 10px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    margin: 0;
  }

  .project-automation-detail-grid div {
    min-width: 0;
  }

  .project-automation-detail-grid dt,
  .project-automation-detail-section h3 {
    color: rgba(244, 244, 245, 0.52);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0;
    margin: 0 0 4px;
    text-transform: uppercase;
  }

  .project-automation-detail-grid dd,
  .project-automation-detail-section p,
  .project-automation-detail-run-stack p {
    color: rgba(244, 244, 245, 0.78);
    font-size: 12px;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .project-automation-detail-grid dd {
    align-items: center;
    display: flex;
    gap: 6px;
    min-width: 0;
  }

  .project-automation-detail-grid dd span {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .project-automation-detail-section {
    display: grid;
    gap: 8px;
    min-width: 0;
  }

  .project-automation-detail-section pre {
    background: rgba(0, 0, 0, 0.22);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 7px;
    color: rgba(244, 244, 245, 0.82);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    line-height: 1.45;
    margin: 0;
    max-height: 220px;
    overflow: auto;
    padding: 10px;
    white-space: pre-wrap;
  }

  .project-automation-detail-run-stack {
    display: grid;
    gap: 8px;
  }

  .project-automation-detail-run-stack div {
    align-items: center;
    background: rgba(255, 255, 255, 0.045);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 7px;
    display: flex;
    justify-content: space-between;
    padding: 8px 10px;
  }

  .project-automation-dialog {
    max-width: min(780px, calc(100vw - 44px));
    width: 780px;
  }

  .project-automation-form {
    gap: 14px;
  }

  .project-automation-form label,
  .project-automation-field-full {
    color: rgba(244, 244, 245, 0.72);
    display: grid;
    font-size: 12px;
    font-weight: 650;
    gap: 6px;
  }

  .project-automation-field-full {
    grid-column: 1 / -1;
  }

  .project-automation-form-grid {
    display: grid;
    gap: 12px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .project-automation-form-section {
    display: grid;
    gap: 10px;
  }

  .project-automation-form-section-title {
    color: rgba(244, 244, 245, 0.52);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .project-automation-select {
    height: var(--project-board-control-height);
    min-width: 0;
    width: 100%;
  }

  .project-automation-agent-option {
    align-items: center;
    display: inline-flex;
    gap: 8px;
    min-width: 0;
  }

  .project-automation-agent-icon {
    display: block;
    flex: 0 0 auto;
    height: 14px;
    mask-position: center;
    mask-repeat: no-repeat;
    mask-size: contain;
    width: 14px;
    -webkit-mask-position: center;
    -webkit-mask-repeat: no-repeat;
    -webkit-mask-size: contain;
  }

  .project-automation-prompt-field textarea {
    min-height: 150px;
  }

  .project-automation-dialog [data-slot="input"],
  .project-automation-dialog [data-slot="textarea"],
  .project-automation-dialog [data-slot="select-trigger"] {
    background: color-mix(in srgb, var(--input) 30%, transparent);
    border: 1px solid var(--input);
  }

  .project-automation-dialog [data-slot="input"]:is(:focus, :focus-visible),
  .project-automation-dialog [data-slot="textarea"]:is(:focus, :focus-visible),
  .project-automation-dialog [data-slot="select-trigger"]:is(:focus, :focus-visible) {
    border-color: var(--project-board-focus-border);
    box-shadow: none;
    outline: none;
  }

  .project-automation-segmented {
    background: rgba(255, 255, 255, 0.055);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    padding: 3px;
  }

  .project-automation-segmented button {
    background: transparent;
    border: 0;
    border-radius: 6px;
    color: rgba(244, 244, 245, 0.72);
    font: inherit;
    font-size: 12px;
    font-weight: 700;
    height: 30px;
  }

  .project-automation-segmented button[data-active="true"] {
    background: rgba(244, 244, 245, 0.9);
    color: #151617;
  }

  .project-automation-segmented button:disabled {
    color: rgba(244, 244, 245, 0.32);
    cursor: not-allowed;
  }

  .project-automation-segmented button:disabled[data-active="true"] {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(244, 244, 245, 0.42);
  }

  .project-automation-inline-note {
    color: rgba(244, 244, 245, 0.54);
    font-size: 12px;
    line-height: 1.4;
    margin: -4px 0 0;
  }

  .project-automation-enabled {
    align-items: center;
    display: flex !important;
    flex-direction: row;
    gap: 8px;
  }

  .project-automation-card-toggle {
    align-items: center;
    display: flex;
    flex-direction: row;
    gap: 6px;
  }

  .project-automation-card-toggle span,
  .project-automation-detail-toggle span {
    color: var(--project-board-muted);
    font-size: 12px;
    line-height: 1;
  }

  .project-automation-card-toggle span[data-enabled="true"],
  .project-automation-detail-toggle span[data-enabled="true"] {
    color: var(--project-board-accent);
  }

  .project-automation-detail-toggle {
    align-items: center;
    display: flex;
    flex-direction: row;
    gap: 8px;
  }

  @media (max-width: 860px) {
    .project-automation-split {
      grid-template-columns: 1fr;
      grid-template-rows: auto minmax(0, 1fr);
    }

    .project-automation-split > :first-child {
      border-bottom: 1px solid rgba(255, 255, 255, 0.08);
      border-right: none;
    }

    .project-automation-form-grid {
      grid-template-columns: 1fr;
    }

    .project-automation-select {
      width: 100%;
    }
  }

  .project-board-filters {
    align-items: center;
    display: flex;
    flex: 0 0 auto;
    gap: 10px;
    min-width: 0;
  }

  .project-board-columns-button {
    flex: 0 0 auto;
  }

  .project-board-columns-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .project-board-columns-row {
    align-items: center;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    display: flex;
    gap: 8px;
    padding: 6px 8px;
  }

  .project-board-columns-row[data-locked="true"] {
    opacity: 0.55;
  }

  .project-board-columns-name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-board-columns-note {
    color: rgba(244, 244, 245, 0.46);
    flex: 0 0 auto;
    font-size: 12px;
  }

  .project-board-columns-add {
    align-items: center;
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }

  .project-board-columns-error {
    color: rgba(248, 113, 113, 0.92);
    font-size: 12px;
    margin: 8px 0 0;
  }

  .project-board-search {
    align-items: center;
    display: flex;
    flex: 1 1 auto;
    min-width: 0;
    position: relative;
  }

  .project-board-search-icon {
    color: rgba(244, 244, 245, 0.42);
    height: 16px;
    pointer-events: none;
    position: absolute;
    right: 12px;
    width: 16px;
    z-index: 1;
  }

  .project-board-search input {
    height: var(--project-board-control-height);
    padding-right: 36px;
  }

  .project-board-search-clear-button {
    align-items: center;
    background: transparent;
    border: none;
    border-radius: 0;
    color: rgba(244, 244, 245, 0.42);
    display: inline-flex;
    height: 24px;
    justify-content: center;
    padding: 0;
    position: absolute;
    right: 8px;
    top: 50%;
    transform: translateY(-50%);
    width: 24px;
    z-index: 1;
  }

  .project-board-search-clear-button:hover,
  .project-board-search-clear-button:focus-visible {
    color: rgba(244, 244, 245, 0.78);
    outline: none;
  }

  .project-board-search-clear-button svg {
    height: 16px;
    pointer-events: none;
    width: 16px;
  }

  .project-board-filter-select,
  .project-board-ticket-button {
    height: var(--project-board-control-height);
    min-width: 124px;
  }

  .project-board-native-filter-select {
    appearance: auto;
    color: var(--foreground);
    font: inherit;
    padding: 0 8px;
  }

  .project-board-ticket-button {
    min-width: 0;
  }

  .project-board-board-region {
    display: flex;
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
    position: relative;
  }

  .project-board-loading-overlay {
    align-items: center;
    background: rgba(10, 10, 10, 0.48);
    color: rgba(244, 244, 245, 0.9);
    display: flex;
    inset: 0;
    justify-content: center;
    pointer-events: auto;
    position: absolute;
    z-index: 20;
  }

  .project-board-loading-spinner {
    animation: project-board-loading-spin 850ms linear infinite;
    filter: drop-shadow(0 2px 8px rgba(0, 0, 0, 0.38));
  }

  @keyframes project-board-loading-spin {
    to { transform: rotate(360deg); }
  }

  .project-board-lanes {
    align-items: stretch;
    display: grid;
    flex: 1 1 auto;
    /*
     * CDXC:ProjectBoardLanes 2026-06-19-09:59:
     * Kanban cards need more usable width, so swimlanes should sit directly beside each other instead of spending horizontal space on gutters.
     * Keep the existing lane grid structure and let the lane border act as the visible separator.
     *
     * CDXC:ScrollFades 2026-06-19-14:16:
     * The Project Board should use the same Codex-style edge fade as the
     * sidebar scroll surface. The board strip owns the horizontal fade while
     * each lane body owns its vertical fade, leaving lane headers and custom
     * scrollbars unmasked.
     *
     * CDXC:BoardScrollbars 2026-08-07:
     * The lane bar is the scroller's own scrollbar now, so it lives inside the
     * mask and fades at the very ends of its travel like the ticket dialog's
     * scrollbar already does.
     *
     * CDXC:ProjectBoardCustomColumns 2026-08-21:
     * The board renders one lane per configured Beads status, so the track count
     * cannot be a fixed six: an explicit six-track template auto-places a
     * seventh lane into an implicit second row, which overflow-y: hidden then
     * clips out of sight. Flowing into implicit columns keeps every rendered
     * lane on the one horizontally scrolling row whatever the board's status
     * list holds, at the same minimum lane width as before.
     */
    --edge-fade-distance: 18px;
    gap: 0;
    grid-auto-columns: minmax(218px, 1fr);
    grid-auto-flow: column;
    min-height: 0;
    overflow-x: auto;
    overflow-y: hidden;
    padding-bottom: 0;
  }

  .project-board-lane {
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 218px;
    overflow: hidden;
    position: relative;
  }

  .project-board-lane + .project-board-lane {
    /*
     * CDXC:ProjectBoardLanes 2026-06-19-09:59:
     * Adjacent zero-gap swimlanes must meet on one separator line, not two stacked borders.
     * Remove the following lane's left border so the previous lane's right border owns the shared boundary.
     */
    border-left-width: 0;
  }

  .project-board-lane[data-drop-target="true"] {
    background: var(--project-board-panel-hover);
    border-color: var(--project-board-border-strong);
  }

  .project-board-lane-header {
    align-items: center;
    display: flex;
    flex: 0 0 auto;
    justify-content: space-between;
    min-height: 44px;
    padding: 0 12px;
  }

  .project-board-lane-header div {
    align-items: center;
    display: flex;
    gap: 8px;
    min-width: 0;
  }

  .project-board-lane-header h2,
  .project-board-lane-header span {
    color: rgba(244, 244, 245, 0.68);
    font-size: 12px;
    font-weight: 650;
    margin: 0;
  }

  .project-board-lane-header-action {
    height: 28px;
    justify-content: flex-end;
    margin-right: 4px;
    position: relative;
    width: 28px;
  }

  .project-board-lane-count,
  .project-board-lane-add {
    transition: opacity 120ms ease;
  }

  .project-board-lane-count {
    display: block;
    min-width: 100%;
    opacity: 1;
    text-align: right;
  }

  .project-board-lane-add {
    opacity: 0;
    pointer-events: none;
    position: absolute;
    right: -3px;
    top: 0;
  }

  .project-board-lane:hover .project-board-lane-count,
  .project-board-lane:focus-within .project-board-lane-count {
    opacity: 0;
  }

  .project-board-lane:hover .project-board-lane-add,
  .project-board-lane:focus-within .project-board-lane-add {
    opacity: 1;
    pointer-events: auto;
  }

  .project-board-lane-dot {
    background: rgba(244, 244, 245, 0.42);
    display: inline-block;
    height: 7px;
    width: 7px;
  }

  .project-board-lane[data-tone="muted"] .project-board-lane-dot { background: #8f9aa7; }
  .project-board-lane[data-tone="blue"] .project-board-lane-dot { background: #5ea4ff; }
  .project-board-lane[data-tone="amber"] .project-board-lane-dot { background: #e7b85b; }
  .project-board-lane[data-tone="violet"] .project-board-lane-dot { background: #b18cff; }
  .project-board-lane[data-tone="green"] .project-board-lane-dot { background: #95d7f6; }

  .project-board-lane-scroll {
    --edge-fade-distance: 18px;
    flex: 1 1 auto;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    padding-right: 0;
  }

  .project-board-card-stack {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    padding: 0 10px 10px;
  }

  .project-board-lane-limit {
    border: 1px dashed rgba(255, 255, 255, 0.12);
    color: rgba(244, 244, 245, 0.48);
    font-size: 11px;
    line-height: 1.4;
    padding: 10px 12px;
  }

  .project-board-card {
    /*
     * CDXC:ProjectBoardCards 2026-06-13-13:55:
     * Kanban bead cards are click, drag, and context-menu targets, so their text should not become selected by accidental pointer movement.
     * Disable selection at the card surface while keeping editable ticket dialog fields selectable.
     */
    background: var(--project-board-card);
    border: 1px solid var(--project-board-border);
    box-shadow: 0 1px 0 rgba(0, 0, 0, 0.28);
    cursor: default;
    gap: 0;
    max-width: 100%;
    min-width: 0;
    padding: 0;
    user-select: none;
    width: 100%;
  }

  .project-board-card:hover { background-color: var(--project-board-card-hover); }
  .project-board-card[data-dragging="true"] { opacity: 0.55; }

  .project-board-card-header {
    gap: 5px;
    min-width: 0;
    padding: 11px 12px 0;
  }

  .project-board-card-header [data-slot="card-title"] {
    color: rgba(250, 250, 250, 0.91);
    font-size: 13px;
    font-weight: 560;
    line-height: 1.35;
    overflow-wrap: anywhere;
    word-break: break-word;
  }

  .project-board-card-header [data-slot="card-description"] {
    color: rgba(244, 244, 245, 0.39);
    font-size: 11px;
  }

  .project-board-card-content {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    padding: 8px 12px 11px;
  }

  .project-board-card-content p {
    color: rgba(244, 244, 245, 0.55);
    display: -webkit-box;
    font-size: 12px;
    line-height: 1.42;
    margin: 0;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    overflow: hidden;
    overflow-wrap: anywhere;
    word-break: break-word;
  }

  .project-board-card-labels {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .project-board-card-label {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(244, 244, 245, 0.72);
    font-size: 10px;
    line-height: 1;
    padding: 4px 7px;
  }

  .project-board-card-meta {
    align-items: center;
    color: rgba(244, 244, 245, 0.46);
    display: flex;
    flex-wrap: wrap;
    font-size: 11px;
    gap: 8px;
    line-height: 1;
  }

  .project-board-priority {
    color: rgba(244, 244, 245, 0.72);
    font-weight: 680;
  }

  .project-board-card-creator {
    max-width: 45%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-board-card-assignee {
    align-items: center;
    color: rgba(244, 244, 245, 0.72);
    display: inline-flex;
    gap: 4px;
    max-width: 50%;
    min-width: 0;
  }

  .project-board-card-assignee svg {
    flex: none;
    height: 13px;
    width: 13px;
  }

  .project-board-card-assignee-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-board-comments {
    align-items: center;
    display: inline-flex;
    gap: 4px;
    margin-left: auto;
  }

  .project-board-comments svg {
    height: 13px;
    width: 13px;
  }

  .project-board-card-conversation {
    align-items: center;
    background: rgba(80, 160, 255, 0.08);
    border: 1px solid rgba(120, 180, 255, 0.15);
    color: rgba(218, 235, 255, 0.86);
    display: flex;
    gap: 8px;
    justify-content: space-between;
    min-height: 30px;
    min-width: 0;
    padding: 4px 5px 4px 8px;
  }

  .project-board-card-conversation span {
    align-items: center;
    display: inline-flex;
    font-size: 11px;
    gap: 5px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-board-card-conversation-label {
    /*
     * CDXC:ProjectBoard 2026-05-28-10:14:
     * Board-card associated session names must show a literal ellipsis when
     * the card is too narrow, while the trailing jump button remains visible.
     * Give the text cluster a zero flex basis and override the broader span
     * rule on the actual tooltip trigger so Chromium/WebKit calculate
     * text-overflow instead of clipping the label.
     */
    flex: 1 1 0;
    max-width: 100%;
    min-width: 0;
    overflow: hidden;
  }

  .project-board-card-conversation-label .project-board-card-conversation-name {
    display: block;
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-board-card-conversation-extra {
    flex: 0 0 auto;
  }

  .project-board-card-conversation svg {
    flex: 0 0 auto;
    height: 13px;
    width: 13px;
  }

  .project-board-notice {
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    box-shadow: 0 12px 34px rgba(0, 0, 0, 0.22);
    color: rgba(244, 244, 245, 0.9);
    flex: 0 0 auto;
  }

  .project-board-notice[data-kind="init"] {
    border-color: rgba(231, 184, 91, 0.28);
  }

  .project-board-notice[data-kind="install"] {
    border-color: rgba(94, 164, 255, 0.26);
  }

  .project-board-notice[data-kind="migration"] {
    border-color: rgba(231, 184, 91, 0.36);
  }

  .project-board-notice[data-kind="install"] .project-board-notice-icon {
    background: rgba(94, 164, 255, 0.12);
    border-color: rgba(94, 164, 255, 0.2);
    color: #7ab7ff;
  }

  .project-board-notice [data-slot="card-content"] {
    align-items: flex-start;
    display: flex;
    gap: 12px;
    padding: 14px;
  }

  .project-board-notice-icon {
    align-items: center;
    background: rgba(231, 184, 91, 0.13);
    border: 1px solid rgba(231, 184, 91, 0.2);
    color: #e7b85b;
    display: flex;
    flex: 0 0 auto;
    height: 34px;
    justify-content: center;
    width: 34px;
  }

  .project-board-notice-icon svg {
    height: 17px;
    width: 17px;
  }

  .project-board-notice-body {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .project-board-notice strong {
    color: rgba(250, 250, 250, 0.94);
    font-size: 13px;
    font-weight: 680;
    letter-spacing: 0;
    line-height: 1.2;
  }

  .project-board-notice p {
    color: rgba(244, 244, 245, 0.64);
    font-size: 12px;
    line-height: 1.45;
    margin: 0;
    max-width: 660px;
  }

  .project-board-notice-body > a {
    align-self: flex-start;
    color: #7ab7ff;
    font-size: 12px;
    margin-top: 2px;
    text-decoration: none;
  }

  .project-board-notice-body > a:hover {
    text-decoration: underline;
  }

  .project-board-migration-options {
    display: grid;
    gap: 8px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    margin-top: 5px;
    max-width: 760px;
    width: 100%;
  }

  .project-board-migration-option {
    background: rgba(0, 0, 0, 0.14);
    border: 1px solid rgba(255, 255, 255, 0.07);
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
    padding: 10px;
  }

  .project-board-migration-option > strong {
    font-size: 12px;
  }

  .project-board-migration-option .project-board-migration-risk {
    color: rgba(231, 184, 91, 0.82);
  }

  .project-board-migration-option .project-board-notice-command {
    max-width: 100%;
  }

  .project-board-migration-option .project-board-notice-command code {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  @media (max-width: 760px) {
    .project-board-migration-options {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .project-board-notice-command {
    align-items: center;
    align-self: flex-start;
    background: rgba(0, 0, 0, 0.22);
    border: 1px solid rgba(255, 255, 255, 0.08);
    display: inline-flex;
    gap: 7px;
    min-height: 30px;
    padding: 3px 4px 3px 9px;
  }

  .project-board-notice-command code {
    color: rgba(250, 250, 250, 0.9);
    font-family: "SF Mono", ui-monospace, monospace;
    font-size: 12px;
    line-height: 1;
    white-space: nowrap;
  }

  .project-board-notice-command button {
    color: rgba(244, 244, 245, 0.58);
    height: 22px;
    width: 22px;
  }

  .project-board-notice-command button:hover {
    color: rgba(250, 250, 250, 0.92);
  }

  .project-board-notice-command .project-board-notice-run-button {
    gap: 5px;
    padding-inline: 7px;
    width: auto;
  }

  .project-board-notice-command .project-board-notice-run-button svg {
    height: 14px;
    width: 14px;
  }

  .project-board-confirm-command {
    max-width: 100%;
  }

  .project-board-confirm-command code {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .project-ticket-dialog {
    /*
     * CDXC:ProjectBoard 2026-05-28-13:52:
     * Project ticket edit/create dialogs should use the same modal background
     * as the rest of Ghostex app-modal surfaces.
     *
     * CDXC:SidebarTheme 2026-06-15-01:43:
     * Project Board dialogs follow --app-modal-background so Dark 1 uses
     * #191919 while Dark 2 preserves the previous #0e0e0e surface.
     */
    background: var(--app-modal-background, #191919);
    background-color: var(--app-modal-background, #191919);
    border-radius: var(--project-board-radius-section);
    max-width: min(780px, calc(100vw - 44px));
    overflow: hidden;
    width: 780px;
    /*
     * CDXC:ProjectBoardDialogHeight 2026-08-22:
     * The dialog is centred with a -50% translate, so a popup taller than the
     * window loses its top and bottom to the viewport edges with no way to
     * reach them. Bound the popup to the window and lay it out as a column so
     * the header and footer stay pinned and only the body scrolls. The base
     * dialog popup is a grid, so the column direction is set here rather than
     * relying on the shared component.
     */
    display: flex;
    flex-direction: column;
    max-height: calc(100vh - 32px);
    max-height: calc(100dvh - 32px);
  }

  .project-ticket-dialog > [data-slot="dialog-header"],
  .project-ticket-dialog > [data-slot="dialog-footer"] {
    flex: 0 0 auto;
  }

  .project-ticket-dialog-body {
    --edge-fade-distance: 16px;
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 16px;
    max-height: min(72vh, 760px);
    min-height: 0;
    overflow: auto;
  }

  .project-ticket-dialog-footer {
    /*
     * CDXC:ProjectBoardTicketEditor 2026-05-28-08:02:
     * The ticket editor footer should not distribute Delete, Start work, and Save as left, center, and right islands. Keep the destructive Delete action isolated while grouping the workflow and save actions together at the right edge.
     */
    align-items: center;
    flex-direction: row;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: space-between;
  }

  .project-ticket-dialog-primary-actions {
    align-items: center;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: flex-end;
    margin-left: auto;
  }

  .project-ticket-create-footer {
    /*
     * CDXC:ProjectBoard 2026-05-28-12:32:
     * New-ticket creation now has two outcomes: queue the bead, or create it and
     * immediately launch work in the selected execution location. Keep agent and
     * location controls grouped with Create & Start so plain Create remains a
     * simple board operation while the start path is explicit.
     */
    align-items: end;
    display: grid;
    gap: 12px;
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .project-ticket-create-start {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-width: 0;
  }

  .project-ticket-create-start-controls {
    align-items: center;
    display: grid;
    gap: 8px;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    justify-items: stretch;
    min-width: 0;
  }

  .project-ticket-footer-select,
  .project-ticket-meta-grid [data-slot="select-trigger"],
  .project-ticket-conversation-controls [data-slot="select-trigger"] {
    height: var(--project-board-control-height);
    min-width: 0;
    width: 100%;
  }

  .project-ticket-title-input,
  .project-ticket-label-editor input {
    height: var(--project-board-control-height);
    min-height: var(--project-board-control-height);
  }

  .project-ticket-create-actions {
    align-items: center;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .project-ticket-create-actions {
    justify-content: flex-end;
  }

  .project-ticket-dialog-footer [data-slot="button"],
  .project-ticket-create-actions > [data-slot="button"],
  .project-ticket-label-editor > [data-slot="button"],
  .project-ticket-conversation-controls > [data-slot="button"] {
    /*
     * CDXC:ProjectBoardForms 2026-06-21-15:30:
     * New-ticket and edit-ticket action buttons must match the adjacent Project Board dropdown height so macOS Kanban dialog control rows align instead of mixing shadcn's default button height with taller select triggers.
     *
     * CDXC:ProjectBoardForms 2026-06-22-02:17:
     * Top-of-dialog Kanban modal dropdowns, label add controls, and ticket title text fields must use the same Project Board control height as the footer buttons so the create/edit dialogs do not show mismatched control rows.
     */
    height: var(--project-board-control-height);
  }

  .project-ticket-meta-grid {
    display: grid;
    gap: 12px;
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .project-ticket-field {
    color: rgba(244, 244, 245, 0.58);
    display: flex;
    flex-direction: column;
    font-size: 12px;
    font-weight: 600;
    gap: 7px;
    min-width: 0;
  }

  .project-ticket-field-inline {
    gap: 6px;
  }

  .project-ticket-creator-value {
    color: rgba(250, 250, 250, 0.68);
    font-weight: 500;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-assignee-value {
    align-items: center;
    color: rgba(250, 250, 250, 0.92);
    display: flex;
    font-weight: 500;
    gap: 5px;
    min-width: 0;
  }

  .project-ticket-assignee-value svg {
    flex: none;
    height: 14px;
    width: 14px;
  }

  .project-ticket-assignee-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-field textarea,
  .project-ticket-field input {
    color: rgba(250, 250, 250, 0.92);
    max-width: 100%;
    min-width: 0;
    overflow-wrap: anywhere;
    word-break: break-word;
  }

  .project-ticket-prompt-input {
    min-height: 190px;
  }

  .project-ticket-title-input {
    /*
    CDXC:ProjectBoardTickets 2026-06-15-21:00:
    Ticket title editing is a single-line text field. Keep the create/edit title control at one input row so it does not inherit prompt textarea height or wrap its value like long-form content.
    */
    line-height: 18px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-label-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .project-ticket-label-chip {
    align-items: center;
    background: rgba(255, 255, 255, 0.08);
    border: 0;
    color: rgba(244, 244, 245, 0.82);
    cursor: pointer;
    display: inline-flex;
    font-size: 11px;
    gap: 4px;
    padding: 4px 8px;
  }

  .project-ticket-label-chip svg {
    height: 12px;
    width: 12px;
  }

  .project-ticket-label-editor {
    align-items: center;
    display: flex;
    gap: 8px;
  }

  .project-ticket-label-editor input {
    flex: 1 1 auto;
  }

  .project-ticket-image-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  /*
   * CDXC:ProjectBoard 2026-05-31-07:15:
   * Prompt image thumbnails below the ticket Prompt field open a full-screen
   * preview on click with a dark overlay; any click on the overlay dismisses
   * the preview and the enlarged image is capped at 90vw by 90vh.
   */
  .project-ticket-image-popup {
    align-items: center;
    background: rgb(0 0 0 / 74%);
    display: flex;
    inset: 0;
    justify-content: center;
    padding: 28px;
    position: fixed;
    z-index: 2000;
  }

  .project-ticket-image-popup img {
    box-shadow: 0 18px 60px rgb(0 0 0 / 50%);
    max-height: 90vh;
    max-width: 90vw;
    object-fit: contain;
  }

  .project-ticket-image-thumb {
    background: rgba(0, 0, 0, 0.24);
    border: 1px solid rgba(255, 255, 255, 0.1);
    display: block;
    height: 72px;
    overflow: hidden;
    position: relative;
    width: 72px;
  }

  .project-ticket-image-thumb[role="button"] {
    cursor: pointer;
  }

  .project-ticket-image-thumb[role="button"]:hover,
  .project-ticket-image-thumb[role="button"]:focus-visible {
    border-color: rgba(255, 255, 255, 0.28);
  }

  .project-ticket-image-thumb img {
    height: 100%;
    object-fit: cover;
    width: 72px;
  }

  .project-ticket-image-thumb span {
    background: linear-gradient(135deg, rgba(255, 255, 255, 0.08), rgba(255, 255, 255, 0.02));
    display: block;
    height: 100%;
    width: 100%;
  }

  .project-ticket-image-remove {
    align-items: center;
    background: rgba(10, 10, 12, 0.78);
    border: 1px solid rgba(255, 255, 255, 0.16);
    color: rgba(255, 255, 255, 0.9);
    cursor: pointer;
    display: inline-flex;
    height: 22px;
    justify-content: center;
    padding: 0;
    position: absolute;
    right: 4px;
    top: 4px;
    width: 22px;
  }

  .project-ticket-image-remove svg {
    height: 13px;
    width: 13px;
  }

  .project-ticket-image-remove:hover {
    background: rgba(32, 32, 36, 0.94);
  }

  .project-ticket-dependencies {
    color: rgba(244, 244, 245, 0.62);
    font-size: 12px;
  }

  .project-ticket-dependencies p {
    margin: 0 0 4px;
  }

  .project-ticket-conversations {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .project-ticket-conversation-controls {
    align-items: center;
    display: grid;
    gap: 8px;
    grid-template-columns: minmax(150px, 1fr) auto;
  }

  .project-ticket-conversation-list {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .project-ticket-conversation-row {
    align-items: center;
    background: rgba(255, 255, 255, 0.035);
    border: 1px solid rgba(255, 255, 255, 0.08);
    display: grid;
    gap: 10px;
    grid-template-columns: minmax(0, 1fr) auto;
    min-height: 42px;
    padding: 7px 8px 7px 10px;
  }

  .project-ticket-conversation-main {
    /*
     * CDXC:ProjectBoard 2026-05-28-09:17:
     * Ticket conversation rows must preserve the right-side jump/unlink controls
     * at narrow widths while the associated session name truncates with an
     * ellipsis and exposes the full name through the hover tooltip.
     *
     * CDXC:ProjectBoard 2026-05-28-10:14:
     * The associated-session tooltip should open below the session name so it
     * does not cover the title area while inspecting a ticket.
     */
    min-width: 0;
    overflow: hidden;
  }

  .project-ticket-conversation-name,
  .project-ticket-conversation-status {
    display: block;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-conversation-name {
    color: rgba(250, 250, 250, 0.9);
    font-size: 12px;
    font-weight: 620;
  }

  .project-ticket-conversation-status {
    color: rgba(244, 244, 245, 0.46);
    font-size: 11px;
    margin-top: 2px;
  }

  .project-ticket-conversation-actions {
    align-items: center;
    display: flex;
    gap: 4px;
  }

  .project-ticket-comments {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .project-ticket-section-title {
    color: rgba(244, 244, 245, 0.58);
    font-size: 12px;
    font-weight: 650;
  }

  .project-ticket-comment-list {
    --edge-fade-distance: 14px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.08);
    max-height: 180px;
    min-height: 92px;
    padding: 6px;
  }

  .project-ticket-comment-list [data-slot="scroll-area-viewport"] > div {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /*
   * CDXC:ProjectBoardComments 2026-06-05-06:43:
   * Ticket comments in the edit dialog need readable author/date separation, author (agent) attribution, and a bottom-aligned full session id while preserving multiline comment text.
   */
  .project-ticket-comment {
    background: rgba(250, 250, 250, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-left: 2px solid rgba(125, 211, 252, 0.72);
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
  }

  .project-ticket-empty {
    padding: 12px;
  }

  .project-ticket-comment-header {
    align-items: baseline;
    display: flex;
    gap: 10px;
    justify-content: space-between;
    min-width: 0;
  }

  .project-ticket-comment-author-row {
    align-items: baseline;
    display: flex;
    gap: 4px;
    min-width: 0;
  }

  .project-ticket-comment-author {
    color: rgba(250, 250, 250, 0.94);
    font-size: 13px;
    font-weight: 700;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-comment-agent {
    color: rgba(186, 230, 253, 0.86);
    font-size: 12px;
    font-weight: 620;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-comment-date {
    color: rgba(244, 244, 245, 0.46);
    flex: 0 0 auto;
    font-size: 11px;
    font-weight: 600;
  }

  .project-ticket-comment p,
  .project-ticket-empty {
    color: rgba(244, 244, 245, 0.72);
    font-size: 13px;
    line-height: 1.45;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .project-ticket-comment p {
    margin: 0;
  }

  .project-ticket-comment-session {
    align-items: center;
    border-top: 1px solid rgba(255, 255, 255, 0.07);
    color: rgba(244, 244, 245, 0.48);
    display: flex;
    gap: 8px;
    justify-content: space-between;
    min-width: 0;
    padding-top: 8px;
  }

  .project-ticket-comment-session span {
    flex: 0 0 auto;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0;
    text-transform: uppercase;
  }

  .project-ticket-comment-session code {
    color: rgba(244, 244, 245, 0.74);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    font-size: 11px;
    min-width: 0;
    overflow-wrap: anywhere;
    text-align: right;
  }

  @media (max-width: 900px) {
    .project-board-shell { padding: 18px 16px; }
    .project-ticket-create-footer,
    .project-ticket-create-start-controls {
      grid-template-columns: 1fr;
    }
    .project-ticket-create-actions {
      justify-content: stretch;
    }
    .project-ticket-create-actions > button {
      flex: 1 1 auto;
    }
    .project-ticket-conversation-controls { grid-template-columns: 1fr; }
    .project-ticket-meta-grid { grid-template-columns: 1fr; }
  }
`;
