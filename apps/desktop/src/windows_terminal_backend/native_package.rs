use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, os::windows::ffi::OsStrExt, path::Path, sync::Mutex};
use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

static REFRESH_LOCK: Mutex<()> = Mutex::new(());
// ghostex.exe is not copied: it is the package's own gxserver.exe under the CLI's name (`link_cli`).
const BINARIES: [&str; 2] = ["gxserver.exe", "wmx.exe"];

/// Records the installed app folder for CLIs and scripts that cannot derive it (see `GhostexPaths::windows_app_dir_file`).
pub(super) fn record_app_dir() {
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    let Some(app_dir) = executable.parent() else {
        return;
    };
    // Unbundled development executables are not an installed app.
    if !app_dir.join("resources/native/gxserver.exe").is_file() {
        return;
    }
    let target = crate::shared_settings::ghostex_storage_paths().windows_app_dir_file();
    let contents = app_dir.to_string_lossy();
    if fs::read_to_string(&target).is_ok_and(|current| current == contents) {
        return;
    }
    let written = target
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::write(&target, contents.as_bytes()));
    if let Err(error) = written {
        eprintln!(
            "Ghostex could not record its install folder in {}: {error}",
            target.display()
        );
    }
}

/// CDXC:PlatformSupport 2026-09-23 WHY:
/// An older managed CLI can replace the app's current server with its own sibling server long after app startup.
/// Refresh the existing managed package from the installed bundle before connecting, retaining replaced images because Windows session hosts may still map them.
pub(super) fn refresh_existing() -> Result<(), String> {
    let _guard = REFRESH_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    refresh()
        .map_err(|error| format!("Could not update the managed Windows CLI package: {error:#}"))
}

/// CDXC:PlatformSupport 2026-10-22 WHY:
/// A package copy an antivirus quarantined is put back from the bundle. Failing on it ("The existing package is missing …") stopped every launch before gxserver started, and reinstalling or downgrading Ghostex never touches this folder.
fn refresh() -> Result<()> {
    let package = crate::shared_settings::ghostex_storage_paths()
        .gxserver_data_dir()
        .join("package");
    if !package.exists() {
        return Ok(());
    }
    let resources = std::env::current_exe()?
        .parent()
        .context("The app executable has no parent folder")?
        .join("resources");
    // Unbundled development executables do not own a packaged runtime.
    if !resources.join("native/gxserver.exe").is_file() {
        return Ok(());
    }
    let identity_bytes = fs::read(resources.join("build-identity.json"))?;
    let identity: serde_json::Value = serde_json::from_slice(&identity_bytes)?;
    let mut source_hashes = Vec::new();
    let mut files = Vec::new();
    for name in BINARIES {
        let source = resources.join("native").join(name);
        let target = package.join("bin").join(name);
        ensure!(
            source.is_file(),
            "{} is missing from the Ghostex install folder. Windows Security or another antivirus program may have quarantined it: restore it from Windows Security > Virus & threat protection > Protection history, or reinstall Ghostex.",
            source.display()
        );
        let hash = file_hash(&source)?;
        source_hashes.push(hash.clone());
        if !target.is_file() || file_hash(&target)? != hash {
            files.push((source, target, hash));
        }
    }
    let fingerprint = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(
            format!("{}\n", source_hashes.join("\n")).as_bytes()
        ))
    );
    ensure!(
        identity["fingerprint"].as_str() == Some(&fingerprint),
        "The bundled runtime fingerprint does not match its executables"
    );
    let version = identity["packageVersion"]
        .as_str()
        .context("Missing runtime package version")?;
    ensure!(
        identity["buildIdentity"].as_str() == Some(&format!("gxserver:{version}:{fingerprint}")),
        "The bundled build identity is inconsistent"
    );
    let identity_target = package.join("build-identity.json");
    if fs::read(&identity_target).ok().as_deref() != Some(identity_bytes.as_slice()) {
        files.push((
            resources.join("build-identity.json"),
            identity_target,
            hex::encode(Sha256::digest(&identity_bytes)),
        ));
    }
    if files.is_empty() {
        return link_cli(&package);
    }

    let transaction = package
        .join(".retired-native")
        .join(uuid::Uuid::new_v4().to_string());
    let staged = transaction.join("new");
    let previous = transaction.join("previous");
    fs::create_dir_all(&staged)?;
    fs::create_dir_all(&previous)?;
    // Verify every staged byte before replacing the first public entrypoint.
    for (source, target, hash) in &files {
        let copy = staged.join(target.file_name().context("Missing package filename")?);
        let staged_hash = fs::copy(source, &copy)
            .and_then(|_| fs::OpenOptions::new().write(true).open(&copy)?.sync_all())
            .map_err(anyhow::Error::from)
            .and_then(|()| file_hash(&copy));
        let Ok(staged_hash) = staged_hash else {
            anyhow::bail!(
                "{} disappeared or became unreadable while Ghostex copied it into {}. Windows Security or another antivirus program is probably removing it: allow it in Windows Security > Virus & threat protection > Protection history, then open Ghostex again.",
                source.display(),
                package.display()
            );
        };
        ensure!(
            staged_hash == *hash,
            "Staged runtime verification failed: {}",
            copy.display()
        );
    }
    let mut replaced = Vec::new();
    for (_, target, _) in &files {
        let name = target.file_name().context("Missing package filename")?;
        if !target.exists() {
            // Nothing to replace or roll back: the verified copy fills the gap.
            fs::rename(staged.join(name), target)
                .with_context(|| format!("Restoring {}", target.display()))?;
            continue;
        }
        let backup = previous.join(name);
        if let Err(error) = replace(target, &staged.join(name), Some(&backup)) {
            // ReplaceFile can have moved the original before a later step fails.
            if backup.exists() {
                replaced.push((target.clone(), backup));
            }
            let mut rollback_errors = Vec::new();
            for (target, backup) in replaced.iter().rev() {
                let restored = if target.exists() {
                    replace(target, backup, None)
                } else {
                    fs::rename(backup, target)
                };
                if let Err(error) = restored {
                    rollback_errors.push(format!("{}: {error}", target.display()));
                }
            }
            anyhow::bail!(
                "Replacing {} failed: {error}; rollback errors: {}",
                target.display(),
                rollback_errors.join(", ")
            );
        }
        replaced.push((target.clone(), backup));
    }
    link_cli(&package)
}

/// A replaced gxserver.exe leaves the package's hard-linked `ghostex.exe` on the old program, and a package from before 2026-10-23 holds a separate CLI there.
fn link_cli(package: &Path) -> Result<()> {
    crate::windows_cli_link::link_cli(&package.join("bin"), &package.join(".retired-native"))
        .with_context(|| format!("Linking {}", package.join("bin/ghostex.exe").display()))
}

fn file_hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("Reading {}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            return Ok(hex::encode(hash.finalize()));
        }
        hash.update(&buffer[..count]);
    }
}

/// `ReplaceFileW`, retried for a while when another program holds one of the files open.
///
/// CDXC:PlatformSupport 2026-10-22 WHY:
/// Antivirus scanners open a freshly written executable for several seconds (Defender's cloud check can hold one for 10 s or more), and ReplaceFile then fails with a sharing or access error although nothing is wrong. Those errors leave both files under their own names, so the call is simply tried again; any other error, or one that lasts past the wait, is reported.
fn replace(target: &Path, staged: &Path, backup: Option<&Path>) -> std::io::Result<()> {
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    const ERROR_UNABLE_TO_REMOVE_REPLACED: i32 = 1175;
    const ERROR_UNABLE_TO_MOVE_REPLACEMENT: i32 = 1176;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        match replace_once(target, staged, backup) {
            Err(error)
                if matches!(
                    error.raw_os_error(),
                    Some(
                        ERROR_ACCESS_DENIED
                            | ERROR_SHARING_VIOLATION
                            | ERROR_LOCK_VIOLATION
                            | ERROR_UNABLE_TO_REMOVE_REPLACED
                            | ERROR_UNABLE_TO_MOVE_REPLACEMENT
                    )
                ) && std::time::Instant::now() < deadline
                    && backup.is_none_or(|backup| !backup.exists()) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            result => return result,
        }
    }
}

fn replace_once(target: &Path, staged: &Path, backup: Option<&Path>) -> std::io::Result<()> {
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let target = wide(target);
    let staged = wide(staged);
    let backup = backup.map(wide);
    let succeeded = unsafe {
        ReplaceFileW(
            target.as_ptr(),
            staged.as_ptr(),
            backup
                .as_ref()
                .map_or(std::ptr::null(), |path| path.as_ptr()),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if succeeded == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
