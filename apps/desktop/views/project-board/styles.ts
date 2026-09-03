export const PROJECT_BOARD_STYLES = `
  :root {
    color-scheme: dark;
    background: var(--app-background, #0e0e0e);
    color: #f4f4f5;
    font-family: Inter Variable, -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    --background: var(--app-background, #0e0e0e);
    --foreground: oklch(0.985 0 0);
    --card: #161616;
    --card-foreground: oklch(0.985 0 0);
    --popover: #161616;
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
    --radius: 8px;
    --project-board-bg: var(--app-background, #0e0e0e);
    --project-board-panel: #161616;
    --project-board-panel-hover: #1b1b1b;
    /*
     * CDXC:ProjectBoard 2026-06-19-09:14:
     * Kanban card surfaces need a brighter resting background than their lane panels so cards stand out in the macOS Project board.
     * Keep hover one step brighter than the resting card color so hover feedback remains visible after raising the base card tone.
     */
    --project-board-card: #1d1d1d;
    --project-board-card-hover: #232323;
    --project-board-border: rgba(255, 255, 255, 0.1);
    --project-board-border-strong: rgba(255, 255, 255, 0.16);
    --project-board-control-height: 32px;
    --project-board-scrollbar: rgba(255, 255, 255, 0.28);
    /*
     * CDXC:ProjectBoard 2026-06-29-20:55:
     * The Kanban ticket dialog and board cards/controls should adopt the Settings surface roundness instead of the global square theme.
     * Small chips/labels use the compact radius, interactive controls/cards/inputs use the control radius, and the dialog plus dropdown popups use the section/control radius. Field focus reuses a neutral dimmed border like Settings rather than a saturated focus ring.
     */
    --project-board-radius-compact: 4px;
    --project-board-radius-control: var(--radius);
    --project-board-radius-section: 12px;
    --project-board-focus-border: color-mix(in srgb, #f4f4f5 58%, var(--project-board-border) 42%);
    /*
     * CDXC:Theming 2026-08-24:
     * The Kanban/Automate page is loaded outside the sidebar chrome effects and
     * its bridge state carries no settings, so it can only paint the shipped
     * default accent until live accentColor plumbing reaches this page.
     */
    --ghostex-accent: #86d3f8;
  }

  * { box-sizing: border-box; }

  body {
    background: var(--project-board-bg);
    margin: 0;
    min-height: 100vh;
    overflow: hidden;
  }

  .project-board-shell {
    background: var(--project-board-bg);
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100vh;
    min-height: 0;
    overflow: hidden;
    padding: 16px 20px 20px;
  }

  /*
   * CDXC:ProjectBoard 2026-08-23:
   * Codex-style control language shared with the Automate surface: one 32px
   * control height (the shadcn default sizes), 8px radius on controls and
   * cards, pill switches, regular-weight button text. The old
   * ".project-board-shell *" square reset and its per-class radius opt-in
   * lists are gone; Tailwind utilities in the components own layout now.
   */
  [data-slot="select-trigger"],
  [data-slot="input"],
  [data-slot="card"] {
    border-radius: var(--project-board-radius-control);
  }

  [data-slot="button"] {
    font-weight: 400;
  }

  /* CDXC:DesignSystem 2026-08-24: one app-wide toggle shape (6px track, 4px thumb). */
  [data-slot="switch"] {
    border-radius: 6px;
  }

  [data-slot="switch"] [data-slot="switch-thumb"] {
    border-radius: 4px;
  }

  [data-slot="select-content"],
  [data-slot="dropdown-menu-content"],
  [data-slot="popover-content"] {
    border-radius: var(--project-board-radius-control) !important;
  }

  [data-slot="select-item"],
  [data-slot="dropdown-menu-item"],
  [data-slot="dropdown-menu-checkbox-item"] {
    border-radius: 6px;
    font-weight: 400;
    overflow: hidden;
  }

  /*
   * CDXC:ProjectBoard 2026-08-24:
   * Select popups size to their trigger (--anchor-width), and long option
   * labels (tags, sort orders) ship in a nowrap flex ItemText (a div) that
   * overflows to the popup's right edge. Ellipsize the label instead: the
   * ItemText becomes a block so text-overflow applies, and the item's pr-8
   * keeps the ellipsis clear of the check indicator.
   */
  [data-slot="select-item"] > div:first-child {
    display: block;
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-dialog [data-slot="textarea"],
  .project-ticket-dialog [data-slot="dialog-close"],
  .project-ticket-dialog .project-ticket-image-thumb,
  .project-ticket-dialog .project-ticket-image-remove,
  .project-ticket-dialog .project-ticket-comment-list,
  .project-ticket-dialog .project-ticket-comment,
  .project-ticket-dialog .project-ticket-conversation-row {
    border-radius: var(--project-board-radius-control);
  }

  .project-ticket-dialog .project-ticket-label-chip {
    border-radius: 999px;
  }

  /*
   * CDXC:ProjectBoard 2026-08-24:
   * One control language for every dialog row: 32px tall inputs and dropdowns,
   * one 14px text scale shared with the buttons beside them, regular weight.
   * This replaces the per-class height opt-in lists the dialogs used to carry,
   * which is what let select triggers render taller and in a different size
   * than the buttons next to them.
   */
  .project-ticket-dialog [data-slot="input"],
  .project-ticket-dialog [data-slot="select-trigger"] {
    height: var(--project-board-control-height);
    min-height: var(--project-board-control-height);
  }

  /*
   * CDXC:ProjectBoard 2026-08-24:
   * Footer actions (Delete / Start work / Save, Create / Create & Start) are
   * pinned to the same 32px control height so no variant or icon combination
   * can render one taller than its neighbors.
   */
  .project-ticket-dialog [data-slot="dialog-footer"] [data-slot="button"] {
    height: var(--project-board-control-height);
    min-height: var(--project-board-control-height);
  }

  .project-ticket-dialog [data-slot="input"],
  .project-ticket-dialog [data-slot="textarea"],
  .project-ticket-dialog [data-slot="select-trigger"],
  .project-ticket-dialog [data-slot="button"] {
    font-size: 14px;
    font-weight: 400;
  }

  /*
   * CDXC:ProjectBoard 2026-06-29-20:55:
   * Give Kanban form controls the Settings field treatment: a subtle translucent fill, a visible neutral border (select triggers ship transparent borders by default), and a dimmed neutral focus border without the saturated shadcn focus ring.
   */
  .project-ticket-dialog [data-slot="input"],
  .project-ticket-dialog [data-slot="textarea"],
  .project-ticket-dialog [data-slot="select-trigger"] {
    background: color-mix(in srgb, var(--input) 22%, transparent);
    border: 1px solid var(--border);
  }

  .project-ticket-dialog [data-slot="input"]:is(:focus, :focus-visible),
  .project-ticket-dialog [data-slot="textarea"]:is(:focus, :focus-visible),
  .project-ticket-dialog [data-slot="select-trigger"]:is(:focus, :focus-visible) {
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
   * CDXC:ProjectBoard 2026-08-07:
   * The board strip and every lane body keep the browser's own scrollbar so the
   * bar stays clickable and draggable instead of wheel-only. Chromium paints
   * ::-webkit-scrollbar geometry only while the scroller keeps scrollbar-width
   * at auto and leaves scrollbar-color unset; either one hands rendering to the
   * standard scrollbar and collapses the gutter to 0px, which is why these two
   * scrollers stay out of the hidden-scrollbar rules above. The 8px box is the
   * mouse target and the thumb's transparent borders keep the painted rail at
   * the board's 2px width.
   *
   * CDXC:AppModal 2026-08-07:
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

  /*
   * CDXC:ProjectBoard 2026-08-24:
   * Reserve the lane's 8px scrollbar rail even when a lane does not overflow,
   * so the card column keeps the same right inset (2px padding + 8px gutter =
   * the 10px the left side gets) whether or not a scrollbar is present.
   */
  .project-board-lane-scroll {
    scrollbar-gutter: stable;
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

  /*
   * CDXC:ProjectBoard 2026-08-24:
   * The dialog body reveals its thumb on hover only. It used to also reveal on
   * :focus-within, but a form dialog always has a focused field, so the bar
   * never went away while the dialog was open.
   */
  .project-board-lanes:hover::-webkit-scrollbar-thumb,
  .project-board-lanes:focus-within::-webkit-scrollbar-thumb,
  .project-board-lane:hover .project-board-lane-scroll::-webkit-scrollbar-thumb,
  .project-board-lane:focus-within .project-board-lane-scroll::-webkit-scrollbar-thumb,
  .project-ticket-dialog-body:hover::-webkit-scrollbar-thumb {
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

  .project-automation-dialog {
    max-width: min(780px, calc(100vw - 44px));
    width: 780px;
  }

  .project-automation-form {
    gap: 14px;
  }

  /*
   * CDXC:ProjectBoard 2026-08-24:
   * Automation field labels and section headers are quiet 12px/500 text, not
   * the old 650/700-weight uppercase headings, so the dialog reads as one
   * typographic scale with the ticket dialogs.
   */
  .project-automation-form label,
  .project-automation-field-full {
    color: var(--muted-foreground);
    display: grid;
    font-size: 12px;
    font-weight: 500;
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

  .project-automation-form-section-title {
    color: var(--muted-foreground);
    font-size: 12px;
    font-weight: 500;
  }

  .project-automation-select {
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

  /*
   * CDXC:DesignSystem 2026-08-24:
   * The Automate dialog's schedule/execution pickers are the shared
   * SegmentedControl now, so they render the same shadcn ButtonGroup strip as
   * Settings and the modals: one bordered container, flat segments sharing a
   * hairline, only the outer corners rounded, highlighted selected segment.
   *
   * The rules are restated here because this page loads only the generated
   * Tailwind sheet, and unlayered bare-button rules from the app themes beat
   * Tailwind's utilities layer wherever such a sheet is present.
   */
  [data-slot="segmented-control"] {
    border: 1px solid var(--border);
    border-radius: var(--project-board-radius-control);
    gap: 0;
    /*
     * The container owns the border-box control height (matching the 32px
     * buttons and selects on the same row) and the segments fill it, exactly
     * like the canonical rules in packages/core-ui/styles.css.
     */
    height: 32px;
    overflow: hidden;
  }

  [data-slot="segmented-control-item"] {
    background: transparent;
    border: 0;
    border-radius: 0;
    color: var(--muted-foreground);
    font: inherit;
    font-size: 13px;
    font-weight: 400;
    height: 100%;
    transition: background-color 120ms ease, color 120ms ease;
  }

  [data-slot="segmented-control-item"] + [data-slot="segmented-control-item"] {
    border-left: 1px solid var(--border);
  }

  [data-slot="segmented-control-item"]:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.04);
    color: var(--foreground);
  }

  [data-slot="segmented-control-item"][aria-pressed="true"] {
    background: color-mix(in srgb, var(--foreground) 14%, transparent);
    color: var(--foreground);
  }

  [data-slot="segmented-control-item"]:disabled {
    color: color-mix(in srgb, var(--muted-foreground) 55%, transparent);
    cursor: not-allowed;
  }

  @media (max-width: 860px) {
    .project-automation-form-grid {
      grid-template-columns: 1fr;
    }
  }
  .project-board-columns-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .project-board-columns-row {
    align-items: center;
    background: var(--project-board-card);
    border: 1px solid var(--border);
    border-radius: var(--project-board-radius-control);
    display: flex;
    gap: 8px;
    min-height: 44px;
    padding: 6px 8px 6px 12px;
    transition: background-color 120ms ease;
  }

  .project-board-columns-row:hover {
    background: var(--project-board-card-hover);
  }

  .project-board-columns-row[data-locked="true"] {
    background: transparent;
    opacity: 0.6;
  }

  .project-board-columns-name {
    flex: 1 1 auto;
    font-size: 13px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-board-columns-note {
    color: var(--muted-foreground);
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
    border-radius: 9999px;
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
    font-weight: 500;
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

  @media (max-width: 760px) {
    .project-board-migration-options {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .project-board-notice-copy-prompt {
    align-self: flex-start;
    gap: 5px;
    margin-top: 3px;
  }

  .project-ticket-dialog {
    /*
     * CDXC:ProjectBoard 2026-08-24:
     * The Codex-style board paints the page at #0e0e0e and every raised panel
     * at #161616, so the dialogs sit on the shared --popover surface instead of
     * the app-modal background they used to borrow from the sidebar theme.
     */
    background: var(--popover, #161616);
    background-color: var(--popover, #161616);
    border-radius: var(--project-board-radius-section);
    max-width: min(780px, calc(100vw - 44px));
    overflow: hidden;
    width: 780px;
    /*
     * CDXC:ProjectBoard 2026-08-22:
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
    /*
     * CDXC:ProjectBoard 2026-08-24:
     * 10px on the right keeps the form fields off the scrollbar rail, and the
     * bottom padding keeps the last field (Add comment) clear of the 16px
     * bottom fade so it never renders cut off at the end of the scroll.
     */
    padding: 0 10px 16px 0;
  }

  .project-ticket-dialog-footer {
    /*
     * CDXC:ProjectBoard 2026-05-28-08:02:
     * The ticket editor footer should not distribute Delete, Start work, and Save as left, center, and right islands. Keep the destructive Delete action isolated while grouping the workflow and save actions together at the right edge.
     */
    align-items: center;
    flex-direction: row;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: space-between;
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
    min-width: 0;
    width: 100%;
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

  .project-ticket-meta-grid {
    display: grid;
    gap: 12px;
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .project-ticket-field {
    color: var(--muted-foreground);
    display: flex;
    flex-direction: column;
    font-size: 12px;
    font-weight: 500;
    gap: 6px;
    min-width: 0;
  }

  .project-ticket-field-inline {
    gap: 6px;
  }

  .project-ticket-creator-value {
    color: rgba(250, 250, 250, 0.72);
    font-size: 13px;
    font-weight: 400;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-assignee-value {
    align-items: center;
    color: rgba(250, 250, 250, 0.92);
    display: flex;
    font-size: 13px;
    font-weight: 400;
    gap: 6px;
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
    CDXC:ProjectBoard 2026-06-15-21:00:
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
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid var(--border);
    color: rgba(244, 244, 245, 0.82);
    cursor: pointer;
    display: inline-flex;
    font-size: 11px;
    font-weight: 400;
    gap: 4px;
    padding: 3px 8px;
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

  .project-ticket-conversations {
    /* Same sectioned rhythm as .project-ticket-section (round 2 redesign). */
    border-top: 1px solid var(--project-board-hairline, rgba(255, 255, 255, 0.07));
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 14px;
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
    background: var(--project-board-card);
    border: 1px solid var(--border);
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
    font-size: 13px;
    font-weight: 400;
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

  .project-ticket-section-title {
    color: var(--muted-foreground);
    font-size: 12px;
    font-weight: 500;
  }

  /*
   * CDXC:ProjectBoard 2026-08-24 (round 2):
   * Titled field groups in the ticket dialogs (Properties, Comments) share the
   * Automate dialog's section rhythm: a quiet 12px/500 header above a 10px
   * grid, separated from the previous block by a hairline so the form reads as
   * organized sections rather than a flat run of fields.
   */
  .project-ticket-section {
    border-top: 1px solid var(--project-board-hairline, rgba(255, 255, 255, 0.07));
    display: grid;
    gap: 10px;
    padding-top: 14px;
  }

  .project-ticket-comment-list {
    /*
     * CDXC:ProjectBoard 2026-08-24:
     * The ScrollArea root only ships position: relative, so without an explicit
     * clip the comment viewport paints past the list and over the Add comment
     * label below it.
     */
    --edge-fade-distance: 14px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--border);
    max-height: 180px;
    min-height: 92px;
    overflow: hidden;
    padding: 6px;
  }

  .project-ticket-comment-list [data-slot="scroll-area-viewport"] > div {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /*
   * CDXC:ProjectBoard 2026-06-05-06:43:
   * Ticket comments in the edit dialog need readable author/date separation, author (agent) attribution, and a bottom-aligned full session id while preserving multiline comment text.
   */
  .project-ticket-comment {
    background: var(--project-board-card);
    border: 1px solid var(--border);
    border-left: 2px solid var(--ghostex-accent);
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
  }

  .project-ticket-empty {
    padding: 12px;
  }

  .project-ticket-comment-agent {
    color: var(--ghostex-accent);
    font-size: 12px;
    font-weight: 400;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    font-weight: 500;
    letter-spacing: 0.02em;
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
