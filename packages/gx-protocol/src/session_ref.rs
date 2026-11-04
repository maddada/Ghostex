//! A session's Global Ref (`<server>:<project>:<session>`, e.g. `S90:P3lv0:G7hgi`), the id every
//! `ghostex` verb takes, and the Markdown mention that names a session by it.
//!
//! CDXC:Cli 2026-10-28 DECISION:
//! User: "i want a new Copy ID that let's me just copy the id and name of an agent so i can mention it in other chats", written `[<session title>](<Global Ref>)` (the "·" in their template was a typo), and "we need to be able to do + to mention agents in the same project". So a mention is a plain Markdown link whose destination is the Global Ref: an agent that reads it can pass the ref to `ghostex agents send`, and the chats draw it as the session's agent pill.
//! SEE-ALSO: packages/gx-core/src/sidebar_menu/copy.rs (Copy ID), packages/gx-chat-core/src/transcript/session_mentions.rs (the pill and the `+` picker), apps/mobile/app/src/screens/sessions-screen/sidebar-menus.ts (the phone's Copy ID), and `is_gxserver_global_session_ref` in server/src/ghostex_cli/rpc.rs, which accepts the same shape.

/// The three parts of a Global Ref.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalSessionRef<'a> {
    pub server_id: &'a str,
    pub project_id: &'a str,
    pub session_id: &'a str,
}

/// `S…:P…:G…`, each part its letter, a digit, then lowercase letters and digits; `None` for
/// anything else, so a URL (`https://…`), a path or a `host:port` never reads as a session.
pub fn parse_global_session_ref(value: &str) -> Option<GlobalSessionRef<'_>> {
    let mut parts = value.split(':');
    let (Some(server_id), Some(project_id), Some(session_id), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    fn segment_ok(segment: &str, prefix: char) -> bool {
        let mut chars = segment.chars();
        chars.next() == Some(prefix)
            && matches!(chars.next(), Some(c) if c.is_ascii_digit())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    }
    (segment_ok(server_id, 'S') && segment_ok(project_id, 'P') && segment_ok(session_id, 'G'))
        .then_some(GlobalSessionRef {
            server_id,
            project_id,
            session_id,
        })
}

/// What every session link starts with; the Global Ref is the rest of it.
const SESSION_LINK_PREFIX: &str = "ghostex://session/";
/// No Global Ref comes near this; a longer link is refused before it is read.
const MAX_SESSION_LINK_LEN: usize = 128;

/// The page that opens a session link; the Global Ref is its fragment.
const SESSION_PAGE_PREFIX: &str = "https://ghostex.dev/open/session#";
/// The retired route, which only answers that it is gone.
const TERMINAL_LINK_ROUTE: &str = "ghostex://terminal";

/// What a session's Copy Link copies: the https page that opens `ghostex://session/<global ref>`.
///
/// CDXC:OsIntegration 2026-10-31 DECISION:
/// User (GitHub issue #373): "Build it, remote machines too", and a closed session offers to restore it. A link can be clicked from any website, so it names a session and nothing else: the ref is the path, and anything beside it (a query, a fragment, a second path part, an escape) makes the link invalid rather than ignored. GitHub drops `ghostex://` links, so asked about a page that makes them clickable the user said "yes now": Copy Link copies the page's address, with the ref in the fragment so no server ever receives it, and `ghostex://session/<global ref>` stays what the page and anything else opens.
/// SEE-ALSO: packages/gx-core/src/session_deep_link.rs (where a link's session is and what opening it may do), apps/desktop/src/app/gx_store/session_deep_link.rs (the route), apps/mobile/app/src/screens/sessions-screen/sessionMention.ts (the phone's Copy Link), public/open/session.html in the maddada-com repo (the page, which checks the ref by these same rules).
pub fn session_page_url(global_ref: &str) -> String {
    format!("{SESSION_PAGE_PREFIX}{global_ref}")
}

fn is_link_route(url: &str, route: &str) -> bool {
    url.get(..route.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(route))
        && matches!(
            url.as_bytes().get(route.len()),
            None | Some(b'/' | b'?' | b'#')
        )
}

/// Whether the link is addressed to the session route at all, valid or not, so a bad one is answered
/// by that route and never read by another. The scheme and host are case-insensitive, as in any URL.
pub fn is_session_link_route(url: &str) -> bool {
    is_link_route(url, &SESSION_LINK_PREFIX[..SESSION_LINK_PREFIX.len() - 1])
}

/// A `ghostex://terminal` link, whatever it asks for.
///
/// CDXC:OsIntegration 2026-10-31 DECISION:
/// Asked why this route exists, when nothing in Ghostex builds it and a web page could use it to run a command with no prompt, the user said "i'm down to ditch it honestly". A link to it now only says the link type is gone. The app's own New Terminal and the script Run/Edit/Cancel flow are not URLs and stay.
pub fn is_terminal_link_route(url: &str) -> bool {
    is_link_route(url, TERMINAL_LINK_ROUTE)
}

/// The Global Ref a session link names; `None` for anything but exactly
/// `ghostex://session/<global ref>`.
pub fn parse_session_link(url: &str) -> Option<GlobalSessionRef<'_>> {
    if url.len() > MAX_SESSION_LINK_LEN {
        return None;
    }
    let start = url.get(..SESSION_LINK_PREFIX.len())?;
    if !start.eq_ignore_ascii_case(SESSION_LINK_PREFIX) {
        return None;
    }
    parse_global_session_ref(&url[SESSION_LINK_PREFIX.len()..])
}

/// `[<title>](<global ref>)`: the title on one line, with the brackets and backslashes Markdown
/// would read escaped; "Session" when it has no title.
pub fn session_mention(title: &str, global_ref: &str) -> String {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = if title.is_empty() { "Session" } else { &title };
    let mut label = String::with_capacity(title.len());
    for character in title.chars() {
        if matches!(character, '\\' | '[' | ']') {
            label.push('\\');
        }
        label.push(character);
    }
    format!("[{label}]({global_ref})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_refs_are_three_lettered_parts() {
        let parsed = parse_global_session_ref("S90:P3lv0:G7hgi").expect("a ref");
        assert_eq!(
            (parsed.server_id, parsed.project_id, parsed.session_id),
            ("S90", "P3lv0", "G7hgi")
        );
        for other in [
            "https://example.com",
            "S90:P3lv0",
            "S90:P3lv0:G7hgi:extra",
            "S90:P3lv0:Gabc",
            "s90:P3lv0:G7hgi",
            "S90:P3LV0:G7hgi",
            "localhost:8080",
            "/tmp/S90:P3lv0:G7hgi",
            "",
        ] {
            assert!(parse_global_session_ref(other).is_none(), "{other}");
        }
    }

    #[test]
    fn a_session_link_reads_its_ref() {
        let parsed = parse_session_link("ghostex://session/S1z:P2if7:G83ae").expect("a link");
        assert_eq!(
            (parsed.server_id, parsed.project_id, parsed.session_id),
            ("S1z", "P2if7", "G83ae")
        );
        // Some launchers change the case of a URL's scheme and host, never of its path.
        assert!(parse_session_link("GHOSTEX://Session/S1z:P2if7:G83ae").is_some());
    }

    #[test]
    fn copy_link_copies_the_page_with_the_ref_in_its_fragment() {
        assert_eq!(
            session_page_url("S1z:P2if7:G83ae"),
            "https://ghostex.dev/open/session#S1z:P2if7:G83ae"
        );
    }

    #[test]
    fn the_retired_terminal_route_is_recognised_however_it_is_written() {
        for route in [
            "ghostex://terminal",
            "GHOSTEX://TERMINAL/",
            "ghostex://terminal?command=id&cwd=/tmp",
            "ghostex://terminal#x",
        ] {
            assert!(is_terminal_link_route(route), "{route}");
        }
        for other in [
            "ghostex://terminals",
            "ghostex://terminal.evil/",
            "ghostex://open?path=/tmp",
            "ghostex://session/S1:P2:G3",
            "",
        ] {
            assert!(!is_terminal_link_route(other), "{other}");
        }
    }

    #[test]
    fn a_session_link_is_refused_with_anything_beside_the_ref() {
        let too_long = format!("ghostex://session/S1z:P2if7:G{}", "8".repeat(200));
        for other in [
            "ghostex://session",
            "ghostex://session/",
            "ghostex://session/S1z:P2if7",
            "ghostex://session/S1z:P2if7:G83ae/",
            "ghostex://session/S1z:P2if7:G83ae/extra",
            "ghostex://session/S1z:P2if7:G83ae?command=rm%20-rf",
            "ghostex://session/S1z:P2if7:G83ae#fragment",
            "ghostex://session/S1z%3AP2if7%3AG83ae",
            "ghostex://session/../terminal?command=id",
            "ghostex://session/S1z:P2if7:G83AE",
            "ghostex://session/S1z:P2if7:G83ae\n",
            " ghostex://session/S1z:P2if7:G83ae",
            "ghostex://session/S1z:P2if7:G83a\u{e9}",
            "ghostex://user@session/S1z:P2if7:G83ae",
            "ghostex://session:80/S1z:P2if7:G83ae",
            "ghostex://terminal?command=id",
            "ghostex://open?path=/etc/passwd",
            "https://session/S1z:P2if7:G83ae",
            "ghostex:session/S1z:P2if7:G83ae",
            "S1z:P2if7:G83ae",
            "",
            too_long.as_str(),
        ] {
            assert!(parse_session_link(other).is_none(), "{other}");
        }
    }

    #[test]
    fn only_session_links_reach_the_session_route() {
        for route in [
            "ghostex://session/S1z:P2if7:G83ae",
            "GHOSTEX://SESSION/S1z:P2if7:G83ae",
            "ghostex://session",
            "ghostex://session/",
            "ghostex://session?x=1",
            "ghostex://session#x",
        ] {
            assert!(is_session_link_route(route), "{route}");
        }
        for other in [
            "ghostex://terminal?command=id",
            "ghostex://open?path=/tmp",
            "ghostex://sessions/S1z:P2if7:G83ae",
            "ghostex://session.evil/S1z:P2if7:G83ae",
            "https://session/S1z:P2if7:G83ae",
            "ghostex://sess",
            "",
        ] {
            assert!(!is_session_link_route(other), "{other}");
        }
    }

    #[test]
    fn a_mention_is_a_markdown_link_to_the_ref() {
        assert_eq!(
            session_mention(
                "Model and effort inventory for agent CLIs",
                "S90:P3lv0:G7hgi"
            ),
            "[Model and effort inventory for agent CLIs](S90:P3lv0:G7hgi)"
        );
        assert_eq!(
            session_mention("Fix [beta]\n  build", "S1:P2:G3"),
            r"[Fix \[beta\] build](S1:P2:G3)"
        );
        assert_eq!(session_mention("  ", "S1:P2:G3"), "[Session](S1:P2:G3)");
    }
}
