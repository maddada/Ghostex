//! This computer's own name, for its default short name on the sidebar's computer pills
//! (gx-core `machine_short_names`). Its own file so the app, the web build (which has none and
//! says "Local") and the modal preview binary can all include it.

/// The host name without its domain ("Studio.local" is "Studio"), else "Local".
pub(crate) fn local_computer_name() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        #[cfg(all(unix, not(ghostex_web)))]
        let host = {
            let mut name = [0u8; 256];
            // SAFETY: the buffer is writable for its whole length and stays alive for the call.
            (unsafe { libc::gethostname(name.as_mut_ptr().cast(), name.len()) } == 0).then(|| {
                let end = name
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(name.len());
                String::from_utf8_lossy(&name[..end]).trim().to_string()
            })
        };
        #[cfg(windows)]
        let host = std::env::var("COMPUTERNAME").ok();
        #[cfg(not(any(all(unix, not(ghostex_web)), windows)))]
        let host: Option<String> = None;
        host.and_then(|host| host.split('.').next().map(str::to_string))
            .filter(|host| !host.trim().is_empty())
            .unwrap_or_else(|| "Local".to_string())
    })
    .clone()
}
