import { describe, expect, test } from "vitest";
import { SIDEBAR_V2_LIFECYCLE_CAPABILITIES_DISABLED } from "./sidebar-v2-lifecycle";
import type { SidebarV2Session } from "./sidebar-v2-session";
import {
  createSidebarV2CreationRankMap,
  orderItemsByPreferredIds,
  partitionSidebarV2Sessions,
  reconcileSidebarV2CreationOrder,
  resolveAdjacentSidebarV2SessionId,
  resolveSidebarV2SettledTimestampMs,
  sortSessionsForSidebarV2,
  sortSettledSessionsForSidebarV2,
  sortSnoozedSessionsForSidebarV2,
} from "./sidebar-v2-sort";

const NOW_MS = Date.parse("2026-07-29T12:00:00.000Z");
const DAY_MS = 24 * 60 * 60 * 1_000;

function iso(offsetMs: number): string {
  return new Date(NOW_MS + offsetMs).toISOString();
}

function session(sessionId: string, overrides: Partial<SidebarV2Session> = {}): SidebarV2Session {
  return { activity: "idle", sessionId, ...overrides };
}

function ids(sessions: readonly SidebarV2Session[]): string[] {
  return sessions.map((entry) => entry.sessionId);
}

describe("sortSessionsForSidebarV2", () => {
  test("newest created first", () => {
    expect(
      ids(
        sortSessionsForSidebarV2([
          session("old", { createdAt: iso(-3 * DAY_MS) }),
          session("new", { createdAt: iso(-1_000) }),
          session("mid", { createdAt: iso(-DAY_MS) }),
        ]),
      ),
    ).toEqual(["new", "mid", "old"]);
  });

  test("activity never reorders the inbox", () => {
    const before = [
      session("a", { createdAt: iso(-3_000) }),
      session("b", { createdAt: iso(-2_000) }),
      session("c", { createdAt: iso(-1_000) }),
    ];
    const after = [
      session("a", { activity: "attention", createdAt: iso(-3_000) }),
      session("b", { activity: "working", createdAt: iso(-2_000) }),
      session("c", { createdAt: iso(-1_000), lifecycleState: "error" }),
    ];
    expect(ids(sortSessionsForSidebarV2(after))).toEqual(ids(sortSessionsForSidebarV2(before)));
  });

  test("pinned rows float above the rest and preserve their persisted input order", () => {
    expect(
      ids(
        sortSessionsForSidebarV2([
          session("new", { createdAt: iso(-1_000) }),
          session("pinned-old", { createdAt: iso(-5 * DAY_MS), isPinned: true }),
          session("pinned-new", { createdAt: iso(-2 * DAY_MS), isPinned: true }),
        ]),
      ),
    ).toEqual(["pinned-old", "pinned-new", "new"]);
  });

  test("equal creation stamps tie-break by session id, not by input order", () => {
    const sessions = [
      session("zulu", { createdAt: iso(0) }),
      session("alpha", { createdAt: iso(0) }),
    ];
    expect(ids(sortSessionsForSidebarV2(sessions))).toEqual(["alpha", "zulu"]);
    expect(ids(sortSessionsForSidebarV2([...sessions].reverse()))).toEqual(["alpha", "zulu"]);
  });

  test("sessions with no creation stamp sink below stamped ones and stay deterministic", () => {
    expect(
      ids(
        sortSessionsForSidebarV2([
          session("unknown-b"),
          session("stamped", { createdAt: iso(-DAY_MS) }),
          session("unknown-a"),
        ]),
      ),
    ).toEqual(["stamped", "unknown-a", "unknown-b"]);
  });

  test("an explicit creation rank replaces createdAt entirely", () => {
    const rank = createSidebarV2CreationRankMap(["c", "a", "b"]);
    expect(
      ids(
        sortSessionsForSidebarV2(
          [
            session("a", { createdAt: iso(0) }),
            session("b", { createdAt: iso(-1_000) }),
            session("c", { createdAt: iso(-99 * DAY_MS) }),
          ],
          { creationRankById: rank },
        ),
      ),
    ).toEqual(["c", "a", "b"]);
  });

  test("an id missing from the rank map sinks to the bottom without scrambling the rest", () => {
    expect(
      ids(
        sortSessionsForSidebarV2([session("a"), session("ghost"), session("b")], {
          creationRankById: createSidebarV2CreationRankMap(["b", "a"]),
        }),
      ),
    ).toEqual(["b", "a", "ghost"]);
  });

  test("does not mutate the input array", () => {
    const sessions = [session("b", { createdAt: iso(-1_000) }), session("a", { createdAt: iso(0) })];
    sortSessionsForSidebarV2(sessions);
    expect(ids(sessions)).toEqual(["b", "a"]);
  });
});

describe("reconcileSidebarV2CreationOrder", () => {
  test("new sessions enter at the top and known rows keep their slot", () => {
    expect(
      reconcileSidebarV2CreationOrder({
        knownOrder: ["b", "a"],
        sessionIds: ["a", "b", "c"],
      }),
    ).toEqual(["c", "b", "a"]);
  });

  test("removed sessions drop out without disturbing the survivors", () => {
    expect(
      reconcileSidebarV2CreationOrder({ knownOrder: ["c", "b", "a"], sessionIds: ["a", "c"] }),
    ).toEqual(["c", "a"]);
  });

  test("several new sessions keep their incoming relative order at the top", () => {
    expect(
      reconcileSidebarV2CreationOrder({ knownOrder: ["a"], sessionIds: ["a", "x", "y"] }),
    ).toEqual(["x", "y", "a"]);
  });

  test("a reordered server payload cannot move an existing row", () => {
    expect(
      reconcileSidebarV2CreationOrder({ knownOrder: ["c", "b", "a"], sessionIds: ["a", "b", "c"] }),
    ).toEqual(["c", "b", "a"]);
  });

  test("rank map is newest-first descending", () => {
    expect([...createSidebarV2CreationRankMap(["new", "old"])]).toEqual([
      ["new", 2],
      ["old", 1],
    ]);
  });
});

describe("settled and snoozed shelf sorts", () => {
  test("settled rows order by when work ENDED", () => {
    expect(
      ids(
        sortSettledSessionsForSidebarV2([
          session("older", { settledAt: iso(-2 * DAY_MS) }),
          session("newest", { settledAt: iso(-1_000) }),
          session("derived", { lastInteractionAt: iso(-60_000) }),
        ]),
      ),
    ).toEqual(["newest", "derived", "older"]);
  });

  test("settled timestamp prefers settledAt, then the activity clock", () => {
    expect(
      resolveSidebarV2SettledTimestampMs(
        session("s", { lastInteractionAt: iso(-DAY_MS), settledAt: iso(-1_000) }),
      ),
    ).toBe(NOW_MS - 1_000);
    expect(
      resolveSidebarV2SettledTimestampMs(
        session("s", { lastInteractionAt: iso(-DAY_MS), settledAt: "nope" }),
      ),
    ).toBe(NOW_MS - DAY_MS);
    expect(resolveSidebarV2SettledTimestampMs(session("s"))).toBeNull();
  });

  test("snoozed rows read as a schedule: soonest wake first", () => {
    expect(
      ids(
        sortSnoozedSessionsForSidebarV2([
          session("next-week", { snoozedUntil: iso(7 * DAY_MS) }),
          session("in-an-hour", { snoozedUntil: iso(60 * 60_000) }),
          session("tomorrow", { snoozedUntil: iso(DAY_MS) }),
        ]),
      ),
    ).toEqual(["in-an-hour", "tomorrow", "next-week"]);
  });

  test("an unusable wake time sinks to the bottom instead of jumping to the top", () => {
    expect(
      ids(
        sortSnoozedSessionsForSidebarV2([
          session("broken", { snoozedUntil: "nope" }),
          session("real", { snoozedUntil: iso(DAY_MS) }),
        ]),
      ),
    ).toEqual(["real", "broken"]);
  });
});

describe("partitionSidebarV2Sessions", () => {
  const options = { autoSettleAfterDays: 3, nowMs: NOW_MS };

  test("splits into active, snoozed, and settled shelves", () => {
    const result = partitionSidebarV2Sessions(
      [
        session("active", { createdAt: iso(-1_000), lastInteractionAt: iso(-60_000) }),
        session("snoozed", { createdAt: iso(-2_000), snoozedAt: iso(-60_000), snoozedUntil: iso(DAY_MS) }),
        session("settled", {
          createdAt: iso(-3_000),
          settledAt: iso(-1_000),
          settledOverride: "settled",
        }),
        session("stale", { createdAt: iso(-4_000), lastInteractionAt: iso(-9 * DAY_MS) }),
      ],
      options,
    );
    expect(ids(result.active)).toEqual(["active"]);
    expect(ids(result.snoozed)).toEqual(["snoozed"]);
    expect(ids(result.settled)).toEqual(["settled", "stale"]);
  });

  test("snooze is checked before settle: a snoozed stale row is still snoozed", () => {
    const result = partitionSidebarV2Sessions(
      [
        session("s", {
          lastInteractionAt: iso(-9 * DAY_MS),
          snoozedAt: iso(-1_000),
          snoozedUntil: iso(DAY_MS),
        }),
      ],
      options,
    );
    expect(ids(result.snoozed)).toEqual(["s"]);
    expect(result.settled).toEqual([]);
  });

  test("blocked and in-motion sessions can never be shelved", () => {
    const result = partitionSidebarV2Sessions(
      [
        session("attention", {
          activity: "attention",
          lastInteractionAt: iso(-30 * DAY_MS),
          settledOverride: "settled",
          snoozedAt: iso(-DAY_MS),
          snoozedUntil: iso(DAY_MS),
        }),
        session("working", {
          activity: "working",
          lastInteractionAt: iso(-30 * DAY_MS),
          settledOverride: "settled",
        }),
      ],
      options,
    );
    expect(ids(result.active).sort()).toEqual(["attention", "working"]);
  });

  test("a merged pull request settles its session", () => {
    const result = partitionSidebarV2Sessions([session("pr", { lastInteractionAt: iso(-1_000) })], {
      ...options,
      changeRequestStateBySessionId: new Map([["pr", "merged" as const]]),
    });
    expect(ids(result.settled)).toEqual(["pr"]);
  });

  test("an older gxserver keeps everything in the inbox", () => {
    const result = partitionSidebarV2Sessions(
      [
        session("stale", { lastInteractionAt: iso(-90 * DAY_MS) }),
        session("snoozed", { snoozedUntil: iso(DAY_MS) }),
      ],
      { ...options, capabilities: SIDEBAR_V2_LIFECYCLE_CAPABILITIES_DISABLED },
    );
    expect(result.active).toHaveLength(2);
    expect(result.settled).toEqual([]);
    expect(result.snoozed).toEqual([]);
  });

  test("the active shelf uses the position-stable inbox order", () => {
    const result = partitionSidebarV2Sessions(
      [
        session("old", { createdAt: iso(-2 * DAY_MS), lastInteractionAt: iso(-1_000) }),
        session("pinned", { createdAt: iso(-5 * DAY_MS), isPinned: true, lastInteractionAt: iso(-1_000) }),
        session("new", { createdAt: iso(-1_000), lastInteractionAt: iso(-1_000) }),
      ],
      options,
    );
    expect(ids(result.active)).toEqual(["pinned", "new", "old"]);
  });
});

describe("resolveAdjacentSidebarV2SessionId", () => {
  const sessionIds = ["a", "b", "c"];

  test("walks the rendered order in both directions", () => {
    expect(
      resolveAdjacentSidebarV2SessionId({ currentSessionId: "b", direction: "next", sessionIds }),
    ).toBe("c");
    expect(
      resolveAdjacentSidebarV2SessionId({ currentSessionId: "b", direction: "previous", sessionIds }),
    ).toBe("a");
  });

  test("stops at the ends instead of wrapping", () => {
    expect(
      resolveAdjacentSidebarV2SessionId({ currentSessionId: "c", direction: "next", sessionIds }),
    ).toBeNull();
    expect(
      resolveAdjacentSidebarV2SessionId({ currentSessionId: "a", direction: "previous", sessionIds }),
    ).toBeNull();
  });

  test("no selection enters from the matching end", () => {
    expect(
      resolveAdjacentSidebarV2SessionId({ currentSessionId: null, direction: "next", sessionIds }),
    ).toBe("a");
    expect(
      resolveAdjacentSidebarV2SessionId({ currentSessionId: null, direction: "previous", sessionIds }),
    ).toBe("c");
  });

  test("an unknown current id yields null instead of guessing a neighbor", () => {
    expect(
      resolveAdjacentSidebarV2SessionId({ currentSessionId: "gone", direction: "next", sessionIds }),
    ).toBeNull();
    expect(
      resolveAdjacentSidebarV2SessionId({ currentSessionId: "a", direction: "next", sessionIds: [] }),
    ).toBeNull();
  });
});

describe("orderItemsByPreferredIds", () => {
  const items = [{ id: "a" }, { id: "b" }, { id: "c" }];
  const getId = (item: { id: string }) => item.id;

  test("floats preferred ids to the front in the requested order", () => {
    expect(
      orderItemsByPreferredIds({ getId, items, preferredIds: ["c", "a"] }).map(getId),
    ).toEqual(["c", "a", "b"]);
  });

  test("an empty preference list is a copy", () => {
    const result = orderItemsByPreferredIds({ getId, items, preferredIds: [] });
    expect(result).toEqual(items);
    expect(result).not.toBe(items);
  });

  test("unknown and duplicate preferred ids are ignored", () => {
    expect(
      orderItemsByPreferredIds({ getId, items, preferredIds: ["zz", "b", "b"] }).map(getId),
    ).toEqual(["b", "a", "c"]);
  });

  test("alias ids let one item match several preferences", () => {
    expect(
      orderItemsByPreferredIds({
        getId,
        getPreferenceIds: (item) => [item.id, `alias-${item.id}`],
        items,
        preferredIds: ["alias-c"],
      }).map(getId),
    ).toEqual(["c", "a", "b"]);
  });
});
