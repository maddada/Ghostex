/*
 * CDXC:Release 2026-08-13:
 * Bounded retries around external dependency fetches only.
 *
 * Never wrap a compile, package, sign, or notarize step in this helper, and
 * never retry a whole job: `withRetry` exists so a closed archive request does
 * not throw away twenty minutes of successful compilation, not so deterministic
 * failures get more chances. A failure that classification does not recognize as
 * transient is re-thrown immediately.
 */

import { classifyError } from './failure-classification.mjs';

/* §6.2: the bounded profiles each call site uses. */
export const RETRY_PROFILES = Object.freeze({
  /* CEF distribution download; integrity is enforced by cef-rs itself. */
  cef: { attempts: 3, baseDelayMs: 5000, factor: 3 },
  /* gh api / gh release view|download|upload / Actions artifact download. */
  github: { attempts: 4, baseDelayMs: 2000, factor: 3 },
  /* apt-get, brew, npm ci, bun install, dotnet tool install, rustup, sdkmanager. */
  toolchain: { attempts: 3, baseDelayMs: 5000, factor: 4 },
  /* Zig package fetches, the release 7.7 failure class. */
  zigFetch: { attempts: 4, baseDelayMs: 5000, factor: 3, jitterMs: 5000 },
});

export function retryDelays({ attempts = 4, baseDelayMs = 2000, factor = 3 } = {}) {
  const delays = [];
  for (let attempt = 1; attempt < attempts; attempt += 1) {
    delays.push(baseDelayMs * factor ** (attempt - 1));
  }
  return delays;
}

function defaultSleep(milliseconds) {
  return new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

export async function withRetry(operation, options = {}) {
  const {
    attempts = 4,
    baseDelayMs = 2000,
    classify = classifyError,
    factor = 3,
    jitterMs = 0,
    label = 'operation',
    onRetry,
    random = Math.random,
    sleep = defaultSleep,
  } = options;
  if (!Number.isInteger(attempts) || attempts < 1) throw new Error('withRetry requires at least one attempt');

  let lastError;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return await operation({ attempt, attempts });
    } catch (error) {
      lastError = error;
      const classification = classify(error);
      if (!classification.retryable) {
        if (error instanceof Error) {
          error.ghostexClassification = classification;
          error.ghostexRetryLabel = label;
        }
        throw error;
      }
      if (attempt === attempts) {
        if (error instanceof Error) {
          error.ghostexClassification = classification;
          error.ghostexRetryLabel = label;
          error.ghostexRetryAttempts = attempts;
        }
        throw error;
      }
      const delay = baseDelayMs * factor ** (attempt - 1) + (jitterMs > 0 ? Math.floor(random() * jitterMs) : 0);
      if (onRetry) {
        onRetry({ attempt, attempts, classification, delayMs: delay, error, label });
      } else {
        process.stdout.write(
          `::notice::retry ${classification.matchedRule} attempt ${attempt}/${attempts} after ${delay}ms (${label})\n`
        );
      }
      await sleep(delay);
    }
  }
  throw lastError;
}

export function withRetryProfile(operation, profileName, overrides = {}) {
  const profile = RETRY_PROFILES[profileName];
  if (!profile) throw new Error(`Unknown retry profile: ${profileName}`);
  return withRetry(operation, { label: profileName, ...profile, ...overrides });
}
