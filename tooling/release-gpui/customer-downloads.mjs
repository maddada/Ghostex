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
  return `iOS: TestFlight through [Discord](${IOS_DISCORD_URL}) (post in the iOS channel)`;
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
  /** @type {(label: string, assetName: string) => { assetName?: string, label: string, url?: string }} */
  const asset = (label, assetName) => ({ assetName, label });
  /** @type {(label: string, url: string) => { assetName?: string, label: string, url?: string }} */
  const link = (label, url) => ({ label, url });
  const groups = [
    {
      line: 1,
      title: 'macOS',
      downloads: [asset('Apple Silicon DMG', `ghostex-${version}-arm64.dmg`)],
    },
    {
      line: 1,
      title: 'Windows',
      downloads: [
        asset('x64 installer', `ghostex-${version}-windows-x64.exe`),
        asset('ARM64 installer', `ghostex-${version}-windows-arm64.exe`),
      ],
    },
    {
      line: 2,
      title: 'Linux x64',
      downloads: [
        asset('.deb', `ghostex_${version}_amd64.deb`),
        asset('.rpm', `ghostex-${version}-1.x86_64.rpm`),
        asset('tarball (mise/ubi)', `ghostex-${version}-linux-x64.tar.zst`),
        asset('AppImage', `ghostex-${version}-linux-x64.AppImage`),
        link('AUR (ghostex-bin)', AUR_PACKAGE_URL),
      ],
    },
    /*
     CDXC:Release 2026-10-19 DECISION:
     User: Linux ARM64 ships the same three packages as x64 (.deb, .rpm, tarball), starting with 10.27.0.
     They share the Linux line as their own group; the AUR package is x86_64 only, so it stays with x64.
    */
    {
      line: 2,
      title: 'Linux ARM64',
      downloads: [
        asset('.deb', `ghostex_${version}_arm64.deb`),
        asset('.rpm', `ghostex-${version}-1.aarch64.rpm`),
        asset('tarball', `ghostex-${version}-linux-arm64.tar.zst`),
        asset('AppImage', `ghostex-${version}-linux-arm64.AppImage`),
      ],
    },
    {
      line: 3,
      title: 'Android',
      downloads: [asset('APK', 'ghostex-android.apk')],
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

export const DOWNLOADS_START = '<!-- ghostex-downloads:start -->';
const DOWNLOADS_END = '<!-- ghostex-downloads:end -->';

/*
 CDXC:Release 2026-10-04 DECISION:
 User: "i want just download links at the top pls instead of bottom but should be max 3 lines not tons of lines".
 The release body opens with one marked block of at most three lines (macOS and Windows, Linux, phones), each platform's
 links inline and separated by " · ", followed by the CHANGELOG notes. Platforms publish on their own, so every
 publish or amend regenerates the block from the live asset list between its markers instead of appending, and strips
 the old bottom "## Download Ghostex" section from bodies written before this change. Checksums and provenance stay
 release assets without links.
*/
export function renderCustomerDownloadNotes(version, assetNames) {
  const groups = customerDownloadEntries(version, assetNames);
  if (groups.length === 0) return '';

  /** @type {Map<number, string[]>} */
  const lines = new Map();
  for (const group of groups) {
    const links = group.downloads.map((download) => `[${download.label}](${download.url})`).join(' · ');
    const parts = lines.get(group.line) ?? [];
    parts.push(`**${group.title}:** ${links}`);
    if (group.title === 'Android') parts.push(renderIosAvailabilityNotes());
    lines.set(group.line, parts);
  }
  const rendered = [...lines.keys()]
    .sort((a, b) => a - b)
    .map((line) => /** @type {string[]} */ (lines.get(line)).join(' · '));
  // A trailing backslash is a GFM hard line break, so the lines stay separate without blank lines between them.
  const body = rendered.map((line, index) => (index < rendered.length - 1 ? `${line}\\` : line)).join('\n');
  return `${DOWNLOADS_START}\n${body}\n${DOWNLOADS_END}`;
}

export function mergeCustomerDownloadNotes(body, version, assetNames) {
  let normalized = String(body ?? '').replaceAll('\r\n', '\n');
  const blockStart = normalized.indexOf(DOWNLOADS_START);
  if (blockStart >= 0) {
    const blockEnd = normalized.indexOf(DOWNLOADS_END, blockStart);
    if (blockEnd < 0) throw new Error('Existing release notes have an unterminated download block');
    normalized = `${normalized.slice(0, blockStart)}${normalized.slice(blockEnd + DOWNLOADS_END.length)}`;
  }
  normalized = normalized.trim();
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
  return `${downloads ? `${downloads}\n\n` : ''}${prose}\n`;
}
