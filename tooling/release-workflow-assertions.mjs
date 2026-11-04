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
    live in other files (the env var the build script reads, the cargo call the
    build script makes). If a contract literal is gone, the assertion
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
const BUILD_SCRIPT_BASENAME = 'build-remote-gxserver-linux-release.sh';

/*
 CDXC:Release 2026-09-02:
 Measured on the 8.5.0 release run: the macOS job took 31 minutes and was the
 critical path, and a cold `cargo build --release` was roughly half of it
 (Linux desktop: 663 s inside "Stage the Linux desktop payload once";
 gxserver x64: 7 min). The workflows cached only ~/.cargo/registry and
 ~/.cargo/git - crate *sources* - so every rustc invocation started from zero.
 sccache with the GitHub Actions cache backend cached the compiled objects
 themselves until 2026-11-04.

 CDXC:ReleaseKache 2026-11-04: the user switched every Rust build, CI included,
 from sccache to kache. Each release build job restores its kache store from
 the Actions cache (actions/cache/restore, key kache-<KACHE_STORE_SET>-<sha>,
 newest-first restore-keys), then kunobi-ninja/kache-action installs kache
 v2.0.0 with its own GitHub cache off and exports RUSTC_WRAPPER for every later
 step. The warm jobs save the store they extended; release jobs only restore.
 The action is pinned to one commit and kache to one version shared by all
 four workflows, so a bump is a single deliberate change, never drift.
*/
const KACHE_ACTION = 'kunobi-ninja/kache-action';
const KACHE_ACTION_REF = 'ad7317540dcdd71c904f7a60a87b620a0c0e67ed';
const KACHE_VERSION = 'v2.0.0';
const KACHE_STORE_ENV = 'KACHE_STORE_SET';
const KACHE_STORE_PATH = '${{ runner.temp }}/kache';
const KACHE_STORE_KEY_PREFIX = 'kache-${{ env.KACHE_STORE_SET }}-';
const KACHE_STORE_KEY = 'kache-${{ env.KACHE_STORE_SET }}-${{ github.sha }}';
const RETIRED_WRAPPER_ENV = ['RUSTC_WRAPPER', 'SCCACHE_GHA_ENABLED', 'SCCACHE_GHA_VERSION'];
const RELEASE_BUILD_WORKFLOWS = [
  {
    // The compile step and the script it runs `cargo build` through. The
    // literal is the cargo call that inherits the job env; if it moves, the
    // assertion cannot know where the wrapper has to reach and is stale. Since
    // 2026-09-04 the macOS cargo call lives in build-macos-rust.sh, which
    // build-macos-app.sh runs, so both links of that chain are asserted.
    compileNeedles: ['macos.sh', 'cargo '],
    contract: [
      { file: 'apps/desktop/scripts/build-macos-app.sh', literal: '"$SCRIPT_DIR/build-macos-rust.sh"' },
      {
        file: 'apps/desktop/scripts/build-macos-rust.sh',
        literal: 'cargo_args=(build --release --bin ghostex-gpui --bin ghostex-gpui-cef-helper)',
      },
    ],
    file: '.github/workflows/release-gpui-macos.yml',
    platform: 'macos',
  },
  {
    // The compile step runs windows.ps1 through windows-build-watchdog.ps1.
    compileNeedles: ['windows.ps1', 'windows-build-watchdog.ps1', 'cargo '],
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
    contract: [{ file: 'server/package-remote-linux.mjs', literal: "run('cargo', ['build', '--release'" }],
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

function describeStep(step, index) {
  return step?.name ? `step "${step.name}"` : `step #${index + 1}`;
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

/*
 Shared check for every job that compiles through kache: a store restore, then
 the pinned kache-action with the pinned kache version, both before the first
 compile step. Returns null when the setup is right, else an outcome.
*/
function kacheSetupProblem(steps, compileIndex, where) {
  const installs = stepsUsing(steps, KACHE_ACTION);
  if (installs.length === 0) {
    return regressed(
      REGRESSION_ABSENT,
      `no ${where} step uses ${KACHE_ACTION}, so cargo runs without kache (or fails on a wrapper that is not installed).`
    );
  }
  const install = installs[0];
  const ref = install.step.uses.split('@')[1] ?? '';
  const settings = install.step.with && typeof install.step.with === 'object' ? install.step.with : {};
  const wrong = [];
  if (ref !== KACHE_ACTION_REF) wrong.push(`${KACHE_ACTION}@${ref} (expected @${KACHE_ACTION_REF})`);
  if (String(settings.version ?? '').trim() !== KACHE_VERSION) {
    wrong.push(`version ${JSON.stringify(settings.version)} (expected ${KACHE_VERSION})`);
  }
  if (String(settings['cache-dir'] ?? '') !== KACHE_STORE_PATH) {
    wrong.push(`cache-dir ${JSON.stringify(settings['cache-dir'])} (expected ${KACHE_STORE_PATH})`);
  }
  if (String(settings['github-cache'] ?? '') !== 'false') {
    wrong.push(
      `github-cache ${JSON.stringify(settings['github-cache'])} (expected 'false'; the store is restored by actions/cache/restore)`
    );
  }
  if (wrong.length > 0) {
    return regressed(REGRESSION_VALUE, `${where} installs kache with ${wrong.join('; ')}.`);
  }
  if (install.index > compileIndex) {
    return regressed(
      REGRESSION_VALUE,
      `${describeStep(install.step, install.index)} runs after ${describeStep(steps[compileIndex], compileIndex)}, so kache is not the wrapper when cargo first runs.`
    );
  }
  const restore = stepsUsing(steps, 'actions/cache/restore').find(
    ({ step }) => String(step.with?.path ?? '') === KACHE_STORE_PATH
  );
  if (!restore) {
    return regressed(
      REGRESSION_ABSENT,
      `no ${where} step restores the kache store (actions/cache/restore of ${KACHE_STORE_PATH}), so every compile starts cold.`
    );
  }
  if (
    String(restore.step.with.key ?? '') !== KACHE_STORE_KEY ||
    String(restore.step.with['restore-keys'] ?? '').trim() !== KACHE_STORE_KEY_PREFIX
  ) {
    return regressed(
      REGRESSION_VALUE,
      `${describeStep(restore.step, restore.index)} restores key ${JSON.stringify(restore.step.with.key)} with restore-keys ${JSON.stringify(restore.step.with['restore-keys'])}; expected ${KACHE_STORE_KEY} and ${KACHE_STORE_KEY_PREFIX}.`
    );
  }
  if (restore.index > install.index) {
    return regressed(
      REGRESSION_VALUE,
      `${describeStep(restore.step, restore.index)} runs after ${describeStep(install.step, install.index)}, so the daemon starts on an empty store.`
    );
  }
  return null;
}

export function kacheWorkflowAssertions({ compileNeedles, contract, file, platform }) {
  const shared = { file, platform };
  return [
    {
      ...shared,
      contract,
      id: `kache/${platform}/store-env`,
      requirement: `jobs.build declares ${KACHE_STORE_ENV} at job level and no ${RETIRED_WRAPPER_ENV.join('/')} (kache-action exports the wrapper)`,
      verify(document) {
        const build = jobSteps(document, 'build');
        if (!build) {
          return stale(`${file} has no jobs.build; the assertion navigates by that job name.`);
        }
        const env = effectiveJobEnv(document, build.job);
        if (!Object.hasOwn(env, KACHE_STORE_ENV) || String(env[KACHE_STORE_ENV]).trim() === '') {
          return regressed(
            REGRESSION_ABSENT,
            `jobs.build env lacks ${KACHE_STORE_ENV}, so its kache store has no namespace to restore.`
          );
        }
        const retired = RETIRED_WRAPPER_ENV.filter((name) => Object.hasOwn(env, name));
        if (retired.length > 0) {
          return regressed(
            REGRESSION_VALUE,
            `jobs.build env still sets ${retired.join(', ')}; a job-level RUSTC_WRAPPER overrides the kache wrapper kache-action exports.`
          );
        }
        return ok(`${KACHE_STORE_ENV}=${env[KACHE_STORE_ENV]}`);
      },
    },
    {
      ...shared,
      contract,
      id: `kache/${platform}/action-pin`,
      requirement: `jobs.build restores the kache store, then installs ${KACHE_ACTION}@${KACHE_ACTION_REF} with kache ${KACHE_VERSION}, before its first cargo-running step`,
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
            `${file} has no jobs.build step whose run mentions ${compileNeedles.map((needle) => JSON.stringify(needle)).join(' or ')}, so the compile step kache must precede cannot be located; update compileNeedles for ${platform}.`
          );
        }
        return (
          kacheSetupProblem(build.steps, compileIndex, 'jobs.build') ??
          ok(`${KACHE_ACTION}@${KACHE_ACTION_REF} kache ${KACHE_VERSION}`)
        );
      },
    },
  ];
}

/*
 CDXC:Release 2026-09-02:
 The build cache (sccache then, kache since 2026-11-04) is only useful when it
 is warm at release dispatch time, so
 .github/workflows/warm-rust-build-cache.yml recompiles each release target
 after every cargo-relevant push and saves the kache store. A kache key covers the rustc version and
 arguments, the CARGO_* env, the cwd, and the dep-info env-deps, so the warm
 job must run the SAME compile on the SAME runner with the SAME job env as the
 release job it mirrors - any drift produces different keys, zero hits, and no
 error anywhere. Nothing at run time can detect that, so these assertions hold
 the two files together: runner label, KACHE_STORE_SET namespace, every
 cargo-affecting job env value, the kache-action pin, the compile entry
 point, and (for the one job that has to run cargo directly because its
 release script cannot reach cargo without non-Rust artifacts) the duplicated
 cargo command against the script line it mirrors.
*/
export const WARM_CACHE_WORKFLOW = '.github/workflows/warm-rust-build-cache.yml';
const WARM_CACHE_CONCURRENCY_GROUP = 'warm-rust-build-cache';
/*
 Job-level env keys that reach cargo/rustc and therefore the kache key (or
 that decide which kache store is used). Any of these present in the
 release job must be byte-identical in the warm job after `${{ inputs.arch }}`
 substitution, and absent from the warm job when absent from the release job.
*/
const CARGO_AFFECTING_JOB_ENV = [
  'KACHE_STORE_SET',
  'RUSTC_WRAPPER',
  'CARGO_INCREMENTAL',
  'CEF_PATH',
  'ZIG_GLOBAL_CACHE_DIR',
];
const CARGO_AFFECTING_ENV_PREFIXES = [
  'CARGO_',
  'RUSTC',
  'RUSTFLAGS',
  'KACHE_',
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
    // release-gpui-linux.yml builds both architectures; this warm job warms the x64 namespace.
    arch: 'x64',
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

/** @param {{ arch: string, compileNeedles?: string[], duplicatedCargo?: any, job: string, release: any }} options */
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
          `${file} has no jobs.${job}, so the ${target} kache store is never warmed.`
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
        return ok(`${KACHE_STORE_ENV}=${warmEnv[KACHE_STORE_ENV]}`);
      },
    },
    {
      ...shared,
      contract: [],
      id: `warm-cache/${job}/action-pin`,
      requirement: `jobs.${job} restores the kache store, then installs ${KACHE_ACTION}@${KACHE_ACTION_REF} with kache ${KACHE_VERSION}, before its compile step`,
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
        return (
          kacheSetupProblem(warm.steps, compileIndex, `jobs.${job}`) ??
          ok(`${KACHE_ACTION}@${KACHE_ACTION_REF} kache ${KACHE_VERSION}`)
        );
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
              `${describeStep(step, warm.steps.indexOf(step))} runs from ${JSON.stringify(step['working-directory'])} instead of ${duplicatedCargo.workingDirectory}; the release compiles from there.`
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
      id: `warm-cache/${job}/kache-stats`,
      requirement: `jobs.${job} always reports kache stats`,
      verify(document, related) {
        const { problem, warm } = locate(document, related);
        if (problem) {
          return problem;
        }
        const stats = warm.steps.filter(
          (step) => step && typeof step.run === 'string' && step.run.trim() === 'kache stats'
        );
        if (stats.length === 0) {
          return regressed(REGRESSION_ABSENT, `no jobs.${job} step runs kache stats, so hit rates are invisible.`);
        }
        if (!stats.some((step) => typeof step.if === 'string' && step.if.includes('always()'))) {
          return regressed(
            REGRESSION_VALUE,
            `jobs.${job} reports kache statistics without an always() guard, so a failed compile hides its hit rate.`
          );
        }
        return ok('kache stats');
      },
    },
    {
      ...shared,
      contract: [],
      id: `warm-cache/${job}/store-save`,
      requirement: `jobs.${job} stops kache and saves ${KACHE_STORE_PATH} under ${KACHE_STORE_KEY} after its compile step`,
      verify(document, related) {
        const { problem, warm } = locate(document, related);
        if (problem) {
          return problem;
        }
        const compileIndex = compileIndexOf(warm.steps);
        const saves = stepsUsing(warm.steps, 'actions/cache/save').filter(
          ({ step }) => String(step.with?.path ?? '') === KACHE_STORE_PATH
        );
        if (saves.length === 0) {
          return regressed(
            REGRESSION_ABSENT,
            `no jobs.${job} step saves the kache store, so the next release restores nothing this run compiled.`
          );
        }
        const save = saves[saves.length - 1];
        if (String(save.step.with.key ?? '') !== KACHE_STORE_KEY) {
          return regressed(
            REGRESSION_VALUE,
            `jobs.${job} saves the kache store as ${JSON.stringify(save.step.with.key)}; release jobs restore ${KACHE_STORE_KEY_PREFIX}*, newest first, so it must be ${KACHE_STORE_KEY}.`
          );
        }
        if (compileIndex !== -1 && save.index < compileIndex) {
          return regressed(REGRESSION_VALUE, `jobs.${job} saves the kache store before its compile step.`);
        }
        const stop = warm.steps.findIndex(
          (step, index) =>
            index < save.index && step && typeof step.run === 'string' && step.run.includes('kache daemon stop')
        );
        if (stop === -1) {
          return regressed(
            REGRESSION_ABSENT,
            `jobs.${job} saves the kache store without running kache daemon stop first, so the archived index can be mid-write.`
          );
        }
        return ok(KACHE_STORE_KEY);
      },
    },
  ];
}

/*
 CDXC:Release 2026-10-07 WHY:
 The Windows workflow compiles the native runtime (gxserver, ghostex, wmx, the
 prompt editor) in jobs.runtime on a second runner while jobs.build compiles
 the desktop app, and jobs.build packages the runtime outputs it downloads.
 Nothing at run time notices when the two jobs drift apart: a different runner
 label or cargo env compiles different bytes and misses the kache store
 the warm jobs fill, an artifact name or path mismatch only fails the packaging
 phase 25 minutes in, and a `needs: runtime` edge or a `-Phase compile` in
 jobs.build silently puts the runtime build back on the critical path. These
 assertions hold the hand-off together.
*/
const WINDOWS_WORKFLOW = '.github/workflows/release-gpui-windows.yml';
const WINDOWS_RUNTIME_ARTIFACT = 'release-windows-native-runtime-${{ inputs.arch }}';
/* Each path build-windows-app.ps1's stage half copies from a runtime build. */
const WINDOWS_RUNTIME_OUTPUTS = [
  {
    literal: '"server/target/release/gxserver.exe"',
    upload: ['server/target/release/gxserver.exe'],
  },
  { literal: '".dependencies/wmx/target/release/wmx.exe"', upload: ['.dependencies/wmx/target/release/wmx.exe'] },
  {
    literal: '"apps/editor/desktop/target/release/ghostex-editor.exe"',
    upload: ['apps/editor/desktop/target/release/ghostex-editor.exe'],
  },
  { literal: '"apps/editor/dist/web"', upload: ['apps/editor/dist/web/'] },
];

function windowsRuntimeJobs(document) {
  const build = jobSteps(document, 'build');
  const runtime = jobSteps(document, 'runtime');
  if (!build) {
    return { problem: stale(`${WINDOWS_WORKFLOW} has no jobs.build; the assertion navigates by that job name.`) };
  }
  if (!runtime) {
    return {
      problem: regressed(
        REGRESSION_ABSENT,
        `${WINDOWS_WORKFLOW} has no jobs.runtime, but jobs.build waits for its release-windows-native-runtime artifact.`
      ),
    };
  }
  return { build, runtime };
}

const runsPhase = (step, phase) =>
  step &&
  typeof step.run === 'string' &&
  step.run.includes('windows-build-watchdog.ps1') &&
  step.run.includes(`-Phase ${phase} `);

const usesArtifact = (step, action) =>
  step &&
  typeof step.uses === 'string' &&
  step.uses.split('@')[0] === action &&
  step.with?.name === WINDOWS_RUNTIME_ARTIFACT;

export function windowsNativeRuntimeAssertions() {
  const shared = { contract: [], file: WINDOWS_WORKFLOW, platform: 'windows-runtime', related: [] };
  return [
    {
      ...shared,
      id: 'windows-runtime/mirrors-build',
      requirement:
        'jobs.runtime runs on the same runs-on expression as jobs.build and matches its job env for every cargo-affecting variable',
      verify(document) {
        const { problem, build, runtime } = windowsRuntimeJobs(document);
        if (problem) {
          return problem;
        }
        const runtimeRunner = runtime.job['runs-on'];
        const buildRunner = build.job['runs-on'];
        if (runtimeRunner !== buildRunner) {
          return regressed(
            REGRESSION_VALUE,
            `jobs.runtime runs on ${JSON.stringify(runtimeRunner)} but jobs.build on ${JSON.stringify(buildRunner)}; the runtime binaries would come from a different toolchain image.`
          );
        }
        const buildEnv = effectiveJobEnv(document, build.job);
        const runtimeEnv = effectiveJobEnv(document, runtime.job);
        const affecting = (name) =>
          CARGO_AFFECTING_JOB_ENV.includes(name) ||
          CARGO_AFFECTING_ENV_PREFIXES.some((prefix) => name.startsWith(prefix));
        const names = new Set([...Object.keys(buildEnv), ...Object.keys(runtimeEnv)].filter(affecting));
        const drift = [...names].filter((name) => String(buildEnv[name]) !== String(runtimeEnv[name]));
        if (drift.length > 0) {
          const described = drift.map(
            (name) => `${name} (${JSON.stringify(runtimeEnv[name])} vs ${JSON.stringify(buildEnv[name])})`
          );
          return regressed(
            REGRESSION_VALUE,
            `jobs.runtime and jobs.build differ on ${described.join(', ')}; the runtime compile keys differently from the warm and release caches.`
          );
        }
        return ok(`runs-on and ${[...names].sort().join(', ')} match`);
      },
    },
    {
      ...shared,
      id: 'windows-runtime/action-pin',
      requirement: `jobs.runtime restores the kache store, then installs ${KACHE_ACTION}@${KACHE_ACTION_REF} with kache ${KACHE_VERSION}, before its compile-runtime step`,
      verify(document) {
        const { problem, runtime } = windowsRuntimeJobs(document);
        if (problem) {
          return problem;
        }
        const compileIndex = runtime.steps.findIndex((step) => runsPhase(step, 'compile-runtime'));
        if (compileIndex === -1) {
          return regressed(
            REGRESSION_ABSENT,
            'jobs.runtime has no step running windows.ps1 -Phase compile-runtime under the watchdog.'
          );
        }
        return (
          kacheSetupProblem(runtime.steps, compileIndex, 'jobs.runtime') ??
          ok(`${KACHE_ACTION}@${KACHE_ACTION_REF} kache ${KACHE_VERSION}`)
        );
      },
    },
    {
      ...shared,
      contract: WINDOWS_RUNTIME_OUTPUTS.map(({ literal }) => ({
        file: 'apps/desktop/scripts/build-windows-app.ps1',
        literal,
      })),
      id: 'windows-runtime/hand-off',
      requirement: `jobs.runtime uploads ${WINDOWS_RUNTIME_ARTIFACT} (hidden files included) with every runtime output the stage half copies, and jobs.build compiles only the app half, has no needs: edge on it, and awaits and downloads it into the workspace before -Phase package`,
      verify(document) {
        const { problem, build, runtime } = windowsRuntimeJobs(document);
        if (problem) {
          return problem;
        }
        const upload = runtime.steps.find((step) => usesArtifact(step, 'actions/upload-artifact'));
        if (!upload) {
          return regressed(REGRESSION_ABSENT, `jobs.runtime uploads no artifact named ${WINDOWS_RUNTIME_ARTIFACT}.`);
        }
        const uploaded = String(upload.with.path ?? '')
          .split('\n')
          .map((line) => line.trim())
          .filter(Boolean);
        const missing = WINDOWS_RUNTIME_OUTPUTS.flatMap(({ upload: paths }) => paths).filter(
          (output) => !uploaded.includes(output)
        );
        if (missing.length > 0) {
          return regressed(
            REGRESSION_ABSENT,
            `jobs.runtime does not upload ${missing.join(', ')}, which the stage half copies.`
          );
        }
        if (String(upload.with['include-hidden-files']) !== 'true') {
          return regressed(
            REGRESSION_VALUE,
            'jobs.runtime uploads without include-hidden-files: true, which drops .dependencies/wmx/target/release/wmx.exe.'
          );
        }
        if ([build.job.needs ?? []].flat().map(String).includes('runtime')) {
          return regressed(REGRESSION_VALUE, 'jobs.build needs jobs.runtime, which serialises the two compiles again.');
        }
        if (build.steps.some((step) => runsPhase(step, 'compile'))) {
          return regressed(
            REGRESSION_VALUE,
            'jobs.build runs -Phase compile, which compiles the native runtime a second time on the critical path; it runs -Phase compile-app.'
          );
        }
        const appIndex = build.steps.findIndex((step) => runsPhase(step, 'compile-app'));
        const awaitIndex = build.steps.findIndex(
          (step) =>
            step &&
            typeof step.run === 'string' &&
            step.run.includes('await-run-artifacts.mjs') &&
            step.run.includes('release-windows-native-runtime-')
        );
        const downloadIndex = build.steps.findIndex((step) => usesArtifact(step, 'actions/download-artifact'));
        const packageIndex = build.steps.findIndex((step) => runsPhase(step, 'package'));
        const absent = [
          appIndex === -1 && 'the -Phase compile-app step',
          awaitIndex === -1 && 'the await for the runtime artifact',
          downloadIndex === -1 && `the download of ${WINDOWS_RUNTIME_ARTIFACT}`,
          packageIndex === -1 && 'the -Phase package step',
        ].filter(Boolean);
        if (absent.length > 0) {
          return regressed(REGRESSION_ABSENT, `jobs.build lacks ${absent.join(', ')}.`);
        }
        if (!(appIndex < awaitIndex && awaitIndex < downloadIndex && downloadIndex < packageIndex)) {
          return regressed(
            REGRESSION_VALUE,
            'jobs.build must compile the app, then await and download the runtime artifact, then package, in that order.'
          );
        }
        const downloadPath = String(build.steps[downloadIndex].with?.path ?? '');
        if (downloadPath !== '${{ github.workspace }}') {
          return regressed(
            REGRESSION_VALUE,
            `jobs.build downloads the runtime artifact to ${JSON.stringify(downloadPath)}, not the workspace root the stage half copies from.`
          );
        }
        return ok(`${uploaded.length} runtime outputs handed from jobs.runtime to jobs.build`);
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
        if (concurrency.group !== WARM_CACHE_CONCURRENCY_GROUP || concurrency['cancel-in-progress'] !== false) {
          return regressed(
            REGRESSION_VALUE,
            `concurrency is ${JSON.stringify(document.concurrency)}; expected group ${WARM_CACHE_CONCURRENCY_GROUP} with cancel-in-progress: false, so the running warm build finishes (Windows takes 21 to 37 minutes) and only the newest waiting push queues behind it.`
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

/** @returns {Array<Record<string, any>>} */
export function releaseWorkflowAssertions() {
  return [
    ...RELEASE_BUILD_WORKFLOWS.flatMap((workflow) => kacheWorkflowAssertions(workflow)),
    ...windowsNativeRuntimeAssertions(),
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
/** @param {{ repoRoot?: string, sources?: Record<string, string> }} [options] */
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
export function preflightLiteralProbes({ sparklePublicKey }) {
  return [
    {
      file: SPARKLE_KEY_SOURCE,
      id: 'sparkle-public-key/build-script',
      literal: sparklePublicKey,
      why: 'the app stamps this key into Info.plist as SUPublicEDKey',
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
      why: 'release-gpui-gxserver.yml and the cache warmer dispatch this script with a pinned --arch',
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
