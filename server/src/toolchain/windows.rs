use std::{
    fs::{self, File, Metadata},
    io::{Error, ErrorKind, Read},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime},
};

use super::{ToolCandidate, ToolCapabilityStatus};

const RECOVERY_GUIDANCE: &str = "Check Windows Security > Virus & threat protection > Protection history, or your antivirus's quarantine, for wmx.exe. Review any detection with the Ghostex maintainers before restoring the file. If no security product blocked it, repair or reinstall Ghostex from the official release.";

/// How long a good read of wmx.exe stands while its size and modified time stay the same.
const GOOD_READ_HOLDS_FOR: Duration = Duration::from_secs(30);

/// The last time wmx.exe's header read as an executable, and the file it read.
struct GoodRead {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
    read_at: Instant,
}

impl GoodRead {
    fn still_holds(&self, path: &Path, metadata: &Metadata) -> bool {
        self.path == path
            && self.len == metadata.len()
            && self.modified == metadata.modified().ok()
            && self.read_at.elapsed() < GOOD_READ_HOLDS_FOR
    }
}

static LAST_GOOD_READ: Mutex<Option<GoodRead>> = Mutex::new(None);

/// Reports wmx.exe availability from its readable executable header, preserving
/// OS errors and recovery guidance when a candidate is missing or blocked.
///
/// CDXC:PlatformSupport 2026-10-31 WHY:
/// Defender removed the packaged wmx.exe while gxserver stayed healthy (GitHub #367, a duplicate of #224). Native Windows uses wmx, so sending users to install zmx cannot repair it. Metadata can still succeed for a file whose contents Defender blocks, so the header is read, without launching a helper. The health answer has an 800ms budget (CDXC:ServerDaemon 2026-10-18 in toolchain.rs), every session operation comes through here too, and opening an executable can wait on antivirus, so a good read stands for 30 seconds while the file's size and modified time stay the same. A removed or replaced file shows at once, and contents blocked in place show within those 30 seconds.
pub(super) fn resolve_wmx_status(candidates: &[ToolCandidate]) -> ToolCapabilityStatus {
    resolve_wmx_status_with(candidates, &LAST_GOOD_READ)
}

fn resolve_wmx_status_with(
    candidates: &[ToolCandidate],
    last_good_read: &Mutex<Option<GoodRead>>,
) -> ToolCapabilityStatus {
    let mut status = ToolCapabilityStatus {
        availability: "missing".to_string(),
        candidate_paths: Some(
            candidates
                .iter()
                .map(|candidate| candidate.executable_path.to_string_lossy().to_string())
                .collect(),
        ),
        capability: "zmxLifecycle".to_string(),
        executable_path: None,
        guidance: Some(RECOVERY_GUIDANCE.to_string()),
        message: format!(
            "Ghostex's Windows session host (wmx.exe) was not found{}. {RECOVERY_GUIDANCE}",
            candidates
                .first()
                .map(|candidate| format!(" at {}", candidate.executable_path.display()))
                .unwrap_or_default()
        ),
        source: None,
        tool: "zmx".to_string(),
    };

    for candidate in candidates {
        let path = &candidate.executable_path;
        let metadata = fs::metadata(path).ok();
        let holds = metadata.as_ref().is_some_and(|metadata| {
            last_good_read.lock().ok().is_some_and(|last| {
                last.as_ref()
                    .is_some_and(|read| read.still_holds(path, metadata))
            })
        });
        let readable = if holds { Ok(*b"MZ") } else { read_header(path) };
        match readable {
            Ok(header) if header == *b"MZ" => {
                if !holds {
                    if let (Some(metadata), Ok(mut last)) =
                        (metadata.as_ref(), last_good_read.lock())
                    {
                        *last = Some(GoodRead {
                            path: path.clone(),
                            len: metadata.len(),
                            modified: metadata.modified().ok(),
                            read_at: Instant::now(),
                        });
                    }
                }
                status.availability = "available".to_string();
                status.candidate_paths = None;
                status.executable_path = Some(path.to_string_lossy().to_string());
                status.guidance = None;
                status.message = format!("wmx.exe resolved from {}.", candidate.source.as_str());
                status.source = Some(candidate.source.as_str().to_string());
                return status;
            }
            Ok(_) => set_invalid_header(&mut status, path),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            // A file too short to hold a header is as broken as one with the wrong header.
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => {
                set_invalid_header(&mut status, path)
            }
            Err(error) => {
                status.availability = "notExecutable".to_string();
                status.message = format!(
                    "Ghostex's Windows session host (wmx.exe) {} at {}: {error}. {RECOVERY_GUIDANCE}",
                    read_failure_reason(&error),
                    path.display()
                );
            }
        }
    }
    status
}

fn read_header(path: &Path) -> Result<[u8; 2], Error> {
    let mut file = File::open(path)?;
    let mut header = [0; 2];
    file.read_exact(&mut header)?;
    Ok(header)
}

fn set_invalid_header(status: &mut ToolCapabilityStatus, path: &Path) {
    status.availability = "notExecutable".to_string();
    status.message = format!(
        "Ghostex's Windows session host (wmx.exe) has an invalid executable header at {}. Repair or reinstall Ghostex from the official release.",
        path.display()
    );
}

/// Win32 ERROR_VIRUS_INFECTED / ERROR_VIRUS_DELETED. Other access errors retain
/// their OS message without attributing them to antivirus.
fn read_failure_reason(error: &Error) -> &'static str {
    if matches!(error.raw_os_error(), Some(225 | 226)) {
        "was blocked or removed by Windows security software"
    } else {
        "could not be read"
    }
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;

    use super::super::ToolSource;
    use super::*;

    fn candidates(path: &Path) -> Vec<ToolCandidate> {
        vec![ToolCandidate {
            executable_path: path.to_path_buf(),
            source: ToolSource::GxserverBundle,
        }]
    }

    fn resolve(path: &Path) -> ToolCapabilityStatus {
        resolve_wmx_status_with(&candidates(path), &Mutex::new(None))
    }

    #[test]
    fn a_file_with_an_executable_header_is_available_under_the_zmx_wire_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("wmx.exe");
        fs::write(&path, b"MZ rest of the program").expect("write");
        let status = resolve(&path);
        assert_eq!(status.availability, "available");
        assert_eq!(status.tool, "zmx");
        assert_eq!(status.capability, "zmxLifecycle");
        assert_eq!(status.source.as_deref(), Some("gxserverBundle"));
        assert_eq!(status.executable_path.as_deref(), path.to_str());
        assert_eq!(status.candidate_paths, None);
        assert_eq!(status.guidance, None);
    }

    #[test]
    fn a_missing_file_is_named_with_where_it_should_be_and_how_to_repair_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("wmx.exe");
        let status = resolve_wmx_status(&candidates(&path));
        assert_eq!(status.availability, "missing");
        assert_eq!(status.tool, "zmx");
        assert_eq!(status.executable_path, None);
        assert!(status.message.contains("wmx.exe) was not found at"));
        assert!(status.message.contains(path.to_str().unwrap()));
        assert!(status.message.contains("Protection history"));
        assert_eq!(status.guidance.as_deref(), Some(RECOVERY_GUIDANCE));
        assert_eq!(
            status.candidate_paths,
            Some(vec![path.to_string_lossy().to_string()])
        );
    }

    #[test]
    fn a_wrong_or_short_header_is_not_executable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("wmx.exe");
        for contents in [&b"#!/bin/sh"[..], b"M", b""] {
            fs::write(&path, contents).expect("write");
            let status = resolve(&path);
            assert_eq!(status.availability, "notExecutable", "{contents:?}");
            assert!(status.message.contains("invalid executable header"));
            assert_eq!(status.executable_path, None);
            assert_eq!(status.guidance.as_deref(), Some(RECOVERY_GUIDANCE));
        }
    }

    /// A folder can't be read as a file anywhere, which stands in for a file Windows refuses.
    #[test]
    fn a_file_that_cannot_be_read_says_so_with_the_os_error_and_guidance() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("wmx.exe");
        fs::create_dir(&path).expect("mkdir");
        let status = resolve(&path);
        assert_eq!(status.availability, "notExecutable");
        assert!(status.message.contains("could not be read at"));
        assert!(status.message.contains("Protection history"));
        assert!(!status.message.contains("security software"));
    }

    #[test]
    fn only_the_two_virus_errors_are_blamed_on_security_software() {
        for code in [225, 226] {
            assert_eq!(
                read_failure_reason(&Error::from_raw_os_error(code)),
                "was blocked or removed by Windows security software"
            );
        }
        for code in [2, 5, 32] {
            assert_eq!(
                read_failure_reason(&Error::from_raw_os_error(code)),
                "could not be read"
            );
        }
    }

    /// Writes `contents` over the file without changing its modified time.
    fn overwrite_keeping_modified(path: &Path, contents: &[u8]) {
        let modified = fs::metadata(path).unwrap().modified().unwrap();
        fs::write(path, contents).expect("write");
        OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }

    #[test]
    fn a_good_read_stands_until_the_file_changes_or_its_time_is_up() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("wmx.exe");
        fs::write(&path, b"MZ12").expect("write");
        let last = Mutex::new(None);
        let status = |last: &Mutex<Option<GoodRead>>| {
            resolve_wmx_status_with(&candidates(&path), last).availability
        };
        assert_eq!(status(&last), "available");
        let first_read = last.lock().unwrap().as_ref().unwrap().read_at;

        // Same size and time: the file isn't opened again, and the read's age isn't renewed.
        overwrite_keeping_modified(&path, b"XX12");
        assert_eq!(status(&last), "available");
        assert_eq!(last.lock().unwrap().as_ref().unwrap().read_at, first_read);

        // Its time is up: the file is read again.
        last.lock().unwrap().as_mut().unwrap().read_at =
            Instant::now() - GOOD_READ_HOLDS_FOR - Duration::from_secs(1);
        assert_eq!(status(&last), "notExecutable");

        // A different size shows at once, however recent the read.
        fs::write(&path, b"MZ12").expect("write");
        assert_eq!(status(&last), "available");
        fs::write(&path, b"XX123").expect("write");
        assert_eq!(status(&last), "notExecutable");

        // A removed file shows at once too.
        fs::write(&path, b"MZ12").expect("write");
        assert_eq!(status(&last), "available");
        fs::remove_file(&path).expect("remove");
        assert_eq!(status(&last), "missing");
    }
}
