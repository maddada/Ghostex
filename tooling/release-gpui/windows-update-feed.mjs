const SUPPORTED_WINDOWS_ARCHES = new Set(["x64", "arm64"]);

function requireString(value, field) {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`Windows update feed has no ${field}.`);
  }
  return value.trim();
}

function normalizeArtifacts(artifacts) {
  if (artifacts instanceof Map) return artifacts;
  return new Map((artifacts ?? []).map((artifact) => [artifact.name, artifact]));
}

export function windowsUpdateArtifactNames(version, arch) {
  if (!/^\d+\.\d+\.\d+$/u.test(version)) throw new Error(`Invalid Windows update version: ${version}`);
  if (!SUPPORTED_WINDOWS_ARCHES.has(arch)) throw new Error(`Unsupported Windows update architecture: ${arch}`);
  const channel = `win-${arch}-stable`;
  return {
    channel,
    deltaPackage: `Ghostex-${version}-${channel}-delta.nupkg`,
    feed: `releases.${channel}.json`,
    fullPackage: `Ghostex-${version}-${channel}-full.nupkg`,
    installer: `ghostex-${version}-windows-${arch}.exe`,
    portable: `ghostex-${version}-windows-${arch}-portable.zip`,
  };
}

function validatePackageEntry({ artifact, entry, expectedType, version }) {
  if (entry.PackageId !== "Ghostex") {
    throw new Error(`${artifact.name} feed entry has package ID ${entry.PackageId ?? "missing"}; expected Ghostex.`);
  }
  if (entry.Version !== version) {
    throw new Error(`${artifact.name} feed entry has version ${entry.Version ?? "missing"}; expected ${version}.`);
  }
  if (typeof entry.Type !== "string" || entry.Type.toLowerCase() !== expectedType.toLowerCase()) {
    throw new Error(`${artifact.name} feed entry has type ${entry.Type ?? "missing"}; expected ${expectedType}.`);
  }
  if (!/^[0-9a-f]{40}$/iu.test(entry.SHA1 ?? "")) {
    throw new Error(`${artifact.name} feed entry has an invalid SHA1.`);
  }
  if (!/^[0-9a-f]{64}$/iu.test(entry.SHA256 ?? "")) {
    throw new Error(`${artifact.name} feed entry has an invalid SHA256.`);
  }
  if (entry.SHA256.toLowerCase() !== artifact.sha256?.toLowerCase()) {
    throw new Error(`${artifact.name} feed SHA256 does not match the packaged artifact.`);
  }
  if (Number(entry.Size) !== Number(artifact.size)) {
    throw new Error(`${artifact.name} feed size ${entry.Size ?? "missing"} does not match ${artifact.size}.`);
  }
  if (expectedType.toLowerCase() === "full") {
    requireString(entry.NotesMarkdown, `${artifact.name} release notes`);
  }
}

export function validateWindowsUpdateFeed({ arch, artifacts, feedText, version }) {
  const names = windowsUpdateArtifactNames(version, arch);
  const byName = normalizeArtifacts(artifacts);
  for (const required of [names.installer, names.portable, names.feed, names.fullPackage]) {
    if (!byName.has(required)) throw new Error(`Windows ${arch} update channel is missing ${required}.`);
  }

  let feed;
  try {
    feed = JSON.parse(feedText);
  } catch (error) {
    throw new Error(`Windows ${arch} update feed is not valid JSON: ${error.message}`);
  }
  if (!Array.isArray(feed.Assets)) {
    throw new Error(`Windows ${arch} update feed has no Assets array.`);
  }

  const currentEntries = feed.Assets.filter((entry) => entry?.Version === version);
  const fullEntry = currentEntries.find((entry) => entry.FileName === names.fullPackage);
  if (!fullEntry) throw new Error(`${names.feed} does not reference ${names.fullPackage}.`);
  validatePackageEntry({
    artifact: byName.get(names.fullPackage),
    entry: fullEntry,
    expectedType: "Full",
    version,
  });

  const deltaArtifact = byName.get(names.deltaPackage);
  const deltaEntry = currentEntries.find((entry) => entry.FileName === names.deltaPackage);
  if (Boolean(deltaArtifact) !== Boolean(deltaEntry)) {
    throw new Error(`${names.feed} and the release disagree about ${names.deltaPackage}.`);
  }
  if (deltaArtifact) {
    validatePackageEntry({ artifact: deltaArtifact, entry: deltaEntry, expectedType: "Delta", version });
  }

  for (const entry of currentEntries) {
    if (entry.PackageId !== "Ghostex") continue;
    if (!byName.has(entry.FileName)) {
      throw new Error(`${names.feed} references unpublished current package ${entry.FileName}.`);
    }
  }
  return {
    channel: names.channel,
    delta: Boolean(deltaArtifact),
    fullPackage: names.fullPackage,
  };
}
