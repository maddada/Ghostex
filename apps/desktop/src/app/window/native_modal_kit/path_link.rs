//! A file or folder path drawn in a dialog, Settings page, card or notice that opens in the Files
//! view when clicked.
//!
//! CDXC:PathLinks 2026-11-08 DECISION:
//! User: "pls whenever we have a file path like this make clicking on it open it in ghostex files"
//! (Settings > Autopilot's `~/.local/state/ghostex/autopilot` chip did nothing). Every path Ghostex
//! shows in its own UI is a link: a click opens it in Files (a folder opens that folder), the
//! pointer is a hand and the text gets an underline on hover; a right-click offers Copy path and
//! Reveal in Finder/Explorer. The look and the click live here once; the app installs the host
//! that knows how to open (`app/path_links.rs`). Without a host (the preview binaries) the link
//! still draws and underlines but a click does nothing.
//!
//! This module depends only on gpui so the preview binaries can include it with `#[path]`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, Point, SharedString, Stateful, StatefulInteractiveElement as _,
    Styled as _, Window, div,
};

/// What the app does with a path link.
#[derive(Clone)]
pub(crate) struct PathLinkHost {
    /// A click: open `path` (a `~/` or absolute path) in the Files view.
    pub(crate) open: Rc<dyn Fn(String, &mut Window, &mut App)>,
    /// A right-click at `position` (in the clicked window): the Copy path / Reveal menu.
    pub(crate) menu: Rc<dyn Fn(String, Point<Pixels>, &mut Window, &mut App)>,
}

thread_local! {
    static PATH_LINK_HOST: RefCell<Option<PathLinkHost>> = const { RefCell::new(None) };
}

/// Installs the host the path links hand clicks to.
pub(crate) fn install_path_link_host(host: PathLinkHost) {
    PATH_LINK_HOST.with(|slot| *slot.borrow_mut() = Some(host));
}

fn path_link_host() -> Option<PathLinkHost> {
    PATH_LINK_HOST.with(|slot| slot.borrow().clone())
}

/// Whether `text` is a path to link: `~/…`, `~` alone, a rooted `/…` path with no spaces or a
/// Windows drive path. Text with spaces, a URL, a command or a bare word is not.
pub(crate) fn looks_like_path(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.contains("://") || text.chars().any(char::is_whitespace) {
        return false;
    }
    let bytes = text.as_bytes();
    if text == "~" || text.starts_with("~/") || text.starts_with("~\\") {
        return text.len() > 1;
    }
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return matches!(bytes[2], b'\\' | b'/');
    }
    // `/a` and longer: a lone `/` or `//` is punctuation, not a path.
    text.starts_with('/') && text.trim_matches('/').len() > 0 && !text.starts_with("//")
}

/// Wraps `child` (a chip or plain text showing `path`) as the link described above.
pub(crate) fn path_link(
    id: impl Into<ElementId>,
    path: impl Into<SharedString>,
    child: impl IntoElement,
) -> Stateful<Div> {
    let path: SharedString = path.into();
    let open_path = path.clone();
    let menu_path = path;
    div()
        .id(id)
        .cursor_pointer()
        .hover(|this| this.underline())
        .on_click(move |event: &ClickEvent, window, cx| {
            if !event.standard_click() {
                return;
            }
            if let Some(host) = path_link_host() {
                (host.open)(open_path.to_string(), window, cx);
            }
        })
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            if let Some(host) = path_link_host() {
                cx.stop_propagation();
                (host.menu)(menu_path.to_string(), event.position, window, cx);
            }
        })
        .child(child)
}

/// `text` with each path in it as a link (the rest as plain text), as one wrapping line of
/// inline items. `id` makes the link ids unique on the page. Text with no path is one plain
/// text block, so a caller can use this for every description.
///
/// CDXC:PathLinks 2026-11-09 WHY: User: "are u serious with this?? what is this man??" (Settings >
/// Secrets' 1Password description ran past the page's right edge, cut off by the window). A flex
/// item never shrinks below its min-content width, and GPUI measures a text's min-content as its
/// whole unwrapped line, so a plain run inside the wrapping flex row stayed one line however
/// narrow the page. Text with no path is a plain block (no flex row at all), and every run of a
/// linked text is `min_w_0` so it wraps inside the column instead of pushing past it.
pub(crate) fn linkified_text(id: &str, text: &str) -> AnyElement {
    if !text.split(' ').any(|word| {
        let path = split_trailing_punctuation(word).0.trim_matches('`');
        looks_like_path(path)
    }) {
        return div().w_full().child(text.to_string()).into_any_element();
    }
    let mut runs: Vec<AnyElement> = Vec::new();
    let mut plain = String::new();
    let mut links = 0usize;
    for (index, word) in text.split(' ').enumerate() {
        if index > 0 {
            plain.push(' ');
        }
        let (core, tail) = split_trailing_punctuation(word);
        let path = core.trim_matches('`');
        if !looks_like_path(path) {
            plain.push_str(word);
            continue;
        }
        if !plain.is_empty() {
            runs.push(
                div()
                    .min_w_0()
                    .child(std::mem::take(&mut plain))
                    .into_any_element(),
            );
        }
        links += 1;
        runs.push(
            path_link(
                SharedString::from(format!("{id}-path-{links}")),
                path.to_string(),
                div().child(path.to_string()),
            )
            .min_w_0()
            .into_any_element(),
        );
        plain.push_str(tail);
    }
    if !plain.is_empty() {
        runs.push(div().min_w_0().child(plain).into_any_element());
    }
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_baseline()
        .children(runs)
        .into_any_element()
}

/// `word` split before its trailing sentence punctuation (`.`, `,`, `;`, `:`, `)`).
fn split_trailing_punctuation(word: &str) -> (&str, &str) {
    let trimmed = word.trim_end_matches(['.', ',', ';', ':', ')', '!', '?']);
    // A path that ends in a dot-file name keeps what is part of it: only strip when something is left.
    if trimmed.is_empty() {
        (word, "")
    } else {
        (trimmed, &word[trimmed.len()..])
    }
}
