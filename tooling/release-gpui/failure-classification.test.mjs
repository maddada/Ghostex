import { describe, expect, test } from "vitest";
import {
  FATAL_RULES,
  RETRYABLE_RULES,
  classifyError,
  classifyFailure,
  failureText,
} from "./failure-classification.mjs";
import { RETRY_PROFILES, retryDelays, withRetry, withRetryProfile } from "./retry.mjs";

describe("transient failure classification", () => {
  test.each([
    ["zig-http-close", "error: invalid HTTP response: HttpConnectionClosing\nfetching zigimg archive"],
    ["zig-fetch-net", "error: ConnectionResetByPeer while fetching dependency"],
    ["curl-transient", "curl: (28) Operation timed out after 30000 milliseconds"],
    ["http-5xx", "unexpected status HTTP/1.1 503 from objects.githubusercontent.com"],
    ["socket", "request failed: ECONNRESET"],
    ["gh-rate-limit", "API rate limit exceeded for installation"],
    ["gh-artifact", "Unable to download artifact release-macos-arm64"],
    ["apt-transient", "Temporary failure resolving 'archive.ubuntu.com'"],
    ["npm-network", "npm ERR! network request to https://registry.npmjs.org failed"],
    ["brew-transient", "Failed to download resource \"cmake\""],
    ["notary-transient", "Unable to reach Apple notary service"],
    ["runner-net", "The remote name could not be resolved"],
  ])("retries the allow-listed %s signature", (ruleId, text) => {
    expect(classifyFailure(text)).toEqual({ category: "transient", matchedRule: ruleId, retryable: true });
  });

  test.each([
    ["rustc", "error[E0412]: cannot find type `SharedString` in this scope"],
    ["rustc", "error: could not compile `ghostex-gpui` (bin) due to 1 previous error"],
    ["linker", "error: linking with `link.exe` failed: exit code 1181"],
    ["zig-compile", "src/main.zig:42:9: error: expected type 'u32'"],
    ["integrity", "hash mismatch: expected abc, found def"],
    ["signature", "codesign failed with exit code 1"],
    ["ghostex-refusal", "Refusing to replace cef-148.4.0-linux-x64.tar.gz"],
    ["test-failure", "FAIL  tooling/release-gpui/plan.test.mjs"],
  ])("never retries the deterministic %s signature", (ruleId, text) => {
    expect(classifyFailure(text)).toEqual({ category: "fatal", matchedRule: ruleId, retryable: false });
  });

  test("defaults to fatal for unrecognized output", () => {
    expect(classifyFailure("something nobody has ever seen before")).toEqual({
      category: "unclassified",
      matchedRule: null,
      retryable: false,
    });
    expect(classifyFailure("")).toEqual({ category: "unclassified", matchedRule: null, retryable: false });
    expect(classifyFailure(undefined).retryable).toBe(false);
  });

  test("fatal rules win over a transient match in the same output", () => {
    const mixed = [
      "warning: retrying download",
      "error: invalid HTTP response: HttpConnectionClosing",
      "error[E0412]: cannot find type `SharedString` in this scope",
    ].join("\n");
    expect(classifyFailure(mixed)).toEqual({ category: "fatal", matchedRule: "rustc", retryable: false });

    const integrityAfterTransient = "curl: (28) Operation timed out\nzig: hash mismatch for zigimg";
    expect(classifyFailure(integrityAfterTransient).retryable).toBe(false);
  });

  test("classifies from every text surface an error carries", () => {
    const error = Object.assign(new Error("command failed"), {
      stderr: Buffer.from("error: ConnectionTimedOut"),
      stdout: Buffer.from(""),
    });
    expect(failureText(error)).toContain("ConnectionTimedOut");
    expect(classifyError(error).retryable).toBe(true);
  });

  test("keeps both rule lists unique and non-empty", () => {
    const ids = [...FATAL_RULES, ...RETRYABLE_RULES].map((rule) => rule.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(FATAL_RULES.length).toBeGreaterThan(0);
    expect(RETRYABLE_RULES.length).toBeGreaterThan(0);
  });
});

describe("bounded retries", () => {
  const sleep = () => Promise.resolve();

  test("retries a transient failure up to the bound and then rethrows", async () => {
    let attempts = 0;
    const retries = [];
    await expect(
      withRetry(
        () => {
          attempts += 1;
          throw new Error("error: invalid HTTP response: HttpConnectionClosing");
        },
        { attempts: 4, baseDelayMs: 5000, onRetry: (event) => retries.push(event), sleep },
      ),
    ).rejects.toThrow(/HttpConnectionClosing/u);
    expect(attempts).toBe(4);
    expect(retries.map((event) => event.delayMs)).toEqual([5000, 15000, 45000]);
    expect(retries.map((event) => event.classification.matchedRule)).toEqual([
      "zig-http-close",
      "zig-http-close",
      "zig-http-close",
    ]);
  });

  test("never retries a deterministic failure", async () => {
    let attempts = 0;
    await expect(
      withRetry(
        () => {
          attempts += 1;
          throw new Error("error[E0412]: cannot find type `SharedString` in this scope");
        },
        { attempts: 4, sleep },
      ),
    ).rejects.toThrow(/SharedString/u);
    expect(attempts).toBe(1);
  });

  test("returns the first successful attempt", async () => {
    let attempts = 0;
    const value = await withRetry(
      () => {
        attempts += 1;
        if (attempts < 3) throw new Error("curl: (52) Empty reply from server");
        return "ok";
      },
      { attempts: 4, sleep },
    );
    expect(value).toBe("ok");
    expect(attempts).toBe(3);
  });

  test("exposes the documented bounded profiles", () => {
    expect(retryDelays(RETRY_PROFILES.github)).toEqual([2000, 6000, 18000]);
    expect(retryDelays(RETRY_PROFILES.zigFetch)).toEqual([5000, 15000, 45000]);
    expect(retryDelays(RETRY_PROFILES.toolchain)).toEqual([5000, 20000]);
    expect(RETRY_PROFILES.zigFetch.jitterMs).toBe(5000);
    expect(() => withRetryProfile(() => "ok", "nonexistent")).toThrow(/Unknown retry profile/u);
  });

  test("adds bounded jitter without exceeding the profile budget", async () => {
    const retries = [];
    await expect(
      withRetry(() => Promise.reject(new Error("EAI_AGAIN")), {
        ...RETRY_PROFILES.zigFetch,
        onRetry: (event) => retries.push(event),
        random: () => 0.5,
        sleep,
      }),
    ).rejects.toThrow(/EAI_AGAIN/u);
    expect(retries.map((event) => event.delayMs)).toEqual([7500, 17500, 47500]);
  });
});
