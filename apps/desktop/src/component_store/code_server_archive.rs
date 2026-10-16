use std::{
    collections::{HashMap, HashSet},
    fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
};

use flate2::read::GzDecoder;
use sha2::{Digest as _, Sha256};

use super::*;

const CODE_SERVER_ARCHIVE_CONTRACT_JSON: &str =
    include_str!("../../../../packages/shared/code-server-archive-contract.json");

struct CodeServerArchiveContract {
    schema_version: u64,
    required_entries: Vec<String>,
    required_entries_by_platform: HashMap<String, Vec<String>>,
    executable_entries: Vec<String>,
    executable_entries_by_platform: HashMap<String, Vec<String>>,
    readiness_entry: String,
    readiness_signal: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VerifiedCodeServerArchive {
    pub component_version: String,
    pub platform: String,
    pub sha256: String,
}

/// CDXC:CodeEditor 2026-10-07 WHY:
/// Availability and WSL command preparation repeatedly validate the same archive. Rehash its current bytes and check the sidecar on every call, then reuse the last successful contract validation instead of decompressing it again.
static VERIFIED_CODE_SERVER_ARCHIVE: Mutex<Option<VerifiedCodeServerArchive>> = Mutex::new(None);

fn code_server_archive_contract() -> Result<CodeServerArchiveContract, String> {
    let value = serde_json::from_str::<serde_json::Value>(CODE_SERVER_ARCHIVE_CONTRACT_JSON)
        .map_err(|error| format!("Invalid bundled code-server archive contract: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Invalid bundled code-server archive contract".to_string())?;
    let expected_keys = [
        "schemaVersion",
        "requiredEntries",
        "requiredEntriesByPlatform",
        "executableEntries",
        "executableEntriesByPlatform",
        "readinessEntry",
        "readinessSignal",
    ]
    .into_iter()
    .collect::<HashSet<_>>();
    if object.keys().map(String::as_str).collect::<HashSet<_>>() != expected_keys {
        return Err("Invalid bundled code-server archive contract fields".to_string());
    }
    let string_array = |key: &str| -> Result<Vec<String>, String> {
        object
            .get(key)
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("Invalid bundled code-server archive contract {key}"))?
            .iter()
            .map(|value| {
                value.as_str().map(str::to_string).ok_or_else(|| {
                    format!("Invalid bundled code-server archive contract {key} entry")
                })
            })
            .collect()
    };
    let string = |key: &str| -> Result<String, String> {
        object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("Invalid bundled code-server archive contract {key}"))
    };
    let string_array_map = |key: &str| -> Result<HashMap<String, Vec<String>>, String> {
        object
            .get(key)
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| format!("Invalid bundled code-server archive contract {key}"))?
            .iter()
            .map(|(platform, values)| {
                let entries = values
                    .as_array()
                    .ok_or_else(|| {
                        format!("Invalid bundled code-server archive contract {key}.{platform}")
                    })?
                    .iter()
                    .map(|value| {
                        value.as_str().map(str::to_string).ok_or_else(|| {
                            format!(
                                "Invalid bundled code-server archive contract {key}.{platform} entry"
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((platform.clone(), entries))
            })
            .collect()
    };
    let contract = CodeServerArchiveContract {
        schema_version: object
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                "Invalid bundled code-server archive contract schemaVersion".to_string()
            })?,
        required_entries: string_array("requiredEntries")?,
        required_entries_by_platform: string_array_map("requiredEntriesByPlatform")?,
        executable_entries: string_array("executableEntries")?,
        executable_entries_by_platform: string_array_map("executableEntriesByPlatform")?,
        readiness_entry: string("readinessEntry")?,
        readiness_signal: string("readinessSignal")?,
    };
    let supported_platforms = HashSet::from([
        "darwin-arm64".to_string(),
        "linux-arm64".to_string(),
        "linux-x64".to_string(),
    ]);
    if contract.schema_version != 2
        || contract.required_entries.is_empty()
        || contract.executable_entries.is_empty()
        || contract
            .required_entries_by_platform
            .keys()
            .cloned()
            .collect::<HashSet<_>>()
            != supported_platforms
        || contract
            .executable_entries_by_platform
            .keys()
            .cloned()
            .collect::<HashSet<_>>()
            != supported_platforms
        || !contract
            .required_entries
            .contains(&contract.readiness_entry)
    {
        return Err("Invalid bundled code-server archive contract".to_string());
    }
    for platform in &supported_platforms {
        let mut required_entries = contract.required_entries.clone();
        required_entries.extend(
            contract.required_entries_by_platform[platform]
                .iter()
                .cloned(),
        );
        let mut executable_entries = contract.executable_entries.clone();
        executable_entries.extend(
            contract.executable_entries_by_platform[platform]
                .iter()
                .cloned(),
        );
        if executable_entries
            .iter()
            .any(|entry| !required_entries.contains(entry))
        {
            return Err("Invalid bundled code-server archive contract".to_string());
        }
    }
    for entry in contract
        .required_entries
        .iter()
        .chain(contract.executable_entries.iter())
        .chain(contract.required_entries_by_platform.values().flatten())
        .chain(contract.executable_entries_by_platform.values().flatten())
    {
        if entry.is_empty()
            || entry.starts_with('/')
            || entry.split('/').any(|segment| {
                segment.is_empty()
                    || segment == "."
                    || segment == ".."
                    || !segment.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'@')
                    })
            })
        {
            return Err("Invalid bundled code-server archive contract path".to_string());
        }
    }
    if contract.readiness_signal.is_empty()
        || !contract
            .readiness_signal
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("Invalid bundled code-server archive readiness signal".to_string());
    }
    Ok(contract)
}

fn code_server_archive_contract_entries(
    contract: &CodeServerArchiveContract,
    platform: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    let platform_required = contract
        .required_entries_by_platform
        .get(platform)
        .ok_or_else(|| format!("Unsupported code-server archive platform: {platform}"))?;
    let platform_executable = contract
        .executable_entries_by_platform
        .get(platform)
        .ok_or_else(|| format!("Unsupported code-server archive platform: {platform}"))?;
    let mut required = contract.required_entries.clone();
    required.extend(platform_required.iter().cloned());
    let mut executable = contract.executable_entries.clone();
    executable.extend(platform_executable.iter().cloned());
    Ok((required, executable))
}

fn valid_code_server_component_version(version: &str) -> bool {
    let Some((revision, fingerprint)) = version.split_once("-p2-") else {
        return false;
    };
    revision.len() == 12
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        && fingerprint.len() == 64
        && fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn code_server_archive_name(component_version: &str, platform: &str) -> Result<String, String> {
    if !valid_code_server_component_version(component_version) {
        return Err("Invalid code-server p2 component identity".to_string());
    }
    if !matches!(platform, "linux-x64" | "linux-arm64") {
        return Err(format!(
            "Unsupported code-server archive platform: {platform}"
        ));
    }
    Ok(format!("code-server-{component_version}-{platform}.tar.gz"))
}

fn code_server_archive_sha256(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| {
        format!(
            "Could not open code-server archive {}: {error}",
            path.display()
        )
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| {
            format!(
                "Could not hash code-server archive {}: {error}",
                path.display()
            )
        })?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn parse_code_server_checksum_sidecar(
    contents: &str,
    expected_archive_name: &str,
) -> Result<String, String> {
    let record = if let Some(record) = contents.strip_suffix("\r\n") {
        record
    } else if let Some(record) = contents.strip_suffix('\n') {
        record
    } else {
        contents
    };
    let Some((sha256, archive_name)) = record.split_once("  ") else {
        return Err("Malformed code-server checksum sidecar".to_string());
    };
    if sha256.len() != 64
        || !sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || archive_name.contains('\r')
        || archive_name.contains('\n')
    {
        return Err("Malformed code-server checksum sidecar".to_string());
    }
    if archive_name != expected_archive_name {
        return Err(format!(
            "Code-server checksum filename mismatch: expected {expected_archive_name}, got {archive_name}"
        ));
    }
    Ok(sha256.to_string())
}

pub(crate) fn verify_code_server_archive(
    archive_path: &Path,
    component_version: &str,
    platform: &str,
) -> Result<VerifiedCodeServerArchive, String> {
    let expected_archive_name = code_server_archive_name(component_version, platform)?;
    if archive_path.file_name().and_then(|name| name.to_str()) != Some(&expected_archive_name) {
        return Err(format!(
            "Code-server archive identity mismatch: expected {expected_archive_name}"
        ));
    }
    let mut sidecar_name = archive_path.as_os_str().to_owned();
    sidecar_name.push(".sha256");
    let sidecar_path = PathBuf::from(sidecar_name);
    let sidecar = fs::read_to_string(&sidecar_path).map_err(|error| {
        format!(
            "Could not read filename-bound code-server checksum sidecar {}: {error}",
            sidecar_path.display()
        )
    })?;
    let expected_sha256 = parse_code_server_checksum_sidecar(&sidecar, &expected_archive_name)?;
    let actual_sha256 = code_server_archive_sha256(archive_path)?;
    if actual_sha256 != expected_sha256 {
        return Err(format!(
            "Code-server archive checksum mismatch for {expected_archive_name}"
        ));
    }

    let verified = VerifiedCodeServerArchive {
        component_version: component_version.to_string(),
        platform: platform.to_string(),
        sha256: actual_sha256,
    };
    if VERIFIED_CODE_SERVER_ARCHIVE
        .lock()
        .ok()
        .is_some_and(|cached| cached.as_ref() == Some(&verified))
    {
        return Ok(verified);
    }

    preflight_code_server_tar_metadata(archive_path)?;

    let contract = code_server_archive_contract()?;
    let (required_entries, executable_entries) =
        code_server_archive_contract_entries(&contract, platform)?;
    let required = required_entries
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let executable = executable_entries
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let file = File::open(archive_path).map_err(|error| {
        format!(
            "Could not open verified code-server archive {}: {error}",
            archive_path.display()
        )
    })?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    let entries = archive.entries().map_err(|error| {
        format!(
            "Invalid code-server archive {}: {error}",
            archive_path.display()
        )
    })?;
    let mut found = HashSet::new();
    let mut seen = HashMap::new();
    let mut links = HashMap::new();
    let mut saw_root_directory = false;
    let mut readiness_found = false;
    for entry in entries {
        let mut entry = entry.map_err(|error| format!("Invalid code-server archive: {error}"))?;
        let entry_type = entry.header().entry_type().as_byte();
        let path = normalized_code_server_tar_path(
            entry
                .path()
                .map_err(|error| format!("Invalid code-server archive entry: {error}"))?
                .as_ref(),
            entry_type == b'5',
        )?;
        let Some(path) = path else {
            if entry_type != b'5' || entry.header().size().unwrap_or(u64::MAX) != 0 {
                return Err("Malformed code-server archive root entry".to_string());
            }
            if saw_root_directory {
                return Err("Duplicate code-server archive root directory".to_string());
            }
            saw_root_directory = true;
            continue;
        };
        match entry_type {
            b'0' => {
                register_code_server_archive_entry(&mut seen, &path, "file")?;
                found.insert(path.clone());
                if !required.contains(path.as_str()) {
                    continue;
                }
                if entry.header().size().unwrap_or(0) == 0 {
                    return Err(format!(
                        "Code-server archive is missing required payload: {path}"
                    ));
                }
                if executable.contains(path.as_str())
                    && entry.header().mode().unwrap_or(0) & 0o111 == 0
                {
                    return Err(format!(
                        "Code-server archive payload is not executable: {path}"
                    ));
                }
                if path == contract.readiness_entry {
                    let mut contents = String::new();
                    entry
                        .take(8 * 1024 * 1024)
                        .read_to_string(&mut contents)
                        .map_err(|error| {
                            format!("Could not read code-server readiness payload: {error}")
                        })?;
                    readiness_found = contents.contains(&contract.readiness_signal);
                }
            }
            b'5' => {
                if entry.header().size().unwrap_or(u64::MAX) != 0 {
                    return Err(format!("Malformed code-server directory entry: {path}"));
                }
                register_code_server_archive_entry(&mut seen, &path, "directory")?;
            }
            b'1' | b'2' => {
                if entry.header().size().unwrap_or(u64::MAX) != 0 {
                    return Err(format!("Malformed code-server link entry: {path}"));
                }
                let link_name = entry
                    .link_name()
                    .map_err(|error| format!("Invalid code-server archive link: {error}"))?
                    .ok_or_else(|| format!("Missing code-server archive link target: {path}"))?;
                let kind = if entry_type == b'1' {
                    "hardlink"
                } else {
                    "symlink"
                };
                let target = normalized_code_server_link_target(
                    &path,
                    link_name.as_ref(),
                    entry_type == b'1',
                )?;
                register_code_server_archive_entry(&mut seen, &path, kind)?;
                links.insert(path, (kind, target));
            }
            _ => {
                return Err(format!(
                    "Unsupported code-server archive entry type {entry_type:#04x} for {path}"
                ));
            }
        }
    }
    for (path, (kind, original_target)) in &links {
        let mut target = original_target.as_str();
        let mut visited = HashSet::from([path.as_str()]);
        while let Some((_, next_target)) = links.get(target) {
            if !visited.insert(target) {
                return Err(format!("Cyclic code-server archive link: {path}"));
            }
            target = next_target;
        }
        let target_kind = seen.get(target);
        if target_kind.is_none() || (*kind == "hardlink" && target_kind != Some(&"file")) {
            return Err(format!(
                "Unsafe or dangling code-server archive link: {path} -> {original_target}"
            ));
        }
    }
    for required_entry in &required_entries {
        if !found.contains(required_entry) {
            return Err(format!(
                "Code-server archive is missing required payload: {required_entry}"
            ));
        }
    }
    if !readiness_found {
        return Err(format!(
            "Code-server archive lacks compiled {} readiness signal",
            contract.readiness_signal
        ));
    }
    if let Ok(mut cached) = VERIFIED_CODE_SERVER_ARCHIVE.lock() {
        *cached = Some(verified.clone());
    }
    Ok(verified)
}

pub(crate) fn verify_installed_windows_code_server_component(
    component_path: &Path,
    component_version: &str,
    windows_platform: &str,
) -> Result<VerifiedCodeServerArchive, String> {
    let linux_platform = match windows_platform {
        "windows-x64" => "linux-x64",
        "windows-arm64" => "linux-arm64",
        _ => {
            return Err(format!(
                "Unsupported installed Windows code-server platform: {windows_platform}"
            ));
        }
    };
    let archive_name = code_server_archive_name(component_version, linux_platform)?;
    verify_code_server_archive(
        &component_path.join(archive_name),
        component_version,
        linux_platform,
    )
}

#[allow(dead_code)] // no caller: the code-server payload is validated by the runtime spawn path now
pub(crate) fn code_server_payload_shell_validation_script() -> Result<String, String> {
    let contract = code_server_archive_contract()?;
    let (required_entries, executable_entries) =
        code_server_archive_contract_entries(&contract, "linux-x64")?;
    let mut checks = Vec::new();
    for entry in &required_entries {
        checks.push(format!("test -s \"$code_server_root/{entry}\""));
    }
    for entry in &executable_entries {
        checks.push(format!("test -x \"$code_server_root/{entry}\""));
    }
    checks.push(format!(
        "grep -Fq -- '{}' \"$code_server_root/{}\"",
        contract.readiness_signal, contract.readiness_entry
    ));
    Ok(checks.join("; "))
}
