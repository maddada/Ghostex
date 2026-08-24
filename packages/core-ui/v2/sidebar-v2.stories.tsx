import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fireEvent, waitFor, within } from 'storybook/test';
import { SIDEBAR_V2_DISCOVERED_ICON_DATA_URL, SIDEBAR_V2_USER_ICON_DATA_URL } from './sidebar-v2-story-fixtures';
import type { SidebarStoryArgs } from '../sidebar-story-fixtures';
import {
  DEFAULT_SIDEBAR_STORY_ARGS,
  SIDEBAR_STORY_ARG_TYPES,
  SIDEBAR_STORY_DECORATORS,
  renderSidebarStory,
} from '../sidebar-story-meta';
import { findSidebarV2Row, waitForSidebarV2 } from './sidebar-v2.story-helpers';

/*
 * CDXC:SidebarV2 2026-07-29:
 * Visual stories for the Inbox sidebar. They render the REAL `SidebarApp`
 * through the shared harness with `sidebarVersion: "v2"`, so what Storybook
 * shows went through the same settings pipeline, message bridge, and store the
 * app uses — a standalone mount of the V2 tree would prove much less.
 */

const meta = {
  title: 'Sidebar/V2 Inbox',
  args: {
    ...DEFAULT_SIDEBAR_STORY_ARGS,
    fixture: 'sidebar-v2-inbox',
    /*
     * CDXC:SidebarV2Lifecycle 2026-07-29:
     * The default V2 story runs against a CURRENT gxserver. The degraded
     * old-daemon case is its own story (`WithoutLifecycleCapabilities`) rather
     * than the baseline, because the shelves are the point of this sidebar.
     *
     * CDXC:SidebarV2Git 2026-07-29:
     * "Current" now includes the git/PR probe, so the baseline shows the third
     * card line where the fixtures have one. The two degraded daemons get their
     * own stories: no capability block at all, and lifecycle without git.
     */
    sidebarLifecycleCapabilities: 'settleSnoozeAndGit',
    sidebarV2Layout: 'flat',
    sidebarVersion: 'v2',
  },
  argTypes: SIDEBAR_STORY_ARG_TYPES,
  decorators: SIDEBAR_STORY_DECORATORS,
  render: renderSidebarStory,
} satisfies Meta<SidebarStoryArgs>;

export default meta;

type Story = StoryObj<typeof meta>;

/** Mixed statuses in one screen: the Browser shelf leading the list, then
    attention, working, done, receded idle, a pinned row floating on top of the
    inbox, plus the Snoozed and Settled shelves below it. */
export const FlatInbox: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step('float the pinned session above the rest of the inbox', async () => {
      /*
       * CDXC:SidebarV2BrowserShelfFirst 2026-07-30:
       * Browser tabs lead the flat list now, and they render as cards too, so
       * "the inbox's first card" is the first card that is not a browser tab.
       * The fixture names every browser session `…-browser`, and nothing else
       * does.
       */
      const cardIds = [...root.querySelectorAll('.sidebar-v2-row[data-variant="card"][data-session-id]')].map((card) =>
        card.getAttribute('data-session-id')
      );
      expect(cardIds.find((sessionId) => !sessionId?.endsWith('-browser'))).toBe('v2-ghostex-pinned');
    });

    await step('render every status hue the resolver can produce', async () => {
      const kinds = new Set(
        [...root.querySelectorAll('.sidebar-v2-status')].map((element) => element.getAttribute('data-kind'))
      );
      expect(kinds.has('working')).toBe(true);
      expect(kinds.has('input')).toBe(true);
      expect(kinds.has('failed')).toBe(true);
      expect(kinds.has('done')).toBe(true);
      expect(kinds.has('idle')).toBe(true);
    });

    await step("paint attention amber, because Ghostex only knows 'act now'", async () => {
      /*
       * Ghostex publishes one `attention` activity with no approval-vs-input
       * split, so every attention row has to read as the loud one. Indigo is
       * reserved for a host that actually says `attentionKind: "input"`.
       */
      const attentionRow = await findSidebarV2Row(storyRoot, 'v2-quick-approval');
      const status = attentionRow.querySelector('.sidebar-v2-status');
      expect(status?.getAttribute('data-hue')).toBe('amber');
      expect([...root.querySelectorAll('.sidebar-v2-status[data-hue="indigo"]')].length).toBe(0);
    });

    await step('highlight exactly one current session', async () => {
      const activeRows = [...root.querySelectorAll('.sidebar-v2-row[data-active="true"]')];
      expect(activeRows.map((row) => row.getAttribute('data-session-id'))).toEqual(['v2-ghostex-working']);
    });

    await step('recede the resting rows and never the loud ones', async () => {
      const idleRow = await findSidebarV2Row(storyRoot, 'v2-quick-idle');
      const workingRow = await findSidebarV2Row(storyRoot, 'v2-ghostex-working');
      expect(idleRow.getAttribute('data-recede')).toBe('true');
      expect(workingRow.getAttribute('data-recede')).toBe('false');
    });
  },
};

/** The Snoozed shelf starts collapsed with its count in the header; Settled
    starts open. Expanding proves the count disappears once rows are visible. */
export const Shelves: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step('collapse Snoozed and show its count in the header', async () => {
      const header = root.querySelector<HTMLElement>('.sidebar-v2-shelf-header[data-tone="snoozed"]');
      expect(header?.getAttribute('aria-expanded')).toBe('false');
      expect(header?.textContent).toContain('Snoozed (1)');
      expect(root.querySelector('[data-session-id="v2-ghostex-snoozed"]')).toBeNull();
    });

    await step('reveal snoozed rows as slim rows when expanded', async () => {
      const header = root.querySelector<HTMLElement>('.sidebar-v2-shelf-header[data-tone="snoozed"]');
      fireEvent.click(header!);
      const row = await findSidebarV2Row(storyRoot, 'v2-ghostex-snoozed');
      expect(row.getAttribute('data-variant')).toBe('slim');
      expect(header?.textContent).toContain('Snoozed');
      expect(header?.textContent).not.toContain('(1)');
    });

    await step('park the long-idle session on the Settled shelf', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-ghostex-settled');
      expect(row.getAttribute('data-variant')).toBe('slim');
    });
  },
};

/**
 * CDXC:SidebarV2Lifecycle 2026-07-29:
 * The lifecycle shelves rendered from REAL server-owned state: a session parked
 * by the auto-settle sweep (override, no `settledAt`), one parked by an explicit
 * click (`settledAt` stamped, activity minutes old), and a snoozed session
 * stating when it comes back.
 */
export const LifecycleShelves: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step('park both settle shapes on the Settled shelf', async () => {
      const autoSettled = await findSidebarV2Row(storyRoot, 'v2-ghostex-settled');
      const handSettled = await findSidebarV2Row(storyRoot, 'v2-ghostex-settled-manual');
      expect(autoSettled.getAttribute('data-variant')).toBe('slim');
      expect(handSettled.getAttribute('data-variant')).toBe('slim');
    });

    await step('offer un-settle on settled rows and wake on snoozed rows', async () => {
      const settledRow = await findSidebarV2Row(storyRoot, 'v2-ghostex-settled');
      expect(settledRow.getAttribute('data-lifecycle-action')).toBe('unsettle');
      expect(settledRow.querySelector('[aria-label="Un-settle session"]')).toBeTruthy();

      fireEvent.click(root.querySelector<HTMLElement>('.sidebar-v2-shelf-header[data-tone="snoozed"]')!);
      const snoozedRow = await findSidebarV2Row(storyRoot, 'v2-ghostex-snoozed');
      expect(snoozedRow.getAttribute('data-lifecycle-action')).toBe('wake');
      expect(snoozedRow.querySelector('[aria-label="Wake session now"]')).toBeTruthy();
    });

    await step("state a snoozed row's return time instead of its last activity", async () => {
      const snoozedRow = await findSidebarV2Row(storyRoot, 'v2-ghostex-snoozed');
      const wakeLabel = snoozedRow.querySelector('[data-lifecycle-label="wake"]');
      expect(wakeLabel?.textContent).toMatch(/^\d+[mhd]$/);
    });

    await step('offer settle and snooze on ordinary inbox cards', async () => {
      const inboxRow = await findSidebarV2Row(storyRoot, 'v2-quick-idle');
      expect(inboxRow.getAttribute('data-lifecycle-action')).toBe('settle');
      expect(inboxRow.querySelector('[aria-label="Settle session"]')).toBeTruthy();
      expect(inboxRow.querySelector('[aria-label="Snooze session"]')).toBeTruthy();
    });

    await step('never offer settle to a session that is working or blocked', async () => {
      const workingRow = await findSidebarV2Row(storyRoot, 'v2-ghostex-working');
      expect(workingRow.querySelector('[aria-label="Settle session"]')).toBeNull();
      // Snooze IS allowed while working: it changes visibility, not the agent.
      expect(workingRow.querySelector('[aria-label="Snooze session"]')).toBeTruthy();

      const blockedRow = await findSidebarV2Row(storyRoot, 'v2-quick-approval');
      expect(blockedRow.querySelector('[aria-label="Settle session"]')).toBeNull();
      expect(blockedRow.querySelector('[aria-label="Snooze session"]')).toBeNull();
    });

    await step('keep lifecycle actions off browser rows', async () => {
      const browserRow = await findSidebarV2Row(storyRoot, 'v2-ghostex-browser');
      expect(browserRow.getAttribute('data-lifecycle-action')).toBe('none');
      expect(browserRow.querySelector('[aria-label="Snooze session"]')).toBeNull();
    });
  },
};

/**
 * A spent snooze and an early hand-raise both put a row back in the inbox. The
 * sort is static, so the row returns to its original slot and the wake signal
 * has to carry the whole message on its own.
 */
export const WokeFromSnooze: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;

    await waitForSidebarV2(storyRoot);

    await step('return an expired snooze to the inbox with a Woke badge', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-zmx-woke');
      expect(row.getAttribute('data-variant')).toBe('card');
      expect(row.getAttribute('data-woke')).toBe('true');
      const woke = row.querySelector('[data-lifecycle-label="woke"]');
      expect(woke?.textContent).toContain('Woke');
      expect(woke?.getAttribute('aria-label')).toBe('Woke from snooze');
    });

    await step('pull a still-snoozed session back the moment it is blocked on you', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-zmx-raised-hand');
      expect(row.getAttribute('data-variant')).toBe('card');
      expect(row.getAttribute('data-woke')).toBe('true');
    });

    await step('let the live status outrank the historical one', async () => {
      /*
       * A raised hand is an attention row first and a woken row second: its
       * slot must show the amber "act now" status, with the wake reduced to a
       * glyph beside it rather than a second competing label. Attention color
       * keys off data-hue, never data-kind.
       */
      const row = await findSidebarV2Row(storyRoot, 'v2-zmx-raised-hand');
      const status = row.querySelector('.sidebar-v2-status');
      expect(status?.getAttribute('data-hue')).toBe('amber');
      expect(row.querySelector('[data-lifecycle-label="woke"]')).toBeNull();
      expect(row.querySelector('[data-lifecycle-mark="woke"]')).toBeTruthy();
    });
  },
};

/**
 * An un-upgraded gxserver (a remote machine on an older build) publishes no
 * capability block. Nothing may classify as settled or snoozed, and no
 * lifecycle control may render — a disabled button would still promise a
 * feature the daemon cannot serve.
 */
export const WithoutLifecycleCapabilities: Story = {
  args: { sidebarLifecycleCapabilities: 'absent' },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step('keep every shelf empty', async () => {
      await waitFor(() => {
        expect(root.querySelector('.sidebar-v2-shelf-header[data-tone="settled"]')).toBeNull();
      });
      expect(root.querySelector('.sidebar-v2-shelf-header[data-tone="snoozed"]')).toBeNull();
    });

    await step('show the would-be settled and snoozed rows in the inbox instead', async () => {
      const settledRow = await findSidebarV2Row(storyRoot, 'v2-ghostex-settled');
      const snoozedRow = await findSidebarV2Row(storyRoot, 'v2-ghostex-snoozed');
      expect(settledRow.getAttribute('data-variant')).toBe('card');
      expect(snoozedRow.getAttribute('data-variant')).toBe('card');
    });

    await step('render no lifecycle affordance anywhere', async () => {
      expect(root.querySelectorAll('[data-lifecycle-action]:not(.sidebar-v2-row)')).toHaveLength(0);
      expect(root.querySelectorAll('[aria-label="Settle session"]')).toHaveLength(0);
      expect(root.querySelectorAll('[aria-label="Snooze session"]')).toHaveLength(0);
      expect(root.querySelectorAll('[data-lifecycle-label="woke"]')).toHaveLength(0);
      expect(root.querySelectorAll('[data-lifecycle-mark="woke"]')).toHaveLength(0);
    });
  },
};

/**
 * CDXC:SidebarV2Git 2026-07-29:
 * The card's third line, across every shape gxserver can publish: a branch on
 * its own, a branch with an open review and a live diff, a merged review parked
 * on the settled shelf, a draft, and a closed one. The two silent cases —
 * a probe that found nothing, and a session with no git data at all — must
 * produce no line whatsoever.
 */
export const GitAndPullRequestCards: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);

    await step("state branch, review, and diff on the card's meta line", async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-ghostex-working');
      const meta = row.querySelector<HTMLElement>('[data-line="meta"]');
      expect(meta?.getAttribute('data-meta')).toBe('git');
      expect(meta?.querySelector('.sidebar-v2-row-branch-name')?.textContent).toBe('ghostex/sidebar-v2-inbox');
      expect(meta?.querySelector('.sidebar-v2-row-pr')?.textContent).toBe('#128');
      expect(meta?.querySelector('.sidebar-v2-row-pr')?.getAttribute('data-pr-state')).toBe('open');
      expect(meta?.querySelector('.sidebar-v2-row-diff-added')?.textContent).toBe('+412');
      expect(meta?.querySelector('.sidebar-v2-row-diff-removed')?.textContent).toBe('−87');
    });

    await step('keep a card three lines with git, exactly as it was without', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-ghostex-working');
      expect(row.closest('.sidebar-v2-row-item')?.getAttribute('data-card-lines')).toBe('3');
    });

    await step('show a lone branch with no badge and no diff', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-ghostex-pinned');
      const meta = row.querySelector<HTMLElement>('[data-line="meta"]');
      expect(meta?.querySelector('.sidebar-v2-row-branch-name')?.textContent).toBe('release/6.9');
      expect(meta?.querySelector('.sidebar-v2-row-pr')).toBeNull();
      expect(meta?.querySelector('.sidebar-v2-row-diff')).toBeNull();
    });

    await step('color the draft and closed reviews by their own state', async () => {
      const draftRow = await findSidebarV2Row(storyRoot, 'v2-zmx-failed');
      expect(draftRow.querySelector('.sidebar-v2-row-pr')?.getAttribute('data-pr-state')).toBe('draft');
      const closedRow = await findSidebarV2Row(storyRoot, 'v2-zmx-done');
      expect(closedRow.querySelector('.sidebar-v2-row-pr')?.getAttribute('data-pr-state')).toBe('closed');
    });

    await step('keep only the PR badge on a slim settled row', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-ghostex-settled-manual');
      expect(row.getAttribute('data-variant')).toBe('slim');
      const badge = row.querySelector('.sidebar-v2-row-pr');
      expect(badge?.textContent).toBe('#124');
      expect(badge?.getAttribute('data-pr-state')).toBe('merged');
      expect(row.querySelector('.sidebar-v2-row-branch')).toBeNull();
      expect(row.querySelector('.sidebar-v2-row-diff')).toBeNull();
    });

    /*
     * 2026-07-30: a probe that found nothing leaves the card with NO meta line
     * at all. It used to fall back to `session.detail`, which gxserver defines
     * as the session's cwd (or the project's path) — a folder path, never the
     * agent name the fixtures' prose suggested.
     */
    await step('render nothing for a probe that found nothing to say', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-quick-approval');
      expect(row.querySelector('[data-sidebar-v2-git]')).toBeNull();
      expect(row.querySelector('[data-line="meta"]')).toBeNull();
      expect(row.closest('.sidebar-v2-row-item')?.getAttribute('data-card-lines')).toBe('2');
    });

    await step('never show a folder path instead of a branch', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-quick-idle');
      expect(row.querySelector('[data-sidebar-v2-git]')).toBeNull();
      expect(row.querySelector('[data-line="meta"]')).toBeNull();
      expect(row.querySelector('.sidebar-v2-row-meta')).toBeNull();
      expect(row.closest('.sidebar-v2-row-item')?.getAttribute('data-card-lines')).toBe('2');
    });
  },
};

/**
 * CDXC:SidebarV2Git 2026-07-29:
 * A daemon upgraded to settle/snooze but not to the git probe. Its rows carry
 * no git data on the wire, and the sidebar must not render a branch line for
 * one anyway — this story pins that the capability, not the fixture, is what
 * decides.
 */
export const WithoutGitCapability: Story = {
  args: { sidebarLifecycleCapabilities: 'settleAndSnooze' },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step('render no branch, badge, or diff anywhere', async () => {
      await waitFor(() => {
        expect(root.querySelectorAll('[data-sidebar-v2-git]')).toHaveLength(0);
      });
      expect(root.querySelectorAll('.sidebar-v2-row-pr')).toHaveLength(0);
      expect(root.querySelectorAll('.sidebar-v2-row-diff')).toHaveLength(0);
    });

    await step('keep the card identical to a session with no git data', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-ghostex-working');
      expect(row.querySelector('[data-line="meta"]')).toBeNull();
      expect(row.closest('.sidebar-v2-row-item')?.getAttribute('data-card-lines')).toBe('2');
    });

    await step('keep the settle/snooze affordances the daemon does support', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-quick-idle');
      expect(row.querySelector('[aria-label="Settle session"]')).toBeTruthy();
    });
  },
};

/** Browser sessions get their own flat-mode section instead of the inbox. */
export const BrowserSection: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step('list browser sessions under their own header', async () => {
      const header = root.querySelector<HTMLElement>('.sidebar-v2-shelf-header[data-tone="browser"]');
      expect(header?.textContent).toContain('Browser');
      await findSidebarV2Row(storyRoot, 'v2-ghostex-browser');
      await findSidebarV2Row(storyRoot, 'v2-zmx-browser');
    });

    await step('keep browser rows out of the agent inbox', async () => {
      const inboxIds = [...root.querySelectorAll('.sidebar-v2-list > .sidebar-v2-row-item[data-variant="card"]')];
      expect(inboxIds.length).toBeGreaterThan(0);
    });
  },
};

/** Group by Project: collapsible project groups, browser rows above agent rows,
    and a per-project Settled shelf. */
export const ByProject: Story = {
  args: { sidebarV2Layout: 'byProject' },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step('render one group per project', async () => {
      await waitFor(() => {
        expect(root.querySelectorAll('[data-sidebar-v2-group-id]').length).toBe(3);
      });
    });

    await step('render browser rows above agent rows inside a project', async () => {
      const group = root.querySelector<HTMLElement>('[data-sidebar-v2-group-id="v2-project-ghostex"]');
      const rowIds = [...group!.querySelectorAll('[data-session-id]')].map((element) =>
        element.getAttribute('data-session-id')
      );
      expect(rowIds[0]).toBe('v2-ghostex-browser');
    });

    /*
     * CDXC:SidebarV2ProjectIcons 2026-07-29:
     * A group header IS the project's name, so it carries the project's own
     * icon. Grouped mode drops the per-card project line, which makes this the
     * only place the identity is stated.
     */
    await step("show each group header's real project icon", async () => {
      const ghostexHeader = root.querySelector<HTMLElement>(
        '[data-sidebar-v2-group-id="v2-project-ghostex"] .group-head'
      );
      expect(ghostexHeader?.querySelector('.sidebar-v2-project-icon[data-icon-variant="tabler"]')).toBeTruthy();
      const zmxHeader = root.querySelector<HTMLElement>('[data-sidebar-v2-group-id="v2-project-zmx"] .group-head');
      expect(zmxHeader?.querySelector('img.sidebar-v2-project-icon')).toBeTruthy();
    });

    await step('give each project its own Settled shelf', async () => {
      const group = root.querySelector<HTMLElement>('[data-sidebar-v2-group-id="v2-project-ghostex"]');
      expect(group!.querySelector('.sidebar-v2-shelf-header[data-tone="settled"]')).toBeTruthy();
    });

    await step('drop the project line: the group header already states it', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-ghostex-working');
      expect(row.querySelector('[data-line="project"]')).toBeNull();
      const status = row.querySelector<HTMLElement>('.sidebar-v2-status');
      expect(status?.closest('[data-line="title"]')).toBeTruthy();
      expect(row.closest('.sidebar-v2-row-item')?.getAttribute('data-card-lines')).toBe('2');
    });
  },
};

/** gxserver is down: the sidebar holds only the synthetic placeholder group.
    V2 must never render that as a project — it shows the same recovery block
    the classic sidebar shows. */
export const GxserverUnavailable: Story = {
  args: { fixture: 'sidebar-v2-gxserver-unavailable' },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step('keep the placeholder group out of the inbox', async () => {
      expect(root.querySelector('[data-sidebar-v2-group-id="gxserver-unavailable"]')).toBeNull();
      expect(root.querySelector('.sidebar-v2-row')).toBeNull();
    });

    await step('show the host recovery copy instead of an inbox empty state', async () => {
      /*
       * The copy is deliberately delayed by 20s while a cold start can still
       * recover (see SIDEBAR_GXSERVER_UNAVAILABLE_EMPTY_STATE_DELAY_MS), so this
       * waits past that window rather than asserting an instant message.
       */
      await waitFor(
        () => {
          expect(root.querySelector('.reference-sidebar-empty-state')?.textContent).toContain(
            'Unable to load sessions.'
          );
        },
        { timeout: 30_000 }
      );
      expect(root.querySelector('.sidebar-v2-empty-message')).toBeNull();
    });
  },
};

/** Nothing to show at all: the one moment a user could suspect V2 lost their
    sessions, so the escape hatch back to the classic sidebar lives here. */
export const EmptyInbox: Story = {
  args: { fixture: 'sidebar-v2-empty' },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step('explain the empty inbox and offer the way back', async () => {
      await waitFor(() => {
        expect(root.querySelector('.sidebar-v2-empty-message')?.textContent).toBe('No sessions yet');
      });
      expect(root.querySelector('.sidebar-v2-empty-action')?.textContent).toContain('classic sidebar');
    });
  },
};

/*
 * 2026-07-30 (UX batch):
 * The row's hover chrome, after three linked decisions that only make sense
 * together:
 *
 * 1. NO SCRIM. The bar used to paint an opaque gradient of the row's own tint to
 *    swallow the text it covers. The controls are CHIPS instead — filled,
 *    bordered squares, the same token-for-token chip the project header and the
 *    RN mobile app use — so they read as chrome sitting on the hovered row.
 * 2. PIN IS UNPIN, AND ONLY WHEN PINNED. A pin control on every row spent the
 *    bar's scarcest space on its rarest action; pinning lives in the menu, and a
 *    pinned row states itself at rest with a small mark in the resting slot.
 * 3. NO ⋯ TRIGGER. Right-click is the menu, so the button only competed with the
 *    triage verbs. This story pins that the menu still opens — anchored to the
 *    pointer, which is the part a removed button could have taken with it.
 *
 * The chip geometry is read off `getComputedStyle` rather than trusted, because
 * the whole point of the change is a visual one and the shipped rule is the only
 * thing that can prove it.
 */
export const RowActionChips: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);
    const view = storyRoot.ownerDocument.defaultView;

    await step('style every control as a 20px chip, with no scrim behind them', async () => {
      const row = await findSidebarV2Row(storyRoot, 'v2-quick-idle');
      const bar = row.querySelector<HTMLElement>('.sidebar-v2-row-actions');
      expect(bar).toBeTruthy();
      const barStyle = view!.getComputedStyle(bar as HTMLElement);
      expect(barStyle.backgroundImage).toBe('none');
      /* Out of flow: the F8 no-reflow invariant is not relaxed by the restyle. */
      expect(barStyle.position).toBe('absolute');

      const snooze = row.querySelector<HTMLElement>('[aria-label="Snooze session"]');
      expect(snooze).toBeTruthy();
      const chipStyle = view!.getComputedStyle(snooze as HTMLElement);
      expect(chipStyle.width).toBe('20px');
      expect(chipStyle.height).toBe('20px');
      expect(chipStyle.borderTopWidth).toBe('1px');
      expect(chipStyle.borderTopStyle).toBe('solid');
      expect(chipStyle.borderTopLeftRadius).toBe('6px');
      /*
       * A filled, OPAQUE chip, not the old transparent icon button — and not a
       * translucent one either: with no scrim behind the bar, an alpha below 1
       * let a long project name read straight through the buttons.
       */
      expect(chipStyle.backgroundColor).not.toBe('rgba(0, 0, 0, 0)');
      expect(chipStyle.backgroundColor).not.toMatch(/\/\s*0?\.\d/);

      /* Settle is the one chip that grows to its label instead of squeezing it. */
      const settle = row.querySelector<HTMLElement>('[aria-label="Settle session"]');
      expect(settle).toBeTruthy();
      expect(Number.parseFloat(view!.getComputedStyle(settle as HTMLElement).width)).toBeGreaterThan(20);
    });

    await step('offer unpin, leftmost, only on a pinned row', async () => {
      const pinnedRow = await findSidebarV2Row(storyRoot, 'v2-ghostex-pinned');
      const bar = pinnedRow.querySelector<HTMLElement>('.sidebar-v2-row-actions');
      const unpin = bar?.querySelector<HTMLElement>('[aria-label="Unpin session"]');
      expect(unpin).toBeTruthy();
      expect(bar?.firstElementChild).toBe(unpin);

      const plainRow = await findSidebarV2Row(storyRoot, 'v2-quick-idle');
      expect(plainRow.querySelector('[aria-label="Unpin session"]')).toBeNull();
      expect(plainRow.querySelector('[aria-label="Pin session"]')).toBeNull();
      expect(root.querySelectorAll('[aria-label="Pin session"]')).toHaveLength(0);
    });

    await step('mark a pinned row at rest, inside the slot that swaps on hover', async () => {
      const pinnedRow = await findSidebarV2Row(storyRoot, 'v2-ghostex-pinned');
      const mark = pinnedRow.querySelector<HTMLElement>('[data-sidebar-v2-pinned]');
      expect(mark).toBeTruthy();
      expect(mark?.closest('.sidebar-v2-row-slot-status')).toBeTruthy();
      expect(pinnedRow.getAttribute('data-pinned')).toBe('true');

      const plainRow = await findSidebarV2Row(storyRoot, 'v2-quick-idle');
      expect(plainRow.querySelector('[data-sidebar-v2-pinned]')).toBeNull();
    });

    await step('keep the menu on right-click, anchored to the pointer', async () => {
      expect(root.querySelectorAll('[aria-label="Session actions"]')).toHaveLength(0);
      const row = await findSidebarV2Row(storyRoot, 'v2-quick-idle');
      /*
       * The pointer coordinates are deliberately a fixed point near the
       * viewport's top-left, NOT the row's own rect: `SidebarContextMenuPortal`
       * viewport-clamps the inline left/top, so anchoring the assertion to a row
       * that happens to sit far right in the story canvas measured the CLAMP
       * instead of the anchor (and made the expectation canvas-width dependent).
       * A contextmenu event's coordinates do not have to lie inside its target,
       * so 40/40 is both legal and always unclamped.
       */
      const clientX = 40;
      const clientY = 40;
      fireEvent.contextMenu(row, { bubbles: true, clientX, clientY });
      const item = await body.findByRole('menuitem', { name: 'Rename' });
      const menu = item.closest<HTMLElement>('.sidebar-v2-session-context-menu');
      expect(menu).toBeTruthy();
      /* Non-vacuity for the "unclamped" premise above. */
      const menuRect = (menu as HTMLElement).getBoundingClientRect();
      expect(menuRect.right).toBeLessThan(view!.innerWidth);
      expect(menuRect.bottom).toBeLessThan(view!.innerHeight);
      expect(menu?.style.left).toBe(`${clientX}px`);
      expect(menu?.style.top).toBe(`${clientY}px`);
      /* Pinning still has a home now that the bar dropped its pin control. */
      await body.findByRole('menuitem', { name: 'Pin' });
    });
  },
};

/*
 * CDXC:SidebarV2RowWidth 2026-07-29:
 * The project line at the DEFAULT 260px sidebar width, measured rather than
 * eyeballed. Two properties have to hold at the same time, and the obvious fix
 * for either one breaks the other:
 *
 * - A name that fits must render in full. Reserving the hover action bar's
 *   width on every resting row ellipsised names against a half-empty line
 *   ("maddada/gh…" with a finger-wide hole before a 3-character status).
 * - Hovering must not move anything. Letting the action bar take that space
 *   back in flow reflows the name every time the pointer crosses a row.
 *
 * The hover half is exercised through `data-menu-open`, which the shipped CSS
 * drives with the SAME declarations as `:hover` (a story cannot trigger a real
 * `:hover`), so this measures the rules that actually ship rather than a copy
 * of them.
 */
export const ProjectLineWidth: Story = {
  args: { fixture: 'sidebar-v2-row-width' },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);

    const projectLabel = async (sessionId: string): Promise<HTMLElement> => {
      const row = await findSidebarV2Row(storyRoot, sessionId);
      const label = row.querySelector<HTMLElement>('.sidebar-v2-row-project');
      expect(label).toBeTruthy();
      return label as HTMLElement;
    };

    await step('show a project name that fits in full, with no ellipsis', async () => {
      const label = await projectLabel('v2-width-fits-session');
      expect(label.textContent).toBe('maddada/ghostex');
      await waitFor(() => {
        expect(label.scrollWidth).toBeLessThanOrEqual(label.clientWidth);
      });
    });

    await step('still truncate a name that genuinely cannot fit', async () => {
      const label = await projectLabel('v2-width-overflows-session');
      expect(label.scrollWidth).toBeGreaterThan(label.clientWidth);
    });

    await step('keep line 1 pixel-identical while the actions are revealed', async () => {
      const label = await projectLabel('v2-width-fits-session');
      const row = label.closest<HTMLElement>('.sidebar-v2-row');
      const actions = row?.querySelector<HTMLElement>('.sidebar-v2-row-actions');
      expect(actions).toBeTruthy();
      const view = label.ownerDocument.defaultView;
      expect(view?.getComputedStyle(actions as HTMLElement).visibility).toBe('hidden');

      const restingRect = label.getBoundingClientRect();
      const restingScroll = label.scrollWidth;

      row?.setAttribute('data-menu-open', 'true');
      try {
        /* Non-vacuity: the swap must really have happened before measuring. */
        expect(view?.getComputedStyle(actions as HTMLElement).visibility).toBe('visible');
        const hoveredRect = label.getBoundingClientRect();
        expect(hoveredRect.width).toBe(restingRect.width);
        expect(hoveredRect.left).toBe(restingRect.left);
        expect(label.scrollWidth).toBe(restingScroll);
        expect(label.scrollWidth).toBeLessThanOrEqual(label.clientWidth);
      } finally {
        row?.setAttribute('data-menu-open', 'false');
      }
    });

    await step("render each project's real icon, folder only when there is none", async () => {
      const fitsRow = await findSidebarV2Row(storyRoot, 'v2-width-fits-session');
      const image = fitsRow.querySelector<HTMLImageElement>('img.sidebar-v2-project-icon');
      expect(image?.getAttribute('src')).toContain('data:image/png;base64,');

      const tablerRow = await findSidebarV2Row(storyRoot, 'v2-width-overflows-session');
      expect(tablerRow.querySelector('.sidebar-v2-project-icon[data-icon-variant="tabler"]')).toBeTruthy();
      expect(tablerRow.querySelector('img.sidebar-v2-project-icon')).toBeNull();

      const plainRow = await findSidebarV2Row(storyRoot, 'v2-width-plain-session');
      expect(plainRow.querySelector('.sidebar-v2-project-icon[data-icon-variant="glyph"]')).toBeTruthy();
    });
  },
};

/*
 * CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
 * The precedence chain: a user-attached IMAGE wins, the icon the project's own
 * repository ships comes next, a typed Tabler glyph after that, and the folder
 * is left only for a project with nothing at all.
 *
 * Every assertion reads the RENDERED variant rather than "is there an image",
 * because an image alone cannot tell the user's PNG apart from a favicon found
 * on disk — and that confusion is exactly the regression this guards against.
 * Every winning row deliberately ALSO carries the losing candidates, so each
 * check is a real comparison instead of a project with only one icon.
 */
export const ProjectIconPrecedence: Story = {
  args: { fixture: 'sidebar-v2-project-icons' },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);

    const projectIcon = async (sessionId: string): Promise<HTMLElement> => {
      const row = await findSidebarV2Row(storyRoot, sessionId);
      const icon = row.querySelector<HTMLElement>('.sidebar-v2-project-icon');
      expect(icon).toBeTruthy();
      return icon as HTMLElement;
    };

    await step('keep a user-attached image ahead of the discovered icon', async () => {
      const icon = await projectIcon('v2-icons-user-image-session');
      expect(icon.getAttribute('data-icon-variant')).toBe('image');
      expect(icon.getAttribute('src')).toBe(SIDEBAR_V2_USER_ICON_DATA_URL);
      expect(icon.getAttribute('src')).not.toBe(SIDEBAR_V2_DISCOVERED_ICON_DATA_URL);
    });

    await step("show the repository's favicon ahead of a stale typed glyph", async () => {
      /*
       * The reported bug: this project carries the same legacy `archive` glyph
       * the user's own Ghostex project still has, migrated forward from the
       * deprecated macOS app's picker. A glyph nobody can even set any more must
       * not hide the icon the repository actually ships.
       */
      const icon = await projectIcon('v2-icons-legacy-glyph-session');
      expect(icon.getAttribute('data-icon-variant')).toBe('discovered');
      expect(icon.getAttribute('src')).toBe(SIDEBAR_V2_DISCOVERED_ICON_DATA_URL);
    });

    await step('keep the typed glyph when the repository ships nothing', async () => {
      const icon = await projectIcon('v2-icons-glyph-only-session');
      expect(icon.getAttribute('data-icon-variant')).toBe('tabler');
    });

    await step("show the repository's own icon when the user chose none", async () => {
      const icon = await projectIcon('v2-icons-discovered-session');
      expect(icon.getAttribute('data-icon-variant')).toBe('discovered');
      expect(icon.getAttribute('src')).toBe(SIDEBAR_V2_DISCOVERED_ICON_DATA_URL);
      /*
       * Rounded and contained like the browser favicons it sits beside, and
       * from the SHARED `.sidebar-v2-project-icon` rule rather than a
       * variant-specific one — the discovered icon is a project icon, not a
       * fourth kind of chrome.
       */
      const view = icon.ownerDocument.defaultView;
      expect(view?.getComputedStyle(icon).borderRadius).toBe('5px');
      expect(view?.getComputedStyle(icon).objectFit).toBe('contain');
    });

    await step('carry the discovered icon onto browser rows too', async () => {
      const icon = await projectIcon('v2-icons-discovered-browser');
      expect(icon.getAttribute('data-icon-variant')).toBe('discovered');
    });

    await step('fall back to the folder only with no icon at all', async () => {
      const icon = await projectIcon('v2-icons-none-session');
      expect(icon.getAttribute('data-icon-variant')).toBe('glyph');
    });

    await step('resolve every scope menu entry through the same chain', async () => {
      const trigger = root.querySelector<HTMLElement>('.sidebar-v2-scope-trigger');
      expect(trigger).toBeTruthy();
      fireEvent.click(trigger as HTMLElement);
      const variantOf = async (name: RegExp): Promise<string | null | undefined> =>
        (await body.findByRole('menuitemradio', { name }))
          .querySelector<HTMLElement>('.sidebar-v2-project-icon')
          ?.getAttribute('data-icon-variant');
      expect(await variantOf(/picked-image/)).toBe('image');
      expect(await variantOf(/legacy-glyph-and-favicon/)).toBe('discovered');
      expect(await variantOf(/glyph-only/)).toBe('tabler');
      expect(await variantOf(/ships-a-favicon/)).toBe('discovered');
      expect(await variantOf(/no-icon-at-all/)).toBe('glyph');
    });
  },
};

/** The same chain on the group headers, which are the ONLY place Group-by-Project
    states a project's identity (grouped rows drop the per-card project line). */
export const ProjectIconPrecedenceInGroups: Story = {
  args: { fixture: 'sidebar-v2-project-icons', sidebarV2Layout: 'byProject' },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    const headerIconVariant = (groupId: string): string | null | undefined =>
      root
        .querySelector<HTMLElement>(`[data-sidebar-v2-group-id="${groupId}"] .group-head .sidebar-v2-project-icon`)
        ?.getAttribute('data-icon-variant');

    await step('resolve every group header through the same chain', async () => {
      await waitFor(() => {
        expect(root.querySelectorAll('[data-sidebar-v2-group-id]').length).toBe(5);
      });
      expect(headerIconVariant('v2-icons-user-image')).toBe('image');
      expect(headerIconVariant('v2-icons-legacy-glyph')).toBe('discovered');
      expect(headerIconVariant('v2-icons-glyph-only')).toBe('tabler');
      expect(headerIconVariant('v2-icons-discovered')).toBe('discovered');
      expect(headerIconVariant('v2-icons-none')).toBe('glyph');
    });
  },
};
