import type { Meta, StoryObj } from '@storybook/react-vite';
import {
  IconAlertTriangle,
  IconCheck,
  IconChevronRight,
  IconFileText,
  IconInfoCircle,
  IconPencil,
  IconPointFilled,
  IconTerminal2,
  IconTool,
  IconWorldSearch,
} from '@tabler/icons-react';
import { createPortal } from 'react-dom';
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import type { SessionChatMessage } from '../../shared/session-chat';
import { SessionChatMessageList } from './session-chat-message-list';

/*
CDXC:SessionChat 2026-08-23:

Showcase for the transcript's LEFT MARGIN: one glyph vocabulary, one gutter.

Everything here is mock transcript data — no gxserver, no session, no host
bridge — so it runs anywhere Storybook does and exercises exactly the renderer
the real chat uses (SessionChatMessageList -> SessionChatToolRun), styled by the
real packages/core-ui/styles/chat.css.

Four columns of the SAME transcript:

  before  — today's mixed glyphs and gutters, restored by the override sheet
            below. Two disclosure metaphors (a filled clip-path triangle for
            reasoning, a stroke chevron for tools), three chevron sizes, and
            four different indents down one column.
  A+B     — one disclosure glyph on one marker column, prose bullet kept.
            This is what ships.
  A+B+C   — the same, with the prose bullet retired (data-chat-bullets="off").
  A+B+D   — the same, with the prose bullet drawn as tabler's point-filled
            glyph instead of the CSS round, sized and nudged to land as close
            to the round as the glyph allows. A review variant, not a
            proposal: the readout under it prints the ink it actually draws
            and the SVG nodes it actually costs.

Each column draws the axis its own top row defines, as a hairline, and prints
the measured left offset of every leading glyph beneath it. Misalignment is a
number, not something the reviewer has to eyeball: `spread` is max - min across
every glyph in that column, and it should be 0.0px for A+B and A+B+C.

The panes are pinned to 385px, the real transcript width in the desktop app's
chat pane. Stories built at Storybook's default preview width never exercise
the layout that actually ships — a preview column is wide enough that a
truncating preview never truncates and a wrapping row never wraps.
*/

/** The real transcript width in the desktop chat pane. */
const TRANSCRIPT_WIDTH_PX = 385;

/**
 * Fixed pane height rather than the viewport's, so the whole transcript is on
 * screen in one go whatever shape the reviewer's window is — the comparison is
 * worthless if a column has to be scrolled to be read.
 */
const TRANSCRIPT_HEIGHT_PX = 1240;

/*
 * The "before" sheet. It restores the geometry and the glyphs the transcript
 * had before this change, so the two states can never drift apart on anything
 * except what is under test — the same trick session-chat-prose-hierarchy
 * uses for the prose colour, extended to cover shape as well as size.
 *
 * The reasoning triangle is rebuilt by painting the chevron's own box and
 * clipping it, with the strokes made transparent: that is byte-for-byte the
 * `.ghostex-chat-thinking-caret` rule this change deleted, so the "before"
 * column shows the real old glyph rather than an impression of it.
 */
const BEFORE_SHEET = `
/*
 * Two columns per row until there is genuinely room for four. The panes are a
 * fixed 385px each and must never be squeezed to fit — squeezing them would
 * destroy the one thing this story is measuring — so the row wraps instead of
 * overflowing, and a reviewer on a laptop reads a 2x2 rather than scrolling a
 * column half off the edge.
 */
.story-marker-columns {
  display: grid;
  gap: 0.5rem;
  grid-template-columns: repeat(2, ${TRANSCRIPT_WIDTH_PX}px);
  justify-content: start;
  overflow: auto;
  padding: 0.5rem;
}

@media (min-width: 1620px) {
  .story-marker-columns {
    grid-template-columns: repeat(4, ${TRANSCRIPT_WIDTH_PX}px);
  }
}

/*
 * The desktop chat pane is a CEF browser whose OWN viewport is 385px, so the
 * "Chat message width" media query in chat.css (max-width: 1070px) always
 * fires there and the transcript runs full-bleed. Storybook renders three of
 * those panes inside one much wider document, so that query does not fire and
 * the transcript would centre itself inside each pane behind invisible side
 * gutters — the same class of mistake as building the story at the preview
 * width. Pin it, so what is on screen is what the pane really does.
 */
.ghostex-session-chat-scope [data-slot="message-scroller-content"] {
  max-width: 100%;
  width: 100%;
}
.ghostex-session-chat-scope[data-marker-variant="before"] {
  --chat-marker-slot: 0.375rem;
  --chat-marker-gap: 0.625rem;
  --chat-marker-inset: 6.5px;
}
/* The work lanes had a 20px slot at a 2px inset, and the tool row stacked its
   own 2px of inline padding on top of that. */
[data-marker-variant="before"] .ghostex-chat-work-icon {
  flex-basis: 1.25rem;
  height: 1.25rem;
  width: 1.25rem;
}
[data-marker-variant="before"] .ghostex-chat-work-trigger,
[data-marker-variant="before"] .ghostex-chat-tool-run-toggle {
  gap: 0.375rem;
  padding-inline: 2px 5px;
}
[data-marker-variant="before"] .ghostex-chat-work-row {
  padding-inline: 0.125rem;
}
/* The suppressed turn had no inset at all and its own 12px chevron. */
[data-marker-variant="before"] .ghostex-chat-suppressed-trigger {
  gap: 0.25rem;
  padding-inline: 0 5px;
}
[data-marker-variant="before"] .ghostex-chat-suppressed-trigger .ghostex-chat-marker-slot {
  flex-basis: 0.75rem;
  width: 0.75rem;
}
[data-marker-variant="before"] .ghostex-chat-suppressed-trigger .ghostex-chat-marker-slot svg {
  height: 0.75rem;
  width: 0.75rem;
}
/* The tool row's trailing chevron was 12px, a third size on one surface. */
[data-marker-variant="before"] .ghostex-chat-work-trigger > .ghostex-chat-disclosure-chevron {
  height: 0.75rem;
  width: 0.75rem;
}
/* The completed-work chevron trailed its label instead of leading it. */
[data-marker-variant="before"] .ghostex-chat-completed-work-trigger {
  padding-inline: 0;
}
[data-marker-variant="before"] .ghostex-chat-completed-work-trigger .ghostex-chat-marker-slot {
  flex-basis: auto;
  order: 1;
  width: auto;
}
/* The reasoning disclosure was a filled triangle, not a chevron. */
[data-marker-variant="before"] .ghostex-chat-thinking-icon {
  flex-basis: 0.375rem;
  height: 1.21875rem;
  width: 0.375rem;
}
[data-marker-variant="before"] .ghostex-chat-thinking-icon svg {
  background: currentColor;
  clip-path: polygon(0 0, 100% 50%, 0 100%);
  height: 0.5625rem;
  stroke: transparent;
  width: 0.4375rem;
}
/* The status badge was a 12px glyph at stroke 2.4 in a 16px round. */
[data-marker-variant="before"] .ghostex-chat-glyph-semantic {
  stroke-width: 1.8;
}

/* --- Variant D: the prose bullet as a tabler glyph --------------------------
 *
 * The CSS ::before collapses to zero (it stays a grid item, so the row keeps
 * its two columns) and a real <IconPointFilled> is portalled into the same
 * cell. Portalled rather than string-injected so this is the ACTUAL tabler
 * component with its real attributes, not an impression of it.
 *
 * Sizing is done honestly rather than to make the glyph look bad. tabler's
 * point-filled is a circle of r=5 in a 24 viewBox, so its ink is 10/24 of the
 * box: to land on the same 4px round the CSS draws, the box has to be
 * 4 × 24/10 = 9.6px. Dropped into the 14px marker slot unadjusted it would ink
 * at 5.83px — half again as wide as the dot, and over twice the area.
 *
 * Vertical placement needs its own nudge for the same reason. The CSS dot is a
 * 4px box offset by margin-block-start to sit on the first line's optical
 * centre; the glyph is a 9.6px box whose ink is centred inside it, so its
 * margin has to be the dot's centre minus half the glyph box.
 */
[data-chat-bullets="glyph"] {
  --chat-marker-bullet: 0rem;
  --story-bullet-ink: 0.25rem;
  --story-bullet-box: calc(var(--story-bullet-ink) * 2.4);
}
/* Absolutely positioned rather than dropped into the grid's marker cell: an
   out-of-flow bullet cannot perturb the row it decorates, so the glyph column
   is measured against exactly the same layout the CSS-dot column has. Its left
   edge is the marker column's own centre, minus half the glyph box. */
[data-chat-bullets="glyph"] .ghostex-chat-agent-message,
[data-chat-bullets="glyph"] .ghostex-chat-thinking-line,
[data-chat-bullets="glyph"] .ghostex-chat-suppressed-inline {
  position: relative;
}
[data-chat-bullets="glyph"] .story-bullet-glyph {
  height: var(--story-bullet-box);
  left: calc(
    var(--chat-marker-inset) + var(--chat-marker-slot) / 2 -
      var(--story-bullet-box) / 2
  );
  position: absolute;
  width: var(--story-bullet-box);
}
[data-chat-bullets="glyph"] .ghostex-chat-agent-message > .story-bullet-glyph {
  top: calc(0.5rem + var(--story-bullet-ink) / 2 - var(--story-bullet-box) / 2);
}
[data-chat-bullets="glyph"] .ghostex-chat-thinking-line > .story-bullet-glyph,
[data-chat-bullets="glyph"] .ghostex-chat-suppressed-inline > .story-bullet-glyph {
  top: calc(
    0.484375rem + var(--story-bullet-ink) / 2 - var(--story-bullet-box) / 2
  );
}
`;

/** Rows that carry a prose bullet, and so host the glyph variant's SVG. */
const BULLET_ROW_SELECTOR = [
  '.ghostex-chat-agent-message',
  '.ghostex-chat-thinking-line',
  '.ghostex-chat-suppressed-inline',
].join(', ');

/* --- Mock transcript --------------------------------------------------------
 *
 * Ordered so that ONE screenshot carries every row type the marker column has
 * to serve. The first user turn closes (it ends on an assistant answer), so the
 * list folds it into a completed-work group and shows that trigger. The last
 * turn is deliberately still open — `isWorking` — so its reasoning rows, tool
 * group and tool rows render at the transcript's top level rather than behind
 * the same fold.
 */

const suppressedOutput = [
  '<bash-stdout>',
  '  gxserver  starting on 127.0.0.1:8421',
  '  gxserver  loaded 14 projects, 61 sessions, 3 sleeping',
  '  gxserver  agent registry: claude, codex, pi, opencode, cursor-agent, grok',
  '  gxserver  prompt index warm in 214ms (41,882 prompts across 9 roots)',
  '  gxserver  hook socket at ~/.ghostex/run/hooks.sock',
  '  gxserver  ready',
  '</bash-stdout>',
].join('\n');

const STORY_MESSAGES: SessionChatMessage[] = [
  {
    blocks: [{ text: 'Why is the left margin so noisy?', type: 'text' }],
    id: 'user-1',
    role: 'user',
    source: 'transcript',
    timestamp: 1_000,
  },
  {
    blocks: [
      {
        text: 'Reading the transcript stylesheet for every rule that paints in the gutter.',
        type: 'text',
      },
      { input: { cmd: "rg -n '::before' chat.css" }, name: 'exec', type: 'tool-call' },
      { output: '9 matches', type: 'tool-result' },
    ],
    id: 'reasoning-1',
    role: 'reasoning',
    source: 'transcript',
    timestamp: 2_000,
  },
  {
    blocks: [
      {
        text: 'Four glyphs at three indents, and two of them are disclosures that disagree about what a disclosure looks like.',
        type: 'text',
      },
    ],
    id: 'assistant-1',
    role: 'assistant',
    source: 'transcript',
    timestamp: 7_000,
  },
  {
    blocks: [
      {
        text: '<task-notification><status>completed</status><summary>Prompt index rebuilt</summary></task-notification>',
        type: 'text',
      },
    ],
    id: 'status-1',
    role: 'user',
    source: 'transcript',
    timestamp: 8_000,
  },
  {
    blocks: [{ text: suppressedOutput, type: 'text' }],
    id: 'suppressed-1',
    role: 'user',
    source: 'transcript',
    timestamp: 9_000,
  },
  {
    blocks: [{ text: 'Put every marker on one axis.', type: 'text' }],
    id: 'user-2',
    role: 'user',
    source: 'transcript',
    timestamp: 10_000,
  },
  /*
   * A tool-only turn whose ANCHOR is the user turn above it. Anchored to an
   * assistant or reasoning turn instead, `foldSessionChatToolMessages` would
   * merge these blocks into that turn and they would render behind its
   * disclosure — which is correct in the product and useless here, because the
   * story needs a tool row standing on the transcript's own top level.
   */
  {
    blocks: [
      { input: { cmd: 'bunx tsc --noEmit' }, name: 'exec', type: 'tool-call' },
      { output: 'clean', type: 'tool-result' },
      {
        input: { file_path: 'packages/core-ui/chat/session-chat-tool-run.tsx' },
        name: 'edit_file',
        type: 'tool-call',
      },
      { output: 'applied', type: 'tool-result' },
      { input: { query: 'tabler chevron stroke' }, name: 'web_search', type: 'tool-call' },
      { output: '3 results', type: 'tool-result' },
    ],
    id: 'tools-1',
    role: 'tool',
    source: 'transcript',
    timestamp: 10_500,
  },
  {
    blocks: [
      {
        text: 'The gutter is the contract, not the glyph — so the width and the inset have to be tokens the rows share.',
        type: 'text',
      },
    ],
    id: 'reasoning-2',
    role: 'reasoning',
    source: 'transcript',
    timestamp: 11_000,
  },
  {
    blocks: [
      {
        text: 'Checking which rows already agree.\n\nThe prose bullet and the reasoning bullet share a slot; nothing else does.',
        type: 'text',
      },
      { input: { cmd: "rg -n 'grid-template-columns' chat.css" }, name: 'exec', type: 'tool-call' },
      { output: '4 matches', type: 'tool-result' },
    ],
    id: 'reasoning-3',
    role: 'reasoning',
    source: 'transcript',
    timestamp: 12_000,
  },
  {
    blocks: [
      {
        text: 'Now sizing the slot.\n\nA 1rem box is the smallest that holds a 14px chevron without clipping its corners, and it centres a 4px dot cleanly.',
        type: 'text',
      },
      {
        input: { file_path: 'packages/core-ui/styles/chat.css' },
        name: 'read_file',
        type: 'tool-call',
      },
      { output: '1,842 lines', type: 'tool-result' },
    ],
    id: 'reasoning-4',
    role: 'reasoning',
    source: 'transcript',
    timestamp: 13_000,
  },
  {
    blocks: [
      {
        text: 'Every top-level row now leads with the same 1rem gutter at the same 2px inset, so the dot, the chevron and the tool icon land on one vertical axis.',
        type: 'text',
      },
      {
        input: { file_path: 'packages/core-ui/styles/chat.css' },
        name: 'edit_file',
        type: 'tool-call',
      },
      { output: 'applied', type: 'tool-result' },
    ],
    id: 'assistant-2',
    role: 'assistant',
    source: 'transcript',
    timestamp: 14_000,
  },
];

/* --- Axis measurement -------------------------------------------------------
 *
 * The hairline is not a guess: it is the axis the column's FIRST prose row
 * defines, read back off the live layout. Every other glyph is then measured
 * against it, so a stepped column shows up as a spread in the readout instead
 * of relying on the reviewer's eye at 385px.
 */

interface AxisSample {
  label: string;
  centre: number;
}

interface AxisReading {
  glyphAxis: number;
  textAxis: number;
  samples: AxisSample[];
  spread: number;
  /** Measured bullet ink diameter, for the glyph variant's honesty check. */
  bulletInk: number | null;
}

/** Centre of a grid row's marker cell, relative to the pane. */
function gridMarkerCentre(row: Element, paneLeft: number): number | null {
  const style = getComputedStyle(row);
  const firstTrack = Number.parseFloat(style.gridTemplateColumns.split(' ')[0] ?? '');
  if (Number.isNaN(firstTrack)) {
    return null;
  }
  return row.getBoundingClientRect().left - paneLeft + Number.parseFloat(style.paddingLeft) + firstTrack / 2;
}

/** Centre of a rendered glyph box, relative to the pane. */
function glyphCentre(glyph: Element | null | undefined, paneLeft: number): number | null {
  if (!glyph) {
    return null;
  }
  const rect = glyph.getBoundingClientRect();
  if (rect.width === 0) {
    return null;
  }
  return rect.left - paneLeft + rect.width / 2;
}

/*
 * The FIRST row of a kind that is genuinely on the transcript's top level.
 * Rows inside an expansion body are deliberately indented behind that
 * disclosure's rail — measuring one of those would report a 24px "miss" that
 * is really correct nesting, and would hide a real one.
 */
function topLevel(pane: HTMLElement, selector: string): Element | null {
  for (const node of pane.querySelectorAll(selector)) {
    if (!node.closest('.ghostex-chat-expansion-body')) {
      return node;
    }
  }
  return null;
}

function measureAxis(pane: HTMLElement): AxisReading | null {
  const paneLeft = pane.getBoundingClientRect().left;
  const prose = topLevel(pane, '.ghostex-chat-agent-message');
  if (!prose) {
    return null;
  }
  const proseStyle = getComputedStyle(prose);
  const proseSlot = Number.parseFloat(proseStyle.gridTemplateColumns.split(' ')[0] ?? '');
  const glyphAxis = gridMarkerCentre(prose, paneLeft);
  if (glyphAxis === null) {
    return null;
  }

  /*
   * The glyph variant's bullet is a real element, so it is measured as one
   * rather than inferred from the grid track — the whole question about it is
   * whether the SVG's ink lands where the CSS round's does.
   */
  const bulletGlyph = topLevel(pane, '.ghostex-chat-agent-message .story-bullet-glyph');
  const bulletBox = bulletGlyph?.getBoundingClientRect() ?? null;

  const candidates: [string, number | null][] = [
    ['prose bullet', glyphCentre(bulletGlyph, paneLeft) ?? glyphAxis],
    ['reasoning bullet', gridMarkerCentre(topLevel(pane, '.ghostex-chat-thinking-line') ?? prose, paneLeft)],
    ['reasoning chevron', glyphCentre(topLevel(pane, '.ghostex-chat-thinking-icon svg'), paneLeft)],
    [
      'tool-group chevron',
      glyphCentre(topLevel(pane, '.ghostex-chat-tool-run-toggle .ghostex-chat-work-icon svg'), paneLeft),
    ],
    ['tool-row icon', glyphCentre(topLevel(pane, '.ghostex-chat-work-trigger .ghostex-chat-work-icon svg'), paneLeft)],
    ['suppressed chevron', glyphCentre(topLevel(pane, '.ghostex-chat-suppressed-trigger svg'), paneLeft)],
    ['completed-work chevron', glyphCentre(topLevel(pane, '.ghostex-chat-completed-work-trigger svg'), paneLeft)],
  ];

  const samples: AxisSample[] = [];
  for (const [label, centre] of candidates) {
    if (centre !== null) {
      samples.push({ label, centre });
    }
  }
  const centres = samples.map((sample) => sample.centre);
  return {
    // tabler point-filled inks 10/24 of its box (a circle of r=5 in a 24
    // viewBox), so the drawn round is that fraction of the rendered SVG.
    bulletInk: bulletBox ? (bulletBox.width * 10) / 24 : null,
    glyphAxis,
    samples,
    spread: Math.max(...centres) - Math.min(...centres),
    textAxis: glyphAxis + proseSlot / 2 + Number.parseFloat(proseStyle.columnGap || '0'),
  };
}

/* --- The pane ------------------------------------------------------------- */

/** css = the shipped 4px round · off = retired · glyph = tabler point-filled. */
type BulletMode = 'css' | 'off' | 'glyph';

function ChatPane({
  bullets,
  theme,
  variant,
}: {
  bullets: BulletMode;
  theme: 'dark' | 'light';
  variant: 'before' | 'after';
}) {
  const paneRef = useRef<HTMLDivElement>(null);
  const [reading, setReading] = useState<AxisReading | null>(null);
  const [bulletHosts, setBulletHosts] = useState<HTMLElement[]>([]);

  const remeasure = useCallback(() => {
    if (paneRef.current) {
      setReading(measureAxis(paneRef.current));
    }
  }, []);

  /*
   * Collect the rows that carry a prose bullet, so the glyph variant can
   * portal a real <IconPointFilled> into each one's marker cell. A portal
   * rather than an injected string keeps React owning the node and keeps the
   * icon the genuine component with its genuine attributes.
   */
  useLayoutEffect(() => {
    if (bullets !== 'glyph' || !paneRef.current) {
      setBulletHosts([]);
      return;
    }
    setBulletHosts(Array.from(paneRef.current.querySelectorAll<HTMLElement>(BULLET_ROW_SELECTOR)));
  }, [bullets, theme, variant]);

  /*
   * One reasoning disclosure is opened on purpose, so the column shows a
   * chevron in BOTH states side by side — a collapsed row and an expanded one
   * have to sit on the same axis, and a rotation that shifts the glyph is
   * exactly the kind of drift this story exists to catch. Driven by a real
   * click rather than a prop, because `open` is the row's own state.
   */
  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      const triggers = paneRef.current?.querySelectorAll<HTMLButtonElement>('.ghostex-chat-thinking-trigger');
      triggers?.[1]?.click();
      window.requestAnimationFrame(remeasure);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [remeasure]);

  // Theme and variant both move the layout; re-read after each flips.
  useEffect(remeasure, [bulletHosts, bullets, remeasure, theme, variant]);

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <div
        className='ghostex-session-chat-scope relative flex min-h-0 flex-1 flex-col bg-background text-foreground'
        data-chat-bullets={bullets}
        data-chat-theme={theme}
        data-marker-variant={variant}
        ref={paneRef}
      >
        {/* The axis the column's own first prose row defines, and the text
            edge that follows from it. Drawn over the transcript so a glyph
            that misses the column reads as a glyph off the line. */}
        {/* Inline styles on purpose: core-ui's Tailwind sheet is PREBUILT
            (bun run build:sidebar-css), so a utility a story invents does not
            exist in it and silently collapses to a 0×0 transparent box. */}
        {reading ? (
          <>
            <div
              style={{
                background: 'rgba(244, 63, 94, 0.75)',
                bottom: 0,
                left: `${reading.glyphAxis}px`,
                pointerEvents: 'none',
                position: 'absolute',
                top: 0,
                width: '1px',
                zIndex: 10,
              }}
            />
            <div
              style={{
                background: 'rgba(56, 189, 248, 0.4)',
                bottom: 0,
                left: `${reading.textAxis}px`,
                pointerEvents: 'none',
                position: 'absolute',
                top: 0,
                width: '1px',
                zIndex: 10,
              }}
            />
          </>
        ) : null}
        <SessionChatMessageList
          hasMore={false}
          isWorking
          loadingEarlier={false}
          messages={STORY_MESSAGES}
          onLoadEarlier={() => undefined}
          verboseMode={false}
        />
        {bulletHosts.map((host, index) =>
          createPortal(
            <IconPointFilled aria-hidden='true' className='story-bullet-glyph' />,
            host,
            `story-bullet-${index}`
          )
        )}
      </div>
      <AxisReadout bullets={bullets} count={bulletHosts.length} reading={reading} />
    </div>
  );
}

function AxisReadout({ bullets, count, reading }: { bullets: BulletMode; count: number; reading: AxisReading | null }) {
  if (!reading) {
    return null;
  }
  const aligned = reading.spread < 0.5;
  return (
    <div className='shrink-0 border-t border-white/10 bg-black/70 px-2 py-1.5 font-mono text-[10px] leading-[1.5] text-white/55'>
      {reading.samples.map((sample) => (
        <div className='flex justify-between gap-2' key={sample.label}>
          <span className='truncate'>{sample.label}</span>
          <span className={Math.abs(sample.centre - reading.glyphAxis) < 0.5 ? 'text-emerald-400' : 'text-rose-400'}>
            {sample.centre.toFixed(1)}px
          </span>
        </div>
      ))}
      <div className='mt-1 flex justify-between gap-2 border-t border-white/10 pt-1'>
        <span>spread</span>
        <span className={aligned ? 'text-emerald-400' : 'text-rose-400'}>{reading.spread.toFixed(1)}px</span>
      </div>
      {/* What the glyph bullet actually costs: the drawn ink next to the 4px
          round it replaces, and one SVG element per bulleted row. */}
      {bullets === 'glyph' && reading.bulletInk !== null ? (
        <>
          <div className='flex justify-between gap-2'>
            <span>bullet ink</span>
            <span className='text-amber-300'>{reading.bulletInk.toFixed(2)}px vs 4.00px css</span>
          </div>
          <div className='flex justify-between gap-2'>
            <span>extra svg nodes</span>
            <span className='text-amber-300'>{count}</span>
          </div>
        </>
      ) : null}
    </div>
  );
}

/* --- The glyph ramp band --------------------------------------------------- */

/*
 * The two tiers, side by side, so it is obvious that each is internally
 * consistent and that they are deliberately distinct from one another. They
 * share a size on purpose — they stand in the same marker slot on the same
 * axis, and a size step between them would put the column back where it
 * started — so the distinction they carry is shape plus stroke weight.
 */
function GlyphRamp({ theme }: { theme: 'dark' | 'light' }) {
  return (
    <div
      className='ghostex-session-chat-scope flex shrink-0 items-stretch gap-4 border-b border-white/10 bg-background px-3 py-2 text-foreground'
      data-chat-theme={theme}
    >
      <RampTier detail='14px · stroke 2 · this row expands' label='control' title='--chat-glyph-control-*'>
        <IconChevronRight aria-hidden='true' className='ghostex-chat-disclosure-chevron' />
        <IconChevronRight aria-hidden='true' className='ghostex-chat-disclosure-chevron is-open' />
      </RampTier>
      <RampTier detail='14px · stroke 1.75 · this row means something' label='semantic' title='--chat-glyph-semantic-*'>
        <IconTerminal2 aria-hidden='true' className='ghostex-chat-glyph-semantic' />
        <IconPencil aria-hidden='true' className='ghostex-chat-glyph-semantic' />
        <IconFileText aria-hidden='true' className='ghostex-chat-glyph-semantic' />
        <IconWorldSearch aria-hidden='true' className='ghostex-chat-glyph-semantic' />
        <IconTool aria-hidden='true' className='ghostex-chat-glyph-semantic' />
      </RampTier>
      <RampTier
        detail='12px · stroke 1.75 · this glyph has its own chrome'
        label='badge'
        title='--chat-glyph-badge-size'
      >
        <IconCheck aria-hidden='true' className='ghostex-chat-glyph-badge' />
        <IconAlertTriangle aria-hidden='true' className='ghostex-chat-glyph-badge' />
        <IconInfoCircle aria-hidden='true' className='ghostex-chat-glyph-badge' />
      </RampTier>
    </div>
  );
}

function RampTier({
  children,
  detail,
  label,
  title,
}: {
  children: ReactNode;
  detail: string;
  label: string;
  title: string;
}) {
  return (
    <div className='flex min-w-0 flex-col gap-1'>
      <div className='flex items-center gap-2 text-muted-foreground'>
        <span className='font-mono text-[10px] tracking-wide uppercase'>{label}</span>
        <span className='font-mono text-[10px] opacity-60'>{title}</span>
      </div>
      <div className='flex items-center gap-2 text-foreground'>{children}</div>
      <div className='font-mono text-[10px] text-muted-foreground'>{detail}</div>
    </div>
  );
}

/* --- The story ------------------------------------------------------------- */

function PaneLabel({ children }: { children: ReactNode }) {
  return (
    <div className='shrink-0 border-b border-white/10 px-3 py-1.5 font-mono text-[11px] tracking-wide text-white/60 uppercase'>
      {children}
    </div>
  );
}

function Column({
  bullets,
  label,
  theme,
  variant,
}: {
  bullets: BulletMode;
  label: string;
  theme: 'dark' | 'light';
  variant: 'before' | 'after';
}) {
  return (
    <div
      className='flex min-h-0 flex-col overflow-hidden rounded-lg border border-white/10'
      style={{
        height: `${TRANSCRIPT_HEIGHT_PX}px`,
        width: `${TRANSCRIPT_WIDTH_PX}px`,
      }}
    >
      <PaneLabel>{label}</PaneLabel>
      <ChatPane bullets={bullets} theme={theme} variant={variant} />
    </div>
  );
}

function SessionChatMarkerColumnStory({ theme }: { theme: 'dark' | 'light' }) {
  return (
    <div className='flex min-h-screen flex-col bg-[#0a0a0a]'>
      {/* eslint-disable-next-line react/no-danger -- story-local override sheet */}
      <style dangerouslySetInnerHTML={{ __html: BEFORE_SHEET }} />
      <GlyphRamp theme={theme} />
      <div className='story-marker-columns'>
        <Column bullets='css' label='before — 4 glyphs, 3 indents' theme={theme} variant='before' />
        <Column bullets='css' label='A+B — one glyph, one column' theme={theme} variant='after' />
        <Column bullets='off' label='A+B+C — bullets retired' theme={theme} variant='after' />
        <Column bullets='glyph' label='A+B+D — tabler point bullet' theme={theme} variant='after' />
      </div>
    </div>
  );
}

const meta = {
  argTypes: {
    theme: { control: 'inline-radio', options: ['dark', 'light'] },
  },
  component: SessionChatMarkerColumnStory,
  parameters: { layout: 'fullscreen' },
  title: 'Chat/Marker column',
} satisfies Meta<typeof SessionChatMarkerColumnStory>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Dark: Story = { args: { theme: 'dark' } };

export const Light: Story = { args: { theme: 'light' } };
