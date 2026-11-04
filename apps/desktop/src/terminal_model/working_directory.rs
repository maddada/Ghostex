//! The working directory a shell reports over OSC 7, turned into the plain path that clicked links are joined onto. The web build lifts these functions into its own `terminal_model` (`apps/gpui-web/extracted-items.txt`), so they use only `std` and this file's own helpers.

/// CDXC:Terminal 2026-10-26 WHY: libghostty-vt stores the OSC 7 payload unparsed (`file://<host>/<percent-encoded path>` or `kitty-shell-cwd://<host>/<raw path>`) and leaves decoding to the embedder, so a Cmd+clicked relative path was joined onto `file://<host>/…` and the file was reported missing. A local host (empty, `localhost` or this computer's name, the check Ghostty's own app makes) becomes the plain path; a report from another computer, or a value that is not such a URL, is returned unchanged so nothing reads it as a path on this one.
pub(crate) fn terminal_working_directory_from_osc7(raw: &str) -> String {
    file_url_local_path(raw, terminal_local_hostname().as_deref())
        .unwrap_or_else(|| raw.to_string())
}

/// The path of a `file://` or `kitty-shell-cwd://` URL whose host is this computer (`local_hostname`), percent-decoded for `file://`. `None` for another computer's URL or a value that is not one of these URLs.
pub(crate) fn file_url_local_path(url: &str, local_hostname: Option<&str>) -> Option<String> {
    let strip_scheme = |scheme: &str| {
        url.get(..scheme.len())
            .filter(|prefix| prefix.eq_ignore_ascii_case(scheme))
            .and_then(|_| url.get(scheme.len()..))
    };
    let (rest, percent_encoded) = match strip_scheme("file://") {
        Some(rest) => (rest, true),
        None => (strip_scheme("kitty-shell-cwd://")?, false),
    };
    let (host, path) = rest.split_at(rest.find('/')?);
    let local = host.is_empty()
        || host.eq_ignore_ascii_case("localhost")
        || local_hostname.is_some_and(|name| !name.is_empty() && host.eq_ignore_ascii_case(name));
    if !local {
        return None;
    }
    let path = if percent_encoded {
        file_url_percent_decode(path.split(['?', '#']).next().unwrap_or_default())?
    } else {
        path.to_string()
    };
    // A Windows shell reports `/C:/Users/…`; the drive path is the part after that slash.
    let bytes = path.as_bytes();
    let windows_drive = bytes.len() >= 4
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && matches!(bytes[3], b'/' | b'\\');
    Some(if windows_drive {
        path[1..].to_string()
    } else {
        path
    })
}

/// `%XX` escapes decoded to bytes; `None` when the result is not UTF-8. A `%` without two hex digits stays as it is.
fn file_url_percent_decode(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let hex = |index: usize| {
        bytes
            .get(index)
            .and_then(|byte| char::from(*byte).to_digit(16))
    };
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let (Some(high), Some(low)) = (hex(index + 1), hex(index + 2))
        {
            decoded.push((high * 16 + low) as u8);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(decoded).ok()
}

/// This computer's host name as a shell reports it in OSC 7 (`$HOST`), read once.
pub(crate) fn terminal_local_hostname() -> Option<String> {
    static HOSTNAME: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    HOSTNAME
        .get_or_init(|| {
            #[cfg(all(unix, not(target_family = "wasm")))]
            let host = {
                let mut name = [0u8; 256];
                // SAFETY: the buffer is writable for its whole length and stays alive for the call.
                (unsafe { libc::gethostname(name.as_mut_ptr().cast(), name.len()) } == 0).then(
                    || {
                        let end = name
                            .iter()
                            .position(|byte| *byte == 0)
                            .unwrap_or(name.len());
                        String::from_utf8_lossy(&name[..end]).trim().to_string()
                    },
                )
            };
            #[cfg(windows)]
            let host = std::env::var("COMPUTERNAME").ok();
            #[cfg(target_family = "wasm")]
            let host: Option<String> = None;
            host.filter(|host| !host.is_empty())
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::file_url_local_path;

    const HOST: Option<&str> = Some("dev-mac.local");

    #[test]
    fn local_hostname_url_is_a_plain_path() {
        assert_eq!(
            file_url_local_path("file://dev-mac.local/Users/madda/dev/_active/Ghostex", HOST)
                .as_deref(),
            Some("/Users/madda/dev/_active/Ghostex")
        );
        assert_eq!(
            file_url_local_path("file://DEV-MAC.LOCAL/tmp", HOST).as_deref(),
            Some("/tmp")
        );
    }

    #[test]
    fn empty_host_is_local() {
        assert_eq!(
            file_url_local_path("file:///Users/madda", HOST).as_deref(),
            Some("/Users/madda")
        );
        assert_eq!(
            file_url_local_path("file:///Users/madda", None).as_deref(),
            Some("/Users/madda")
        );
    }

    #[test]
    fn localhost_is_local() {
        assert_eq!(
            file_url_local_path("file://localhost/srv/app", None).as_deref(),
            Some("/srv/app")
        );
        assert_eq!(
            file_url_local_path("FILE://LocalHost/srv/app", HOST).as_deref(),
            Some("/srv/app")
        );
    }

    #[test]
    fn remote_host_is_not_a_local_path() {
        assert_eq!(
            file_url_local_path("file://build-box/home/ci/repo", HOST),
            None
        );
        assert_eq!(
            file_url_local_path("kitty-shell-cwd://build-box/home/ci", HOST),
            None
        );
        assert_eq!(file_url_local_path("file://dev-mac.local/x", None), None);
    }

    #[test]
    fn percent_encoded_spaces_are_decoded() {
        assert_eq!(
            file_url_local_path("file://dev-mac.local/Users/madda/My%20Project/a%2Bb", HOST)
                .as_deref(),
            Some("/Users/madda/My Project/a+b")
        );
        assert_eq!(
            file_url_local_path("file:///caf%C3%A9%", None).as_deref(),
            Some("/café%")
        );
    }

    #[test]
    fn kitty_shell_cwd_path_is_raw() {
        assert_eq!(
            file_url_local_path(
                "kitty-shell-cwd://dev-mac.local/Users/madda/100%20done",
                HOST
            )
            .as_deref(),
            Some("/Users/madda/100%20done")
        );
    }

    #[test]
    fn other_values_are_not_file_urls() {
        assert_eq!(file_url_local_path("/Users/madda", HOST), None);
        assert_eq!(file_url_local_path("https://example.com/x", HOST), None);
        assert_eq!(file_url_local_path("file://dev-mac.local", HOST), None);
    }

    #[test]
    fn windows_drive_loses_its_leading_slash() {
        assert_eq!(
            file_url_local_path("file:///C:/Users/madda", None).as_deref(),
            Some("C:/Users/madda")
        );
    }
}
