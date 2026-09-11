use super::{model::Provider, session_identity::login_identity};
use std::path::{Path, PathBuf};

pub(crate) fn identity(process_id: i64, agent: &str, home: &Path) -> Option<String> {
    let (provider, key) = match agent {
        "claude" => (Provider::Claude, "CLAUDE_CONFIG_DIR"),
        "codex" => (Provider::Codex, "CODEX_HOME"),
        _ => return None,
    };
    let environment = process_environment(process_id)?;
    let prefix = format!("{key}=");
    let value = environment
        .split(|byte| *byte == 0)
        .find_map(|entry| entry.strip_prefix(prefix.as_bytes()))?;
    let root = PathBuf::from(std::str::from_utf8(value).ok()?);
    if !root.is_absolute() {
        return None;
    }
    login_identity(provider, &root, home)
}

#[cfg(target_os = "linux")]
fn process_environment(process_id: i64) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(format!("/proc/{process_id}/environ"))
        .ok()?
        .take(4 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

#[cfg(target_os = "macos")]
fn process_environment(process_id: i64) -> Option<Vec<u8>> {
    let pid = i32::try_from(process_id).ok().filter(|pid| *pid > 0)?;
    let mut argmax = 0_i32;
    let mut size = std::mem::size_of_val(&argmax);
    let mut limit_mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
    // macOS rejects PROCARGS2 buffers larger than kern.argmax with EINVAL.
    let result = unsafe {
        libc::sysctl(
            limit_mib.as_mut_ptr(),
            limit_mib.len() as u32,
            (&mut argmax as *mut i32).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || argmax <= 0 {
        return None;
    }
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut bytes = vec![0_u8; argmax as usize];
    let mut size = bytes.len();
    // KERN_PROCARGS2 returns argc, the executable path, padding, argv, then NUL-separated environment entries.
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        return None;
    }
    bytes.truncate(size);
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    if argc < 0 {
        return None;
    }
    let mut offset = 4;
    offset += bytes.get(offset..)?.iter().position(|byte| *byte == 0)? + 1;
    while bytes.get(offset) == Some(&0) {
        offset += 1;
    }
    for _ in 0..argc {
        offset += bytes.get(offset..)?.iter().position(|byte| *byte == 0)? + 1;
    }
    Some(bytes.get(offset..)?.to_vec())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_environment(_process_id: i64) -> Option<Vec<u8>> {
    None
}
