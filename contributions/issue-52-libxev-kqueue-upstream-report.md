# Upstream bug report — null-optional unwrap in `backend.kqueue.Loop.tick`

> Draft intended for filing against **mitchellh/libxev** (the kqueue backend), cross-referenced from Ghostex issue #52. Ghostex vendors libxev transitively through Ghostty; the crashing frame is in libxev, not in Ghostex or Ghostty source.

## Summary

On macOS (kqueue backend), libxev aborts with a Zig null-optional unwrap panic inside `backend.kqueue.Loop.tick`, running on a background I/O thread. The crash is a `SIGABRT` raised by Zig's `FullPanic(defaultPanic).unwrapNull`, i.e. a `.?` unwrap of a `null` optional (or equivalent `orelse unreachable`) during event-loop processing.

## Affected versions

- **libxev:** commit `34fa50878aec6e5fa8f532867001ab3c36fae23e` (as pinned by Ghostty `build.zig.zon`, dependency `libxev-0.0.0-86vtc4IcEwCqEYxEYoN_3KXmc6A9VLcm22aVImfvecYs`).
- **Ghostty:** `1.3.2-dev`, `minimum_zig_version = 0.15.2`.
- **Host application:** Ghostex `4.21.4` (build `42104`).
- **OS / HW:** macOS `26.5.1` (25F80), Apple `M1`, 8 GB.

## Faulting thread (named `io`)

Trimmed, symbolicated frames as reported:

```
abort + 148
debug.defaultPanic + 1476
debug.FullPanic(defaultPanic).unwrapNull + 48
backend.kqueue.Loop.tick + 3196
Thread.PosixThreadImpl.spawn(...).entryFn + 376
```

Crash type: `EXC_CRASH (SIGABRT)` triggered by a Zig panic (`unwrapNull`).

## Environment / workload at crash

- Process uptime ≈ 28 hours across several sleep/wake cycles, with low-power mode active.
- Heavy, multi-pane load: several Claude Code sessions and several OpenCode sessions open concurrently.
- Immediately prior: a **new git worktree was created** and an OpenCode session was spawned inside it; input was being sent to another OpenCode session concurrently.
- Reporter's note: first-ever crash, and one of the rare times using git worktrees — "the worktree path may be worth a look." Because the crash is on the `io` thread in `tick`, they suspect a **PTY/pane being set up or torn down concurrently** as the trigger.

## Analysis / hypothesis

`Loop.tick` on the kqueue backend processes returned kevents and dispatches completions. A null-unwrap there typically indicates a completion, watcher, or fd-associated optional that was expected to be present but had already been cleared/freed — consistent with a **race between event delivery and teardown**:

- A pane/PTY (kqueue fd registration + associated completion/userdata) is torn down on one thread while `tick` is mid-processing a kevent that references it on the `io` thread.
- After 28h and many sleep/wake cycles, EVFILT registrations and completion state can drift; a `.?` on a `?*Completion` / `?userdata` that was reset during cancellation would abort exactly as shown.

This is a latent concurrency/lifetime issue rather than a deterministic input bug, which matches the "no clean repro, first crash in 28h" report.

## Reproduction

No deterministic repro. Best-effort stress scenario to attempt to surface it:

1. macOS on Apple Silicon; a libxev/Ghostty build with the kqueue backend.
2. Open many concurrent PTY-backed panes (10+), each running an active process.
3. Repeatedly create/destroy panes and PTYs while I/O is flowing (spawn a session inside a freshly created git worktree, then close/rotate panes rapidly).
4. Cycle sleep/wake and low-power mode over an extended run.
5. Watch for a `unwrapNull` abort on the `io` thread in `Loop.tick`.

## What upstream maintainers likely need

- The **full** crash report / incident ID (Ghostex offered to share privately) with complete, symbolicated frames and register state at the faulting instruction.
- The exact libxev build hash (above) and Zig version (0.15.2).
- Whether the app uses `Loop` from multiple threads or cancels completions from a thread other than the one running `tick` (libxev's threading contract is single-loop-per-thread; cross-thread cancellation would be a strong lead).

## Recommended handling in Ghostex (until upstream fix)

This crash is **not** in Ghostex or Ghostty source and should not be blind-patched in the vendored dependency. Recommended posture:

1. File/track upstream against libxev with the details above; link from issue #52.
2. Audit Ghostex/Ghostty pane+PTY teardown to ensure libxev completions tied to a PTY fd are cancelled and drained **on the loop's own thread** before the backing objects are freed (i.e., no cross-thread free while `tick` may still reference them).
3. If a defensive guard is desired before upstream lands, it belongs at the Ghostty↔libxev integration boundary (completion lifetime/cancellation ordering), not as an `orelse return` inside vendored `tick`, which would mask the underlying lifetime bug (per repo policy against fallback patches).

## Verification note

No Zig toolchain is available in this workspace; this is a documentation deliverable only. No source in `ghostty/` or the vendored libxev cache was modified.
