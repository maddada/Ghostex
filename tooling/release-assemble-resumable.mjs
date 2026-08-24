#!/usr/bin/env node
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  assetSha256,
  findAsset,
  getRelease,
  markPublished,
  RELEASE_REPO,
  run,
  validateStagedRelease,
} from './release-state-lib.mjs';

const [version, requestedSourceSha] = process.argv.slice(2);
const updateSparkle = process.env.GHOSTEX_RELEASE_UPDATE_SPARKLE !== '0';
const {
  completed,
  release: initialRelease,
  state,
} = validateStagedRelease(version, {
  requireComplete: true,
  sourceSha: requestedSourceSha,
});

if (updateSparkle !== Boolean(state.sparkle.requested)) {
  throw new Error(`Sparkle input ${updateSparkle} does not match durable release state ${state.sparkle.requested}`);
}
if (state.macos_notarization?.status !== 'accepted' || state.macos_notarization?.stapled !== true) {
  throw new Error('macOS state does not prove accepted notarization and stapling');
}

const temporary = mkdtempSync(path.join(os.tmpdir(), `ghostex-assemble-${version}-`));
try {
  const deliverables = state.expected.map((name) => {
    const asset = findAsset(initialRelease, name);
    if (!asset) throw new Error(`Missing expected deliverable ${name}`);
    const target = path.join(temporary, name);
    const response = run(
      'gh',
      ['release', 'download', `v${version}`, '--repo', RELEASE_REPO, '--pattern', name, '--dir', temporary],
      { allowFailure: true }
    );
    if (response.status !== 0 || !existsSync(target)) throw new Error(`Could not download staged asset ${name}`);
    return { asset, name, path: target, sha256: assetSha256(asset) };
  });

  const apk = deliverables.find((entry) => entry.name === 'ghostex-android.apk');
  if (apk) {
    run('unzip', ['-tqq', apk.path]);
    const androidHome = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT;
    if (!androidHome) throw new Error('ANDROID_HOME is required for independent APK signature verification');
    const apkSigner = path.join(
      androidHome,
      'build-tools',
      '36.0.0',
      process.platform === 'win32' ? 'apksigner.bat' : 'apksigner'
    );
    if (!existsSync(apkSigner)) throw new Error(`Pinned apksigner is missing: ${apkSigner}`);
    run(apkSigner, ['verify', '--verbose', '--print-certs', apk.path]);
  }

  for (const [arch, marker] of [
    ['x64', 'x86-64'],
    ['arm64', 'ARM aarch64'],
  ]) {
    const archive = deliverables.find((entry) => entry.name === `gxserver-linux-${arch}.tar.gz`);
    run('tar', ['-tzf', archive.path]);
    const extract = path.join(temporary, `gxserver-${arch}`);
    run('mkdir', ['-p', extract]);
    run('tar', ['-xzf', archive.path, '-C', extract, './bin/gxserver']);
    const fileOutput = run('file', [path.join(extract, 'bin/gxserver')], { capture: true }).stdout;
    if (!fileOutput.includes(marker)) throw new Error(`${archive.name} architecture is wrong: ${fileOutput}`);
  }

  const dmg = deliverables.find((entry) => entry.name === `ghostex-${version}-arm64.dmg`);
  if (process.platform !== 'darwin') throw new Error('The assembler must run on macOS to independently verify the DMG');
  run('xcrun', ['stapler', 'validate', dmg.path]);
  const mountPoint = path.join(temporary, 'mounted-dmg');
  run('mkdir', ['-p', mountPoint]);
  run('hdiutil', ['attach', '-nobrowse', '-readonly', '-mountpoint', mountPoint, dmg.path]);
  try {
    const appPath = path.join(mountPoint, 'Ghostex.app');
    const infoPlist = path.join(appPath, 'Contents', 'Info.plist');
    if (!existsSync(infoPlist)) throw new Error('DMG does not contain Ghostex.app');
    const bundleName = run('plutil', ['-extract', 'CFBundleName', 'raw', infoPlist], { capture: true }).stdout;
    const bundleVersion = run('plutil', ['-extract', 'CFBundleShortVersionString', 'raw', infoPlist], {
      capture: true,
    }).stdout;
    if (bundleName !== 'Ghostex' || bundleVersion !== version) {
      throw new Error(`DMG bundle identity is ${bundleName} ${bundleVersion}; expected Ghostex ${version}`);
    }
    run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', appPath]);
  } finally {
    run('hdiutil', ['detach', mountPoint]);
  }

  const macosRunId = completed['macos-arm64'].run_id;
  const macosArtifactSourceSha = state.source_compatibility?.['macos-arm64']?.built_source_sha ?? state.source_sha;
  const macosArtifactName = `${version}-${macosArtifactSourceSha}-macos-final`;
  const macosPrivate = path.join(temporary, 'macos-final');
  run('gh', [
    'run',
    'download',
    String(macosRunId),
    '--repo',
    RELEASE_REPO,
    '--name',
    macosArtifactName,
    '--dir',
    macosPrivate,
  ]);
  const generatedAppcast = path.join(macosPrivate, 'appcast.xml');
  if (updateSparkle && !existsSync(generatedAppcast))
    throw new Error('The preserved macOS result is missing appcast.xml');

  if (updateSparkle) {
    const xml = readFileSync(generatedAppcast, 'utf8');
    const buildNumber = version
      .split('.')
      .map(Number)
      .reduce((value, part, index) => value + part * [10000, 100, 1][index], 0);
    const appcastBuild = run(
      'xmllint',
      ['--xpath', "string((//*[local-name()='item'][1]/*[local-name()='version'])[1])", generatedAppcast],
      { capture: true }
    ).stdout;
    const appcastUrl = run(
      'xmllint',
      ['--xpath', "string((//*[local-name()='item'][1]/*[local-name()='enclosure']/@url)[1])", generatedAppcast],
      { capture: true }
    ).stdout;
    const expectedUrl = `https://github.com/${RELEASE_REPO}/releases/download/v${version}/ghostex-${version}-arm64.dmg`;
    if (appcastBuild !== String(buildNumber) || appcastUrl !== expectedUrl) {
      throw new Error('Generated Sparkle feed does not point to the staged DMG and build number');
    }
    const signature = xml.match(/sparkle:edSignature="([^"]+)"/)?.[1];
    if (!signature) throw new Error('Generated Sparkle feed has no EdDSA signature');
    if (!process.env.SPARKLE_PRIVATE_KEY)
      throw new Error('SPARKLE_PRIVATE_KEY is required for independent signature verification');
    const sparkleRoot = run('bash', ['tooling/release-gpui/prepare-sparkle.sh'], { capture: true }).stdout;
    run(path.join(sparkleRoot, 'bin/sign_update'), ['--ed-key-file', '-', '--verify', dmg.path, signature], {
      input: process.env.SPARKLE_PRIVATE_KEY,
    });
  }

  const changelog = run('git', ['show', `${state.source_sha}:CHANGELOG.md`], { capture: true }).stdout;
  const sectionStart = changelog.indexOf(`## ${version} -`);
  if (sectionStart < 0) throw new Error(`CHANGELOG.md at source_sha has no ${version} section`);
  const nextSection = changelog.indexOf('\n## ', sectionStart + 4);
  const notes = [
    changelog.slice(sectionStart, nextSection < 0 ? undefined : nextSection).trim(),
    '',
    '> **Upgrade note:** Quit all running Ghostex instances before installing this release so the shared-state migration completes before an older build can write state.',
    '',
    '## Verified downloads',
    '',
    ...deliverables.map((entry) => `- \`${entry.name}\` — SHA256 \`${entry.sha256}\``),
    '',
  ].join('\n');
  const notesPath = path.join(temporary, 'release-notes.md');
  writeFileSync(notesPath, notes);

  let release = getRelease(version);
  if (release.draft) {
    run('gh', ['release', 'edit', `v${version}`, '--repo', RELEASE_REPO, '--notes-file', notesPath]);
    run('gh', ['release', 'edit', `v${version}`, '--repo', RELEASE_REPO, '--draft=false']);
    release = getRelease(version);
  }
  if (release.draft) throw new Error(`v${version} is still a draft after publication`);
  const remoteTagLines = run('git', ['ls-remote', 'origin', `refs/tags/v${version}`, `refs/tags/v${version}^{}`], {
    capture: true,
  })
    .stdout.split(/\r?\n/)
    .filter(Boolean);
  const peeled = remoteTagLines.find((line) => line.endsWith(`refs/tags/v${version}^{}`));
  const direct = remoteTagLines.find((line) => line.endsWith(`refs/tags/v${version}`));
  const tagCommit = (peeled ?? direct)?.split(/\s+/)[0];
  if (tagCommit !== state.source_sha)
    throw new Error(`Published tag resolves to ${tagCommit ?? 'missing'}; expected ${state.source_sha}`);

  // Re-read every live digest after publication. Sparkle remains untouched until
  // this public-download proof succeeds.
  const liveValidation = validateStagedRelease(version, { requireComplete: true, sourceSha: state.source_sha });
  if (liveValidation.release.draft) throw new Error('Live release verification unexpectedly returned a draft');
  markPublished(version, { githubPublished: true });

  if (updateSparkle) {
    const remoteMainBefore = run('git', ['ls-remote', 'origin', 'refs/heads/main'], { capture: true }).stdout.split(
      /\s+/
    )[0];
    const localMain = run('git', ['rev-parse', 'HEAD'], { capture: true }).stdout;
    if (remoteMainBefore !== localMain)
      throw new Error(`main moved before Sparkle publication (${localMain} -> ${remoteMainBefore})`);
    const current = existsSync('appcast.xml') ? readFileSync('appcast.xml', 'utf8') : '';
    const next = readFileSync(generatedAppcast, 'utf8');
    if (current !== next) {
      writeFileSync('appcast.xml', next);
      run('git', ['config', 'user.name', 'github-actions[bot]']);
      run('git', ['config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com']);
      run('git', ['add', 'appcast.xml']);
      run('git', ['commit', '-m', `chore: publish Sparkle ${version}`]);
      run('git', ['push', 'origin', 'HEAD:main']);
    }
    let liveAppcast = '';
    // raw.githubusercontent.com can lag a successful main push by more than a
    // minute. Keep the check bounded, but allow the CDN enough time to expose
    // the appcast that production Sparkle clients will actually fetch.
    for (let attempt = 0; attempt < 60; attempt += 1) {
      const response = run(
        'curl',
        [
          '-fsSL',
          `https://raw.githubusercontent.com/${RELEASE_REPO}/main/appcast.xml?release=${version}&attempt=${attempt}`,
        ],
        {
          allowFailure: true,
          capture: true,
        }
      );
      if (response.status === 0 && response.stdout.includes(`ghostex-${version}-arm64.dmg`)) {
        liveAppcast = response.stdout;
        break;
      }
      run('sleep', ['5']);
    }
    if (!liveAppcast) throw new Error(`Production Sparkle feed did not advance to ${version} within 5 minutes`);
    markPublished(version, { sparklePublished: true });
  }

  console.log(`Assembled and verified v${version}; no package build ran in this workflow.`);
} finally {
  rmSync(temporary, { force: true, recursive: true });
}
