/**
 * Diffs the CALLS the Rust sidebar actions make against the calls the TypeScript they replace
 * makes, for every payload of every scenario.
 *
 *   bun tooling/gx-core/menu-parity.ts scenarios <frames.jsonl> <out-dir> [settings.json]
 *   bun tooling/gx-core/action-parity.ts snooze-clock <out-dir>
 *   cargo run --release --example sidebar_menu_parity   -- <out-dir>   # from packages/gx-core
 *   cargo run --release --example sidebar_action_parity -- <out-dir>   # from packages/gx-core
 *   bun tooling/gx-core/action-parity.ts compare <out-dir>
 *
 * `snooze-clock` runs first and writes the clock facts the snooze half is answered against: a
 * fixed instant per case, the local UTC offset at it, and the offset at 09:00 local on each of the
 * next seven days. It exists because the snooze rule is the one that reads a clock AND a calendar,
 * so neither side may read a real one while the gate runs, and because the cases that matter are
 * the two days a year a zone changes offset, which a list of arbitrary instants would miss.
 *
 * **Why this gate exists and what it has to be able to do.** A sidebar action does not move the
 * sidebar, so no comparison of the drawn list can see a wrong one: the wrong native action, or the
 * right one carrying the wrong project id, produces exactly the same rows. The gate therefore
 * compares the calls, and it enumerates rather than samples: every project of the recording gets
 * every message type, every id shape that resolves to no project is probed, both text edges of the
 * two copy actions are probed, and every read-only command the real menus build is added on top.
 *
 * A gate that cannot fail is worse than none, so this one is proved discriminating by
 * `--inject <mutation>`, which mutates the Rust side after the fact and expects a non-zero
 * difference count.
 *
 * Recordings and dumps hold private data: keep <out-dir> outside the repository.
 */
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

type Json = Record<string, any>;

/**
 * The zone the whole gate runs in.
 *
 * Pinned rather than inherited: "Tomorrow" and "Next week" mean 09:00 LOCAL time, so the answer
 * depends on the machine's zone, and the cases worth asking about are the two days a year the
 * offset changes. America/New_York has both, on dates far enough from the recording's own
 * timestamps to be unambiguous. Everything else in this gate is zone-independent, which the
 * counters prove: pinning it left every other total unchanged.
 */
const SNOOZE_TIME_ZONE = 'America/New_York';
const SNOOZE_CLOCK_FILE = 'snooze-clock.json';

/**
 * Pins the zone and checks that it took.
 *
 * Bun applies a runtime `process.env.TZ`, but a gate that assumed it without looking would compare
 * two zones and report whatever the machine happened to be set to, so the two known offsets of the
 * pinned zone are asserted before anything is measured.
 */
function pinTimeZone(): void {
  process.env.TZ = SNOOZE_TIME_ZONE;
  const winter = new Date('2026-01-15T12:00:00Z').getTimezoneOffset();
  const summer = new Date('2026-07-15T12:00:00Z').getTimezoneOffset();
  if (winter !== 300 || summer !== 240) {
    console.error(
      `the gate could not pin ${SNOOZE_TIME_ZONE} (offsets read ${winter} and ${summer}); run it with TZ=${SNOOZE_TIME_ZONE}`
    );
    process.exit(2);
  }
}

/**
 * The instants the snooze rule is asked about, as local wall-clock times in the pinned zone.
 *
 * Chosen rather than sampled. The four that matter are the ones where the offset at the target
 * morning is not the offset now, which is the whole reason the port takes the target's offset as
 * data; the rest cover every weekday (which is what "Next week" counts from), the ends of a month,
 * a year and a leap February, and the two ticks either side of local midnight, which is where a
 * wrong local-day calculation shows up.
 */
const SNOOZE_CLOCK_CASES: [string, string][] = [
  ['springForwardEve', '2026-03-07T20:00:00'],
  ['springForwardMonday', '2026-03-02T09:30:00'],
  ['fallBackEve', '2026-10-31T20:00:00'],
  ['fallBackTuesday', '2026-10-27T09:30:00'],
  ['monday', '2026-06-01T15:30:00'],
  ['tuesday', '2026-06-02T15:30:00'],
  ['wednesday', '2026-06-03T15:30:00'],
  ['thursday', '2026-06-04T15:30:00'],
  ['friday', '2026-06-05T15:30:00'],
  ['saturday', '2026-06-06T15:30:00'],
  ['sunday', '2026-06-07T15:30:00'],
  ['monthEnd', '2026-01-31T23:00:00'],
  ['yearEnd', '2026-12-31T22:00:00'],
  ['leapDayEve', '2028-02-28T21:00:00'],
  ['justBeforeMidnight', '2026-06-15T23:59:59.999'],
  ['justAfterMidnight', '2026-06-16T00:00:00.000'],
  ['beforeNineInTheMorning', '2026-06-16T08:59:00'],
];

/** Writes the clock facts the Rust example answers against. */
function writeSnoozeClock([outDir]: string[]): void {
  if (!outDir) throw new Error('snooze-clock <out-dir>');
  pinTimeZone();
  const cases = SNOOZE_CLOCK_CASES.map(([name, local]) => {
    // A date-time with no offset is local time, which is what makes these wall-clock cases.
    const now = new Date(local);
    const morningOffsetsMs: number[] = [];
    for (let days = 0; days < 8; days += 1) {
      const morning = new Date(now);
      morning.setDate(morning.getDate() + days);
      morning.setHours(9, 0, 0, 0);
      morningOffsetsMs.push(-morning.getTimezoneOffset() * 60_000);
    }
    return {
      case: name,
      local,
      nowMs: now.getTime(),
      offsetMs: -now.getTimezoneOffset() * 60_000,
      morningOffsetsMs,
    };
  });
  writeFileSync(join(outDir, SNOOZE_CLOCK_FILE), JSON.stringify({ timeZone: SNOOZE_TIME_ZONE, cases }), {
    mode: 0o600,
  });
  console.log(`${SNOOZE_CLOCK_FILE}: ${cases.length} cases in ${SNOOZE_TIME_ZONE}`);
}

/**
 * The mutations `--inject` can apply to the Rust side, each one a plausible port mistake. A
 * mutation is handed the entry as well as its calls, so a mistake that shows up as a MISSING call
 * can be injected too: a branch whose right answer is "no call at all" is exactly the one a diff
 * of two call lists is weakest on.
 */
const MUTATIONS: Record<string, (calls: Json[], entry: Json, dump: Json) => Json[]> = {
  // The right call with the wrong action name: what a copy-and-paste between the three
  // project-path actions looks like, and what no list comparison can see.
  'swap-finder-for-ide': (calls) =>
    calls.map((call) =>
      call.call === 'nativeProjectPathAction' && call.payload?.action === 'openWorkspaceProjectInFinder'
        ? { ...call, payload: { ...call.payload, action: 'openWorkspaceProjectInIde' } }
        : call
    ),
  // The text trimmed on the way to the clipboard: `normalizeNonEmptyString` tests the trim and
  // returns the original, and getting that backwards changes what the user pastes.
  'trim-copied-text': (calls) =>
    calls.map((call) => (call.call === 'copyText' ? { ...call, text: String(call.text).trim() } : call)),
  // The bridge payload built without one of its two fixed fields, which the native side would
  // then refuse; the action would simply not happen and nothing would say so.
  'drop-the-version-field': (calls) =>
    calls.map((call) =>
      call.call === 'nativeProjectPathAction'
        ? { ...call, payload: Object.fromEntries(Object.entries(call.payload).filter(([key]) => key !== 'version')) }
        : call
    ),
  // The remote leg answered as a local one.
  'remote-as-local': (calls) =>
    calls.map((call) =>
      call.call === 'nativeProjectPathAction' && String(call.payload?.action).startsWith('copyRemote')
        ? { ...call, payload: { ...call.payload, action: 'copyWorkspaceProjectPath' } }
        : call
    ),
  // The toast dropped, which is the one leg with no call at all to compare against.
  'drop-the-toast': (calls) => calls.filter((call) => call.call !== 'toast'),
  // A project the daemon parked as a Recent Project still resolving to a group. This is the one
  // mistake whose symptom is an EXTRA call where the right answer is none, and it is the reason
  // the dump carries a second variant with the first project parked.
  'parked-still-resolves': (calls, entry, dump) => {
    const parked = typeof dump.parkedProjectId === 'string' ? dump.parkedProjectId : undefined;
    if (entry.variant !== 'firstParked' || !parked || calls.length) return calls;
    const groupId = typeof entry.payload?.groupId === 'string' ? entry.payload.groupId : '';
    if (groupId !== `combined-project:${encodeURIComponent(parked)}`) return calls;
    const action = LOCAL_ACTION_BY_TYPE[String(entry.payload.type)];
    if (!action) return calls;
    return [
      {
        call: 'nativeProjectPathAction',
        payload: {
          action,
          projectId: parked,
          type: 'ghostex.gpui.sidebar.nativeProjectPathAction',
          version: 1,
        },
      },
    ];
  },
};

/**
 * The transition half's mutations. They act on a whole dump entry rather than on a call list,
 * because what they have to be able to break is a STATE the daemon then contradicts.
 */
/**
 * The close half's mutations. The first is the port's own deliberate difference removed, which
 * must show up as the gate's classification going to zero rather than as agreement.
 */
function mutateClose(name: string | undefined, entry: Json): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  switch (name) {
    // The row left missing after a call that never came home, which is what the TypeScript does
    // and what this port deliberately does not.
    case 'never-restore-the-row':
      if (clone.answers?.failed) clone.answers.failed.drawn = false;
      if (clone.answers?.neverAnswered) clone.answers.neverAnswered.drawn = false;
      return clone;
    // The row not taken away at all, so the click does nothing until the daemon answers.
    case 'close-is-not-optimistic':
      if (clone.optimistic) clone.optimistic.drawn = true;
      if (clone.answers?.accepted) clone.answers.accepted.drawn = true;
      return clone;
    // The row put back even when the daemon DID take the close, which would make every close
    // flicker and then depend on the removal delta to finish.
    case 'restore-after-an-accepted-close':
      if (clone.answers?.accepted) clone.answers.accepted.drawn = true;
      return clone;
    // The focus left on the row that is going away.
    case 'drop-the-close-focus':
      if (clone.optimistic) clone.optimistic.focus = [];
      return clone;
    // A daemon removal that does not retire the local hide, which is the leak that would make a
    // re-created session with the same id invisible for the rest of the run.
    case 'keep-the-hide-after-removal':
      if (clone.echo?.removed) clone.echo.removed.drawn = true;
      return clone;
    default:
      return clone;
  }
}

/** The fork half's mutations. */
function mutateFork(name: string | undefined, entry: Json): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  switch (name) {
    // A pane placed for a success envelope that carried no session id, which is the leg a port is
    // most likely to answer with a pane that has nothing behind it.
    case 'place-a-pane-with-no-session':
      if (clone.answers?.emptyFork) clone.answers.emptyFork = clone.answers.accepted;
      return clone;
    // The new pane appended beside whichever pane is focused instead of beside the row the fork
    // came from, which is the placement bug the TypeScript's own comment warns about.
    case 'forget-the-placement-target':
      for (const answer of Object.values(clone.answers ?? {}) as Json[][])
        for (const follow of answer) if (follow.follow === 'placePane') delete follow.placementTarget;
      return clone;
    // A failed fork that moves the user's project anyway when it should not have to.
    case 'always-activate':
      if (clone.request) clone.request.activate = `combined-project:x`;
      return clone;
    default:
      return clone;
  }
}

/** The flags half's mutations. */
function mutateFlags(name: string | undefined, entry: Json): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  switch (name) {
    // A cleared tag written as a null instead of as an absent field, which is the shape the row
    // really takes and the one a port collapses.
    case 'clear-a-tag-as-null':
      for (const answer of Object.values(clone.answers ?? {}) as Json[])
        if (answer?.row && answer.row.sessionTag === null) answer.row.sessionTag = 'null';
      return clone;
    // A tag set without the star it implies, which leaves the two disagreeing until the daemon's
    // own row arrives.
    case 'tag-without-the-star':
      if (clone.request?.rpc?.params) delete clone.request.rpc.params.isFavorite;
      return clone;
    // An optimistic value applied to a call that failed.
    case 'patch-a-failed-flag-call':
      if (clone.answers?.failed) clone.answers.failed.row = clone.answers?.accepted?.row ?? null;
      return clone;
    // Parking that never asks for the sleep the setting turns on.
    case 'park-never-sleeps':
      if (clone.request) clone.request.thenSleep = false;
      return clone;
    default:
      return clone;
  }
}

/** The dialog half's mutations. */
function mutateModal(name: string | undefined, entry: Json): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  switch (name) {
    // An absent note seeded as a missing key rather than as an empty string, which opens the note
    // dialog with nothing in it and overwrites the real note on confirm.
    case 'seed-an-absent-note-as-missing':
      if (clone.action?.open?.initialNote === '') delete clone.action.open.initialNote;
      return clone;
    // The agent icon dropped from the rename dialog.
    case 'drop-the-rename-icon':
      if (clone.action?.open?.modal === 'renameSession') delete clone.action.open.sessionAgentIcon;
      return clone;
    default:
      return clone;
  }
}

/**
 * The title rule's mutations, which act on the direct probe rather than on a drawn row: the
 * recording has no padded or blank title, so the same mutation applied to a row changed nothing
 * and the gate could not fail.
 */
function mutateTitleRule(name: string | undefined, entry: Json): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  switch (name) {
    // The seed taken untrimmed, so a padded title reaches the dialog with its padding.
    case 'seed-the-untrimmed-title':
      clone.title = String(entry.primaryTitle ?? entry.terminalTitle ?? entry.alias ?? '');
      return clone;
    // A blank title accepted instead of falling through to the next candidate.
    case 'accept-a-blank-title':
      if (String(clone.title) === String(entry.alias ?? '') && entry.primaryTitle !== null)
        clone.title = String(entry.primaryTitle ?? '');
      return clone;
    // The alias trimmed too, which the chain does not do.
    case 'trim-the-alias':
      clone.title = String(clone.title).trim();
      return clone;
    default:
      return clone;
  }
}

/** The bulk half's mutations. */
function mutateBulk(name: string | undefined, entry: Json): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  const messages = (clone.request?.messages ?? []) as Json[];
  switch (name) {
    // Every fan-out paced, which would make a project Wake of fifty rows take twenty seconds.
    case 'pace-everything':
      if (clone.request) clone.request.intervalMs = 350;
      return clone;
    // Nothing paced, which is the bulk sleep hammering the daemon that the 2026-06-27 pacing
    // decision exists to prevent.
    case 'pace-nothing':
      if (clone.request) clone.request.intervalMs = 0;
      return clone;
    // The set sent in the store's own by-id order rather than the daemon's array order. Invisible
    // for a concurrent fan-out and visible for a paced one, which is the order rows fall asleep in.
    case 'reorder-the-bulk-set':
      if (clone.request) clone.request.messages = [...messages].reverse();
      return clone;
    // A stopped row that is pinned or tagged swept into Sleep Inactive, which promotes it back
    // into the active shelf.
    case 'sleep-inactive-takes-stopped-rows':
      if (String((clone.payload as Json)?.type) === 'sleepInactiveProjectSessions' && clone.request)
        clone.request.messages = [
          ...messages,
          { sessionId: 'combined-session:extra:row', sleeping: true, type: 'setSessionSleeping' },
        ];
      return clone;
    // A group sleep that also asks the already-sleeping rows to sleep.
    case 'group-sleep-ignores-lifecycle':
      if (String((clone.payload as Json)?.type) === 'setGroupSleeping' && clone.request)
        clone.request.messages = [
          ...messages,
          { sessionId: 'combined-session:extra:row', sleeping: true, type: 'setSessionSleeping' },
        ];
      return clone;
    // The project wake that forgets to move the active project first.
    case 'drop-the-project-focus':
      if (clone.request) clone.request.focusProject = null;
      return clone;
    default:
      return clone;
  }
}

function mutateBatch(name: string | undefined, entry: Json): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  switch (name) {
    // The multi-selection left in place, so the rows stay selected under an action that consumed
    // them.
    case 'batch-keeps-the-selection':
      if (clone.plan) clone.plan.clearSelection = false;
      return clone;
    // The messages reordered, which for a batch that pairs a tag with a park changes which one the
    // daemon sees first.
    case 'reorder-the-batch':
      if (clone.plan) clone.plan.messages = [...((clone.plan.messages ?? []) as Json[])].reverse();
      return clone;
    default:
      return clone;
  }
}

const BULK_MUTATIONS = [
  'pace-everything',
  'pace-nothing',
  'reorder-the-bulk-set',
  'sleep-inactive-takes-stopped-rows',
  'group-sleep-ignores-lifecycle',
  'drop-the-project-focus',
  'batch-keeps-the-selection',
  'reorder-the-batch',
];

/**
 * The snooze half's mutations.
 *
 * The wake-time ones recompute the instant with the rule deliberately got wrong, rather than
 * nudging the string: the answer is a calendar calculation and a mutation that only shifted the
 * text would be caught by arithmetic nobody ships.
 */
function mutateSnoozeClock(name: string | undefined, entry: Json, clockCase: Json | undefined): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  if (!clockCase || !clone.wake) return clone;
  const dayMs = 86_400_000;
  const nowMs = Number(clockCase.nowMs);
  const offsetMs = Number(clockCase.offsetMs);
  const offsets = (clockCase.morningOffsetsMs ?? []) as number[];
  const localDay = Math.floor((nowMs + offsetMs) / dayMs);
  const weekday = ((localDay + 4) % 7 + 7) % 7;
  const daysFor = (preset: string): number => (preset === 'tomorrow' ? 1 : (8 - weekday) % 7 || 7);
  const morning = (days: number, offset: number, hourMs: number): string =>
    new Date((localDay + days) * dayMs + hourMs - offset).toISOString();
  switch (name) {
    // Today's offset used for a target on the other side of a daylight-saving change, which is the
    // near-miss this port takes the target's offset as data to avoid.
    case 'snooze-uses-todays-offset':
      for (const preset of ['tomorrow', 'nextWeek'])
        clone.wake[preset] = morning(daysFor(preset), offsetMs, 9 * 3_600_000);
      return clone;
    // The wake hour lost.
    case 'snooze-wakes-at-midnight':
      for (const preset of ['tomorrow', 'nextWeek'])
        clone.wake[preset] = morning(daysFor(preset), offsets[daysFor(preset)] ?? offsetMs, 0);
      return clone;
    // "Next week" as a flat seven days instead of the next Monday.
    case 'next-week-is-seven-days':
      clone.wake.nextWeek = morning(7, offsets[7] ?? offsetMs, 9 * 3_600_000);
      return clone;
    // A duration preset off by an order of magnitude, which is the leg with no calendar in it.
    case 'one-hour-is-a-minute':
      clone.wake.oneHour = new Date(nowMs + 60_000).toISOString();
      return clone;
    default:
      return clone;
  }
}

function mutateSnoozeBoundary(name: string | undefined, entry: Json): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  switch (name) {
    // `>=` instead of `>`: a session counts as snoozed at the exact millisecond it wakes.
    case 'unsnooze-a-tick-early':
      if (clone.case === 'exactly') {
        clone.isSnoozed = true;
        clone.section = 'snoozed';
      }
      return clone;
    // The predicate and the drawing disagreeing by a tick, which is the failure this probe exists
    // for and which no comparison of the predicate alone would see.
    case 'draw-and-decide-disagree':
      if (clone.case === 'aTickAfter') clone.section = 'sessions';
      return clone;
    default:
      return clone;
  }
}

function mutateSnoozeAction(name: string | undefined, entry: Json): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  const messages = (clone.messages ?? []) as Json[];
  switch (name) {
    // The tag the submenu row carries dropped, so "Snooze with this tag" only snoozes.
    case 'snooze-skips-the-tag':
      clone.messages = messages.filter((message) => message.type !== 'setSessionTag');
      return clone;
    // The tag posted after the snooze, which the daemon would apply to an already sleeping row.
    case 'snooze-tags-after-the-call':
      clone.messages = [...messages].reverse();
      return clone;
    // A command carrying no tag at all answered as an explicit clear, which is the absent-versus-
    // null class again and would strip the tag off every plain preset row.
    case 'snooze-clears-an-absent-tag':
      if (messages.length && !messages.some((message) => message.type === 'setSessionTag'))
        clone.messages = [
          { type: 'setSessionTag', sessionId: (clone.payload as Json)?.sessionId, sessionTag: null },
          ...messages,
        ];
      return clone;
    default:
      return clone;
  }
}

function mutateSnoozeCall(name: string | undefined, entry: Json): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  switch (name) {
    // An accepted snooze that never puts the session to sleep, which is the user decision of
    // 2026-09-12 ("a snoozed session is always asleep") quietly undone.
    case 'snooze-without-the-sleep':
      if (clone.answers?.accepted)
        clone.answers.accepted = (clone.answers.accepted as Json[]).filter((follow) => follow.follow !== 'sleep');
      return clone;
    // The sleep run for a snooze the daemon refused, which is the same decision's other half: a
    // wake time in the past leaves the session running.
    case 'snooze-sleeps-after-a-refusal':
      if (clone.answers?.failed)
        clone.answers.failed = [...((clone.answers.failed ?? []) as Json[]), { follow: 'sleep', session: '' }];
      return clone;
    // The refusal that says nothing at all, which is a row that silently never moves.
    case 'drop-the-snooze-toast':
      for (const key of ['accepted', 'failed'])
        if (clone.answers?.[key])
          clone.answers[key] = (clone.answers[key] as Json[]).filter((follow) => follow.follow !== 'toast');
      return clone;
    // The wake time missing from the call.
    case 'snooze-without-the-wake-time':
      if (clone.request?.rpc?.params) delete clone.request.rpc.params.snoozedUntil;
      return clone;
    // Unsnooze reported under the snooze title, which is the wrong sentence on the wrong failure.
    case 'unsnooze-titled-as-a-snooze':
      for (const key of ['accepted', 'failed'])
        for (const follow of (clone.answers?.[key] ?? []) as Json[])
          if (follow.follow === 'toast' && follow.title === 'Wake failed') follow.title = 'Snooze failed';
      return clone;
    default:
      return clone;
  }
}

const SNOOZE_MUTATIONS = [
  'snooze-uses-todays-offset',
  'snooze-wakes-at-midnight',
  'next-week-is-seven-days',
  'one-hour-is-a-minute',
  'unsnooze-a-tick-early',
  'draw-and-decide-disagree',
  'snooze-skips-the-tag',
  'snooze-tags-after-the-call',
  'snooze-clears-an-absent-tag',
  'snooze-without-the-sleep',
  'snooze-sleeps-after-a-refusal',
  'drop-the-snooze-toast',
  'snooze-without-the-wake-time',
  'unsnooze-titled-as-a-snooze',
];

const MODAL_MUTATIONS = [
  'seed-the-untrimmed-title',
  'accept-a-blank-title',
  'trim-the-alias',
  'seed-an-absent-note-as-missing',
  'drop-the-rename-icon',
];

const FLAGS_MUTATIONS = [
  'clear-a-tag-as-null',
  'tag-without-the-star',
  'patch-a-failed-flag-call',
  'park-never-sleeps',
];

const FORK_MUTATIONS = ['place-a-pane-with-no-session', 'forget-the-placement-target', 'always-activate'];

const CLOSE_MUTATIONS = [
  'never-restore-the-row',
  'close-is-not-optimistic',
  'restore-after-an-accepted-close',
  'drop-the-close-focus',
  'keep-the-hide-after-removal',
];

const LIFECYCLE_MUTATIONS = [
  'patch-a-declined-sleep',
  'overlay-outlives-the-daemon',
  'drop-the-replacement-focus',
  'wake-steals-the-focus',
  'swap-sleep-and-wake',
  'drop-the-rpc-reason',
];

function mutateLifecycle(name: string | undefined, entry: Json): Json {
  const clone = JSON.parse(JSON.stringify(entry)) as Json;
  switch (name) {
    // The optimistic value applied before the daemon agreed, which is the 2026-08-19 KeepAwake
    // bug: a declined sleep publishing a row state the daemon never entered.
    case 'patch-a-declined-sleep':
      if (clone.answers?.declined) clone.answers.declined.state = clone.answers?.accepted?.state ?? null;
      return clone;
    // The overlay outliving a daemon row that moved somewhere else, which is the user-visible
    // "I put it to sleep and it came back" in reverse: the row says asleep for ever.
    case 'overlay-outlives-the-daemon':
      if (clone.echo) clone.echo.movedOn = clone.answers?.accepted?.state ?? null;
      return clone;
    // The sleep handing the focus to nobody.
    case 'drop-the-replacement-focus':
      if (clone.answers?.accepted) clone.answers.accepted.focus = [];
      return clone;
    // A wake taking the focus back after the user moved on, which is `movesDuringCall` and
    // nothing else: with the focus unchanged across the round trip the branch cannot be reached,
    // and this mutation found no difference at all until that case was added.
    case 'wake-steals-the-focus':
      if (clone.sleeping === false && clone.focus === 'movesDuringCall' && clone.answers?.accepted)
        clone.answers.accepted.focus = [{ follow: 'focus', session: clone.sessionId }];
      return clone;
    // The two calls swapped.
    case 'swap-sleep-and-wake':
      if (clone.request?.rpc?.path)
        clone.request.rpc.path =
          clone.request.rpc.path === '/api/sleepSession' ? '/api/wakeSession' : '/api/sleepSession';
      return clone;
    // The reason the daemon logs the call under.
    case 'drop-the-rpc-reason':
      if (clone.request?.rpc?.params) delete clone.request.rpc.params.reason;
      return clone;
    default:
      return clone;
  }
}

const LOCAL_ACTION_BY_TYPE: Record<string, string> = {
  copyWorkspaceProjectPathForGroup: 'copyWorkspaceProjectPath',
  openWorkspaceProjectInFinderForGroup: 'openWorkspaceProjectInFinder',
  openWorkspaceProjectInIdeForGroup: 'openWorkspaceProjectInIde',
};

async function compare([outDir, ...flags]: string[]) {
  if (!outDir) throw new Error('compare <out-dir> [--inject <mutation>]');
  pinTimeZone();
  const injectAt = flags.indexOf('--inject');
  const mutationName = injectAt >= 0 ? flags[injectAt + 1] : undefined;
  const mutate = mutationName ? (MUTATIONS[mutationName] ?? ((calls: Json[]) => calls)) : undefined;
  const known = [
    ...Object.keys(MUTATIONS),
    ...LIFECYCLE_MUTATIONS,
    ...CLOSE_MUTATIONS,
    ...FORK_MUTATIONS,
    ...FLAGS_MUTATIONS,
    ...MODAL_MUTATIONS,
    ...SNOOZE_MUTATIONS,
    ...BULK_MUTATIONS,
  ];
  if (mutationName && !known.includes(mutationName)) {
    console.error(`unknown mutation ${mutationName}; one of ${known.join(', ')}`);
    process.exit(2);
  }
  const { runTypeScriptActions } = await import('./action-parity-typescript.ts');
  const {
    runTypeScriptLifecycle,
    runTypeScriptClose,
    runTypeScriptFork,
    runTypeScriptFlags,
    runTypeScriptModals,
    runTypeScriptTitleRule,
    runTypeScriptSnoozeClock,
    runTypeScriptSnoozeBoundary,
    runTypeScriptSnoozeActions,
    runTypeScriptSnoozeCalls,
    runTypeScriptBulk,
    runTypeScriptBulkPacing,
  } = await import('./lifecycle-parity-typescript.ts');
  // The instants the snooze half was answered against, by case name, so a mutation can recompute
  // a wake time with the rule got wrong.
  const clockCases = new Map<string, Json>();
  try {
    const file = JSON.parse(readFileSync(join(outDir, SNOOZE_CLOCK_FILE), 'utf8')) as Json;
    for (const entry of (file.cases ?? []) as Json[]) clockCases.set(String(entry.case), entry);
  } catch {
    console.error(`no ${SNOOZE_CLOCK_FILE} in ${outDir}: run \`bun tooling/gx-core/action-parity.ts snooze-clock ${outDir}\` and re-run the example`);
    process.exit(2);
  }
  const names = readdirSync(outDir)
    .filter((name) => name.startsWith('scenario-') && name.endsWith('.json'))
    .sort();
  let payloads = 0;
  let rustCalls = 0;
  let tsCalls = 0;
  let transitions = 0;
  let closes = 0;
  let forks = 0;
  let flagCalls = 0;
  let modalOpens = 0;
  let modalRefusals = 0;
  let titleCases = 0;
  let overlayKept = 0;
  let closesRestored = 0;
  let stoppedUnhidden = 0;
  let snoozeWakes = 0;
  let snoozeBoundaries = 0;
  let snoozeActions = 0;
  let snoozeCalls = 0;
  let snoozeRefusals = 0;
  let bulkSets = 0;
  let bulkRefusals = 0;
  let bulkMessages = 0;
  let batchPlans = 0;
  const differences: string[] = [];
  // The pacing, measured once through the shipped helper with its real timer rather than per
  // scenario: at 350 ms a row it is the only probe here whose cost is wall-clock time.
  const pacing = await runTypeScriptBulkPacing();
  for (const measured of pacing) {
    const floor = Number(measured.intervalMs) * (Number(measured.rows) - 1);
    const paced = Number(measured.elapsedMs) >= floor;
    if (measured.sleeping === true && !paced)
      differences.push(`pacing: a bulk sleep of ${measured.rows} rows finished in ${measured.elapsedMs} ms, under the ${floor} ms a paced fan-out cannot beat`);
    if (measured.sleeping === false && Number(measured.elapsedMs) >= Number(measured.intervalMs))
      differences.push(`pacing: a bulk wake of ${measured.rows} rows took ${measured.elapsedMs} ms, which is a paced fan-out where the shipped code runs them together`);
  }
  for (const name of names) {
    const rustPath = join(outDir, name.replace('scenario-', 'rust-actions-'));
    let rust: Json;
    try {
      rust = JSON.parse(readFileSync(rustPath, 'utf8')) as Json;
    } catch {
      differences.push(`${name}: no rust-actions dump beside it; run the sidebar_action_parity example`);
      continue;
    }
    const scenario = JSON.parse(readFileSync(join(outDir, name), 'utf8')) as Json;
    const ours = await runTypeScriptActions(scenario, rust);
    writeFileSync(join(outDir, name.replace('scenario-', 'ts-actions-')), JSON.stringify(ours), { mode: 0o600 });
    const entries = rust.entries as Json[];
    if (entries.length !== ours.length) {
      differences.push(`${name}: ${entries.length} payloads against ${ours.length} answers`);
      continue;
    }
    // The rule the recording cannot reach, driven directly on both sides.
    const theirTitles = runTypeScriptTitleRule(rust);
    for (const [index, entry] of ((rust.titleRule ?? []) as Json[]).entries()) {
      titleCases += 1;
      const mine = mutate ? mutateTitleRule(mutationName, entry) : entry;
      if (String(mine.title) !== String(theirTitles[index]))
        differences.push(
          `${name} titleRule #${index}: rust ${JSON.stringify(mine.title)} ts ${JSON.stringify(theirTitles[index])}`
        );
    }
    // The plural payloads: which rows, in which order, through which per-session action.
    const theirBulk = await runTypeScriptBulk(scenario, rust);
    for (const [index, entry] of ((rust.bulk ?? []) as Json[]).entries()) {
      const theirs = theirBulk[index];
      const payload = (entry.payload ?? {}) as Json;
      const where = `${name} bulk #${index} ${String(payload.type)}`;
      if (entry.owned !== true) {
        bulkRefusals += 1;
        continue;
      }
      bulkSets += 1;
      const mine = mutate ? mutateBulk(mutationName, entry) : entry;
      const myCalls = ((mine.request?.messages ?? []) as Json[]).map((message) => ({
        call:
          message.type === 'closeSession' ? 'close' : message.sleeping === true ? 'sleep' : 'wake',
        session: message.sessionId,
      }));
      bulkMessages += myCalls.length;
      if (canonical(myCalls) !== canonical(theirs?.calls ?? []))
        differences.push(`${where}: rust ${canonical(myCalls)} ts ${canonical(theirs?.calls ?? [])}`);
      const myFocus = (mine.request?.focusProject ?? null) as string | null;
      if (canonical(myFocus) !== canonical(theirs?.focusProject ?? null))
        differences.push(`${where} focusProject: rust ${canonical(myFocus)} ts ${canonical(theirs?.focusProject ?? null)}`);
      // The interval is compared against the shipped CALL GRAPH, which the pacing probe above
      // measured through the real helper. The first cut of this rule read only
      // `setSessionsSleeping` and reported 364 differences that were the harness being wrong:
      // `setGroupSleeping(true)` and `sleepInactiveProjectSessions` both delegate to
      // `setSessionsSleeping(ids, true)` and are paced with it, while every wake and both closes
      // go through `Promise.all`.
      const expected = pacedPayload(payload) ? Number(pacing[0]?.intervalMs ?? 0) : 0;
      if (Number(mine.request?.intervalMs ?? -1) !== expected)
        differences.push(`${where} intervalMs: rust ${mine.request?.intervalMs} expected ${expected}`);
    }
    // The renderer's batch envelope, which is a pass-through of the two lines controller.ts runs.
    for (const [index, entry] of ((rust.batch ?? []) as Json[]).entries()) {
      if (entry.owned !== true) continue;
      batchPlans += 1;
      const mine = mutate ? mutateBatch(mutationName, entry) : entry;
      const command = (entry.command ?? {}) as Json;
      const expected = {
        clearSelection: command.clearSelection === true,
        messages: (command.messages ?? []) as Json[],
      };
      if (canonical(mine.plan) !== canonical(expected))
        differences.push(`${name} batch #${index}: rust ${canonical(mine.plan)} controller ${canonical(expected)}`);
    }
    // The wake-time rule, at the fixed instants the clock file names.
    const theirClock = runTypeScriptSnoozeClock(rust);
    for (const [index, entry] of ((rust.snoozeClock ?? []) as Json[]).entries()) {
      snoozeWakes += 1;
      const clockCase = clockCases.get(String(entry.case));
      const mine = mutate ? mutateSnoozeClock(mutationName, entry, clockCase) : entry;
      const theirs = theirClock[index];
      for (const preset of ['oneHour', 'threeHours', 'tomorrow', 'nextWeek', 'notAPreset']) {
        const left = mine.wake?.[preset] ?? null;
        const right = theirs?.wake?.[preset] ?? null;
        if (canonical(left) !== canonical(right))
          differences.push(
            `${name} snoozeClock ${String(entry.case)} ${preset}: rust ${canonical(left)} ts ${canonical(right)}`
          );
      }
    }
    // The moment a snooze ends, asked of the predicate AND of the drawn section.
    const theirBoundary = runTypeScriptSnoozeBoundary(rust);
    for (const [index, entry] of ((rust.snoozeBoundary ?? []) as Json[]).entries()) {
      snoozeBoundaries += 1;
      const mine = mutate ? mutateSnoozeBoundary(mutationName, entry) : entry;
      const theirs = theirBoundary[index];
      const where = `${name} snoozeBoundary ${String(entry.case)}`;
      if (mine.isSnoozed !== theirs?.isSnoozed)
        differences.push(`${where} isSnoozed: rust ${String(mine.isSnoozed)} ts ${String(theirs?.isSnoozed)}`);
      if (canonical(mine.parsedMs ?? null) !== canonical(theirs?.parsedMs ?? null))
        differences.push(
          `${where} parsedMs: rust ${canonical(mine.parsedMs ?? null)} ts ${canonical(theirs?.parsedMs ?? null)}`
        );
      // The half a comparison of the predicate alone cannot see: the row is drawn under the
      // Snoozed heading exactly when the predicate says it is snoozed.
      if ((mine.section === 'snoozed') !== theirs?.isSnoozed)
        differences.push(
          `${where} section: rust ${String(mine.section)} ts isSnoozed ${String(theirs?.isSnoozed)}`
        );
    }
    // The menu row: what it posts.
    const theirSnoozeActions = await runTypeScriptSnoozeActions(rust);
    for (const [index, entry] of ((rust.snoozeActions ?? []) as Json[]).entries()) {
      snoozeActions += 1;
      const mine = mutate ? mutateSnoozeAction(mutationName, entry) : entry;
      const theirs = theirSnoozeActions[index];
      const where = `${name} snoozeAction #${index} ${String((entry.payload as Json)?.preset ?? 'noPreset')} drawn=${String(entry.drawn)}`;
      // A row the list does not draw stops the action before the switch on both sides, so the
      // comparable thing is always the posted list and a refusal is an empty one.
      const left = (mine.owned === true ? (mine.messages ?? []) : []) as Json[];
      if (canonical(left) !== canonical(theirs?.posts ?? []))
        differences.push(`${where}: rust ${canonical(left)} ts ${canonical(theirs?.posts ?? [])}`);
    }
    // The two calls, and what an accepted and a refused answer do.
    const theirSnoozeCalls = await runTypeScriptSnoozeCalls(scenario, rust);
    for (const [index, entry] of ((rust.snoozeCalls ?? []) as Json[]).entries()) {
      const theirs = theirSnoozeCalls[index];
      const payload = (entry.payload ?? {}) as Json;
      const where = `${name} snoozeCall #${index} ${String(payload.type)}`;
      if (entry.owned !== true) {
        snoozeRefusals += 1;
        // A refusal is a HAND-OFF and is counted rather than compared, but the two shapes that
        // have nothing to hand off to must reach nothing at all: a browser row is an app tab with
        // no daemon session, and an unparseable id is the TypeScript's own early return.
        const sessionId = String(payload.sessionId ?? '');
        if (!sessionId.startsWith('remote:') && (theirs?.rpc || theirs?.remote))
          differences.push(`${where}: refused here, but the TypeScript still calls ${canonical(theirs?.rpc ?? 'remote')}`);
        if (sessionId.startsWith('remote:') && !theirs?.remote)
          differences.push(`${where}: refused as a remote hand-off, but the TypeScript made no remote call`);
        continue;
      }
      snoozeCalls += 1;
      const mine = mutate ? mutateSnoozeCall(mutationName, entry) : entry;
      if (canonical(mine.request?.rpc ?? null) !== canonical(theirs?.rpc ?? null))
        differences.push(`${where} rpc: rust ${canonical(mine.request?.rpc ?? null)} ts ${canonical(theirs?.rpc ?? null)}`);
      for (const answer of ['accepted', 'failed']) {
        // The sleep is compared by its presence, not by its session id: the TypeScript hands
        // `setSessionSleeping` the sidebar id it was given and the port hands it the key it
        // parsed, which are the same row written two ways.
        const left = summarizeSnoozeFollowUps((mine.answers?.[answer] ?? []) as Json[]);
        const right = summarizeSnoozeFollowUps((theirs?.answers?.[answer] ?? []) as Json[]);
        if (canonical(left) !== canonical(right))
          differences.push(`${where} ${answer}: rust ${canonical(left)} ts ${canonical(right)}`);
      }
    }
    const theirModals = await runTypeScriptModals(scenario, rust);
    for (const [index, entry] of ((rust.modals ?? []) as Json[]).entries()) {
      const theirs = theirModals[index];
      const where = `${name} modal #${index} ${String((entry.payload as Json)?.action)}`;
      const theirCalls = ((theirs?.calls ?? []) as Json[]).map((call) => call);
      // A refusal is a HAND-OFF, not a loss: the host forwards the command and the old runtime
      // opens the dialog, so the TypeScript making a call there is the refusal working. What is
      // worth asserting is the one refusal that should reach nothing at all: `firstMessage` is
      // gated on `firstUserMessage`, which this client never has, so a call for it would mean the
      // item is reachable after all and declared difference 14 is wrong.
      if (entry.owned !== true) {
        modalRefusals += 1;
        if (String((entry.payload as Json)?.action) === 'firstMessage' && theirCalls.length)
          differences.push(
            `${where}: firstMessage is supposed to be unreachable, ts makes ${theirCalls.length} call(s)`
          );
        continue;
      }
      modalOpens += 1;
      const mine = mutate ? mutateModal(mutationName, entry) : entry;
      const mineCalls = [{ call: 'close' }, { call: 'open', open: mine.action?.open }];
      if (canonical(mineCalls) !== canonical(theirCalls))
        differences.push(`${where}: rust ${canonical(mineCalls)} ts ${canonical(theirCalls)}`);
    }
    const theirFlags = await runTypeScriptFlags(scenario, rust);
    for (const [index, entry] of ((rust.flags ?? []) as Json[]).entries()) {
      if (entry.owned !== true) continue;
      flagCalls += 1;
      const theirs = theirFlags[index];
      if (!theirs) {
        differences.push(`${name} flags #${index}: the TypeScript side produced no answer`);
        continue;
      }
      const where = `${name} flags #${index} ${String((entry.payload as Json)?.type)} sleepWhenParking=${entry.sleepWhenParking}`;
      const mine = mutate ? mutateFlags(mutationName, entry) : entry;
      if (canonical(mine.request?.rpc ?? null) !== canonical(theirs.rpc))
        differences.push(`${where} rpc: rust ${canonical(mine.request?.rpc ?? null)} ts ${canonical(theirs.rpc)}`);
      if ((mine.request?.thenSleep ?? false) !== (theirs.thenSleep ?? false))
        differences.push(`${where} thenSleep: rust ${mine.request?.thenSleep} ts ${theirs.thenSleep}`);
      for (const answer of ['accepted', 'failed']) {
        const left = mine.answers?.[answer]?.row ?? null;
        const right = theirs.answers?.[answer]?.row ?? null;
        if (canonical(left) !== canonical(right))
          differences.push(`${where} ${answer} row: rust ${canonical(left)} ts ${canonical(right)}`);
      }
    }
    const theirFork = await runTypeScriptFork(scenario, rust);
    for (const [index, entry] of ((rust.fork ?? []) as Json[]).entries()) {
      if (entry.owned !== true) continue;
      forks += 1;
      const theirs = theirFork[index];
      if (!theirs) {
        differences.push(`${name} fork #${index}: the TypeScript side produced no answer`);
        continue;
      }
      const where = `${name} fork #${index} active=${entry.active}`;
      const mine = mutate ? mutateFork(mutationName, entry) : entry;
      if (canonical(mine.request?.rpc ?? null) !== canonical(theirs.rpc))
        differences.push(`${where} rpc: rust ${canonical(mine.request?.rpc ?? null)} ts ${canonical(theirs.rpc)}`);
      if (canonical(mine.request?.activate ?? null) !== canonical(theirs.activate))
        differences.push(
          `${where} activate: rust ${canonical(mine.request?.activate ?? null)} ts ${canonical(theirs.activate)}`
        );
      for (const answer of ['accepted', 'emptyFork', 'failed', 'neverAnswered']) {
        const left = mine.answers?.[answer] ?? null;
        const right = theirs.answers?.[answer] ?? null;
        if (canonical(left) !== canonical(right))
          differences.push(`${where} ${answer}: rust ${canonical(left)} ts ${canonical(right)}`);
      }
    }
    const theirClose = await runTypeScriptClose(scenario, rust);
    for (const [index, entry] of ((rust.close ?? []) as Json[]).entries()) {
      if (entry.owned !== true) continue;
      closes += 1;
      const theirs = theirClose[index];
      if (!theirs) {
        differences.push(`${name} close #${index}: the TypeScript side produced no answer`);
        continue;
      }
      const where = `${name} close #${index} focus=${entry.focus}`;
      const mine = mutate ? mutateClose(mutationName, entry) : entry;
      const myRpc = mine.request?.rpc ?? null;
      if (canonical(myRpc) !== canonical(theirs.rpc))
        differences.push(`${where} rpc: rust ${canonical(myRpc)} ts ${canonical(theirs.rpc)}`);
      if (canonical(mine.optimistic) !== canonical(theirs.optimistic))
        differences.push(`${where} optimistic: rust ${canonical(mine.optimistic)} ts ${canonical(theirs.optimistic)}`);
      for (const answer of ['accepted', 'failed', 'neverAnswered']) {
        const left = mine.answers?.[answer] ?? null;
        const right = theirs.answers?.[answer] ?? null;
        if (canonical(left) === canonical(right)) continue;
        // Declared difference 25: a close the daemon never confirmed puts the row back here and
        // leaves it missing there. Allowed in exactly that shape and for those two answers only.
        if ((answer === 'failed' || answer === 'neverAnswered') && left?.drawn === true && right?.drawn === false) {
          closesRestored += 1;
          continue;
        }
        differences.push(`${where} ${answer}: rust ${canonical(left)} ts ${canonical(right)}`);
      }
      for (const key of ['removed', 'stillRunning', 'stopped']) {
        const left = mine.echo?.[key] ?? null;
        const right = theirs.echo?.[key] ?? null;
        if (canonical(left) === canonical(right)) continue;
        // Declared difference 26: a daemon that still lists the row as STOPPED retires the local
        // hide here, because a stopped row the daemon keeps is one the user pinned, starred or
        // tagged and must be able to see. The TypeScript's hidden set is never cleared at all.
        if (key === 'stopped' && left?.drawn === true && right?.drawn === false) {
          stoppedUnhidden += 1;
          continue;
        }
        differences.push(`${where} echo.${key}: rust ${canonical(left)} ts ${canonical(right)}`);
      }
    }
    const theirLifecycle = await runTypeScriptLifecycle(scenario, rust);
    for (const [index, entry] of ((rust.lifecycle ?? []) as Json[]).entries()) {
      if (entry.owned !== true) continue;
      transitions += 1;
      const theirs = theirLifecycle[index];
      if (!theirs) {
        differences.push(`${name} lifecycle #${index}: the TypeScript side produced no answer`);
        continue;
      }
      const where = `${name} lifecycle #${index} ${entry.sleeping ? 'sleep' : 'wake'} focus=${entry.focus}`;
      const mine = mutate ? mutateLifecycle(mutationName, entry) : entry;
      const myRpc = mine.request?.rpc?.path ? mine.request.rpc : null;
      if (canonical(myRpc) !== canonical(theirs.rpc))
        differences.push(`${where} rpc: rust ${canonical(myRpc)} ts ${canonical(theirs.rpc)}`);
      for (const answer of ['accepted', 'declined', 'failed']) {
        const left = mine.answers?.[answer] ?? null;
        const right = theirs.answers?.[answer] ?? null;
        if (canonical(left) !== canonical(right))
          differences.push(`${where} ${answer}: rust ${canonical(left)} ts ${canonical(right)}`);
      }
      for (const key of ['agrees', 'stillOld', 'movedOn']) {
        const left = mine.echo?.[key] ?? null;
        const right = theirs.echo?.[key] ?? null;
        if (left === right) continue;
        // The one difference this port makes on purpose: the store's overlay records the value it
        // predicted FROM, so a daemon row that merely REPEATS that value leaves the prediction in
        // place. The TypeScript wrote the optimistic value into its copy of the row, so the same
        // repeat overwrites it and the row flickers back for as long as the real transition takes.
        // Allowed only in exactly that shape, and never for the other two echoes.
        if (key === 'stillOld' && left === (mine.answers?.accepted?.state ?? null)) {
          overlayKept += 1;
          continue;
        }
        differences.push(`${where} echo.${key}: rust ${String(left)} ts ${String(right)}`);
      }
    }
    for (const [index, entry] of entries.entries()) {
      payloads += 1;
      const mine = mutate ? mutate(entry.calls as Json[], entry, rust) : (entry.calls as Json[]);
      const theirs = ours[index]!;
      rustCalls += mine.length;
      tsCalls += theirs.length;
      const left = canonical(mine);
      const right = canonical(theirs);
      if (left !== right)
        differences.push(
          `${name} #${index} ${describe(entry.payload as Json)} [${entry.variant}]: rust ${left} ts ${right}`
        );
    }
  }
  console.log(
    `scenarios ${names.length} payloads ${payloads} rustCalls ${rustCalls} tsCalls ${tsCalls} transitions ${transitions} closes ${closes} forks ${forks} flagCalls ${flagCalls} modalOpens ${modalOpens} modalRefusals ${modalRefusals} titleCases ${titleCases} snoozeWakes ${snoozeWakes} snoozeBoundaries ${snoozeBoundaries} snoozeActions ${snoozeActions} snoozeCalls ${snoozeCalls} snoozeRefusals ${snoozeRefusals} bulkSets ${bulkSets} bulkMessages ${bulkMessages} bulkRefusals ${bulkRefusals} batchPlans ${batchPlans} overlayKept ${overlayKept} closesRestored ${closesRestored} stoppedUnhidden ${stoppedUnhidden} differences ${differences.length}${
      mutationName ? ` (injected ${mutationName})` : ''
    }`
  );
  for (const difference of differences.slice(0, 40)) console.log(`  ${difference}`);
  if (differences.length > 40) console.log(`  … and ${differences.length - 40} more`);
  // Every counter that has to be non-zero for the run to have measured anything. A classification
  // counts a difference this port makes deliberately and on every row of a real recording; a
  // coverage counter counts a probe that exists at all, and a zero there is the clock file missing
  // or a probe list that silently came back empty.
  const measured: [string, number][] = [
    ['overlayKept', overlayKept],
    ['closesRestored', closesRestored],
    ['stoppedUnhidden', stoppedUnhidden],
    ['snoozeWakes', snoozeWakes],
    ['snoozeBoundaries', snoozeBoundaries],
    ['snoozeActions', snoozeActions],
    ['snoozeCalls', snoozeCalls],
    ['snoozeRefusals', snoozeRefusals],
    ['bulkSets', bulkSets],
    ['bulkMessages', bulkMessages],
    ['bulkRefusals', bulkRefusals],
    ['batchPlans', batchPlans],
  ];
  const collapsed = measured.filter(([, count]) => count === 0);
  // With a mutation injected the gate is being tested. A mutation passes when it either creates a
  // difference OR collapses one of the counters above: undoing a difference this port makes on
  // purpose (`never-restore-the-row` is exactly that) makes the two sides AGREE, and a criterion
  // that only looked at the difference count would read that as the gate failing to notice.
  if (mutate) {
    const noticed = differences.length > 0 || collapsed.length > 0;
    process.exitCode = noticed ? 0 : 1;
    if (!noticed)
      console.log(`  the injected mutation ${mutationName} produced NO difference and collapsed no counter`);
    return;
  }
  // A clean run with a zero among them is a gate that stopped measuring.
  if (!differences.length && collapsed.length) {
    console.log(`  ${collapsed.map(([label]) => label).join(', ')} counted nothing, so the gate is not exercising what it claims`);
    process.exitCode = 1;
    return;
  }
  process.exitCode = differences.length ? 1 : 0;
}

/**
 * Whether the shipped code reaches `runGpuiSidebarBulkSleepPaced` for this payload: the three ways
 * into `setSessionsSleeping` with `sleeping === true`, and nothing else.
 */
function pacedPayload(payload: Json): boolean {
  const kind = String(payload.type);
  if (kind === 'sleepInactiveProjectSessions') return true;
  return (kind === 'setSessionsSleeping' || kind === 'setGroupSleeping') && payload.sleeping === true;
}

/**
 * A snooze answer's follow-ups, with the one field the two sides write differently left out: the
 * sleep carries a session id, and the TypeScript passes on the sidebar id it was handed while the
 * port passes the key it parsed, which are the same row spelled two ways. Everything that decides
 * what the user sees (whether a sleep happens at all, and the toast's level and title) is kept.
 */
function summarizeSnoozeFollowUps(followUps: Json[]): Json[] {
  return followUps.map((follow) =>
    follow.follow === 'sleep'
      ? { follow: 'sleep' }
      : { follow: follow.follow, level: follow.level, title: follow.title, hasDescription: follow.hasDescription }
  );
}

/**
 * A payload named without its content: a project path, a session title and a resume command line
 * all ride in these, and the difference lines are read and pasted into reports.
 */
function describe(payload: Json): string {
  const kind = String(payload.type ?? '?');
  if (typeof payload.groupId === 'string') return `${kind} group=${idShape(payload.groupId)}`;
  if (typeof payload.sessionId === 'string') return `${kind} session=${idShape(payload.sessionId)}`;
  return kind;
}

function idShape(id: string): string {
  if (!id) return 'empty';
  if (id.startsWith('remote:')) return 'remote';
  if (id.startsWith('combined-project:')) return 'project';
  if (id.startsWith('combined-session:')) return 'session';
  if (id.startsWith('gpui-wsg:')) return 'userGroup';
  return 'other';
}

/** Key order must not decide a difference, so both sides are written with sorted keys. */
function canonical(value: unknown): string {
  return JSON.stringify(sortKeys(value));
}

function sortKeys(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortKeys);
  if (value && typeof value === 'object')
    return Object.fromEntries(
      Object.entries(value as Json)
        .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
        .map(([key, item]) => [key, sortKeys(item)])
    );
  return value;
}

// Last, not first: `MUTATIONS` is a `const` and a top-level await above it runs in its temporal
// dead zone, so `--inject` would throw before it could inject anything.
const [mode, ...rest] = process.argv.slice(2);
if (mode === 'compare') await compare(rest);
else if (mode === 'snooze-clock') writeSnoozeClock(rest);
else {
  console.error('usage: action-parity.ts snooze-clock <out-dir> | compare <out-dir> [--inject <mutation>]');
  process.exit(2);
}
