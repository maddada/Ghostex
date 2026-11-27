use super::catalog::Definition;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

const FOUND_TTL: Duration = Duration::from_secs(60 * 60);
const MISSING_TTL: Duration = Duration::from_secs(5 * 60);

static CACHE: LazyLock<Mutex<HashMap<String, (Instant, Option<String>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// CDXC:AgentProviders 2026-09-28 WHY:
/// "Update available" compares the installed version with the vendor's own release channel (`latestVersion` in the catalog), because npm lags behind the native installers that Ghostex now installs by default. Agents without a channel fall back to their npm package; agents with neither (Cursor's dated builds) never claim an update. Answers are cached for an hour so opening Settings does not hit every vendor.
///
/// CDXC:AgentProviders 2026-10-17 WHY:
/// Antigravity CLI has no npm package; its installers (`install.sh`, `install.ps1`) read the version from the auto-updater's per-platform manifest (`manifests/<os>_<arch>.json`).
///
/// CDXC:AgentProviders 2026-11-27 DECISION:
/// User (Q5213): check for a newer agy the way other ACP clients now do. They read the same
/// auto-updater manifest, but this computer's own (`darwin_arm64`, `linux_amd64_musl`, ...: what
/// agy's installer reads here), at most once an hour whether it answered or not, and believe it
/// only when it is a whole manifest: a plain `major.minor.patch`, a download for this platform on
/// Google's public Antigravity bucket, and a SHA-512. Before, Ghostex read `linux_amd64` on every
/// computer, asked again 5 minutes after a failed read, and took any `version` field. When it
/// runs is unchanged: when Settings or Updates asks (Updates on the schedule the user sets there).
/// No account or credential goes with the request.
pub(crate) async fn latest(definition: &'static Definition) -> Option<String> {
    let (url, field) = match (&definition.latest_version, &definition.npm_package) {
        (Some(source), _) => (source.url.clone(), source.json_field.clone()),
        (None, Some(package)) => (
            format!("https://registry.npmjs.org/{package}/latest"),
            Some("version".to_string()),
        ),
        (None, None) => return None,
    };
    let agy = match url.contains(AGY_TARGET) {
        true => Some(AgyTarget::here()?),
        false => None,
    };
    if let Some((at, value)) = CACHE.lock().ok()?.get(&definition.agent_id).cloned() {
        let ttl = if value.is_some() || agy.is_some() {
            FOUND_TTL
        } else {
            MISSING_TTL
        };
        if at.elapsed() < ttl {
            return value;
        }
    }
    // One agy manifest read at a time, and at most one an hour across gxserver restarts.
    let _one_agy_read = match agy {
        Some(_) => Some(AGY_READ.lock().await),
        None => None,
    };
    let record = agy.map(|_| agy_record_path());
    if let Some(value) = record.as_deref().and_then(recent_agy_read) {
        if let Ok(mut cache) = CACHE.lock() {
            cache.insert(definition.agent_id.clone(), (Instant::now(), value.clone()));
        }
        return value;
    }
    let value = tokio::task::spawn_blocking(move || match agy {
        Some(target) => {
            let body = get(&url.replace(AGY_TARGET, &target.manifest_name()))?;
            target.version_in_manifest(&body)
        }
        None => fetch(&url, field.as_deref()),
    })
    .await
    .ok()
    .flatten();
    if let Some(record) = record.as_deref() {
        save_agy_read(record, value.as_deref());
    }
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(definition.agent_id.clone(), (Instant::now(), value.clone()));
    }
    value
}

/// CDXC:AgentProviders 2026-11-27 WHY: The Fable re-review found the hourly limit on the agy
/// manifest read was kept in memory only, so every gxserver restart read it again and two reads at
/// once both fetched. The last read's time and answer are kept on disk beside the lineup marker
/// (`agy-lineup-version.txt`), and one read runs at a time.
static AGY_READ: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

fn agy_record_path() -> std::path::PathBuf {
    crate::paths::get_gxserver_paths(None)
        .app_cache_dir
        .join("agy-latest-read.json")
}

/// The answer of the last agy manifest read kept at `path`, when it was made within
/// [`FOUND_TTL`]: `Some(None)` for a read that found no version.
fn recent_agy_read(path: &std::path::Path) -> Option<Option<String>> {
    let record: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let at = std::time::UNIX_EPOCH + Duration::from_secs(record.get("readAt")?.as_u64()?);
    // A time ahead of the clock is not trusted to hold reads back.
    let age = std::time::SystemTime::now().duration_since(at).ok()?;
    (age < FOUND_TTL).then(|| {
        record
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn save_agy_read(path: &std::path::Path, version: Option<&str>) {
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        path,
        serde_json::json!({ "readAt": at, "version": version }).to_string(),
    );
}

/// The version npm's `latest` tag names for `package`, from a blocking context, cached like
/// [`latest`] (keyed by package, so it never shares an agent's entry).
pub(crate) fn npm_latest_blocking(package: &str) -> Option<String> {
    let key = format!("npm:{package}");
    if let Some((at, value)) = CACHE.lock().ok()?.get(&key).cloned() {
        let ttl = if value.is_some() {
            FOUND_TTL
        } else {
            MISSING_TTL
        };
        if at.elapsed() < ttl {
            return value;
        }
    }
    let value = fetch(
        &format!("https://registry.npmjs.org/{package}/latest"),
        Some("version"),
    );
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, (Instant::now(), value.clone()));
    }
    value
}

/// Where the catalog's Antigravity manifest address names this computer's platform.
const AGY_TARGET: &str = "{agyTarget}";

/// The platform agy's installer downloads for on this computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AgyTarget {
    /// `darwin`, `linux` or `windows`.
    os: &'static str,
    arm: bool,
    musl: bool,
}

impl AgyTarget {
    fn here() -> Option<Self> {
        let os = match std::env::consts::OS {
            "macos" => "darwin",
            "linux" => "linux",
            "windows" => "windows",
            _ => return None,
        };
        let arm = match std::env::consts::ARCH {
            "aarch64" => true,
            "x86_64" => false,
            _ => return None,
        };
        let musl = os == "linux"
            && std::path::Path::new(if arm {
                "/lib/libc.musl-aarch64.so.1"
            } else {
                "/lib/libc.musl-x86_64.so.1"
            })
            .exists();
        Some(Self { os, arm, musl })
    }

    /// `darwin_arm64`, `linux_amd64_musl`, `windows_amd64`.
    fn manifest_name(self) -> String {
        format!(
            "{}_{}{}",
            self.os,
            if self.arm { "arm64" } else { "amd64" },
            if self.musl { "_musl" } else { "" }
        )
    }

    /// The release a manifest names, only when the manifest is whole: a plain `major.minor.patch`,
    /// a download for this platform on Google's public Antigravity bucket
    /// (`.../antigravity-cli/<version>-<build>/darwin-arm/cli_mac_arm64.tar.gz`), and a SHA-512.
    fn version_in_manifest(self, body: &str) -> Option<String> {
        let manifest: Value = serde_json::from_str(body).ok()?;
        let version = manifest.get("version")?.as_str()?;
        let plain = version.split('.').count() == 3
            && version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
        let sha512 = manifest.get("sha512")?.as_str()?;
        let hashed = sha512.len() == 128 && sha512.bytes().all(|b| b.is_ascii_hexdigit());
        let url = manifest.get("url")?.as_str()?;
        let prefix =
            format!("https://storage.googleapis.com/antigravity-public/antigravity-cli/{version}-");
        let rest = url.strip_prefix(&prefix)?;
        let (build, file) = rest.split_once('/')?;
        let built = !build.is_empty() && build.bytes().all(|b| b.is_ascii_digit());
        let windows = self.os == "windows";
        let suffix = if self.musl { "_musl" } else { "" };
        let expected = format!(
            "{}-{}{}/cli_{}_{}{suffix}.{}",
            self.os,
            if self.arm { "arm" } else { "x64" },
            if self.musl { "-musl" } else { "" },
            match self.os {
                "darwin" => "mac",
                other => other,
            },
            if self.arm { "arm64" } else { "x64" },
            if windows { "exe" } else { "tar.gz" },
        );
        (plain && hashed && built && file == expected).then(|| version.to_string())
    }
}

/// The body at `url`, within 4 seconds.
fn get(url: &str) -> Option<String> {
    let agent: ureq::Agent = crate::http_agent::config()
        .timeout_global(Some(Duration::from_secs(4)))
        .proxy(ureq::Proxy::try_from_env())
        .build()
        .into();
    agent
        .get(url)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()
}

fn fetch(url: &str, field: Option<&str>) -> Option<String> {
    let body = get(url)?;
    let text = match field {
        Some(field) => serde_json::from_str::<Value>(&body)
            .ok()?
            .get(field)?
            .as_str()?
            .to_string(),
        None => body,
    };
    version_in(&text)
}

/// The first `major.minor.patch` (with any pre-release suffix) in a version line such as
/// `2.1.283 (Claude Code)`, `codex-cli 0.157.1` or `rust-v0.157.1`.
pub(crate) fn version_in(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if bytes[start].is_ascii_digit() && (start == 0 || !bytes[start - 1].is_ascii_digit()) {
            let rest = &text[start..];
            let end = rest
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '-'))
                .unwrap_or(rest.len());
            let candidate = rest[..end].trim_end_matches(['.', '-']);
            if numbers(candidate).len() >= 3 {
                return Some(candidate.to_string());
            }
        }
        start += 1;
    }
    None
}

/// True when `latest` is a strictly higher release than `current`. Pre-release suffixes are ignored, so a
/// build ahead of the channel never shows as outdated.
pub(crate) fn is_newer(latest: &str, current: &str) -> bool {
    let latest = numbers(latest);
    let current = numbers(current);
    if latest.len() < 3 || current.len() < 3 {
        return false;
    }
    latest[..3] > current[..3]
}

fn numbers(version: &str) -> Vec<u64> {
    version
        .split('-')
        .next()
        .unwrap_or_default()
        .split('.')
        .map_while(|part| part.parse().ok())
        .collect()
}

#[cfg(test)]
mod agy_manifest_tests {
    use super::AgyTarget;

    const MAC_ARM: AgyTarget = AgyTarget {
        os: "darwin",
        arm: true,
        musl: false,
    };

    fn manifest(version: &str, url: &str, sha512: &str) -> String {
        serde_json::json!({ "version": version, "url": url, "sha512": sha512 }).to_string()
    }

    #[test]
    fn the_manifest_is_this_computers_own() {
        assert_eq!(MAC_ARM.manifest_name(), "darwin_arm64");
        let linux = AgyTarget {
            os: "linux",
            arm: false,
            musl: true,
        };
        assert_eq!(linux.manifest_name(), "linux_amd64_musl");
    }

    #[test]
    fn only_a_whole_manifest_names_a_release() {
        let hash = "a".repeat(128);
        let good = "https://storage.googleapis.com/antigravity-public/antigravity-cli/1.5.13-4021/darwin-arm/cli_mac_arm64.tar.gz";
        assert_eq!(
            MAC_ARM.version_in_manifest(&manifest("1.5.13", good, &hash)),
            Some("1.5.13".to_string())
        );
        for (version, url, sha512) in [
            ("1.5.13-rc1", good, hash.as_str()),
            ("1.5", good, hash.as_str()),
            ("1.5.13", good, "abc"),
            ("1.5.13", "https://example.com/agy/1.5.13-4021/darwin-arm/cli_mac_arm64.tar.gz", hash.as_str()),
            ("1.5.13", "https://storage.googleapis.com/antigravity-public/antigravity-cli/1.5.13-4021/linux-x64/cli_linux_x64.tar.gz", hash.as_str()),
            ("1.5.14", good, hash.as_str()),
        ] {
            assert_eq!(
                MAC_ARM.version_in_manifest(&manifest(version, url, sha512)),
                None,
                "{version} {url}"
            );
        }
        assert_eq!(MAC_ARM.version_in_manifest("not json"), None);
    }
}

#[cfg(test)]
mod agy_read_record_tests {
    use super::{recent_agy_read, save_agy_read};

    /// The hour between agy manifest reads holds across a gxserver restart: the last read is kept
    /// on disk, a failed read too.
    #[test]
    fn the_last_agy_read_is_kept_on_disk() {
        let temp = tempfile::tempdir().unwrap();
        let record = temp.path().join("cache/agy-latest-read.json");
        assert_eq!(recent_agy_read(&record), None);
        save_agy_read(&record, Some("1.5.13"));
        assert_eq!(recent_agy_read(&record), Some(Some("1.5.13".to_string())));
        save_agy_read(&record, None);
        assert_eq!(recent_agy_read(&record), Some(None));
    }

    #[test]
    fn an_old_or_future_read_does_not_hold_reads_back() {
        let temp = tempfile::tempdir().unwrap();
        let record = temp.path().join("agy-latest-read.json");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        for at in [now - 2 * 60 * 60, now + 24 * 60 * 60] {
            std::fs::write(
                &record,
                serde_json::json!({ "readAt": at, "version": "1.5.13" }).to_string(),
            )
            .unwrap();
            assert_eq!(recent_agy_read(&record), None, "{at}");
        }
    }
}
