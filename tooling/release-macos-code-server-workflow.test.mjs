import { chmod, mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { existsSync, readFileSync, readdirSync, readlinkSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, test } from 'vitest';

import {
  CODE_SERVER_NODE_PAYLOAD_INPUTS,
  codeServerComponentIdentity,
  codeServerComponentNames,
} from './release-gpui/code-server-component-identity.mjs';
import { CODE_SERVER_ARCHIVE_CONTRACT } from './release-gpui/verify-code-server-archive.mjs';

const repoFile = (relativePath) => readFileSync(new URL(`../${relativePath}`, import.meta.url), 'utf8');
const workflow = (name) => repoFile(`.github/workflows/${name}`);
const temporaryRoots = [];

function runGit(root, ...args) {
  const result = spawnSync('git', ['-C', root, ...args], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(result.stderr.trim() || `git ${args.join(' ')} failed`);
  }
}

afterEach(async () => {
  await Promise.all(temporaryRoots.splice(0).map((root) => rm(root, { force: true, recursive: true })));
});

async function createCodeServerIdentityFixture() {
  const root = await mkdtemp(path.join(tmpdir(), 'ghostex-code-server-identity-'));
  temporaryRoots.push(root);
  for (const input of CODE_SERVER_NODE_PAYLOAD_INPUTS) {
    if (['src/common', 'src/node', 'typings'].includes(input)) {
      await mkdir(path.join(root, input), { recursive: true });
      await writeFile(path.join(root, input, 'fixture.ts'), `export const input = ${JSON.stringify(input)};\n`);
    } else {
      await mkdir(path.dirname(path.join(root, input)), { recursive: true });
      await writeFile(path.join(root, input), `${input}\n`);
    }
  }
  const readinessSource = path.join(root, 'src/node/routes/health.ts');
  await mkdir(path.dirname(readinessSource), { recursive: true });
  await writeFile(readinessSource, 'export const promptEditorIpcReady = false;\n');
  runGit(root, 'init', '--quiet');
  runGit(root, 'config', 'user.email', 'release-test@ghostex.local');
  runGit(root, 'config', 'user.name', 'Ghostex Release Test');
  runGit(root, 'add', '.');
  runGit(root, 'commit', '--quiet', '-m', 'fixture');
  return { readinessSource, root };
}

describe('immutable code-server component identity', () => {
  test('changes when the prompt-editor readiness source changes and bypasses commit-p1', async () => {
    const fixture = await createCodeServerIdentityFixture();
    const sourceRevision = '6b4cfff155c0';
    const before = await codeServerComponentIdentity({ codeServerRoot: fixture.root, sourceRevision });
    await writeFile(fixture.readinessSource, 'export const promptEditorIpcReady = true;\n');
    runGit(fixture.root, 'add', '.');
    runGit(fixture.root, 'commit', '--quiet', '-m', 'update readiness');
    const after = await codeServerComponentIdentity({ codeServerRoot: fixture.root, sourceRevision });

    expect(before.componentVersion).toMatch(/^6b4cfff155c0-p2-[0-9a-f]{64}$/);
    expect(before.componentVersion).not.toBe('6b4cfff155c0-p1');
    expect(after.componentVersion).not.toBe(before.componentVersion);
    expect(after.payloadFingerprint).not.toBe(before.payloadFingerprint);
  });

  test('uses one source identity for Darwin, Linux, and Windows architectures', async () => {
    const fixture = await createCodeServerIdentityFixture();
    const { componentVersion } = await codeServerComponentIdentity({
      codeServerRoot: fixture.root,
      sourceRevision: '6b4cfff155c0',
    });
    const names = ['darwin-arm64', 'linux-x64', 'linux-arm64', 'windows-x64', 'windows-arm64'].map((platform) =>
      codeServerComponentNames(componentVersion, platform)
    );

    expect(new Set(names.map((entry) => entry.downloadTag))).toEqual(new Set([`code-server-${componentVersion}`]));
    expect(names.map((entry) => entry.archiveName)).toEqual([
      `code-server-${componentVersion}-darwin-arm64.tar.gz`,
      `code-server-${componentVersion}-linux-x64.tar.gz`,
      `code-server-${componentVersion}-linux-arm64.tar.gz`,
      `code-server-${componentVersion}-windows-x64.tar.gz`,
      `code-server-${componentVersion}-windows-arm64.tar.gz`,
    ]);
    expect(names.map((entry) => entry.artifactName)).toEqual([
      `release-code-server-${componentVersion}-darwin-arm64`,
      `release-code-server-${componentVersion}-linux-x64`,
      `release-code-server-${componentVersion}-linux-arm64`,
      `release-code-server-${componentVersion}-windows-x64`,
      `release-code-server-${componentVersion}-windows-arm64`,
    ]);
  });
});

describe('phased macOS code-server prerequisite contract', () => {
  test.each([
    ['x64', 'release-build-gxserver-x64.yml'],
    ['arm64', 'release-build-gxserver-arm64.yml'],
  ])('publishes the Linux %s archive from its gxserver phase', (arch, workflowName) => {
    const source = workflow(workflowName);
    expect(source).toContain(`--platform linux-${arch} --github-output`);
    expect(source).toContain('name: ${{ steps.code_server_identity.outputs.artifact_name }}');
    expect(source).toContain('ARCHIVE="$OUTPUT/${{ steps.code_server_identity.outputs.archive_name }}"');
    expect(source).toContain('lib/node');
    expect(source).toContain('out/node/entry.js');
    expect(source).toContain('lib/vscode/out/server-main.js');
    expect(source).toContain('if-no-files-found: error');
  });

  test('downloads both archives from the exact release-state prerequisite runs', () => {
    const source = workflow('release-build-macos.yml');
    for (const arch of ['x64', 'arm64']) {
      expect(source).toContain(`path: build/runtime-artifacts/code-server-${arch}`);
      expect(source).toContain(`run-id: \${{ inputs.gxserver_${arch}_run_id }}`);
      expect(source).toContain(`linux-${arch}.tar.gz`);
    }
    expect(source).toContain('name: ${{ steps.code_server_identity.outputs.artifact_name }}');
    expect(source).toContain(
      'name: release-code-server-${{ steps.code_server_identity.outputs.component_version }}-linux-arm64'
    );
    expect(source).toContain(
      'GHOSTEX_CODE_SERVER_COMPONENT_VERSION: ${{ steps.code_server_identity.outputs.component_version }}'
    );
    expect(source).toContain('does not match verified release state run');
    expect(source).toContain('.release-automation/tooling/release-gpui/verify-code-server-archive.mjs');
    expect(source).toContain('--platform "linux-$1"');
  });

  test('keeps every other active macOS release entry path fail-closed', () => {
    const reusableWorkflow = workflow('release-gpui-macos.yml');
    // CDXC:ReleaseChangeAwarePlanning 2026-08-13: release-gpui-runtime.yml was
    // split into release-gpui-gxserver.yml (the package) and
    // release-gpui-code-server.yml (the immutable component, reuse-first).
    const codeServerWorkflow = workflow('release-gpui-code-server.yml');
    const windowsWorkflow = workflow('release-gpui-windows.yml');
    const orchestratorWorkflow = workflow('release-gpui.yml');
    const prerequisiteScript = repoFile('tooling/release-gpui/macos-prerequisite.sh');
    const localReleaseScript = repoFile('tooling/release-gpui/macos.sh');
    const prepareRuntimeScript = repoFile('apps/desktop/scripts/prepare-macos-runtime.sh');
    const windowsBuildScript = repoFile('apps/desktop/scripts/build-windows-app.ps1');

    for (const arch of ['x64', 'arm64']) {
      expect(reusableWorkflow).toContain(`linux-${arch}.tar.gz`);
      expect(prerequisiteScript).toContain(
        `code-server-${arch}/code-server-$CODE_SERVER_COMPONENT_VERSION-linux-${arch}.tar.gz`
      );
      expect(localReleaseScript).toContain(
        `code-server-${arch}/code-server-$CODE_SERVER_COMPONENT_VERSION-linux-${arch}.tar.gz`
      );
    }
    expect(codeServerWorkflow).toContain('--platform "linux-${{ inputs.arch }}" --github-output');
    expect(codeServerWorkflow).toContain('name: ${{ steps.code_server_identity.outputs.artifact_name }}');
    expect(codeServerWorkflow).toContain(
      'ARCHIVE="$OUTPUT_DIR/${{ steps.code_server_identity.outputs.archive_name }}"'
    );
    // Reuse before build must stay at least as strict as a fresh build: the
    // downloaded archive is digest-checked by publish-component.mjs and then
    // structurally validated by the same verifier the build path runs.
    expect(codeServerWorkflow).toContain('--reuse-published');
    expect(codeServerWorkflow).toContain('--require-sha256-sidecars');
    expect(codeServerWorkflow).toContain('tooling/release-gpui/verify-code-server-archive.mjs');
    expect(reusableWorkflow).toContain('name: ${{ steps.code_server_identity.outputs.artifact_name }}');
    expect(reusableWorkflow).toContain(
      'name: release-code-server-${{ steps.code_server_identity.outputs.component_version }}-linux-arm64'
    );
    expect(windowsWorkflow).toContain('name: ${{ steps.code_server_identity.outputs.artifact_name }}');
    expect(windowsWorkflow).toContain('${{ steps.code_server_identity.outputs.archive_name }}');
    expect(windowsBuildScript).toContain('code-server-$ComponentVersion-linux-$ReleaseArch.tar.gz');
    expect(windowsBuildScript).toContain('verify-code-server-archive.mjs');
    expect(windowsBuildScript).toContain('--platform "linux-$ReleaseArch"');
    // The orchestrator gates the component jobs on the resolved plan and no
    // longer threads component names through gxserver job outputs: every
    // consumer resolves the identity itself from the pinned submodule, so
    // codeServerComponentNames() stays the single source of the artifact name.
    expect(orchestratorWorkflow).toContain('uses: ./.github/workflows/release-gpui-code-server.yml');
    expect(orchestratorWorkflow).toContain("needs.prepare.outputs.job_code_server_x64 != 'skip'");
    expect(orchestratorWorkflow).toContain("needs.prepare.outputs.job_code_server_arm64 != 'skip'");
    expect(orchestratorWorkflow).not.toContain('code_server_artifact_name:');
    expect(prerequisiteScript).toContain('macOS runtime preparation requires Linux x64 code-server archive');
    expect(prerequisiteScript).toContain('macOS runtime preparation requires Linux arm64 code-server archive');
    expect(localReleaseScript).toContain('macOS release requires Linux x64 code-server archive');
    expect(localReleaseScript).toContain('macOS release requires Linux arm64 code-server archive');
    expect(prepareRuntimeScript).toContain(
      'macOS release preparation requires the Linux $linux_arch code-server component archive'
    );
    expect(prepareRuntimeScript).toContain('code_server_node_payload_digest');
    expect(prepareRuntimeScript).toContain('--path "$CODE_SERVER_ROOT/src/node"');
    expect(prepareRuntimeScript).toContain('--path "$CODE_SERVER_ROOT/.node-version"');
    expect(prepareRuntimeScript).toContain('cache_matches "code-server-node-payload"');
    expect(prepareRuntimeScript).toContain('code-server-$component_version-linux-$linux_arch.tar.gz');
    expect(prepareRuntimeScript).toContain('verify-code-server-archive.mjs');
    expect(prepareRuntimeScript).toContain('--platform "linux-$linux_arch"');
    expect(reusableWorkflow).toContain('Validate required remote runtime archives');
    expect(reusableWorkflow).toContain('tooling/release-gpui/verify-code-server-archive.mjs');
  });

  test('publishes and authenticates the exact Darwin component archive before reuse', () => {
    const prepareRuntimeScript = repoFile('apps/desktop/scripts/prepare-macos-runtime.sh');
    const localReleaseScript = repoFile('tooling/release-gpui/macos.sh');
    const downloadIndex = prepareRuntimeScript.indexOf('gh release download "$component_tag"');
    const verifierIndex = prepareRuntimeScript.indexOf(
      'node "$REPO_ROOT/tooling/release-gpui/verify-code-server-archive.mjs"',
      downloadIndex
    );
    const metadataIndex = prepareRuntimeScript.indexOf(
      'node "$REPO_ROOT/tooling/release-gpui/publish-component.mjs"',
      verifierIndex
    );

    expect(downloadIndex).toBeGreaterThan(-1);
    expect(verifierIndex).toBeGreaterThan(downloadIndex);
    expect(metadataIndex).toBeGreaterThan(verifierIndex);
    expect(prepareRuntimeScript).toContain('sidecar_name="$asset_name.sha256"');
    expect(prepareRuntimeScript).toContain('--pattern "$(basename "$asset_sidecar")"');
    expect(prepareRuntimeScript).toContain('--platform darwin-arm64');
    expect(prepareRuntimeScript).toContain('printf \'%s  %s\\n\' "$asset_sha256" "$(basename "$asset_path")"');
    expect(prepareRuntimeScript).toContain('--require-sha256-sidecars');
    expect(prepareRuntimeScript).not.toContain('/usr/bin/tar -xOzf "$asset_path"');
    expect(localReleaseScript).toContain(
      'PUBLISH_ARGS+=(--require-platforms darwin-arm64,linux-x64,linux-arm64 --require-sha256-sidecars)'
    );
  });

  test.each(['release-build-gxserver-x64.yml', 'release-build-gxserver-arm64.yml', 'release-gpui-code-server.yml'])(
    'binds the producer checksum to the exact archive name in %s',
    (workflowName) => {
      const source = workflow(workflowName);
      expect(source).toContain(`printf '%s  %s\\n' "$archive_sha256" "$(basename "$ARCHIVE")"`);
    }
  );
});

describe('active WSL2 code-server consumer contract', () => {
  test('shares one complete archive payload contract between release and installed consumers', () => {
    const nativeVerifier = repoFile('apps/desktop/src/component_store.rs');
    const windowsConsumer = repoFile('apps/desktop/src/windows_terminal_backend.rs');

    expect(CODE_SERVER_ARCHIVE_CONTRACT.requiredEntries).toEqual(
      expect.arrayContaining([
        'lib/node',
        'out/node/entry.js',
        'out/node/routes/health.js',
        'lib/vscode/out/server-main.js',
        'lib/vscode/extensions/git/node_modules/@vscode/fs-copyfile/package.json',
        'lib/vscode/node_modules/@vscode/ripgrep/bin/rg',
      ])
    );
    expect(CODE_SERVER_ARCHIVE_CONTRACT.requiredEntriesByPlatform['linux-x64']).toContain(
      'lib/vscode/node_modules/node-pty/build/Release/pty.node'
    );
    expect(CODE_SERVER_ARCHIVE_CONTRACT.requiredEntriesByPlatform['darwin-arm64']).toEqual(
      expect.arrayContaining([
        'lib/vscode/node_modules/node-pty/prebuilds/darwin-arm64/pty.node',
        'lib/vscode/node_modules/node-pty/prebuilds/darwin-arm64/spawn-helper',
      ])
    );
    expect(CODE_SERVER_ARCHIVE_CONTRACT.executableEntries).toEqual(
      expect.arrayContaining(['lib/node', 'lib/vscode/node_modules/@vscode/ripgrep/bin/rg'])
    );
    expect(CODE_SERVER_ARCHIVE_CONTRACT.readinessSignal).toBe('promptEditorIpcReady');
    expect(nativeVerifier).toContain('include_str!("../../../packages/shared/code-server-archive-contract.json")');
    expect(nativeVerifier).toContain('verify_installed_windows_code_server_component');
    expect(windowsConsumer).toContain('verify_code_server_archive');
    expect(windowsConsumer).toContain('code_server_payload_shell_validation_script');
  });

  test('routes start-gpui through the WSL builder and downloads the canonical producer archive plus sidecar', () => {
    const launcher = repoFile('tooling/start-gpui.mjs');

    expect(launcher).toContain('isWsl ? "build-windows-app-wsl.sh" : "build-windows-app.ps1"');
    expect(launcher).toContain(
      'codeServerComponentIdentity({ codeServerRoot: path.join(repoRoot, ".dependencies/code-server") })'
    );
    expect(launcher).toContain(
      'codeServerComponentNames(windowsCodeServerIdentity.componentVersion, `linux-${windowsArch}`)'
    );
    expect(launcher).toContain('windowsCodeServerNames.archiveName');
    expect(launcher).toContain('windowsCodeServerNames.artifactName');
    expect(launcher).toContain('hasWindowsWslCodeServerArchive !== hasWindowsWslCodeServerSidecar');
    expect(launcher).toContain('GHOSTEX_CODE_SERVER_COMPONENT_VERSION: windowsCodeServerIdentity.componentVersion');
    expect(launcher).not.toContain('Windows WSL2 Source runtime extraction');
  });

  test('authenticates the exact source-derived archive before WSL staging or component repackaging', () => {
    const consumer = repoFile('apps/desktop/scripts/build-windows-app-wsl.sh');
    const identityIndex = consumer.indexOf('code-server-component-identity.mjs');
    const verifierIndex = consumer.indexOf('verify-code-server-archive.mjs');
    const stagingIndex = consumer.indexOf('stage_verified_code_server_archive "$WSL_CODE_SERVER_ARCHIVE"');
    const repackagingIndex = consumer.indexOf('create-deterministic-tar.sh" "$CODE_SERVER_STAGE"');

    expect(identityIndex).toBeGreaterThan(-1);
    expect(verifierIndex).toBeGreaterThan(identityIndex);
    expect(stagingIndex).toBeGreaterThan(verifierIndex);
    expect(repackagingIndex).toBeGreaterThan(verifierIndex);
    expect(consumer).toContain(
      'CODE_SERVER_ARCHIVE_NAME="code-server-$CODE_SERVER_VERSION-$CODE_SERVER_PLATFORM.tar.gz"'
    );
    expect(consumer).toContain('Configured code-server component version does not match its Node payload identity.');
    expect(consumer).toContain('WSL code-server archive identity mismatch: expected $CODE_SERVER_ARCHIVE_NAME.');
    expect(consumer).toContain('--version "$CODE_SERVER_VERSION"');
    expect(consumer).toContain('--platform "$CODE_SERVER_PLATFORM"');
    expect(consumer).toContain(
      'cp "$WSL_CODE_SERVER_ARCHIVE.sha256" "$CODE_SERVER_STAGE/$CODE_SERVER_ARCHIVE_NAME.sha256"'
    );
    expect(consumer).toContain('stage_verified_code_server_archive "$WSL_CODE_SERVER_ARCHIVE"');
    expect(consumer).toContain(
      'CODE_SERVER_ASSET="$COMPONENT_ASSET_DIR/code-server-$CODE_SERVER_VERSION-windows-$RELEASE_ARCH.tar.gz"'
    );
    expect(consumer).toContain('>"$CODE_SERVER_ASSET.sha256"');
    expect(consumer).toContain('--require-sha256-sidecars');
    expect(consumer).toContain('--version "$CODE_SERVER_VERSION"');
    expect(consumer).not.toContain('CODE_SERVER_COMMIT');
    expect(consumer).not.toContain('$CODE_SERVER_COMMIT-p1');
  });

  test('native Windows reseals the exact archive and sidecar and publishes both', () => {
    const builder = repoFile('apps/desktop/scripts/build-windows-app.ps1');
    const publisher = repoFile('tooling/release-gpui/windows.ps1');
    const verifierIndex = builder.indexOf('verify-code-server-archive.mjs');
    const archiveCopyIndex = builder.indexOf(
      'Copy-Item $WslCodeServerArchive (Join-Path $ComponentStage $InnerArchiveName)'
    );
    const sidecarCopyIndex = builder.indexOf('Copy-Item "$WslCodeServerArchive.sha256"');

    expect(verifierIndex).toBeGreaterThan(-1);
    expect(archiveCopyIndex).toBeGreaterThan(verifierIndex);
    expect(sidecarCopyIndex).toBeGreaterThan(archiveCopyIndex);
    expect(builder).toContain('$InnerArchiveName = $ExpectedArchiveName');
    expect(builder).toContain('"$ComponentAsset.sha256"');
    expect(builder).toContain('--require-sha256-sidecars');
    expect(publisher).toContain('$PublishArgs += "--require-sha256-sidecars"');
  });

  test('authenticates every configured, bundled, and on-demand archive before WSL extraction or reuse', () => {
    const componentStore = repoFile('apps/desktop/src/component_store.rs');
    const sourceServer = repoFile('apps/desktop/src/app/helpers/source_server.rs');
    const windowsConsumer = repoFile('apps/desktop/src/windows_terminal_backend.rs');
    const verifyIndex = windowsConsumer.indexOf('crate::component_store::verify_code_server_archive(');
    const extractIndex = windowsConsumer.indexOf('tar -xzf - -C', verifyIndex);
    const outerSidecarDownloadIndex = componentStore.indexOf('if let Some(sidecar_name) = &asset.sha256_sidecar_name');
    const outerSidecarParseIndex = componentStore.indexOf(
      'parse_code_server_checksum_sidecar(&sidecar, &asset.asset_name)',
      outerSidecarDownloadIndex
    );
    const outerUnpackIndex = componentStore.indexOf('unpack_tar_gz(&archive_path', outerSidecarParseIndex);

    expect(verifyIndex).toBeGreaterThan(-1);
    expect(extractIndex).toBeGreaterThan(verifyIndex);
    expect(windowsConsumer).toContain('GHOSTEX_WSL_CODE_SERVER_ARCHIVE');
    expect(windowsConsumer).toContain('bundled_source_runtime_archive');
    expect(windowsConsumer).toContain('store.query_current("code-server")?');
    expect(windowsConsumer).toContain('source-runtime/component-version');
    expect(windowsConsumer).toContain('cleanup_partial_source_runtime_release(');
    expect(windowsConsumer).not.toContain('source-runtime/windows-app-runtime.sha256');
    expect(componentStore).toContain('parse_code_server_checksum_sidecar');
    expect(componentStore).toContain('sha256_sidecar_name');
    expect(componentStore).toContain('sidecar_sha256 != asset.sha256');
    expect(outerSidecarDownloadIndex).toBeGreaterThan(-1);
    expect(outerSidecarParseIndex).toBeGreaterThan(outerSidecarDownloadIndex);
    expect(outerUnpackIndex).toBeGreaterThan(outerSidecarParseIndex);
    expect(componentStore).toContain('Code-server archive checksum mismatch');
    expect(componentStore).toContain('Code-server archive payload is not executable');
    expect(componentStore).toContain('readiness_found');
    expect(sourceServer).toContain('verify_installed_windows_code_server_component');
  });

  test.each([
    ['before package link update', 'package_touched=1', 'false\npackage_touched=1'],
    [
      'after package link update',
      'ln -sfn -- "$final_release" "$package_path"',
      'ln -sfn -- "$final_release" "$package_path"\nfalse',
    ],
    ['before component marker update', 'marker_touched=1', 'false\nmarker_touched=1'],
    [
      'after component marker update',
      'mv -f -- "$marker_next" "$marker_path"',
      'mv -f -- "$marker_next" "$marker_path"\nfalse',
    ],
  ])('rolls back a WSL Source install failure %s', async (_label, needle, injected) => {
    const windowsConsumer = repoFile('apps/desktop/src/windows_terminal_backend.rs');
    const installerSection = windowsConsumer.slice(
      windowsConsumer.indexOf('fn install_packaged_source_runtime('),
      windowsConsumer.indexOf('fn ensure_source_runtime_installed(')
    );
    const scriptMatch = /let script = format!\(\s*r#"([\s\S]*?)"#\s*\);/.exec(installerSection);
    expect(scriptMatch).not.toBeNull();
    const baseScript = scriptMatch[1].replace('{payload_checks}', 'true').replaceAll('{{', '{').replaceAll('}}', '}');
    expect(baseScript.match(new RegExp(needle.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g'))).toHaveLength(1);
    const script = baseScript.replace(needle, injected);
    const cleanupIndex = script.indexOf('trap rollback_install EXIT HUP INT TERM');
    const extractIndex = script.indexOf('tar -xzf - -C "$release_dir"');
    const symlinkIndex = script.indexOf('ln -sfn -- "$final_release" "$package_path"');
    const markerIndex = script.indexOf('mv -f -- "$marker_next" "$marker_path"');
    const finalizeIndex = script.indexOf('rollback_armed=0');
    expect(cleanupIndex).toBeGreaterThan(-1);
    expect(extractIndex).toBeGreaterThan(cleanupIndex);
    expect(symlinkIndex).toBeGreaterThan(extractIndex);
    expect(markerIndex).toBeGreaterThan(symlinkIndex);
    expect(finalizeIndex).toBeGreaterThan(markerIndex);

    const root = await mkdtemp(path.join(tmpdir(), 'ghostex-wsl-source-cleanup-'));
    temporaryRoots.push(root);
    const installRoot = path.join(root, 'source-runtime');
    const oldRelease = path.join(installRoot, 'releases', 'new-version');
    const payload = path.join(root, 'payload');
    const archive = path.join(root, 'payload.tar.gz');
    await mkdir(oldRelease, { recursive: true });
    await writeFile(path.join(oldRelease, 'prior-install'), 'prior payload\n');
    await mkdir(payload, { recursive: true });
    await mkdir(path.join(payload, 'lib'), { recursive: true });
    await mkdir(path.join(payload, 'out', 'node'), { recursive: true });
    await writeFile(path.join(payload, 'lib', 'node'), '#!/bin/sh\nexit 0\n');
    await chmod(path.join(payload, 'lib', 'node'), 0o755);
    await writeFile(path.join(payload, 'out', 'node', 'entry.js'), 'entry\n');
    await writeFile(path.join(installRoot, 'component-version'), 'old-version\n');
    const symlinkResult = spawnSync('ln', ['-s', oldRelease, path.join(installRoot, 'package')]);
    expect(symlinkResult.status).toBe(0);
    const archiveResult = spawnSync(path.resolve('tooling/release-gpui/create-deterministic-tar.sh'), [
      payload,
      archive,
    ]);
    expect(archiveResult.status).toBe(0);

    const installResult = spawnSync('sh', ['-lc', script, 'ghostex-wsl-source-installer', installRoot, 'new-version'], {
      input: readFileSync(archive),
    });
    expect(installResult.status).not.toBe(0);
    expect(readFileSync(path.join(installRoot, 'component-version'), 'utf8')).toBe('old-version\n');
    expect(readlinkSync(path.join(installRoot, 'package'))).toBe(oldRelease);
    expect(readFileSync(path.join(oldRelease, 'prior-install'), 'utf8')).toBe('prior payload\n');
    expect(
      readdirSync(installRoot).filter((entry) => entry.includes('.next-') || entry.includes('.previous-'))
    ).toEqual([]);
    expect(
      readdirSync(path.join(installRoot, 'releases')).filter(
        (entry) => entry.startsWith('.install-') || entry.startsWith('.previous-')
      )
    ).toEqual([]);
  });
});
