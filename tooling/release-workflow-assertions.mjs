/*
 CDXC:Release 2026-09-01-11:35:
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
const STAGE_ADVANCE_COMMAND = 'stage-package-and-advance';
const ARM64_NATIVE_RUNNER = 'ubuntu-24.04-arm';

/*
 CDXC:Release 2026-09-02:
 Measured on the 8.5.0 release run: the macOS job took 31 minutes and was the
 critical path, and a cold `cargo build --release` was roughly half of it
 (Linux desktop: 663 s inside "Stage the Linux desktop payload once";
 gxserver x64: 7 min). The workflows cached only ~/.cargo/registry and
 ~/.cargo/git - crate *sources* - so every rustc invocation started from zero.
 sccache with the GitHub Actions cache backend caches the compiled objects
 themselves. It only works when two things hold in EVERY release build
 workflow: cargo must see `RUSTC_WRAPPER=sccache` (declared as job-level env so
 the build scripts inherit it without edits), and the wrapper must actually be
 on PATH before the first cargo invocation (mozilla-actions/sccache-action
 installs it; a set RUSTC_WRAPPER that is not on PATH fails the compile hard,
 it does not fall back). The action is pinned to one exact tag shared by all
 four workflows so a bump is a single deliberate change, never drift.
*/
const SCCACHE_ACTION = 'mozilla-actions/sccache-action';
const SCCACHE_ACTION_TAG = 'v0.0.11';
const SCCACHE_WRAPPER_ENV = 'RUSTC_WRAPPER';
const SCCACHE_WRAPPER_VALUE = 'sccache';
const SCCACHE_GHA_ENV = 'SCCACHE_GHA_ENABLED';
const RELEASE_BUILD_WORKFLOWS = [
  {
    // The compile step and the script it runs `cargo build` through. The
    // literal is the cargo call that inherits the job env; if it moves, the
    // assertion cannot know where the wrapper has to reach and is stale.
    compileNeedles: ['macos.sh', 'cargo '],
    contract: [{ file: 'apps/desktop/scripts/build-macos-app.sh', literal: 'cargo build --release' }],
    file: '.github/workflows/release-gpui-macos.yml',
    platform: 'macos',
  },
  {
    compileNeedles: ['windows.ps1', 'cargo '],
    contract: [{ file: 'apps/desktop/scripts/build-windows-app.ps1', literal: 'cargo build --release' }],
    file: '.github/workflows/release-gpui-windows.yml',
    platform: 'windows',
  },
  {
    compileNeedles: ['linux-stage.sh', 'cargo '],
    contract: [{ file: 'apps/desktop/scripts/build-linux-app.sh', literal: 'cargo build --release' }],
    file: '.github/workflows/release-gpui-linux.yml',
    platform: 'linux',
  },
  {
    compileNeedles: [BUILD_SCRIPT_BASENAME, 'cargo '],
    contract: [{ file: 'server/package-remote-linux.mjs', literal: "'build', '--release'" }],
    file: '.github/workflows/release-gpui-gxserver.yml',
    platform: 'gxserver',
  },
];

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
            `jobs.build runs on ${JSON.stringify(runsOn)}, not the native ARM64 runner ${ARM64_NATIVE_RUNNER}; the musl cargo build must run on real ARM64 hardware.`
          );
        }
        return ok(ARM64_NATIVE_RUNNER);
      },
    });
  }
  return assertions;
}

/*
 Job-level env is what the build scripts inherit. Workflow-level env would also
 reach them, so both are accepted; step-level env is not, because the wrapper
 has to reach every cargo invocation in the job, not one step's.
*/
function effectiveJobEnv(document, job) {
  const workflowEnv = document?.env && typeof document.env === 'object' ? document.env : {};
  const jobEnv = job?.env && typeof job.env === 'object' ? job.env : {};
  return { ...workflowEnv, ...jobEnv };
}

function stepsUsing(steps, actionName) {
  return steps
    .map((step, index) => ({ index, step }))
    .filter(({ step }) => step && typeof step.uses === 'string' && step.uses.split('@')[0] === actionName);
}

export function sccacheWorkflowAssertions({ compileNeedles, contract, file, platform }) {
  const shared = { file, platform };
  return [
    {
      ...shared,
      contract,
      id: `sccache/${platform}/wrapper-env`,
      requirement: `jobs.build declares ${SCCACHE_WRAPPER_ENV}: ${SCCACHE_WRAPPER_VALUE} and ${SCCACHE_GHA_ENV}: "true" at job level so every cargo build is wrapped by sccache`,
      verify(document) {
        const build = jobSteps(document, 'build');
        if (!build) {
          return stale(`${file} has no jobs.build; the assertion navigates by that job name.`);
        }
        const env = effectiveJobEnv(document, build.job);
        const missing = [SCCACHE_WRAPPER_ENV, SCCACHE_GHA_ENV].filter((name) => !Object.hasOwn(env, name));
        if (missing.length > 0) {
          return regressed(
            REGRESSION_ABSENT,
            `jobs.build env lacks ${missing.join(' and ')}, so its cargo builds compile cold instead of through the sccache GitHub Actions cache.`
          );
        }
        const wrong = [];
        // Parsed, so `true`, `'true'`, and `"true"` are the same value by construction.
        if (String(env[SCCACHE_WRAPPER_ENV]).trim() !== SCCACHE_WRAPPER_VALUE) {
          wrong.push(
            `${SCCACHE_WRAPPER_ENV}=${JSON.stringify(env[SCCACHE_WRAPPER_ENV])} (expected ${SCCACHE_WRAPPER_VALUE})`
          );
        }
        if (String(env[SCCACHE_GHA_ENV]).trim() !== 'true') {
          wrong.push(`${SCCACHE_GHA_ENV}=${JSON.stringify(env[SCCACHE_GHA_ENV])} (expected true)`);
        }
        if (wrong.length > 0) {
          return regressed(REGRESSION_VALUE, `jobs.build env sets ${wrong.join('; ')}.`);
        }
        return ok(`${SCCACHE_WRAPPER_ENV}=${SCCACHE_WRAPPER_VALUE} ${SCCACHE_GHA_ENV}=true`);
      },
    },
    {
      ...shared,
      contract,
      id: `sccache/${platform}/action-pin`,
      requirement: `jobs.build installs ${SCCACHE_ACTION}@${SCCACHE_ACTION_TAG} before its first cargo-running step`,
      verify(document) {
        const build = jobSteps(document, 'build');
        if (!build) {
          return stale(`${file} has no jobs.build; the assertion navigates by that job name.`);
        }
        const compileIndex = build.steps.findIndex(
          (step) => step && typeof step.run === 'string' && compileNeedles.some((needle) => step.run.includes(needle))
        );
        if (compileIndex === -1) {
          return stale(
            `${file} has no jobs.build step whose run mentions ${compileNeedles.map((needle) => JSON.stringify(needle)).join(' or ')}, so the compile step sccache must precede cannot be located; update compileNeedles for ${platform}.`
          );
        }
        const installs = stepsUsing(build.steps, SCCACHE_ACTION);
        if (installs.length === 0) {
          return regressed(
            REGRESSION_ABSENT,
            `no jobs.build step uses ${SCCACHE_ACTION}; with ${SCCACHE_WRAPPER_ENV} set and no sccache on PATH the cargo build fails outright.`
          );
        }
        const refs = installs.map(({ step }) => step.uses.split('@')[1] ?? '');
        const unpinned = refs.filter((ref) => ref !== SCCACHE_ACTION_TAG);
        if (unpinned.length > 0) {
          return regressed(
            REGRESSION_VALUE,
            `uses ${SCCACHE_ACTION}@${unpinned.join(', @')} instead of @${SCCACHE_ACTION_TAG}; every release build workflow must pin the same exact tag (bump SCCACHE_ACTION_TAG in tooling/release-workflow-assertions.mjs together with all four workflows).`
          );
        }
        const late = installs.filter(({ index }) => index > compileIndex);
        if (late.length === installs.length) {
          return regressed(
            REGRESSION_VALUE,
            `${describeStep(installs[0].step, installs[0].index)} runs after ${describeStep(build.steps[compileIndex], compileIndex)}, so sccache is not on PATH when cargo first runs.`
          );
        }
        return ok(`${SCCACHE_ACTION}@${SCCACHE_ACTION_TAG}`);
      },
    },
  ];
}

/*
 CDXC:Release 2026-09-02:
 The sccache cache is only useful when it is warm at release dispatch time, so
 .github/workflows/warm-rust-build-cache.yml recompiles each release target
 after every cargo-relevant push. An sccache key covers the rustc version and
 arguments, the CARGO_* env, the cwd, and the dep-info env-deps, so the warm
 job must run the SAME compile on the SAME runner with the SAME job env as the
 release job it mirrors - any drift produces different keys, zero hits, and no
 error anywhere. Nothing at run time can detect that, so these assertions hold
 the two files together: runner label, SCCACHE_GHA_VERSION namespace, every
 cargo-affecting job env value, the sccache action tag, the compile entry
 point, and (for the one job that has to run cargo directly because its
 release script cannot reach cargo without non-Rust artifacts) the duplicated
 cargo command against the script line it mirrors.
*/
export const WARM_CACHE_WORKFLOW = '.github/workflows/warm-rust-build-cache.yml';
const WARM_CACHE_CONCURRENCY_GROUP = 'warm-rust-build-cache';
/*
 Job-level env keys that reach cargo/rustc and therefore the sccache key (or
 that decide whether sccache is used at all). Any of these present in the
 release job must be byte-identical in the warm job after `${{ inputs.arch }}`
 substitution, and absent from the warm job when absent from the release job.
*/
const CARGO_AFFECTING_JOB_ENV = [
  'SCCACHE_GHA_ENABLED',
  'SCCACHE_GHA_VERSION',
  'RUSTC_WRAPPER',
  'CARGO_INCREMENTAL',
  'CEF_PATH',
  'ZIG_GLOBAL_CACHE_DIR',
];
const CARGO_AFFECTING_ENV_PREFIXES = [
  'CARGO_',
  'RUSTC',
  'RUSTFLAGS',
  'SCCACHE_',
  'CC_',
  'CXX_',
  'AR_',
  'CFLAGS',
  'CXXFLAGS',
];
const ARCH_INPUT = /\$\{\{\s*inputs\.arch\s*\}\}/g;
const ARCH_RUNNER_EXPRESSION = /^\$\{\{\s*inputs\.arch == '([^']+)' && '([^']+)' \|\| '([^']+)'\s*\}\}$/;
const releaseBuildWorkflow = (platform) => RELEASE_BUILD_WORKFLOWS.find((workflow) => workflow.platform === platform);
const WARM_CACHE_JOBS = [
  {
    arch: null,
    compileNeedles: ['macos.sh --phase build-server', 'macos.sh --phase build-desktop'],
    job: 'macos_arm64',
    release: releaseBuildWorkflow('macos'),
  },
  {
    arch: 'x64',
    compileNeedles: ['windows.ps1 -Version $env:WARM_MARKETING_VERSION -Arch x64 -Phase compile'],
    job: 'windows_x64',
    release: releaseBuildWorkflow('windows'),
  },
  {
    arch: 'arm64',
    compileNeedles: ['windows.ps1 -Version $env:WARM_MARKETING_VERSION -Arch arm64 -Phase compile'],
    job: 'windows_arm64',
    release: releaseBuildWorkflow('windows'),
  },
  {
    arch: null,
    // linux-stage.sh refuses to start without the gxserver package and the
    // code-server archive, so the warm job runs the cargo command itself.
    duplicatedCargo: {
      command: 'cargo build --release --bins',
      contract: [
        { file: 'apps/desktop/scripts/build-linux-app.sh', literal: 'cd "$GPUI_DIR"\n\tcargo build --release --bins' },
        { file: 'tooling/release-gpui/linux-stage.sh', literal: 'GHOSTEX_GPUI_MARKETING_VERSION="$VERSION"' },
      ],
      requiredStepEnv: ['GHOSTEX_GPUI_MARKETING_VERSION'],
      workingDirectory: 'apps/desktop',
    },
    job: 'linux_x64',
    release: releaseBuildWorkflow('linux'),
  },
  {
    arch: 'x64',
    compileNeedles: ['build-remote-gxserver-linux-release.sh --arch x64'],
    job: 'gxserver_linux_x64',
    release: releaseBuildWorkflow('gxserver'),
  },
  {
    arch: 'arm64',
    compileNeedles: ['build-remote-gxserver-linux-release.sh --arch arm64'],
    job: 'gxserver_linux_arm64',
    release: releaseBuildWorkflow('gxserver'),
  },
];

function substituteArch(value, arch) {
  const text = String(value);
  if (arch === null) {
    return text;
  }
  return text.replace(ARCH_INPUT, arch);
}

function resolveReleaseRunner(runsOn, arch) {
  if (typeof runsOn !== 'string') {
    return { unresolvable: `runs-on is ${JSON.stringify(runsOn)}` };
  }
  if (!runsOn.includes('${{')) {
    return { label: runsOn };
  }
  const match = ARCH_RUNNER_EXPRESSION.exec(runsOn.trim());
  if (!match || arch === null) {
    return {
      unresolvable: `runs-on expression ${JSON.stringify(runsOn)} is not the inputs.arch ternary this assertion evaluates`,
    };
  }
  return { label: arch === match[1] ? match[2] : match[3] };
}

function warmCacheJobAssertions({ arch, compileNeedles, duplicatedCargo, job, release }) {
  const file = WARM_CACHE_WORKFLOW;
  const shared = { file, platform: `warm/${job}`, related: [release.file] };
  const target = `${release.file} jobs.build${arch === null ? '' : ` (arch ${arch})`}`;
  const locate = (document, related) => {
    const warm = jobSteps(document, job);
    if (!warm) {
      return {
        problem: regressed(
          REGRESSION_ABSENT,
          `${file} has no jobs.${job}, so the ${target} sccache namespace is never warmed.`
        ),
      };
    }
    const build = jobSteps(related[release.file], 'build');
    if (!build) {
      return { problem: stale(`${release.file} has no jobs.build; the assertion navigates by that job name.`) };
    }
    return { build, warm };
  };
  const compileIndexOf = (steps) =>
    steps.findIndex(
      (step) =>
        step &&
        typeof step.run === 'string' &&
        (duplicatedCargo
          ? step.run.trim() === duplicatedCargo.command
          : compileNeedles.some((needle) => step.run.includes(needle)))
    );
  return [
    {
      ...shared,
      contract: [],
      id: `warm-cache/${job}/runner`,
      requirement: `jobs.${job} runs on the same runner label as ${target}`,
      verify(document, related) {
        const { problem, build, warm } = locate(document, related);
        if (problem) {
          return problem;
        }
        const expected = resolveReleaseRunner(build.job['runs-on'], arch);
        if (expected.unresolvable) {
          return stale(`${release.file} jobs.build ${expected.unresolvable}; update resolveReleaseRunner.`);
        }
        const actual = warm.job['runs-on'];
        if (actual === undefined) {
          return regressed(REGRESSION_ABSENT, `jobs.${job} declares no runs-on.`);
        }
        if (actual !== expected.label) {
          return regressed(
            REGRESSION_VALUE,
            `jobs.${job} runs on ${JSON.stringify(actual)} but ${target} runs on ${expected.label}; a different runner image compiles with a different toolchain and warms nothing the release can hit.`
          );
        }
        return ok(expected.label);
      },
    },
    {
      ...shared,
      contract: [],
      id: `warm-cache/${job}/cargo-env`,
      requirement: `jobs.${job} job env matches ${target} for every cargo-affecting variable (${CARGO_AFFECTING_JOB_ENV.join(', ')}) and adds none`,
      verify(document, related) {
        const { problem, build, warm } = locate(document, related);
        if (problem) {
          return problem;
        }
        const releaseEnv = effectiveJobEnv(related[release.file], build.job);
        const warmEnv = effectiveJobEnv(document, warm.job);
        const missing = [];
        const wrong = [];
        const extra = [];
        for (const name of CARGO_AFFECTING_JOB_ENV) {
          const inRelease = Object.hasOwn(releaseEnv, name);
          const inWarm = Object.hasOwn(warmEnv, name);
          if (inRelease && !inWarm) {
            missing.push(name);
            continue;
          }
          if (!inRelease && inWarm) {
            extra.push(name);
            continue;
          }
          if (!inRelease) {
            continue;
          }
          const expected = substituteArch(releaseEnv[name], arch);
          if (expected.includes('${{ inputs.')) {
            return stale(
              `${release.file} jobs.build env ${name}=${JSON.stringify(releaseEnv[name])} uses an input this assertion cannot resolve.`
            );
          }
          const actual = String(warmEnv[name]);
          if (actual !== expected) {
            wrong.push(`${name}=${JSON.stringify(actual)} (release: ${JSON.stringify(expected)})`);
          }
        }
        for (const name of Object.keys(warmEnv)) {
          if (
            !CARGO_AFFECTING_JOB_ENV.includes(name) &&
            !Object.hasOwn(releaseEnv, name) &&
            CARGO_AFFECTING_ENV_PREFIXES.some((prefix) => name.startsWith(prefix))
          ) {
            extra.push(name);
          }
        }
        if (missing.length > 0) {
          return regressed(
            REGRESSION_ABSENT,
            `jobs.${job} env lacks ${missing.join(', ')}, which ${target} sets; the warm compile keys differently from the release compile.`
          );
        }
        if (wrong.length > 0) {
          return regressed(REGRESSION_VALUE, `jobs.${job} env sets ${wrong.join('; ')}.`);
        }
        if (extra.length > 0) {
          return regressed(
            REGRESSION_VALUE,
            `jobs.${job} env sets ${extra.join(', ')}, which ${target} does not; the warm compile keys differently from the release compile.`
          );
        }
        return ok(`SCCACHE_GHA_VERSION=${warmEnv.SCCACHE_GHA_VERSION}`);
      },
    },
    {
      ...shared,
      contract: [],
      id: `warm-cache/${job}/action-pin`,
      requirement: `jobs.${job} installs ${SCCACHE_ACTION}@${SCCACHE_ACTION_TAG} before its compile step`,
      verify(document, related) {
        const { problem, warm } = locate(document, related);
        if (problem) {
          return problem;
        }
        const compileIndex = compileIndexOf(warm.steps);
        if (compileIndex === -1) {
          return stale(
            `${file} jobs.${job} has no step matching the compile entry point this assertion looks for; update WARM_CACHE_JOBS.`
          );
        }
        const installs = stepsUsing(warm.steps, SCCACHE_ACTION);
        if (installs.length === 0) {
          return regressed(
            REGRESSION_ABSENT,
            `no jobs.${job} step uses ${SCCACHE_ACTION}; with ${SCCACHE_WRAPPER_ENV} set and no sccache on PATH the compile fails outright.`
          );
        }
        const unpinned = installs
          .map(({ step }) => step.uses.split('@')[1] ?? '')
          .filter((ref) => ref !== SCCACHE_ACTION_TAG);
        if (unpinned.length > 0) {
          return regressed(
            REGRESSION_VALUE,
            `jobs.${job} uses ${SCCACHE_ACTION}@${unpinned.join(', @')} instead of the release workflows' @${SCCACHE_ACTION_TAG}.`
          );
        }
        if (installs.every(({ index }) => index > compileIndex)) {
          return regressed(
            REGRESSION_VALUE,
            `jobs.${job} installs sccache after its compile step, so the compile runs without it.`
          );
        }
        return ok(`${SCCACHE_ACTION}@${SCCACHE_ACTION_TAG}`);
      },
    },
    {
      ...shared,
      contract: duplicatedCargo ? duplicatedCargo.contract : release.contract,
      id: `warm-cache/${job}/compile-entry-point`,
      requirement: duplicatedCargo
        ? `jobs.${job} runs the duplicated \`${duplicatedCargo.command}\` from ${duplicatedCargo.workingDirectory} with ${duplicatedCargo.requiredStepEnv.join(', ')}, mirroring the cited script line`
        : `jobs.${job} runs the release compile entry point (${compileNeedles.join(' and ')})`,
      verify(document, related) {
        const { problem, warm } = locate(document, related);
        if (problem) {
          return problem;
        }
        if (duplicatedCargo) {
          const matches = warm.steps.filter(
            (step) => step && typeof step.run === 'string' && step.run.trim() === duplicatedCargo.command
          );
          if (matches.length === 0) {
            return regressed(REGRESSION_ABSENT, `no jobs.${job} step runs exactly \`${duplicatedCargo.command}\`.`);
          }
          const step = matches[0];
          if (step['working-directory'] !== duplicatedCargo.workingDirectory) {
            return regressed(
              REGRESSION_VALUE,
              `${describeStep(step, warm.steps.indexOf(step))} runs from ${JSON.stringify(step['working-directory'])} instead of ${duplicatedCargo.workingDirectory}; sccache hashes the cwd.`
            );
          }
          const env = step.env && typeof step.env === 'object' ? step.env : {};
          const missingEnv = duplicatedCargo.requiredStepEnv.filter((name) => !Object.hasOwn(env, name));
          if (missingEnv.length > 0) {
            return regressed(
              REGRESSION_ABSENT,
              `${describeStep(step, warm.steps.indexOf(step))} lacks ${missingEnv.join(', ')}, which the release script sets for the same cargo build.`
            );
          }
          return ok(`${duplicatedCargo.workingDirectory}: ${duplicatedCargo.command}`);
        }
        const missing = compileNeedles.filter(
          (needle) => !warm.steps.some((step) => step && typeof step.run === 'string' && step.run.includes(needle))
        );
        if (missing.length > 0) {
          return regressed(
            REGRESSION_ABSENT,
            `no jobs.${job} step runs ${missing.map((needle) => JSON.stringify(needle)).join(' or ')}.`
          );
        }
        return ok(compileNeedles.join(' + '));
      },
    },
    {
      ...shared,
      contract: [],
      id: `warm-cache/${job}/sccache-stats`,
      requirement: `jobs.${job} always reports sccache --show-stats`,
      verify(document, related) {
        const { problem, warm } = locate(document, related);
        if (problem) {
          return problem;
        }
        const stats = warm.steps.filter(
          (step) => step && typeof step.run === 'string' && step.run.includes('--show-stats')
        );
        if (stats.length === 0) {
          return regressed(
            REGRESSION_ABSENT,
            `no jobs.${job} step runs sccache --show-stats, so hit rates are invisible.`
          );
        }
        if (!stats.some((step) => typeof step.if === 'string' && step.if.includes('always()'))) {
          return regressed(
            REGRESSION_VALUE,
            `jobs.${job} reports sccache statistics without an always() guard, so a failed compile hides its hit rate.`
          );
        }
        return ok('sccache --show-stats');
      },
    },
  ];
}

export function warmCacheWorkflowAssertions() {
  const file = WARM_CACHE_WORKFLOW;
  return [
    {
      contract: [],
      file,
      id: 'warm-cache/workflow-shape',
      platform: 'warm',
      related: [],
      requirement: `${file} is push+schedule+dispatch triggered on every release build workflow and compile script, serialised under concurrency group ${WARM_CACHE_CONCURRENCY_GROUP}, with contents: read only`,
      verify(document) {
        const triggers = document?.on && typeof document.on === 'object' ? document.on : null;
        if (!triggers) {
          return regressed(REGRESSION_ABSENT, `${file} declares no triggers.`);
        }
        const missingTriggers = ['push', 'schedule', 'workflow_dispatch'].filter(
          (name) => !Object.hasOwn(triggers, name)
        );
        if (missingTriggers.length > 0) {
          return regressed(REGRESSION_ABSENT, `${file} lacks the ${missingTriggers.join(', ')} trigger(s).`);
        }
        const paths = Array.isArray(triggers.push?.paths) ? triggers.push.paths.map(String) : [];
        const watched = [
          ...RELEASE_BUILD_WORKFLOWS.map((workflow) => workflow.file),
          file,
          'tooling/release-gpui/macos.sh',
          'tooling/release-gpui/windows.ps1',
          'tooling/release-gpui/linux-stage.sh',
          GXSERVER_BUILD_SCRIPT,
        ];
        const unwatched = watched.filter((path) => !paths.includes(path));
        if (unwatched.length > 0) {
          return regressed(
            REGRESSION_ABSENT,
            `on.push.paths does not include ${unwatched.join(', ')}, so a compile-env change there is not re-warmed.`
          );
        }
        const concurrency =
          document.concurrency && typeof document.concurrency === 'object' ? document.concurrency : {};
        if (concurrency.group !== WARM_CACHE_CONCURRENCY_GROUP || concurrency['cancel-in-progress'] !== true) {
          return regressed(
            REGRESSION_VALUE,
            `concurrency is ${JSON.stringify(document.concurrency)}; expected group ${WARM_CACHE_CONCURRENCY_GROUP} with cancel-in-progress: true so back-to-back pushes do not pile up runners.`
          );
        }
        const permissions = document.permissions;
        if (
          !permissions ||
          typeof permissions !== 'object' ||
          Object.keys(permissions).length !== 1 ||
          permissions.contents !== 'read'
        ) {
          return regressed(
            REGRESSION_VALUE,
            `permissions is ${JSON.stringify(permissions)}; expected exactly {contents: read}.`
          );
        }
        return ok('push+schedule+workflow_dispatch, concurrency-serialised, contents: read');
      },
    },
    ...WARM_CACHE_JOBS.flatMap((warmJob) => warmCacheJobAssertions(warmJob)),
  ];
}

export function releaseWorkflowAssertions() {
  return [
    ...gxserverLinuxWorkflowAssertions('x64'),
    ...gxserverLinuxWorkflowAssertions('arm64'),
    ...RELEASE_BUILD_WORKFLOWS.flatMap((workflow) => sccacheWorkflowAssertions(workflow)),
    ...warmCacheWorkflowAssertions(),
  ];
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
    const load = async (file) => {
      if (!parsed.has(file)) {
        const source = Object.hasOwn(sources, file) ? sources[file] : await readIfPresent(path.join(repoRoot, file));
        if (source === null) {
          parsed.set(file, { missing: true });
        } else {
          try {
            parsed.set(file, { document: parseWorkflowYaml(source) });
          } catch (error) {
            parsed.set(file, {
              parseError: error instanceof WorkflowYamlError ? error.message : String(error?.message ?? error),
            });
          }
        }
      }
      return parsed.get(file);
    };
    const entry = await load(assertion.file);
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
    /*
     Assertions that compare two workflows (the cache-warming jobs against the
     release jobs they mirror) declare the other files as `related`. A missing
     or unparseable related file makes the comparison unrunnable, which is a
     stale check, not a verdict about the file under test.
    */
    const related = {};
    let relatedProblem = null;
    for (const relatedFile of assertion.related ?? []) {
      const relatedEntry = await load(relatedFile);
      if (relatedEntry.missing) {
        relatedProblem = `${relatedFile} does not exist, so ${assertion.file} cannot be compared against it`;
        break;
      }
      if (relatedEntry.parseError) {
        relatedProblem = `${relatedFile} could not be parsed: ${relatedEntry.parseError}`;
        break;
      }
      related[relatedFile] = relatedEntry.document;
    }
    if (relatedProblem) {
      results.push({
        ...stale(relatedProblem),
        file: assertion.file,
        id: assertion.id,
        requirement: assertion.requirement,
      });
      continue;
    }
    let outcome;
    try {
      outcome = assertion.verify(entry.document, related);
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
