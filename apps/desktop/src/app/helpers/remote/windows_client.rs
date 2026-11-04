//! Native Windows SSH client and Credential Manager integration.

use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Child,
    sync::Mutex,
};

use windows_sys::Win32::{
    Foundation::{ERROR_NOT_FOUND, GetLastError},
    Security::Credentials::{
        CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW,
        CredDeleteW, CredFree, CredReadW, CredWriteW,
    },
};

use crate::app::helpers::*;

pub(crate) fn gpui_windows_system_executable(relative: &str) -> String {
    PathBuf::from(env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
        .join("System32")
        .join(relative)
        .to_string_lossy()
        .into_owned()
}

fn credential_target(machine_id: &str, kind: &str) -> Vec<u16> {
    format!("Ghostex/Remote/{kind}/{machine_id}")
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

/// CDXC:RemoteMachines 2026-09-23 WHY:
/// SSH workers have no GPUI App context. Use the same native Credential Manager API as GPUI so saved passwords and daemon tokens are available to the askpass child without placing secrets in settings, command lines, or helper files.
fn save_remote_secret(machine_id: &str, kind: &str, secret: &str) -> Result<(), ()> {
    if machine_id.contains('\0') || secret.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
        return Err(());
    }
    let mut target = credential_target(machine_id, kind);
    if secret.is_empty() {
        return if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0
            || unsafe { GetLastError() } == ERROR_NOT_FOUND
        {
            Ok(())
        } else {
            Err(())
        };
    }
    let mut username: Vec<u16> = "Ghostex".encode_utf16().chain(Some(0)).collect();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_mut_ptr(),
        CredentialBlobSize: secret.len() as u32,
        CredentialBlob: secret.as_ptr() as *mut u8,
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: username.as_mut_ptr(),
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { CredWriteW(&credential, 0) } != 0 {
        Ok(())
    } else {
        Err(())
    }
}

pub(crate) fn gpui_save_remote_machine_password_to_keychain(
    remote_machine_id: &str,
    password: &str,
) -> GpuiRemoteSshPasswordKeychainResult {
    match save_remote_secret(remote_machine_id, "ssh-password", password) {
        Ok(()) => GpuiRemoteSshPasswordKeychainResult::Success,
        Err(()) => GpuiRemoteSshPasswordKeychainResult::Failed,
    }
}

pub(crate) fn gpui_save_remote_gxserver_token_to_keychain(
    remote_machine_id: &str,
    token: &str,
) -> GpuiRemoteTokenKeychainResult {
    match save_remote_secret(remote_machine_id, "gxserver-token", token) {
        Ok(()) => GpuiRemoteTokenKeychainResult::Success,
        Err(()) => GpuiRemoteTokenKeychainResult::Failed,
    }
}

pub(crate) fn gpui_read_remote_ssh_password_from_keychain(
    remote_machine_id: &str,
) -> Result<Vec<u8>, String> {
    let unavailable =
        || "Could not read the saved SSH password from Windows Credential Manager.".to_string();
    if remote_machine_id.contains('\0') {
        return Err(unavailable());
    }
    let target = credential_target(remote_machine_id, "ssh-password");
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0
        || credential.is_null()
    {
        return Err(unavailable());
    }
    let password = unsafe {
        let blob = (*credential).CredentialBlob;
        let size = (*credential).CredentialBlobSize as usize;
        let password = if blob.is_null() || size == 0 {
            Vec::new()
        } else {
            let bytes = std::slice::from_raw_parts_mut(blob, size);
            let password = bytes.to_vec();
            bytes.fill(0);
            password
        };
        CredFree(credential.cast());
        password
    };
    if password.is_empty() {
        Err(unavailable())
    } else {
        Ok(password)
    }
}

pub(crate) fn gpui_remote_ssh_askpass_script(
    config: &GpuiRemoteMachineConfig,
) -> Result<Option<GpuiRemoteAskpassScript>, String> {
    if !config.has_saved_password {
        return Ok(None);
    }
    Ok(Some(GpuiRemoteAskpassScript {
        script: env::current_exe()
            .map_err(|_| "Could not locate the SSH password helper.".to_string())?,
        remote_machine_id: config.remote_machine_id.clone(),
    }))
}

/// CDXC:RemoteMachines 2026-09-23 WHY:
/// Windows OpenSSH requires an executable askpass helper. Run this entry before UI/updater startup; inherited stdout is OpenSSH's private pipe, and every reconnect reads the current saved credential.
pub(crate) fn gpui_run_windows_remote_ssh_askpass() -> bool {
    let Ok(machine_id) = env::var("GHOSTEX_REMOTE_SSH_ASKPASS_MACHINE") else {
        return false;
    };
    if let Ok(mut password) = gpui_read_remote_ssh_password_from_keychain(&machine_id) {
        let mut stdout = std::io::stdout().lock();
        let result = stdout
            .write_all(&password)
            .and_then(|_| stdout.write_all(b"\n"))
            .and_then(|_| stdout.flush());
        password.fill(0);
        if result.is_ok() {
            return true;
        }
    }
    std::process::exit(1);
}

pub(crate) fn gpui_terminate_remote_process(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub(crate) fn gpui_bundled_remote_gxserver_package_dir(
    _target: &GpuiRemoteInstallTarget,
) -> Option<PathBuf> {
    // Windows ships no unpacked package; Linux remotes get the archive below.
    None
}

/// CDXC:RemoteMachines 2026-10-20 WHY:
/// The Windows app unpacks no Linux package. Dev builds bundle the static (musl) Linux gxserver package their WSL backend installs, `resources/wsl/gxserver-linux-<arch>.tar.gz`, which has the layout and `gxserver setup` a macOS app's upload has, so remote installs and updates upload that archive as is. Release builds no longer bundle it (supersedes the 2026-10-06 note): their sealed manifest lists the same archive, and installs fall through to `gpui_on_demand_gxserver_archive`, as on macOS.
pub(crate) fn gpui_bundled_remote_gxserver_archive(
    target: &GpuiRemoteInstallTarget,
) -> Option<PathBuf> {
    // The asset key is fixed text per supported arch, so the remote's uname output never becomes part of a local path.
    let asset_key = gpui_on_demand_gxserver_asset_key(target)?;
    let archive_path = env::current_exe()
        .ok()?
        .parent()?
        .join("resources")
        .join("wsl")
        .join(format!("{asset_key}.tar.gz"));
    archive_path.is_file().then_some(archive_path)
}

/// The archive's `build-identity.json` sits behind its binaries, so reading it decompresses most of the archive; the archive cannot change while the app runs, so each answer is kept.
pub(crate) fn gpui_bundled_remote_gxserver_archive_build_identity(
    archive_path: &Path,
) -> Option<String> {
    static IDENTITIES: Mutex<Vec<(PathBuf, Option<String>)>> = Mutex::new(Vec::new());
    let mut identities = IDENTITIES.lock().ok()?;
    if let Some((_, identity)) = identities.iter().find(|(path, _)| path == archive_path) {
        return identity.clone();
    }
    let identity = read_archive_build_identity(archive_path);
    identities.push((archive_path.to_path_buf(), identity.clone()));
    identity
}

fn read_archive_build_identity(archive_path: &Path) -> Option<String> {
    let file = fs::File::open(archive_path).ok()?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(io::BufReader::new(file)));
    for entry in archive.entries().ok()? {
        let mut entry = entry.ok()?;
        let path = entry.path().ok()?.into_owned();
        if path.strip_prefix(".").unwrap_or(path.as_path()) != Path::new("build-identity.json") {
            continue;
        }
        let mut text = String::new();
        entry.read_to_string(&mut text).ok()?;
        return gpui_gxserver_build_identity_from_json(&text);
    }
    None
}

pub(crate) fn gpui_launch_windows_remote_editor(
    command: &str,
    arguments: &[&str],
) -> Result<(), String> {
    use base64::Engine as _;
    let executable = env::var_os("PATH")
        .and_then(|path| {
            env::split_paths(&path).find_map(|directory| {
                [".exe", ".cmd", ".bat"]
                    .into_iter()
                    .map(|extension| directory.join(format!("{command}{extension}")))
                    .find(|path| path.is_file())
            })
        })
        .ok_or_else(|| {
            "Configured editor is not available for GPUI remote IDE open.".to_string()
        })?;
    let script = format!(
        "& {} {}",
        gpui_powershell_quote(&executable.to_string_lossy()),
        arguments
            .iter()
            .map(|argument| gpui_powershell_quote(argument))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    gpui_remote_background_command(&gpui_windows_system_executable(
        "WindowsPowerShell/v1.0/powershell.exe",
    ))
    .args(["-NoLogo", "-NoProfile", "-EncodedCommand", &encoded])
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .spawn_reaped()
    .map_err(|_| "Configured editor could not open the remote target.".to_string())
}
