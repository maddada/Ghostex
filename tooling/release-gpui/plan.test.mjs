import { afterAll, beforeAll, describe, expect, test } from 'vitest';
import { computePlan, planSummaryLine, renderPlanText, scopeFromEnv, validatePlan } from './plan.mjs';
import { defaultScope } from './product-inputs.mjs';
import {
  componentTagStateFixture,
  createFixtureRepo,
  releaseBaselineFromPlan,
  sourceRunFromPlan,
} from './plan-test-fixtures.mjs';

const repo = createFixtureRepo();
afterAll(() => repo.dispose());

function planAt(sourceSha, options = {}) {
  return computePlan({
    entries: repo.reader.listTree(sourceSha),
    isAncestor: (candidateSha) => repo.reader.isAncestor(candidateSha, sourceSha),
    readObject: (objectId) => repo.reader.readObject(objectId),
    scope: defaultScope(),
    sourceSha,
    version: '7.7.0',
    ...options,
  });
}

function actions(plan) {
  return Object.fromEntries(Object.entries(plan.products).map(([id, entry]) => [id, entry.action]));
}

/*
 * The scenarios mutate one shared fixture repository in sequence, so each one
 * builds its own baseline from the commit immediately before its change.
 */
function baselineAt(commit, version = '7.7.0') {
  return releaseBaselineFromPlan({ commit, plan: planAt(commit, { version }), version });
}

const baseCommit = repo.head;
/* The bootstrap release: no provenance baseline exists, so everything builds. */
const bootstrapPlan = planAt(baseCommit, { version: '7.7.0' });

describe('bootstrap release', () => {
  test('builds every in-scope product when no provenance baseline exists', () => {
    expect(new Set(Object.values(actions(bootstrapPlan)))).toEqual(new Set(['build']));
    for (const entry of Object.values(bootstrapPlan.products)) {
      expect(entry.reason).toBe('no provenance baseline for this product; building');
    }
    expect(bootstrapPlan.expectedPlatforms).toHaveLength(11);
    expect(bootstrapPlan.jobs.reuse_matrix).toEqual([]);
    expect(bootstrapPlan.jobs.windows_x64).toBe('build');
    expect(bootstrapPlan.jobs.windows_arm64).toBe('build');
    expect(bootstrapPlan.jobs.linux_packages).toEqual(['deb', 'rpm', 'tar']);
  });
});

describe('Scenario A — desktop-only change', () => {
  const baselineRelease = baselineAt(repo.head);
  const sourceSha = (() => {
    repo.write('apps/desktop/src/main.rs', 'fn main() { println!("desktop change"); }\n');
    return repo.commit('desktop-only change');
  })();
  const plan = planAt(sourceSha, { baselines: [baselineRelease], version: '7.8.0' });

  test('rebuilds the desktop products and reuses Android and both gxservers', () => {
    expect(actions(plan)).toEqual({
      android: 'reuse',
      'gxserver-linux-arm64': 'reuse',
      'gxserver-linux-x64': 'reuse',
      'gxserver-wsl-windows-arm64': 'build',
      'gxserver-wsl-windows-x64': 'build',
      'linux-deb-x64': 'build',
      'linux-rpm-x64': 'build',
      'linux-tar-x64': 'build',
      'macos-arm64': 'build',
      'windows-arm64': 'build',
      'windows-x64': 'build',
    });
    expect(plan.products.android.reuse).toMatchObject({
      artifacts: [{ name: 'ghostex-android.apk' }],
      productVersion: '7.7.0',
      tag: 'v7.7.0',
      tier: 'release',
    });
    expect(plan.products.android.reason).toMatch(/all relevant inputs match v7\.7\.0/u);
    expect(plan.products['macos-arm64'].reason).toMatch(/apps\/desktop\/\*\*/u);
  });

  test('still publishes every in-scope product and reports the reuse jobs', () => {
    expect(plan.expectedPlatforms).toHaveLength(11);
    expect(plan.jobs.reuse_matrix).toEqual(['gxserver-linux-x64', 'gxserver-linux-arm64', 'android']);
    expect(plan.jobs.gxserver_x64).toBe('reuse');
    expect(plan.jobs.android).toBe('reuse');
    expect(plan.estimates.savedRunnerMinutes).toBeGreaterThan(50);
  });

  test('advances Sparkle, Homebrew, and both Windows feeds because macOS is built', () => {
    expect(plan.feeds).toEqual({ homebrew: true, sparkle: true, windowsFeeds: ['x64', 'arm64'] });
  });

  test('explains the version-stamped WSL rebuild without claiming a source change', () => {
    expect(plan.products['gxserver-wsl-windows-x64'].reason).toMatch(/version-stamped payload/u);
  });
});

describe('Scenario B — Android-only change', () => {
  const previousCommit = repo.head;
  const baselineRelease = baselineAt(previousCommit);
  const sourceSha = (() => {
    repo.setGitlink('apps/mobile/app', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
    return repo.commit('mobile submodule bump');
  })();

  test('builds Android and reuses both gxservers', () => {
    const plan = planAt(sourceSha, { baselines: [baselineRelease], version: '7.8.0' });
    expect(plan.products.android.action).toBe('build');
    expect(plan.products.android.reason).toMatch(/apps\/mobile\/app/u);
    expect(plan.products['gxserver-linux-x64'].action).toBe('reuse');
    expect(plan.products['gxserver-linux-arm64'].action).toBe('reuse');
  });

  test('rebuilds the version-stamped desktop products only because the version moved', () => {
    const plan = planAt(sourceSha, { baselines: [baselineRelease], version: '7.8.0' });
    for (const product of ['macos-arm64', 'linux-deb-x64', 'windows-x64', 'gxserver-wsl-windows-arm64']) {
      expect(plan.products[product].action).toBe('build');
      expect(plan.products[product].reason).toMatch(/version-stamped payload/u);
    }
  });

  test('reuses unchanged desktop packages inside the same version', () => {
    const plan = planAt(sourceSha, { baselines: [baselineRelease], version: '7.7.0' });
    expect(plan.products['linux-deb-x64'].action).toBe('reuse');
    expect(plan.products['windows-x64'].action).toBe('reuse');
    expect(plan.products.android.action).toBe('build');
    /* macOS ships appcast.xml beside its manifest, so it is never rebuilt from a release. */
    expect(plan.products['macos-arm64'].action).toBe('build');
    expect(plan.products['macos-arm64'].reason).toMatch(/publishes side files/u);
  });
});

describe('Scenario C — gxserver-only change', () => {
  const baselineRelease = baselineAt(repo.head);
  const sourceSha = (() => {
    repo.setGitlink('.dependencies/zmx', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb');
    return repo.commit('zmx submodule bump');
  })();
  const plan = planAt(sourceSha, { baselines: [baselineRelease], version: '7.7.0' });

  test('rebuilds gxserver and everything that embeds it, and nothing else', () => {
    expect(plan.products['gxserver-linux-x64'].action).toBe('build');
    expect(plan.products['gxserver-linux-arm64'].action).toBe('build');
    expect(plan.products['gxserver-wsl-windows-x64'].action).toBe('build');
    expect(plan.products.android.action).toBe('reuse');
  });

  test('names the embedded payload as the reason a desktop package rebuilds', () => {
    expect(plan.products['linux-deb-x64'].action).toBe('build');
    expect(plan.products['linux-deb-x64'].reason).toMatch(/embedded gxserver-linux-x64/u);
    expect(plan.products['windows-arm64'].reason).toMatch(/embedded gxserver-linux-arm64/u);
    expect(plan.products['gxserver-linux-x64'].reason).toMatch(/zmx/u);
  });
});

describe('Scenario D — Windows-only fix after a partial failure', () => {
  const failedCommit = repo.commit('checkpoint before the windows fix');
  const failedPlan = planAt(failedCommit, { version: '7.8.0' });
  const sourceRun = sourceRunFromPlan({ headSha: failedCommit, plan: failedPlan, version: '7.8.0' });
  const fixedCommit = (() => {
    repo.write('tooling/release-gpui/windows.ps1', '# windows\n# qualify gpui::SharedString\n');
    return repo.commit('windows-only compile fix');
  })();

  test('rebuilds only Windows and reuses every unaffected product across the new SHA', () => {
    const plan = planAt(fixedCommit, { reuseFromRunId: sourceRun.runId, sourceRun, version: '7.8.0' });
    expect(actions(plan)).toEqual({
      android: 'reuse',
      'gxserver-linux-arm64': 'reuse',
      'gxserver-linux-x64': 'reuse',
      'gxserver-wsl-windows-arm64': 'reuse',
      'gxserver-wsl-windows-x64': 'reuse',
      'linux-deb-x64': 'reuse',
      'linux-rpm-x64': 'reuse',
      'linux-tar-x64': 'reuse',
      'macos-arm64': 'reuse',
      'windows-arm64': 'build',
      'windows-x64': 'build',
    });
    expect(plan.products['windows-x64'].reason).toMatch(/windows\.ps1/u);
    expect(plan.products['macos-arm64'].reuse).toMatchObject({ tier: 'run', runId: sourceRun.runId });
    expect(plan.jobs.windows_x64).toBe('build');
    expect(plan.jobs.macos).toBe('reuse');
  });

  test('still rebuilds every desktop product when the fix lands in shared GPUI source', () => {
    repo.write('apps/desktop/src/main.rs', 'fn main() { /* shared fix */ }\n');
    const sharedFixCommit = repo.commit('shared source fix');
    const plan = planAt(sharedFixCommit, { reuseFromRunId: sourceRun.runId, sourceRun, version: '7.8.0' });
    for (const product of ['macos-arm64', 'linux-deb-x64', 'linux-rpm-x64', 'windows-x64', 'windows-arm64']) {
      expect(plan.products[product].action).toBe('build');
    }
    expect(plan.products.android.action).toBe('reuse');
    expect(plan.products['gxserver-linux-x64'].action).toBe('reuse');
  });

  test('refuses a source run that does not match the requested reuse run', () => {
    expect(() => planAt(fixedCommit, { reuseFromRunId: 42, sourceRun, version: '7.8.0' })).toThrow(
      /does not match the supplied source run/u
    );
  });

  test('falls back to building when the source run artifacts have expired', () => {
    const expiredRun = { ...sourceRun, expiredArtifacts: ['release-android'] };
    const plan = planAt(fixedCommit, { sourceRun: expiredRun, version: '7.8.0' });
    expect(plan.products.android.action).toBe('build');
    expect(plan.products.android.reason).toMatch(/artifacts have expired/u);
  });

  /*
   * 7.8.0 regression guard: the recovery run is dispatched against a *failed*
   * run, and the whole point of `--reuse-from-run` is that the products whose
   * jobs succeeded inside it stay reusable. A run-level "failure" conclusion
   * must not poison per-product reuse.
   */
  test('reuses surviving products of a run whose overall conclusion is failure', () => {
    const failedRun = { ...sourceRun, conclusion: 'failure' };
    const plan = planAt(fixedCommit, { reuseFromRunId: sourceRun.runId, sourceRun: failedRun, version: '7.8.0' });
    expect(plan.products['macos-arm64'].action).toBe('reuse');
    expect(plan.products.android.action).toBe('reuse');
    expect(plan.products['windows-x64'].action).toBe('build');
  });

  test('falls back to building a product whose package artifact the failed run never uploaded', () => {
    const partialRun = {
      ...sourceRun,
      availableArtifacts: Object.keys(sourceRun.products)
        .filter((product) => product !== 'android')
        .map((product) => `release-${product}`),
      conclusion: 'failure',
    };
    const plan = planAt(fixedCommit, { sourceRun: partialRun, version: '7.8.0' });
    expect(plan.products.android.action).toBe('build');
    expect(plan.products.android.reason).toMatch(/never uploaded/u);
    expect(plan.products['macos-arm64'].action).toBe('reuse');
  });

  test('rejects a source run that is still in progress', () => {
    const runningRun = { ...sourceRun, conclusion: null };
    const plan = planAt(fixedCommit, { sourceRun: runningRun, version: '7.8.0' });
    expect(plan.products.android.action).toBe('build');
    expect(plan.products.android.reason).toMatch(/not a completed release run/u);
  });
});

/*
 * The gxserver build script is an input of both gxserver packages, which the
 * macOS DMG embeds, so changing it has to invalidate all three. It is the
 * clearest example of composition doing its job: nothing under `apps/desktop/**` moved,
 * yet the DMG must be rebuilt.
 */
describe('gxserver build script change', () => {
  /*
   * The fixture repository is shared and mutated in sequence, so this suite
   * commits from `beforeAll` (which runs after every earlier suite's tests) and
   * never from the describe body (which would run during collection, before
   * them).
   */
  let baselineRelease;
  let movedCommit;
  let sourceRun;
  beforeAll(() => {
    const previousCommit = repo.commit('checkpoint before the gxserver build script change');
    baselineRelease = baselineAt(previousCommit, '7.8.0');
    sourceRun = sourceRunFromPlan({
      headSha: previousCommit,
      plan: planAt(previousCommit, { version: '7.8.0' }),
      version: '7.8.0',
    });
    repo.write('tooling/build-remote-gxserver-linux-release.sh', '#!/usr/bin/env bash\nset -euo pipefail\n');
    movedCommit = repo.commit('change the gxserver build script');
  });

  test('rebuilds both gxserver packages and names the script', () => {
    const plan = planAt(movedCommit, { baselines: [baselineRelease], version: '7.8.0' });
    for (const product of ['gxserver-linux-x64', 'gxserver-linux-arm64']) {
      expect(plan.products[product].action).toBe('build');
      expect(plan.products[product].reason).toMatch(/tooling\/build-remote-gxserver-linux-release\.sh/u);
    }
    /* Android never embeds gxserver, so it stays reusable. */
    expect(plan.products.android.action).toBe('reuse');
  });

  test('rebuilds macOS because it embeds both gxservers', () => {
    const plan = planAt(movedCommit, { reuseFromRunId: sourceRun.runId, sourceRun, version: '7.8.0' });
    expect(plan.products['macos-arm64'].action).toBe('build');
    expect(plan.products['macos-arm64'].reason).toMatch(/embedded .*gxserver-linux-x64/u);
  });
});

/*
 * Workflow files are declared fingerprint inputs precisely so that changing how a
 * product is built invalidates it. release-gpui-runtime.yml no longer exists;
 * these are the files that replaced it.
 */
describe('workflow-file invalidation', () => {
  let baselineRelease;
  let editedCommit;
  beforeAll(() => {
    const previousCommit = repo.commit('checkpoint before the workflow edits');
    baselineRelease = baselineAt(previousCommit, '7.8.0');
    repo.write('.github/workflows/release-gpui-gxserver.yml', 'name: gxserver package\n# retry apt\n');
    editedCommit = repo.commit('edit the gxserver workflow');
  });

  test('editing release-gpui-gxserver.yml rebuilds the gxserver products only', () => {
    const plan = planAt(editedCommit, { baselines: [baselineRelease], version: '7.8.0' });
    for (const product of ['gxserver-linux-x64', 'gxserver-linux-arm64']) {
      expect(plan.products[product].action).toBe('build');
      expect(plan.products[product].reason).toMatch(/release-gpui-gxserver\.yml/u);
    }
    expect(plan.products.android.action).toBe('reuse');
  });

  test('editing release-gpui-android.yml rebuilds Android only', () => {
    repo.write('.github/workflows/release-gpui-android.yml', 'name: android\n# bump build-tools\n');
    const androidCommit = repo.commit('edit the android workflow');
    const baseline = baselineAt(editedCommit, '7.8.0');
    const plan = planAt(androidCommit, { baselines: [baseline], version: '7.8.0' });
    expect(plan.products.android.action).toBe('build');
    expect(plan.products.android.reason).toMatch(/release-gpui-android\.yml/u);
    expect(plan.products['gxserver-linux-x64'].action).toBe('reuse');
  });
});

describe('Scenario F — unchanged immutable components', () => {
  const sourceSha = repo.commit('checkpoint for component planning');
  const baselineRelease = baselineAt(sourceSha);

  test('reuses complete component tags instead of rebuilding them', () => {
    const plan = planAt(sourceSha, {
      baselines: [baselineRelease],
      componentTagState: componentTagStateFixture(),
      version: '7.8.0',
    });
    expect(plan.components.cef.action).toBe('reuse');
    expect(plan.components.cef.reason).toMatch(/already has darwin-arm64, linux-x64, windows-arm64, windows-x64/u);
    expect(plan.components['code-server'].action).toBe('reuse');
    expect(plan.jobs.code_server_x64).toBe('reuse');
    expect(plan.jobs.code_server_arm64).toBe('reuse');
  });

  test('builds a component whose tag is missing a required platform', () => {
    const state = componentTagStateFixture();
    delete state['code-server'].platforms['linux-arm64'];
    const plan = planAt(sourceSha, { baselines: [baselineRelease], componentTagState: state, version: '7.8.0' });
    expect(plan.components['code-server'].action).toBe('build');
    expect(plan.components['code-server'].reason).toMatch(/component tag missing linux-arm64/u);
    expect(plan.jobs.code_server_arm64).toBe('build');
  });

  test('refuses to reuse a component whose identity revision inputs changed', () => {
    const state = componentTagStateFixture();
    state['code-server'].identityRevisionInputsDigest = 'f'.repeat(64);
    const plan = planAt(sourceSha, { baselines: [baselineRelease], componentTagState: state, version: '7.8.0' });
    expect(plan.components['code-server'].action).toBe('build');
    expect(plan.components['code-server'].reason).toMatch(/identity revision inputs changed/u);
    expect(plan.components['code-server'].identityRevisionInputsDigest).toMatch(/^[0-9a-f]{64}$/u);
  });

  test('skips components no building product requires', () => {
    const plan = planAt(sourceSha, {
      baselines: [baselineRelease],
      componentTagState: componentTagStateFixture(),
      scope: defaultScope({
        linuxDeb: false,
        linuxRpm: false,
        linuxTar: false,
        macos: false,
        updateSparkle: false,
        windowsArm64: false,
        windowsX64: false,
      }),
      version: '7.8.0',
    });
    expect(plan.components.cef.action).toBe('skip');
    expect(plan.components['code-server'].action).toBe('skip');
    expect(plan.jobs.code_server_x64).toBe('skip');
  });
});

describe('Scenario G — forced full rebuild', () => {
  const sourceSha = repo.commit('checkpoint for force-all');
  const baselineRelease = baselineAt(sourceSha);

  test('force-all rebuilds every in-scope product and reuses nothing', () => {
    const plan = planAt(sourceSha, { baselines: [baselineRelease], forceAll: true, version: '7.8.0' });
    expect(new Set(Object.values(actions(plan)))).toEqual(new Set(['build']));
    expect(plan.jobs.reuse_matrix).toEqual([]);
    expect(plan.forceAll).toBe(true);
    for (const entry of Object.values(plan.products)) {
      expect(entry.reason).toMatch(/force-all requested/u);
    }
  });

  test('forcing a single product leaves the rest change-aware', () => {
    const plan = planAt(sourceSha, {
      baselines: [baselineRelease],
      forcedProducts: ['android'],
      version: '7.8.0',
    });
    expect(plan.products.android.action).toBe('build');
    expect(plan.products.android.reason).toMatch(/explicitly forced/u);
    expect(plan.products['gxserver-linux-x64'].action).toBe('reuse');
    expect(() => planAt(sourceSha, { forcedProducts: ['not-a-product'], version: '7.8.0' })).toThrow(
      /Unknown release product/u
    );
  });
});

describe('scope, validation, and reporting', () => {
  const sourceSha = repo.commit('checkpoint for scope');
  const baselineRelease = baselineAt(sourceSha);

  test('skips products the operator excluded and holds their feeds', () => {
    const plan = planAt(sourceSha, {
      baselines: [baselineRelease],
      scope: defaultScope({ android: false, macos: false, updateSparkle: false }),
      version: '7.8.0',
    });
    expect(plan.products.android.action).toBe('skip');
    expect(plan.products.android.reason).toBe('not in the requested release scope');
    expect(plan.products['macos-arm64'].action).toBe('skip');
    expect(plan.expectedPlatforms).not.toContain('android');
    expect(plan.expectedPlatforms).not.toContain('macos-arm64');
    expect(plan.feeds).toEqual({ homebrew: false, sparkle: false, windowsFeeds: ['x64', 'arm64'] });
  });

  test('refuses a plan with no enabled platform', () => {
    expect(() =>
      planAt(sourceSha, {
        scope: defaultScope({
          android: false,
          gxserverLinuxArm64: false,
          gxserverLinuxX64: false,
          gxserverWslWindowsArm64: false,
          gxserverWslWindowsX64: false,
          linuxDeb: false,
          linuxRpm: false,
          linuxTar: false,
          macos: false,
          updateSparkle: false,
          windowsArm64: false,
          windowsX64: false,
        }),
      })
    ).toThrow(/At least one platform must be enabled/u);
  });

  test('validatePlan refuses a tampered reuse of a version-stamped product', () => {
    const plan = planAt(sourceSha, { baselines: [baselineRelease], version: '7.8.0' });
    const tampered = structuredClone(plan);
    tampered.products['macos-arm64'] = {
      ...tampered.products['macos-arm64'],
      action: 'reuse',
      reuse: { artifacts: [{ name: 'x.dmg', sha256: 'a'.repeat(64), size: 1 }], productVersion: '7.7.0' },
    };
    expect(() => validatePlan(tampered)).toThrow(/version-stamped and may never be reused across releases/u);

    const inconsistent = structuredClone(plan);
    inconsistent.expectedPlatforms = ['macos-arm64'];
    expect(() => validatePlan(inconsistent)).toThrow(/expectedPlatforms does not match/u);
  });

  test('renders BUILD, REUSE, and SKIP lines with their reasons', () => {
    const plan = planAt(sourceSha, {
      baselines: [baselineRelease],
      componentTagState: componentTagStateFixture(),
      scope: defaultScope({ android: true, linuxRpm: false }),
      version: '7.8.0',
    });
    const text = renderPlanText(plan);
    expect(text).toMatch(/^Ghostex release plan — 7\.8\.0$/mu);
    expect(text).toMatch(/^macos-arm64\s+BUILD\s+\S/mu);
    expect(text).toMatch(/^android\s+REUSE\s+all relevant inputs match v7\.7\.0/mu);
    expect(text).toMatch(/^linux-rpm-x64\s+SKIP\s+not in the requested release scope$/mu);
    expect(text).toMatch(/^cef\s+\S+\s+REUSE\s+/mu);
    expect(text).toContain('FEEDS         sparkle=update  homebrew=update  windows-feeds=x64,arm64');
    expect(text).toContain('JOBS          macos, windows_arm64, windows_x64, wsl_arm64, wsl_x64, linux_x64(deb,tar)');
    expect(text).not.toContain('validate_windows');
    expect(text).toMatch(/SAVED\s+~\d+ runner-minutes/u);
    expect(planSummaryLine(plan)).toMatch(/\d+ built, \d+ reused, 1 skipped by flag/u);
  });

  test('reads the release scope from the workflow environment variables', () => {
    const scope = scopeFromEnv({
      GHOSTEX_RELEASE_ANDROID: 'false',
      GHOSTEX_RELEASE_GXSERVER_LINUX_ARM64: 'true',
      GHOSTEX_RELEASE_GXSERVER_LINUX_X64: 'true',
      GHOSTEX_RELEASE_GXSERVER_WSL_WINDOWS_ARM64: 'true',
      GHOSTEX_RELEASE_GXSERVER_WSL_WINDOWS_X64: 'true',
      GHOSTEX_RELEASE_LINUX_DEB: 'true',
      GHOSTEX_RELEASE_LINUX_RPM: 'true',
      GHOSTEX_RELEASE_LINUX_TAR: 'true',
      GHOSTEX_RELEASE_MACOS: 'true',
      GHOSTEX_RELEASE_SIGN_WINDOWS: 'true',
      GHOSTEX_RELEASE_UPDATE_SPARKLE: 'true',
      GHOSTEX_RELEASE_WINDOWS_ARM64: 'true',
      GHOSTEX_RELEASE_WINDOWS_X64: 'true',
    });
    expect(scope.android).toBe(false);
    expect(scope.macos).toBe(true);
    expect(scope.signWindows).toBe(true);
  });
});
