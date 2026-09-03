/*
 * CDXC:Release 2026-08-13:
 * The planner CLI's pure surface: argument parsing, scope precedence, component
 * identity inference, and the outputs the workflow graph reads. The network
 * collectors are intentionally not covered here — they are thin `gh` wrappers,
 * and mocking `gh` would only assert that the mock was called.
 */

import { describe, expect, test } from 'vitest';

import { parsePlanCliArgs, planGithubOutputs, renderPlanMarkdown, resolvePlanScope } from './plan-cli.mjs';
import { resolveComponentIdentities } from './plan-cli.mjs';
import { assertPlansAgree } from './publish-provenance.mjs';
import { computePlan, validatePlan } from './plan.mjs';
import { createFixtureRepo, releaseBaselineFromPlan } from './plan-test-fixtures.mjs';
import { COMPONENT_IDS, PRODUCT_IDS, defaultScope } from './product-inputs.mjs';

function planFor(repo, overrides = {}) {
  return computePlan({
    entries: repo.reader.listTree(repo.head),
    isAncestor: () => true,
    readObject: repo.reader.readObject,
    scope: defaultScope(),
    sourceSha: repo.head,
    version: '7.8.0',
    ...overrides,
  });
}

describe('plan-cli argument parsing', () => {
  test('reads the version positionally and defaults the rest', () => {
    const options = parsePlanCliArgs(['7.8.0']);
    expect(options.version).toBe('7.8.0');
    expect(options.forceAll).toBe(false);
    expect(options.forcedProducts).toEqual([]);
    expect(options.format).toBe('text');
    expect(options.reuseFromRunId).toBeNull();
  });

  test('parses every planning override', () => {
    const options = parsePlanCliArgs([
      '--version',
      '7.8.0',
      '--force',
      'android, windows-x64',
      '--reuse-from-run',
      '31644067583',
      '--baseline-count',
      '4',
      '--format',
      'json',
      '--emit-github-output',
      '--emit-step-summary',
      '--offline',
    ]);
    expect(options.forcedProducts).toEqual(['android', 'windows-x64']);
    expect(options.reuseFromRunId).toBe('31644067583');
    expect(options.baselineCount).toBe(4);
    expect(options.format).toBe('json');
    expect(options.emitGithubOutput).toBe(true);
    expect(options.emitStepSummary).toBe(true);
    expect(options.offline).toBe(true);
  });

  test('refuses contradictory, malformed, or unknown options', () => {
    expect(() => parsePlanCliArgs(['7.8.0', '--force-all', '--force', 'android'])).toThrow(/already rebuilds/u);
    expect(() => parsePlanCliArgs(['7.8.0', '--force', 'not-a-product'])).toThrow(/Unknown release product/u);
    expect(() => parsePlanCliArgs(['7.8.0', '--reuse-from-run', 'abc'])).toThrow(/run id/u);
    expect(() => parsePlanCliArgs(['7.8.0', '--format', 'yaml'])).toThrow(/--format/u);
    expect(() => parsePlanCliArgs(['7.8.0', '--baseline-count', '0'])).toThrow(/positive integer/u);
    expect(() => parsePlanCliArgs(['7.8.0', '--nope'])).toThrow(/Unknown option/u);
    expect(() => parsePlanCliArgs(['7.8.0', '--scope-json'])).toThrow(/requires a value/u);
  });
});

describe('plan-cli scope resolution', () => {
  test('prefers an explicit scope document', () => {
    const scope = resolvePlanScope({ env: {}, scopeJson: JSON.stringify({ android: false, macos: true }) });
    expect(scope.android).toBe(false);
    expect(scope.macos).toBe(true);
    /* Unspecified flags keep the full-release default rather than becoming false. */
    expect(scope.windowsX64).toBe(true);
  });

  test('falls back to the workflow environment when one is present', () => {
    const scope = resolvePlanScope({
      env: { GHOSTEX_RELEASE_ANDROID: 'false', GHOSTEX_RELEASE_MACOS: 'true' },
      scopeJson: null,
    });
    expect(scope.macos).toBe(true);
    expect(scope.android).toBe(false);
    expect(scope.windowsX64).toBe(false);
  });

  test('defaults to a full release for a local preview with no environment', () => {
    expect(resolvePlanScope({ env: {}, scopeJson: null })).toEqual(defaultScope());
  });
});

describe('plan-cli workflow outputs', () => {
  const repo = createFixtureRepo();
  const plan = planFor(repo);

  test('emits one single-line value per job decision the graph reads', () => {
    const outputs = planGithubOutputs(plan);
    for (const [key, value] of Object.entries(outputs)) {
      expect(`${key}=${value}`).not.toContain('\n');
    }
    expect(JSON.parse(outputs.plan).version).toBe('7.8.0');
    expect(outputs.expected_platforms.split(',')).toEqual(plan.expectedPlatforms);
    expect(outputs.job_macos).toBe(plan.jobs.macos);
    expect(outputs.job_windows_x64).toBe(plan.jobs.windows_x64);
    expect(outputs.job_validate_windows).toBeUndefined();
    expect(outputs.linux_packages).toBe('deb,rpm,tar');
    expect(JSON.parse(outputs.reuse_matrix)).toEqual([]);
    expect(outputs.reuse_count).toBe('0');
  });

  test('renders a step summary naming every product and component', () => {
    const markdown = renderPlanMarkdown(plan);
    for (const productId of PRODUCT_IDS) expect(markdown).toContain(`\`${productId}\``);
    for (const component of COMPONENT_IDS) expect(markdown).toContain(`\`${component}\``);
    expect(markdown).toContain('| Product | Action | Fingerprint | Reason |');
  });

  test('the reuse matrix output drives the materialization jobs', () => {
    const baseline = releaseBaselineFromPlan({ commit: repo.head, plan, version: '7.7.0' });
    const reusing = planFor(repo, { baselines: [baseline] });
    const outputs = planGithubOutputs(reusing);
    expect(JSON.parse(outputs.reuse_matrix)).toEqual(reusing.jobs.reuse_matrix);
    expect(outputs.reuse_count).toBe(String(reusing.jobs.reuse_matrix.length));
    expect(JSON.parse(outputs.reuse_matrix).length).toBeGreaterThan(0);
  });

  /*
   * The threaded plan is re-serialized into eight jobs, so it carries only what a
   * job reads. `rejectedReuse` is planner diagnostics for the operator and the
   * step summary; the full document still goes to the `release-plan` artifact.
   */
  test("the threaded plan drops the planner's rejection diagnostics", () => {
    const stale = releaseBaselineFromPlan({
      commit: repo.head,
      plan: planFor(repo, { version: '7.6.0' }),
      seed: 'stale',
      version: '7.6.0',
    });
    /* Digests that no longer match anything, so every product records a rejection. */
    for (const record of Object.values(stale.provenance.products)) record.fingerprint = '9'.repeat(64);
    const rejecting = planFor(repo, { baselines: [stale] });
    const rejections = PRODUCT_IDS.filter((id) => (rejecting.products[id].rejectedReuse ?? []).length > 0);
    expect(rejections.length).toBeGreaterThan(0);

    const threaded = JSON.parse(planGithubOutputs(rejecting).plan);
    for (const productId of PRODUCT_IDS) {
      expect(threaded.products[productId].rejectedReuse).toBeUndefined();
      /* write-provenance.mjs copies these into every product record; never strip them. */
      expect(threaded.products[productId].inputs).toBeDefined();
      expect(threaded.products[productId].action).toBe(rejecting.products[productId].action);
      expect(threaded.products[productId].fingerprint).toBe(rejecting.products[productId].fingerprint);
    }
    expect(threaded.expectedPlatforms).toEqual(rejecting.expectedPlatforms);
    expect(JSON.stringify(threaded).length).toBeLessThan(JSON.stringify(rejecting).length);
  });

  test("the compacted plan still passes plan validation and the publisher's agreement check", () => {
    const threaded = JSON.parse(planGithubOutputs(plan).plan);
    expect(() => validatePlan(threaded)).not.toThrow();
    /* assemble.mjs compares the threaded copy against the uploaded full plan. */
    expect(assertPlansAgree(threaded, plan)).toBe(true);
  });
});

describe('plan-cli component identity inference', () => {
  const repo = createFixtureRepo();
  const plan = planFor(repo);
  const baseline = releaseBaselineFromPlan({ commit: repo.head, plan, version: '7.7.0' });

  test('an explicit override always wins', () => {
    const identities = resolveComponentIdentities({
      baselines: [],
      entries: repo.reader.listTree(repo.head),
      overrides: { cef: '148.4.0-148.0.10' },
      readObject: repo.reader.readObject,
      scope: defaultScope(),
      version: '7.8.0',
    });
    expect(identities.cef).toBe('148.4.0-148.0.10');
  });

  test('infers an identity from a baseline whose recorded composition still matches', () => {
    baseline.provenance.components = {
      cef: { componentVersion: '148.4.0-148.0.10' },
      'code-server': { componentVersion: '390f119a145e-p2-abc' },
    };
    const identities = resolveComponentIdentities({
      baselines: [baseline],
      entries: repo.reader.listTree(repo.head),
      readObject: repo.reader.readObject,
      scope: defaultScope(),
      version: '7.8.0',
    });
    expect(identities.cef).toBe('148.4.0-148.0.10');
    expect(identities['code-server']).toBe('390f119a145e-p2-abc');
  });

  test("infers nothing once the component's own inputs move", () => {
    baseline.provenance.components = { cef: { componentVersion: '148.4.0-148.0.10' } };
    repo.setGitlink('.dependencies/cef-rs', '1111111111111111111111111111111111111111');
    const moved = repo.commit('bump cef-rs');
    const identities = resolveComponentIdentities({
      baselines: [baseline],
      entries: repo.reader.listTree(moved),
      readObject: repo.reader.readObject,
      scope: defaultScope(),
      version: '7.8.0',
    });
    expect(identities.cef).toBeUndefined();
  });
});
