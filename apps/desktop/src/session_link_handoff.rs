//! How a session link reaches the app on Windows and Linux, where the operating system starts a new
//! process with the link as an argument instead of telling the running app. A process that finds
//! Ghostex already running passes its links over and exits; otherwise it keeps them and becomes
//! the app. Either way they end in `queue_gpui_os_integration_urls`, as a macOS open does.
//!
//! CDXC:OsIntegration 2026-10-31 WHY:
//! The GPUI platform crates store `on_open_urls` on Windows and Linux and never call it, and these platforms never had a `ghostex://` route at all: Windows registered no scheme and its second launch dropped its arguments, and the Linux desktop entries declared no `x-scheme-handler`. Only session links are ever passed on, and retired `ghostex://terminal` ones so they can say they are gone; the app parses each again before acting on it.
//! SEE-ALSO: apps/desktop/src/windows_single_instance.rs (the Windows pipe), apps/desktop/src/windows_url_scheme.rs (the Windows registration), server/src/ghostex_cli/launchers.rs (`ghostex <link>`, which Linux desktop entries run), the `MimeType` lines in tooling/release-gpui/linux-stage.sh and linux.sh.

use ghostex_gx_protocol::{is_session_link_route, is_terminal_link_route};

/// More links than one launch or hand-off ever carries are dropped.
const MAX_LINKS: usize = 8;
/// The most one hand-off reads, far above eight links.
pub(crate) const MAX_HANDOFF_BYTES: u64 = 4096;

/// The links among this process's arguments.
#[cfg(not(target_os = "macos"))]
pub(crate) fn links_from_launch_args() -> Vec<String> {
    session_links(
        std::env::args_os()
            .skip(1)
            .map(|argument| argument.to_string_lossy().into_owned()),
    )
}

fn session_links(values: impl Iterator<Item = String>) -> Vec<String> {
    values
        .map(|value| value.trim().to_string())
        .filter(|value| is_session_link_route(value) || is_terminal_link_route(value))
        .take(MAX_LINKS)
        .collect()
}

pub(crate) fn encode_links(links: &[String]) -> Vec<u8> {
    links.join("\n").into_bytes()
}

pub(crate) fn decode_links(payload: &[u8]) -> Vec<String> {
    session_links(String::from_utf8_lossy(payload).lines().map(str::to_string))
}

/// Starts taking links: this launch's own, then every later launch's. They wait in the OS
/// integration queue until the first window registers.
#[cfg(not(target_os = "macos"))]
pub(crate) fn start_receiving(launch_links: Vec<String>, cx: &mut gpui::App) {
    use futures::StreamExt as _;
    let (sender, mut receiver) = futures::channel::mpsc::unbounded::<String>();
    for link in launch_links {
        let _ = sender.unbounded_send(link);
    }
    let deliver = move |link: String| {
        let _ = sender.unbounded_send(link);
    };
    #[cfg(target_os = "windows")]
    crate::windows_single_instance::serve_links(deliver);
    #[cfg(target_os = "linux")]
    if let Err(error) = serve_socket(&link_socket_path(), deliver) {
        eprintln!("Ghostex could not listen for session links: {error}");
    }
    cx.spawn(async move |_| {
        while let Some(link) = receiver.next().await {
            crate::queue_gpui_os_integration_urls(vec![link]);
        }
    })
    .detach();
}

/// Passes the links to a Ghostex already running on this data folder. False when none answers, and
/// this process should become the app. A launch with no links starts as it always has.
#[cfg(target_os = "linux")]
pub(crate) fn hand_off_to_running_app(links: &[String]) -> bool {
    !links.is_empty() && hand_off_over_socket(&link_socket_path(), links)
}

#[cfg(target_os = "linux")]
fn link_socket_path() -> std::path::PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .gxserver_state_dir()
        .join("desktop-links.sock")
}

#[cfg(any(target_os = "linux", test))]
fn hand_off_over_socket(path: &std::path::Path, links: &[String]) -> bool {
    use std::io::Write as _;
    // A socket left by a Ghostex that crashed refuses the connection.
    let Ok(mut socket) = std::os::unix::net::UnixStream::connect(path) else {
        return false;
    };
    socket.write_all(&encode_links(links)).is_ok()
}

#[cfg(any(target_os = "linux", test))]
fn serve_socket(
    path: &std::path::Path,
    mut deliver: impl FnMut(String) + Send + 'static,
) -> std::io::Result<()> {
    use std::io::Read as _;
    use std::os::unix::fs::PermissionsExt as _;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Reached only when no running app answered on it.
    let _ = std::fs::remove_file(path);
    let listener = std::os::unix::net::UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    std::thread::Builder::new()
        .name("ghostex-session-links".into())
        .spawn(move || {
            for socket in listener.incoming().flatten() {
                let _ = socket.set_read_timeout(Some(std::time::Duration::from_secs(1)));
                let mut payload = Vec::new();
                let _ = (&socket).take(MAX_HANDOFF_BYTES).read_to_end(&mut payload);
                for link in decode_links(&payload) {
                    deliver(link);
                }
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: &str = "ghostex://session/S1z:P2if7:G83ae";

    #[test]
    fn only_answered_links_are_taken_from_the_arguments() {
        let arguments = [
            "--flag",
            LINK,
            "ghostex://open?path=/etc",
            "/some/file",
            "  ghostex://session/junk  ",
            // What a hostile link's quote would leave behind `--` on Windows.
            "--renderer-cmd-prefix=calc",
        ];
        assert_eq!(
            session_links(arguments.iter().map(|value| value.to_string())),
            vec![LINK.to_string(), "ghostex://session/junk".to_string()]
        );
    }

    #[test]
    fn a_launch_carries_a_bounded_number_of_links() {
        let links = session_links(std::iter::repeat_n(LINK.to_string(), 100));
        assert_eq!(links.len(), MAX_LINKS);
    }

    #[test]
    fn a_payload_round_trips_and_drops_other_routes() {
        let links = vec![LINK.to_string(), LINK.to_string()];
        assert_eq!(decode_links(&encode_links(&links)), links);
        assert_eq!(
            decode_links(b"ghostex://open?path=/etc\nghostex://session/S1:P2:G3\n\xff\xfe"),
            vec!["ghostex://session/S1:P2:G3".to_string()]
        );
    }

    #[test]
    fn a_running_app_takes_a_second_launchs_links_over_its_socket() {
        let folder = std::env::temp_dir().join(format!("gx-link-{}", std::process::id()));
        let path = folder.join("links.sock");
        // Nobody listening: this process would become the app.
        assert!(!hand_off_over_socket(&path, &[LINK.to_string()]));
        let (sender, receiver) = std::sync::mpsc::channel();
        serve_socket(&path, move |link| {
            let _ = sender.send(link);
        })
        .expect("the socket binds");
        assert!(hand_off_over_socket(&path, &[LINK.to_string()]));
        assert_eq!(
            receiver.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(LINK.to_string())
        );
        // Another route sent straight at the socket is dropped; the next link still arrives.
        assert!(hand_off_over_socket(
            &path,
            &["ghostex://open?path=/etc".to_string()]
        ));
        assert!(hand_off_over_socket(&path, &[LINK.to_string()]));
        assert_eq!(
            receiver.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(LINK.to_string())
        );
        // Only this user can reach it.
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path)
            .expect("the socket")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
        let _ = std::fs::remove_dir_all(folder);
    }
}
