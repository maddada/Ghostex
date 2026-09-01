/*
 CDXC:ReleaseAutomation 2026-09-01-11:35:
 Release preflight asserted on literal substrings of the Actions workflow YAML.
 Prettier normalised `GHOSTEX_REQUIRE_BEADS_SMOKE: "1"` to single quotes in the
 49831862 formatting pass, the substring stopped matching, and the gate check
 started failing for a reason that did not exist. It was already broken at
 v8.3.0, so that release shipped past a gate providing zero protection, and
 8.4.0 only caught it by hand.

 The class of bug is "an assertion about another file's contents goes stale and
 is indistinguishable from a real regression". This module removes the class by
 doing three things:

 1. Asserting on a PARSED document, not on source text. Quote style, key order,
    indentation, and line wrapping cannot change a parsed value, so a formatting
    pass can no longer break a gate check.
 2. Giving every assertion an explicit CONTRACT: the literals it depends on that
    live in other files (the env var the build script reads, the subcommand the
    resumable driver implements). If a contract literal is gone, the assertion
    cannot be trusted and is reported as STALE - "fix the check" - never as a
    product regression.
 3. Separating the three outcomes a gate check can have:
      stale             the check itself is out of date or unrunnable
      regression/absent the gate is simply not in the workflow any more
      regression/value  the gate is there with the wrong value
    Before this, all three printed the same sentence.

 Deliberately no fallbacks: nothing here downgrades a mismatch to a pass. Every
 non-ok outcome fails preflight; only the wording and the remedy differ.
*/

import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { parseWorkflowYaml, WorkflowYamlError } from './release-workflow-yaml.mjs';

export const OUTCOME_OK = 'ok';
export const OUTCOME_STALE = 'stale';
export const OUTCOME_REGRESSION = 'regression';

export const REGRESSION_ABSENT = 'absent';
export const REGRESSION_VALUE = 'value';

const GXSERVER_BUILD_SCRIPT = 'tooling/build-remote-gxserver-linux-release.sh';
const RESUMABLE_DRIVER = 'tooling/release-resumable.mjs';
const BUILD_SCRIPT_BASENAME = 'build-remote-gxserver-linux-release.sh';
const BEADS_SMOKE_ENV = 'GHOSTEX_REQUIRE_BEADS_SMOKE';
const STAGE_ADVANCE_COMMAND = 'stage-package-and-advance';
const ARM64_NATIVE_RUNNER = 'ubuntu-24.04-arm';

function ok(detail) {
  return { detail, outcome: OUTCOME_OK };
}

function stale(detail) {
  return { detail, outcome: OUTCOME_STALE };
}

function regressed(reason, detail) {
  return { detail, outcome: OUTCOME_REGRESSION, reason };
}

function jobSteps(document, jobName) {
  const job = document?.jobs?.[jobName];
  if (!job || typeof job !== 'object') {
    return null;
  }
  return { job, steps: Array.isArray(job.steps) ? job.steps : [] };
}

function stepsRunning(steps, needle) {
  return steps.filter((step) => step && typeof step.run === 'string' && step.run.includes(needle));
}

function describeStep(step, index) {
  return step?.name ? `step "${step.name}"` : `step #${index + 1}`;
}

/*
 Every assertion the release preflight makes about a workflow file. Each one
 states, in order: which file it reads, which job/step it navigates to, which
 literals in OTHER files it depends on, and what the parsed value must be.
*/
export function gxserverLinuxWorkflowAssertions(arch) {
  const file = `.github/workflows/release-build-gxserver-${arch}.yml`;
  const shared = { arch, file };
  const assertions = [
    {
      ...shared,
      contract: [
        { file: GXSERVER_BUILD_SCRIPT, literal: '--arch' },
        { file: GXSERVER_BUILD_SCRIPT, literal: arch },
      ],
      id: `gxserver-linux-${arch}/build-arch`,
      requirement: `the build job runs ${BUILD_SCRIPT_BASENAME} --arch ${arch}`,
      verify(document) {
        const build = jobSteps(document, 'build');
        if (!build) {
          return stale(`${file} has no jobs.build; the assertion navigates by that job name.`);
        }
        const matches = stepsRunning(build.steps, BUILD_SCRIPT_BASENAME);
        if (matches.length === 0) {
          return regressed(REGRESSION_ABSENT, `no jobs.build step runs ${BUILD_SCRIPT_BASENAME}.`);
        }
        const requested = matches.flatMap((step) =>
          [...step.run.matchAll(/build-remote-gxserver-linux-release\.sh\s+--arch\s+(\S+)/g)].map((match) => match[1])
        );
        if (requested.length === 0) {
          return regressed(
            REGRESSION_ABSENT,
            `${BUILD_SCRIPT_BASENAME} runs without --arch, so this workflow no longer pins an architecture.`
          );
        }
        if (!requested.includes(arch)) {
          return regressed(REGRESSION_VALUE, `builds --arch ${requested.join(', ')} instead of ${arch}.`);
        }
        return ok(`--arch ${arch}`);
      },
    },
    {
      ...shared,
      contract: [{ file: GXSERVER_BUILD_SCRIPT, literal: BEADS_SMOKE_ENV }],
      id: `gxserver-linux-${arch}/beads-smoke-gate`,
      requirement: `the build step sets ${BEADS_SMOKE_ENV} to 1`,
      verify(document) {
        const build = jobSteps(document, 'build');
        if (!build) {
          return stale(`${file} has no jobs.build; the assertion navigates by that job name.`);
        }
        const matches = stepsRunning(build.steps, BUILD_SCRIPT_BASENAME);
        if (matches.length === 0) {
          return stale(
            `${file} has no jobs.build step running ${BUILD_SCRIPT_BASENAME}, so the step this env var belongs to cannot be located.`
          );
        }
        const missing = [];
        const wrong = [];
        for (const [index, step] of matches.entries()) {
          const env = step.env && typeof step.env === 'object' ? step.env : {};
          if (!Object.hasOwn(env, BEADS_SMOKE_ENV)) {
            missing.push(describeStep(step, index));
            continue;
          }
          // Parsed, so `1`, `'1'`, and `"1"` are the same value by construction.
          if (String(env[BEADS_SMOKE_ENV]).trim() !== '1') {
            wrong.push(`${describeStep(step, index)} sets ${JSON.stringify(env[BEADS_SMOKE_ENV])}`);
          }
        }
        if (missing.length > 0) {
          return regressed(
            REGRESSION_ABSENT,
            `${missing.join(', ')} builds the package without ${BEADS_SMOKE_ENV}, so the packaged Beads embedded-Dolt smoke test is not required.`
          );
        }
        if (wrong.length > 0) {
          return regressed(REGRESSION_VALUE, `${wrong.join('; ')}; expected 1.`);
        }
        return ok(`${BEADS_SMOKE_ENV}=1`);
      },
    },
    {
      ...shared,
      contract: [{ file: RESUMABLE_DRIVER, literal: STAGE_ADVANCE_COMMAND }],
      id: `gxserver-linux-${arch}/stage-and-advance`,
      requirement: `the stage job advances durable release state with ${STAGE_ADVANCE_COMMAND}`,
      verify(document) {
        const stage = jobSteps(document, 'stage');
        if (!stage) {
          return stale(`${file} has no jobs.stage; the assertion navigates by that job name.`);
        }
        const matches = stepsRunning(stage.steps, 'release-resumable.mjs');
        if (matches.length === 0) {
          return regressed(REGRESSION_ABSENT, 'no jobs.stage step invokes release-resumable.mjs.');
        }
        if (!matches.some((step) => step.run.includes(STAGE_ADVANCE_COMMAND))) {
          return regressed(
            REGRESSION_VALUE,
            `jobs.stage invokes release-resumable.mjs without ${STAGE_ADVANCE_COMMAND}, so durable release state is never advanced.`
          );
        }
        return ok(STAGE_ADVANCE_COMMAND);
      },
    },
  ];
  if (arch === 'arm64') {
    assertions.push({
      ...shared,
      contract: [],
      id: 'gxserver-linux-arm64/native-runner',
      requirement: `the build job runs on ${ARM64_NATIVE_RUNNER}`,
      verify(document) {
        const build = jobSteps(document, 'build');
        if (!build) {
          return stale(`${file} has no jobs.build; the assertion navigates by that job name.`);
        }
        if (!Object.hasOwn(build.job, 'runs-on')) {
          return regressed(REGRESSION_ABSENT, 'jobs.build declares no runs-on.');
        }
        const runsOn = build.job['runs-on'];
        if (runsOn !== ARM64_NATIVE_RUNNER) {
          return regressed(
            REGRESSION_VALUE,
            `jobs.build runs on ${JSON.stringify(runsOn)}, not the native ARM64 runner ${ARM64_NATIVE_RUNNER}; the packaged Beads smoke test needs real ARM64 hardware.`
          );
        }
        return ok(ARM64_NATIVE_RUNNER);
      },
    });
  }
  return assertions;
}

export function releaseWorkflowAssertions() {
  return [...gxserverLinuxWorkflowAssertions('x64'), ...gxserverLinuxWorkflowAssertions('arm64')];
}

async function readIfPresent(filePath) {
  if (!existsSync(filePath)) {
    return null;
  }
  return readFile(filePath, 'utf8');
}

async function checkContract(repoRoot, contract, sources) {
  const contents = Object.hasOwn(sources, contract.file)
    ? sources[contract.file]
    : await readIfPresent(path.join(repoRoot, contract.file));
  if (contents === null) {
    return `${contract.file} does not exist, so the assertion's contract cannot be confirmed`;
  }
  if (!contents.includes(contract.literal)) {
    return `${contract.file} no longer contains ${JSON.stringify(contract.literal)}`;
  }
  return null;
}

/*
 Evaluate every workflow assertion. `documents` lets a caller substitute file
 contents (used by the tests that prove each check still fails when the real
 gate is absent) without touching a workflow file on disk.
*/
export async function evaluateWorkflowAssertions({ repoRoot, sources = {} } = {}) {
  const assertions = releaseWorkflowAssertions();
  const parsed = new Map();
  const results = [];
  for (const assertion of assertions) {
    const contractProblems = [];
    for (const contract of assertion.contract) {
      const problem = await checkContract(repoRoot, contract, sources);
      if (problem) {
        contractProblems.push(problem);
      }
    }
    if (contractProblems.length > 0) {
      results.push({
        ...stale(contractProblems.join('; ')),
        file: assertion.file,
        id: assertion.id,
        requirement: assertion.requirement,
      });
      continue;
    }
    if (!parsed.has(assertion.file)) {
      const source = Object.hasOwn(sources, assertion.file)
        ? sources[assertion.file]
        : await readIfPresent(path.join(repoRoot, assertion.file));
      if (source === null) {
        parsed.set(assertion.file, { missing: true });
      } else {
        try {
          parsed.set(assertion.file, { document: parseWorkflowYaml(source) });
        } catch (error) {
          parsed.set(assertion.file, {
            parseError: error instanceof WorkflowYamlError ? error.message : String(error?.message ?? error),
          });
        }
      }
    }
    const entry = parsed.get(assertion.file);
    if (entry.missing) {
      results.push({
        ...regressed(REGRESSION_ABSENT, `${assertion.file} does not exist, so this release job cannot be dispatched.`),
        file: assertion.file,
        id: assertion.id,
        requirement: assertion.requirement,
      });
      continue;
    }
    if (entry.parseError) {
      results.push({
        ...stale(`${assertion.file} could not be parsed: ${entry.parseError}`),
        file: assertion.file,
        id: assertion.id,
        requirement: assertion.requirement,
      });
      continue;
    }
    let outcome;
    try {
      outcome = assertion.verify(entry.document);
    } catch (error) {
      outcome = stale(`assertion threw while reading ${assertion.file}: ${String(error?.message ?? error)}`);
    }
    results.push({ ...outcome, file: assertion.file, id: assertion.id, requirement: assertion.requirement });
  }
  return {
    ok: results.filter((result) => result.outcome === OUTCOME_OK),
    regressions: results.filter((result) => result.outcome === OUTCOME_REGRESSION),
    results,
    stale: results.filter((result) => result.outcome === OUTCOME_STALE),
  };
}

/*
 The Sparkle EdDSA public key preflight compares `generate_keys -p` against is
 not preflight's to invent: apps/desktop/scripts/build-macos-app.sh stamps it
 into Info.plist as SUPublicEDKey. Read it from there instead of duplicating the
 base64, so changing the key in the build script cannot leave preflight failing
 against a value nothing ships any more.
*/
export const SPARKLE_KEY_SOURCE = 'apps/desktop/scripts/build-macos-app.sh';
const SPARKLE_KEY_ASSIGNMENT =
  /GHOSTEX_GPUI_SPARKLE_PUBLIC_ED_KEY="\$\{GHOSTEX_GPUI_SPARKLE_PUBLIC_ED_KEY:-([^"}]+)\}"/;

export function extractSparklePublicKey(buildScript) {
  const match = SPARKLE_KEY_ASSIGNMENT.exec(buildScript);
  return match ? match[1].trim() : null;
}

/*
 Literals preflight still has to hold as constants because their real home is a
 JS object literal or a shell default, not a parseable document. Each one is
 probed against the file that owns it, so a rename there is reported as a stale
 preflight constant instead of quietly comparing against a dead value.
*/
export function preflightLiteralProbes({ sparklePublicKey, signingIdentity, githubRepo }) {
  return [
    {
      file: SPARKLE_KEY_SOURCE,
      id: 'sparkle-public-key/build-script',
      literal: sparklePublicKey,
      why: 'the app stamps this key into Info.plist as SUPublicEDKey',
    },
    {
      file: 'tooling/release-ghostex.mjs',
      id: 'sparkle-public-key/release-driver',
      literal: sparklePublicKey,
      why: 'release-ghostex.mjs verifies signed appcasts against the same key',
    },
    {
      file: 'tooling/release-ghostex.mjs',
      id: 'signing-identity/release-driver',
      literal: signingIdentity,
      why: 'preflight probes the local keychain for the identity release-ghostex.mjs signs with',
    },
    {
      file: 'tooling/release-ghostex.mjs',
      id: 'github-repo/release-driver',
      literal: githubRepo,
      why: 'preflight looks for an existing release in the repository release-ghostex.mjs publishes to',
    },
    {
      file: 'appcast.xml',
      id: 'sparkle-version-element/appcast',
      literal: '<sparkle:version>',
      why: 'the Sparkle build-number check counts these elements and would see an empty feed if the element were renamed',
    },
    {
      file: GXSERVER_BUILD_SCRIPT,
      id: 'gxserver-build-script/path',
      literal: '--arch',
      why: 'both Linux gxserver workflows dispatch this script with a pinned --arch',
    },
  ];
}

export async function evaluatePreflightLiteralProbes({ repoRoot, constants }) {
  const probes = preflightLiteralProbes(constants);
  const results = [];
  for (const probe of probes) {
    if (typeof probe.literal !== 'string' || probe.literal.length === 0) {
      results.push({ ...probe, ...stale('preflight has no value to probe for; its constant is empty.') });
      continue;
    }
    const contents = await readIfPresent(path.join(repoRoot, probe.file));
    if (contents === null) {
      results.push({ ...probe, ...stale(`${probe.file} does not exist.`) });
      continue;
    }
    if (!contents.includes(probe.literal)) {
      results.push({
        ...probe,
        ...stale(`${probe.file} no longer contains ${JSON.stringify(probe.literal)} (${probe.why}).`),
      });
      continue;
    }
    results.push({ ...probe, ...ok(probe.file) });
  }
  return { results, stale: results.filter((result) => result.outcome === OUTCOME_STALE) };
}

export function formatStale(entries) {
  return entries.map((entry) => `${entry.id}: ${entry.detail}`).join('; ');
}

export function formatRegressions(entries) {
  return entries
    .map(
      (entry) => `${entry.id} (${entry.reason === REGRESSION_ABSENT ? 'gate absent' : 'wrong value'}): ${entry.detail}`
    )
    .join('; ');
}
