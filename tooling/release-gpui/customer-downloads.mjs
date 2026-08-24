const GITHUB_REPOSITORY = "maddada/Ghostex";

export const IOS_DISCORD_URL = "https://discord.gg/df7b3G92CS";

export function renderIosAvailabilityNotes() {
  return [
    "### iOS",
    "",
    `The iOS TestFlight is available through [Discord](${IOS_DISCORD_URL}). Join and post in the iOS channel to get the app.`,
  ].join("\n");
}

function assertVersion(version) {
  if (!/^\d+\.\d+\.\d+$/u.test(version ?? "")) {
    throw new Error(`Version must be MAJOR.MINOR.PATCH, got ${version ?? "nothing"}`);
  }
}

export function customerDownloadUrl(version, assetName) {
  assertVersion(version);
  return `https://github.com/${GITHUB_REPOSITORY}/releases/download/v${version}/${encodeURIComponent(assetName)}`;
}

export function customerDownloadEntries(version, assetNames) {
  assertVersion(version);
  const available = new Set(assetNames ?? []);
  const groups = [
    {
      title: "macOS ARM",
      downloads: [["Download DMG", `ghostex-${version}-arm64.dmg`]],
    },
    {
      title: "Android",
      downloads: [["Download APK", "ghostex-android.apk"]],
    },
    {
      title: "Windows",
      downloads: [
        ["x64 installer", `ghostex-${version}-windows-x64.exe`],
        ["x64 portable", `ghostex-${version}-windows-x64-portable.zip`],
        ["ARM64 installer", `ghostex-${version}-windows-arm64.exe`],
        ["ARM64 portable", `ghostex-${version}-windows-arm64-portable.zip`],
      ],
    },
    {
      title: "Linux",
      downloads: [
        ["x64 Debian package", `ghostex_${version}_amd64.deb`],
        ["x64 RPM package", `ghostex-${version}-1.x86_64.rpm`],
        ["x64 tarball (Arch & other distros, mise/ubi)", `ghostex-${version}-linux-x64.tar.zst`],
      ],
    },
  ];

  return groups
    .map((group) => ({
      ...group,
      downloads: group.downloads
        .filter(([, assetName]) => available.has(assetName))
        .map(([label, assetName]) => ({
          assetName,
          label,
          url: customerDownloadUrl(version, assetName),
        })),
    }))
    .filter((group) => group.downloads.length > 0);
}

export function renderCustomerDownloadNotes(version, assetNames) {
  const groups = customerDownloadEntries(version, assetNames);
  if (groups.length === 0) return "";

  const lines = [`## Download Ghostex ${version}`, ""];
  for (const group of groups) {
    lines.push(`### ${group.title}`, "");
    for (const download of group.downloads) {
      lines.push(`- [${download.label}](${download.url})`);
    }
    lines.push("");
    if (group.title === "Android") {
      lines.push(renderIosAvailabilityNotes(), "");
    }
  }
  return lines.join("\n").trimEnd();
}

export function mergeCustomerDownloadNotes(body, version, assetNames) {
  const normalized = String(body ?? "").replaceAll("\r\n", "\n").trimEnd();
  if (!normalized) throw new Error("Existing release notes are empty");

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
  return `${prose}${downloads ? `\n\n${downloads}` : ""}\n`;
}
