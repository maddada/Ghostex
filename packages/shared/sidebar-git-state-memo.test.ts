import { describe, expect, test } from "vite-plus/test";
import {
  SIDEBAR_GIT_HUB_MEMO_TTL_MS,
  SIDEBAR_GIT_MEMO_MAX_ENTRIES,
  SIDEBAR_GIT_STATE_MEMO_TTL_MS,
  SidebarGitTtlMemo,
} from "./sidebar-git-state-memo";

describe("SidebarGitTtlMemo", () => {
  test("should serve a stored value while it is inside the ttl", () => {
    const memo = new SidebarGitTtlMemo<string>({ ttlMs: 1000 });

    memo.set("project-a", "branch-a", 0);

    expect(memo.get("project-a", 0)).toBe("branch-a");
    expect(memo.get("project-a", 999)).toBe("branch-a");
  });

  test("should expire a stored value once the ttl elapses", () => {
    const memo = new SidebarGitTtlMemo<string>({ ttlMs: 1000 });

    memo.set("project-a", "branch-a", 0);

    expect(memo.get("project-a", 1000)).toBeUndefined();
    expect(memo.size).toBe(0);
  });

  test("should not extend the ttl when an entry is read", () => {
    const memo = new SidebarGitTtlMemo<string>({ ttlMs: 1000 });

    memo.set("project-a", "branch-a", 0);
    expect(memo.get("project-a", 900)).toBe("branch-a");

    expect(memo.get("project-a", 1000)).toBeUndefined();
  });

  test("should return undefined for unknown keys", () => {
    const memo = new SidebarGitTtlMemo<string>({ ttlMs: 1000 });

    expect(memo.get("missing", 0)).toBeUndefined();
    expect(memo.peek("missing")).toBeUndefined();
    expect(memo.isFreshKey("missing", 0)).toBe(false);
  });

  test("should replace a value and restart its ttl on set", () => {
    const memo = new SidebarGitTtlMemo<string>({ ttlMs: 1000 });

    memo.set("project-a", "branch-a", 0);
    memo.set("project-a", "branch-b", 800);

    expect(memo.get("project-a", 1500)).toBe("branch-b");
    expect(memo.size).toBe(1);
  });

  test("should report freshness without consuming the entry", () => {
    const memo = new SidebarGitTtlMemo<string>({ ttlMs: 1000 });

    memo.set("project-a", "branch-a", 0);

    expect(memo.isFreshKey("project-a", 500)).toBe(true);
    expect(memo.isFreshKey("project-a", 1000)).toBe(false);
    expect(memo.peek("project-a")).toBe("branch-a");
  });

  test("should peek stale values so callers can revalidate without blanking", () => {
    const memo = new SidebarGitTtlMemo<string>({ ttlMs: 1000 });

    memo.set("project-a", "pr-42", 0);

    expect(memo.isFreshKey("project-a", 5000)).toBe(false);
    expect(memo.peek("project-a")).toBe("pr-42");
  });

  test("should drop an expired entry so a later peek cannot revive it", () => {
    const memo = new SidebarGitTtlMemo<string>({ ttlMs: 1000 });

    memo.set("project-a", "branch-a", 0);
    expect(memo.get("project-a", 2000)).toBeUndefined();

    expect(memo.peek("project-a")).toBeUndefined();
  });

  test("should evict the least recently used entry beyond the bound", () => {
    const memo = new SidebarGitTtlMemo<number>({ maxEntries: 2, ttlMs: 1000 });

    memo.set("a", 1, 0);
    memo.set("b", 2, 0);
    memo.set("c", 3, 0);

    expect(memo.size).toBe(2);
    expect(memo.get("a", 0)).toBeUndefined();
    expect(memo.get("b", 0)).toBe(2);
    expect(memo.get("c", 0)).toBe(3);
  });

  test("should keep an entry alive when reads renew its recency", () => {
    const memo = new SidebarGitTtlMemo<number>({ maxEntries: 2, ttlMs: 1000 });

    memo.set("a", 1, 0);
    memo.set("b", 2, 0);
    expect(memo.get("a", 100)).toBe(1);
    memo.set("c", 3, 100);

    expect(memo.get("a", 100)).toBe(1);
    expect(memo.get("b", 100)).toBeUndefined();
  });

  test("should support explicit invalidation and clearing", () => {
    const memo = new SidebarGitTtlMemo<number>({ ttlMs: 1000 });

    memo.set("a", 1, 0);
    memo.set("b", 2, 0);
    memo.delete("a");

    expect(memo.get("a", 0)).toBeUndefined();
    expect(memo.get("b", 0)).toBe(2);

    memo.clear();
    expect(memo.size).toBe(0);
  });

  test("should never store more entries than the default bound", () => {
    const memo = new SidebarGitTtlMemo<number>({ ttlMs: 1000 });

    for (let index = 0; index < SIDEBAR_GIT_MEMO_MAX_ENTRIES + 25; index += 1) {
      memo.set(`project-${index}`, index, 0);
    }

    expect(memo.size).toBe(SIDEBAR_GIT_MEMO_MAX_ENTRIES);
    expect(memo.get("project-0", 0)).toBeUndefined();
    expect(memo.get(`project-${SIDEBAR_GIT_MEMO_MAX_ENTRIES + 24}`, 0)).toBe(
      SIDEBAR_GIT_MEMO_MAX_ENTRIES + 24,
    );
  });

  test("should treat a zero ttl as always stale", () => {
    const memo = new SidebarGitTtlMemo<number>({ ttlMs: 0 });

    memo.set("a", 1, 0);

    expect(memo.get("a", 0)).toBeUndefined();
  });
});

describe("sidebar git memo policy constants", () => {
  test("should keep github results cached far longer than local git probes", () => {
    expect(SIDEBAR_GIT_STATE_MEMO_TTL_MS).toBe(45 * 1000);
    expect(SIDEBAR_GIT_HUB_MEMO_TTL_MS).toBe(5 * 60 * 1000);
    expect(SIDEBAR_GIT_HUB_MEMO_TTL_MS).toBeGreaterThan(SIDEBAR_GIT_STATE_MEMO_TTL_MS);
  });
});
