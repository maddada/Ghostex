#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{SystemTime, UNIX_EPOCH},
};

const BUNDLED_SETUP_MAGIC: &[u8; 16] = b"GXSETUP1TRAILER!";
const BUNDLED_SETUP_LENGTH_LEN: usize = 8;
const BUNDLED_SETUP_SEARCH_LEN: u64 = 1024 * 1024;
const INSTALL_ROOT_NAME: &str = "Ghostex";
const INSTALLED_LAUNCHER_NAME: &str = "Ghostex.exe";
const INSTALLER_VERSION: &str = match option_env!("GHOSTEX_BUILD_MARKETING_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            show_error(&format!("Ghostex Setup could not finish.\n\n{error}"));
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let original_args: Vec<OsString> = env::args_os().skip(1).collect();
    let install_root = requested_install_root(&original_args)?;
    let replacing_existing_install = directory_has_entries(&install_root);
    let explicitly_silent = original_args
        .iter()
        .any(|argument| argument == OsStr::new("--silent") || argument == OsStr::new("-s"));

    let extracted_setup = extract_bundled_setup()?;
    let mut setup_args = original_args.clone();
    let automatically_silent = replacing_existing_install
        && !explicitly_silent
        && installer_is_not_older_than_installed(&install_root);
    if automatically_silent {
        setup_args.insert(0, OsString::from("--silent"));
    }

    let status = Command::new(extracted_setup.path())
        .args(&setup_args)
        .status()
        .map_err(|error| format!("The bundled installer could not be started: {error}"))?;
    if !status.success() {
        return Err(match status.code() {
            Some(code) => format!("The bundled installer exited with code {code}."),
            None => "The bundled installer stopped unexpectedly.".to_string(),
        });
    }

    if automatically_silent {
        let launcher = install_root.join(INSTALLED_LAUNCHER_NAME);
        if !launcher.is_file() {
            return Err(format!(
                "The installation completed, but the Ghostex launcher is missing at {}.",
                launcher.display()
            ));
        }
        let restart_args = executable_args(&original_args);
        Command::new(&launcher)
            .args(restart_args)
            .current_dir(&install_root)
            .spawn()
            .map_err(|error| format!("Ghostex was installed but could not be started: {error}"))?;
    }

    Ok(())
}

fn requested_install_root(arguments: &[OsString]) -> Result<PathBuf, String> {
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument == OsStr::new("--") {
            break;
        }
        if argument == OsStr::new("--installto") || argument == OsStr::new("-t") {
            let destination = arguments
                .get(index + 1)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "--installto requires an installation directory.".to_string())?;
            return Ok(PathBuf::from(destination));
        }
        if let Some(argument) = argument.to_str()
            && let Some(destination) = argument.strip_prefix("--installto=")
        {
            if destination.is_empty() {
                return Err("--installto requires an installation directory.".to_string());
            }
            return Ok(PathBuf::from(destination));
        }
        index += 1;
    }

    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|directory| directory.join(INSTALL_ROOT_NAME))
        .ok_or_else(|| "Windows did not provide a Local AppData directory.".to_string())
}

fn directory_has_entries(directory: &Path) -> bool {
    fs::read_dir(directory)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_some()
}

fn installer_is_not_older_than_installed(install_root: &Path) -> bool {
    let manifest = match fs::read_to_string(install_root.join("current/sq.version")) {
        Ok(manifest) => manifest,
        Err(_) => return true,
    };
    let Some(version_start) = manifest.find("<version>") else {
        return true;
    };
    let version_start = version_start + "<version>".len();
    let Some(version_end) = manifest[version_start..].find("</version>") else {
        return true;
    };
    let installed_version = &manifest[version_start..version_start + version_end];
    match (
        numeric_version(INSTALLER_VERSION),
        numeric_version(installed_version),
    ) {
        (Some(installer), Some(installed)) => installer >= installed,
        _ => true,
    }
}

fn numeric_version(version: &str) -> Option<[u64; 4]> {
    let mut parsed = [0_u64; 4];
    let version = version.split_once('-').map_or(version, |(core, _)| core);
    let mut found_component = false;
    for (index, component) in version.split('.').take(4).enumerate() {
        parsed[index] = component.parse().ok()?;
        found_component = true;
    }
    found_component.then_some(parsed)
}

fn executable_args(arguments: &[OsString]) -> &[OsString] {
    arguments
        .iter()
        .position(|argument| argument == OsStr::new("--"))
        .map_or(&[], |separator| &arguments[separator + 1..])
}

fn extract_bundled_setup() -> Result<TemporarySetup, String> {
    let executable = env::current_exe()
        .map_err(|error| format!("The downloaded installer path is unavailable: {error}"))?;
    let mut source = File::open(&executable)
        .map_err(|error| format!("The downloaded installer could not be opened: {error}"))?;
    let total_len = source
        .metadata()
        .map_err(|error| format!("The downloaded installer could not be inspected: {error}"))?
        .len();
    let minimum_bundle_len = (BUNDLED_SETUP_MAGIC.len() + BUNDLED_SETUP_LENGTH_LEN) as u64;
    if total_len < minimum_bundle_len {
        return Err(
            "The downloaded installer does not contain its signed setup payload.".to_string(),
        );
    }

    /*
    Authenticode signing appends a certificate table after the executable's
    ordinary overlay. Find Ghostex's long trailer marker near the end instead
    of assuming the marker remains the final bytes after SignTool runs.
    */
    let search_start = total_len.saturating_sub(BUNDLED_SETUP_SEARCH_LEN);
    source
        .seek(SeekFrom::Start(search_start))
        .map_err(|error| format!("The setup payload could not be located: {error}"))?;
    let mut trailer_search = vec![0_u8; (total_len - search_start) as usize];
    source
        .read_exact(&mut trailer_search)
        .map_err(|error| format!("The setup payload header could not be read: {error}"))?;
    let marker_index = trailer_search
        .windows(BUNDLED_SETUP_MAGIC.len())
        .rposition(|candidate| candidate == BUNDLED_SETUP_MAGIC)
        .ok_or_else(|| {
            "The downloaded installer does not contain a valid setup payload.".to_string()
        })?;
    let length_start = marker_index + BUNDLED_SETUP_MAGIC.len();
    let length_end = length_start + BUNDLED_SETUP_LENGTH_LEN;
    let mut length_bytes = [0_u8; BUNDLED_SETUP_LENGTH_LEN];
    length_bytes.copy_from_slice(
        trailer_search
            .get(length_start..length_end)
            .ok_or_else(|| "The setup payload length is incomplete.".to_string())?,
    );
    let payload_len = u64::from_le_bytes(length_bytes);
    let payload_end = search_start + marker_index as u64;
    let payload_start = payload_end.checked_sub(payload_len).ok_or_else(|| {
        "The downloaded installer contains an invalid setup payload length.".to_string()
    })?;
    if payload_len == 0 {
        return Err("The downloaded installer contains an empty setup payload.".to_string());
    }

    source
        .seek(SeekFrom::Start(payload_start))
        .map_err(|error| format!("The setup payload could not be opened: {error}"))?;
    let (temporary, mut destination) = create_temporary_setup()?;
    io::copy(&mut source.take(payload_len), &mut destination)
        .map_err(|error| format!("The setup payload could not be extracted: {error}"))?;
    destination
        .flush()
        .map_err(|error| format!("The setup payload could not be saved: {error}"))?;
    drop(destination);
    Ok(temporary)
}

fn create_temporary_setup() -> Result<(TemporarySetup, File), String> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for suffix in 0..10 {
        let path = env::temp_dir().join(format!(
            "Ghostex-Setup-{}-{unique}-{suffix}.exe",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((TemporarySetup { path }, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "A temporary setup file could not be created: {error}"
                ));
            }
        }
    }
    Err("A unique temporary setup file could not be created.".to_string())
}

struct TemporarySetup {
    path: PathBuf,
}

impl TemporarySetup {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporarySetup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(target_os = "windows")]
fn show_error(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

    let title: Vec<u16> = "Ghostex Setup\0".encode_utf16().collect();
    let body: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn show_error(message: &str) {
    eprintln!("{message}");
}
