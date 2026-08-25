use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    validate_extension_id, validate_sha256, ExtensionError, ExtensionManifest, ExtensionResult,
};

const MANIFEST_FILE: &str = "ghostex-extension.json";
const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;

pub(crate) fn read_manifest(
    payload_dir: &Path,
    expected_id: Option<&str>,
) -> ExtensionResult<ExtensionManifest> {
    let manifest_path = payload_dir.join(MANIFEST_FILE);
    let metadata = fs::metadata(&manifest_path).map_err(|error| {
        ExtensionError::bad_request(format!(
            "Could not read extension manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return Err(ExtensionError::bad_request(format!(
            "Extension manifest must be a file no larger than {MAX_MANIFEST_BYTES} bytes."
        )));
    }
    let bytes = fs::read(&manifest_path).map_err(|error| {
        ExtensionError::bad_request(format!(
            "Could not read extension manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest: ExtensionManifest = serde_json::from_slice(&bytes).map_err(|error| {
        ExtensionError::bad_request(format!(
            "Extension manifest {} is invalid: {error}",
            manifest_path.display()
        ))
    })?;
    manifest.validate(Some(payload_dir), expected_id)?;
    Ok(manifest)
}

pub(crate) fn stage_local_payload(
    extensions_dir: &Path,
    source: &Path,
) -> ExtensionResult<(PathBuf, PathBuf, ExtensionManifest)> {
    let source = fs::canonicalize(source).map_err(|error| {
        ExtensionError::bad_request(format!(
            "Could not resolve local extension folder {}: {error}",
            source.display()
        ))
    })?;
    if !source.is_dir() {
        return Err(ExtensionError::bad_request(format!(
            "Local extension path is not a directory: {}",
            source.display()
        )));
    }
    if extensions_dir.starts_with(&source) {
        return Err(ExtensionError::bad_request(format!(
            "Local extension folder may not contain the Ghostex extensions directory: {}",
            extensions_dir.display()
        )));
    }
    let folder_id = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ExtensionError::bad_request("Local extension folder has no valid id."))?;
    validate_extension_id(folder_id)?;
    let manifest = read_manifest(&source, Some(folder_id))?;
    let work_dir = install_work_dir(extensions_dir)?;
    let staged_payload = work_dir.join("payload");
    if let Err(error) = copy_directory(&source, &staged_payload) {
        let _ = fs::remove_dir_all(&work_dir);
        return Err(error);
    }
    if let Err(error) = read_manifest(&staged_payload, Some(&manifest.name)) {
        let _ = fs::remove_dir_all(&work_dir);
        return Err(error);
    }
    Ok((work_dir, staged_payload, manifest))
}

pub(crate) fn stage_zip_payload(
    extensions_dir: &Path,
    id: &str,
    url: &str,
    expected_sha256: &str,
) -> ExtensionResult<(PathBuf, PathBuf, ExtensionManifest)> {
    validate_extension_id(id)?;
    validate_sha256(expected_sha256)?;
    let parsed_url = url::Url::parse(url)
        .map_err(|error| ExtensionError::bad_request(format!("Invalid extension URL: {error}")))?;
    if !matches!(parsed_url.scheme(), "http" | "https") {
        return Err(ExtensionError::bad_request(
            "Extension download URL must use HTTP or HTTPS.",
        ));
    }
    let work_dir = install_work_dir(extensions_dir)?;
    let archive_path = work_dir.join("payload.zip");
    let staged_payload = work_dir.join("payload");
    let outcome = (|| {
        download_archive(url, &archive_path)?;
        verify_archive_sha256(&archive_path, expected_sha256)?;
        unpack_archive(&archive_path, &staged_payload)?;
        let manifest = read_manifest(&staged_payload, Some(id))?;
        Ok(manifest)
    })();
    match outcome {
        Ok(manifest) => Ok((work_dir, staged_payload, manifest)),
        Err(error) => {
            let _ = fs::remove_dir_all(&work_dir);
            Err(error)
        }
    }
}

pub(crate) fn activate_staged_payload(
    installed_dir: &Path,
    id: &str,
    staged_payload: &Path,
) -> ExtensionResult<()> {
    validate_extension_id(id)?;
    fs::create_dir_all(installed_dir).map_err(|error| {
        ExtensionError::internal(format!(
            "Could not create installed extensions directory {}: {error}",
            installed_dir.display()
        ))
    })?;
    let destination = installed_dir.join(id);
    let backup_root = installed_dir.parent().unwrap_or(installed_dir);
    let backup = backup_root.join(format!(".{id}-backup-{}", Uuid::new_v4()));
    let had_existing = destination.exists();
    if had_existing {
        fs::rename(&destination, &backup).map_err(|error| {
            ExtensionError::internal(format!(
                "Could not stage the existing extension {id} for replacement: {error}"
            ))
        })?;
    }
    if let Err(error) = fs::rename(staged_payload, &destination) {
        if had_existing {
            let _ = fs::rename(&backup, &destination);
        }
        return Err(ExtensionError::internal(format!(
            "Could not activate extension {id}: {error}"
        )));
    }
    if had_existing {
        fs::remove_dir_all(&backup).map_err(|error| {
            ExtensionError::internal(format!(
                "Extension {id} was updated, but its previous payload could not be removed from {}: {error}",
                backup.display()
            ))
        })?;
    }
    Ok(())
}

fn install_work_dir(extensions_dir: &Path) -> ExtensionResult<PathBuf> {
    fs::create_dir_all(extensions_dir).map_err(|error| {
        ExtensionError::internal(format!(
            "Could not create extensions directory {}: {error}",
            extensions_dir.display()
        ))
    })?;
    let work_dir = extensions_dir.join(format!(".install-{}", Uuid::new_v4()));
    fs::create_dir(&work_dir).map_err(|error| {
        ExtensionError::internal(format!(
            "Could not create extension installation directory {}: {error}",
            work_dir.display()
        ))
    })?;
    Ok(work_dir)
}

fn download_archive(url: &str, destination: &Path) -> ExtensionResult<()> {
    let response = ureq::get(url)
        .timeout(Duration::from_secs(120))
        .call()
        .map_err(|error| ExtensionError::internal(format!("Extension download failed: {error}")))?;
    if response
        .header("content-length")
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_ARCHIVE_BYTES)
    {
        return Err(ExtensionError::bad_request(format!(
            "Extension archive exceeds the {MAX_ARCHIVE_BYTES} byte limit."
        )));
    }
    let mut input = response.into_reader().take(MAX_ARCHIVE_BYTES + 1);
    let mut output = fs::File::create(destination).map_err(|error| {
        ExtensionError::internal(format!(
            "Could not create temporary extension archive {}: {error}",
            destination.display()
        ))
    })?;
    let written = std::io::copy(&mut input, &mut output).map_err(|error| {
        ExtensionError::internal(format!("Could not save extension archive: {error}"))
    })?;
    output.sync_all().map_err(|error| {
        ExtensionError::internal(format!("Could not flush extension archive: {error}"))
    })?;
    if written > MAX_ARCHIVE_BYTES {
        return Err(ExtensionError::bad_request(format!(
            "Extension archive exceeds the {MAX_ARCHIVE_BYTES} byte limit."
        )));
    }
    Ok(())
}

fn verify_archive_sha256(path: &Path, expected: &str) -> ExtensionResult<()> {
    let mut file = fs::File::open(path).map_err(|error| {
        ExtensionError::internal(format!("Could not open extension archive: {error}"))
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            ExtensionError::internal(format!("Could not hash extension archive: {error}"))
        })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    let actual = format!("{:x}", digest.finalize());
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(ExtensionError::bad_request(format!(
            "Extension archive SHA-256 mismatch: expected {}, received {}.",
            expected.to_ascii_lowercase(),
            actual
        )));
    }
    Ok(())
}

fn unpack_archive(archive_path: &Path, destination: &Path) -> ExtensionResult<()> {
    fs::create_dir(destination).map_err(|error| {
        ExtensionError::internal(format!(
            "Could not create extension unpack directory {}: {error}",
            destination.display()
        ))
    })?;
    let file = fs::File::open(archive_path).map_err(|error| {
        ExtensionError::bad_request(format!("Could not open extension ZIP: {error}"))
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        ExtensionError::bad_request(format!("Extension archive is not a valid ZIP: {error}"))
    })?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(ExtensionError::bad_request(format!(
            "Extension archive contains more than {MAX_ARCHIVE_ENTRIES} entries."
        )));
    }
    let mut total_size = 0_u64;
    let mut extracted_paths = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            ExtensionError::bad_request(format!("Could not read extension ZIP entry: {error}"))
        })?;
        let relative_path = entry.enclosed_name().ok_or_else(|| {
            ExtensionError::bad_request(format!(
                "Extension ZIP contains an unsafe path: {}",
                entry.name()
            ))
        })?;
        if relative_path.as_os_str().is_empty() || !extracted_paths.insert(relative_path.clone()) {
            return Err(ExtensionError::bad_request(format!(
                "Extension ZIP contains an empty or duplicate path: {}",
                entry.name()
            )));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(ExtensionError::bad_request(format!(
                "Extension ZIP may not contain symlinks: {}",
                entry.name()
            )));
        }
        total_size = total_size.saturating_add(entry.size());
        if total_size > MAX_UNPACKED_BYTES {
            return Err(ExtensionError::bad_request(format!(
                "Extension ZIP expands beyond the {MAX_UNPACKED_BYTES} byte limit."
            )));
        }
        let output_path = destination.join(&relative_path);
        if entry.is_dir() {
            fs::create_dir_all(&output_path).map_err(|error| {
                ExtensionError::internal(format!(
                    "Could not create extension directory {}: {error}",
                    output_path.display()
                ))
            })?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                ExtensionError::internal(format!(
                    "Could not create extension directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let mut output = fs::File::create(&output_path).map_err(|error| {
            ExtensionError::internal(format!(
                "Could not create extension file {}: {error}",
                output_path.display()
            ))
        })?;
        std::io::copy(&mut entry, &mut output).map_err(|error| {
            ExtensionError::internal(format!(
                "Could not unpack extension file {}: {error}",
                output_path.display()
            ))
        })?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output_path, fs::Permissions::from_mode(mode & 0o777)).map_err(
                |error| {
                    ExtensionError::internal(format!(
                        "Could not set extension file permissions {}: {error}",
                        output_path.display()
                    ))
                },
            )?;
        }
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> ExtensionResult<()> {
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        ExtensionError::bad_request(format!(
            "Could not inspect local extension path {}: {error}",
            source.display()
        ))
    })?;
    if metadata.file_type().is_symlink() {
        return Err(ExtensionError::bad_request(format!(
            "Local extensions may not contain symlinks: {}",
            source.display()
        )));
    }
    fs::create_dir(destination).map_err(|error| {
        ExtensionError::internal(format!(
            "Could not create staged extension directory {}: {error}",
            destination.display()
        ))
    })?;
    for entry in fs::read_dir(source).map_err(|error| {
        ExtensionError::bad_request(format!(
            "Could not read local extension directory {}: {error}",
            source.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            ExtensionError::bad_request(format!("Could not read local extension entry: {error}"))
        })?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path).map_err(|error| {
            ExtensionError::bad_request(format!(
                "Could not inspect local extension entry {}: {error}",
                source_path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ExtensionError::bad_request(format!(
                "Local extensions may not contain symlinks: {}",
                source_path.display()
            )));
        }
        if metadata.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                ExtensionError::internal(format!(
                    "Could not copy local extension file {}: {error}",
                    source_path.display()
                ))
            })?;
        } else {
            return Err(ExtensionError::bad_request(format!(
                "Local extension contains an unsupported filesystem entry: {}",
                source_path.display()
            )));
        }
    }
    Ok(())
}
