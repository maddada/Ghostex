use std::{fs::File, io::Read};

use super::{ToolCandidate, ToolCapabilityStatus};

const RECOVERY_GUIDANCE: &str = "Check Windows Security > Virus & threat protection > Protection history, or your antivirus's quarantine, for wmx.exe. Review any detection with the Ghostex maintainers before restoring the file. If no security product blocked it, repair or reinstall Ghostex from the official release.";

/// CDXC:PlatformSupport 2026-10-31 WHY:
/// Defender removed the packaged wmx.exe while gxserver stayed healthy (GitHub #367). Native Windows uses wmx, so sending users to install zmx cannot repair it. Metadata can still succeed for a file whose contents Defender blocks; read its header without launching a helper on every health request.
pub(super) fn resolve_wmx_status(candidates: &[ToolCandidate]) -> ToolCapabilityStatus {
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
        let readable = (|| {
            let mut file = File::open(path)?;
            let mut header = [0; 2];
            file.read_exact(&mut header)?;
            Ok::<_, std::io::Error>(header)
        })();
        match readable {
            Ok(header) if header == *b"MZ" => {
                status.availability = "available".to_string();
                status.candidate_paths = None;
                status.executable_path = Some(path.to_string_lossy().to_string());
                status.guidance = None;
                status.message = format!("wmx.exe resolved from {}.", candidate.source.as_str());
                status.source = Some(candidate.source.as_str().to_string());
                return status;
            }
            Ok(_) => {
                status.availability = "notExecutable".to_string();
                status.message = format!(
                    "Ghostex's Windows session host (wmx.exe) has an invalid executable header at {}. Repair or reinstall Ghostex from the official release.",
                    path.display()
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                // Win32 ERROR_VIRUS_INFECTED / ERROR_VIRUS_DELETED. Other access
                // errors retain their OS message without attributing them to antivirus.
                let reason = if matches!(error.raw_os_error(), Some(225 | 226)) {
                    "was blocked or removed by Windows security software"
                } else {
                    "could not be read"
                };
                status.availability = "notExecutable".to_string();
                status.message = format!(
                    "Ghostex's Windows session host (wmx.exe) {reason} at {}: {error}. {RECOVERY_GUIDANCE}",
                    path.display()
                );
            }
        }
    }
    status
}
