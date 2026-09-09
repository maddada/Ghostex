const GITHUB_REPOSITORY = 'maddada/Ghostex';

export const IOS_DISCORD_URL = 'https://discord.gg/df7b3G92CS';

/*
 CDXC:Release 2026-09-10 DECISION:
 User: the Linux downloads list the AUR package alongside the release assets.
 It is a rolling package rather than an uploaded asset, so it carries its own
 URL and is not filtered against the release's asset names; it only appears
 when the release actually shipped Linux builds.
*/
export const AUR_PACKAGE_URL = 'https://aur.archlinux.org/packages/ghostex-bin';

export function renderIosAvailabilityNotes() {
  return [
    '### iOS',
    '',
    `The iOS TestFlight is available through [Discord](${IOS_DISCORD_URL}). Join and post in the iOS channel to get the app.`,
  ].join('\n');
}

function assertVersion(version) {
  if (!/^\d+\.\d+\.\d+$/u.test(version ?? '')) {
    throw new Error(`Version must be MAJOR.MINOR.PATCH, got ${version ?? 'nothing'}`);
  }
}

export function customerDownloadUrl(version, assetName) {
  assertVersion(version);
  return `https://github.com/${GITHUB_REPOSITORY}/releases/download/v${version}/${encodeURIComponent(assetName)}`;
}

export function customerDownloadEntries(version, assetNames) {
  assertVersion(version);
  const available = new Set(assetNames ?? []);
  const asset = (label, assetName) => ({ assetName, label });
  const link = (label, url) => ({ label, url });
  const groups = [
    {
      title: 'macOS ARM',
      downloads: [asset('Download DMG', `ghostex-${version}-arm64.dmg`)],
    },
    {
      title: 'Android',
      downloads: [asset('Download APK', 'ghostex-android.apk')],
    },
    {
      title: 'Windows',
      downloads: [
        asset('x64 installer', `ghostex-${version}-windows-x64.exe`),
        asset('x64 portable', `ghostex-${version}-windows-x64-portable.zip`),
        asset('ARM64 installer', `ghostex-${version}-windows-arm64.exe`),
        asset('ARM64 portable', `ghostex-${version}-windows-arm64-portable.zip`),
      ],
    },
    {
      title: 'Linux',
      downloads: [
        asset('x64 Debian package', `ghostex_${version}_amd64.deb`),
        asset('x64 RPM package', `ghostex-${version}-1.x86_64.rpm`),
        link('AUR link (ghostex-bin)', AUR_PACKAGE_URL),
        asset('x64 tarball (Arch & other distros, mise/ubi)', `ghostex-${version}-linux-x64.tar.zst`),
      ],
    },
  ];

  return groups
    .map((group) => ({
      ...group,
      downloads: group.downloads
        .filter((download) => download.assetName === undefined || available.has(download.assetName))
        .map((download) => ({
          ...download,
          url: download.url ?? customerDownloadUrl(version, download.assetName),
        })),
    }))
    .filter((group) => group.downloads.some((download) => download.assetName !== undefined));
}

export function renderCustomerDownloadNotes(version, assetNames) {
  const groups = customerDownloadEntries(version, assetNames);
  if (groups.length === 0) return '';

  const lines = [`## Download Ghostex ${version}`, ''];
  for (const group of groups) {
    lines.push(`### ${group.title}`, '');
    for (const download of group.downloads) {
      lines.push(`- [${download.label}](${download.url})`);
    }
    lines.push('');
    if (group.title === 'Android') {
      lines.push(renderIosAvailabilityNotes(), '');
    }
  }
  return lines.join('\n').trimEnd();
}

export function mergeCustomerDownloadNotes(body, version, assetNames) {
  const normalized = String(body ?? '')
    .replaceAll('\r\n', '\n')
    .trimEnd();
  if (!normalized) throw new Error('Existing release notes are empty');

  const removableHeadings = [
    /^## Downloads\s*$/mu,
    /^## Download Ghostex \d+\.\d+\.\d+\s*$/mu,
    /^## Build provenance\s*$/mu,
  ];
  const cutAt = removableHeadings.reduce((earliest, pattern) => {
    const match = pattern.exec(normalized);
    return match && match.index < earliest ? match.index : earliest;
  }, normalized.length);
  const prose = normalized.slice(0, cutAt).trimEnd();
  const downloads = renderCustomerDownloadNotes(version, assetNames);
  return `${prose}${downloads ? `\n\n${downloads}` : ''}\n`;
}
