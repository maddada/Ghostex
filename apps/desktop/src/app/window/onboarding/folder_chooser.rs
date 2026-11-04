//! Get started's own folder chooser: a typed path with Tab completion and the folders under it,
//! for when the system's folder dialog cannot open.
//!
//! CDXC:Onboarding 2026-10-28 WHY:
//! On Linux GPUI's folder dialog is the xdg-desktop-portal file chooser; with no portal (or a
//! portal without a file chooser backend, common on minimal installs and tiling desktops) the
//! request fails at once and "Choose folder" did nothing, leaving "Open Ghostex" disabled with no
//! way forward. Windows and macOS always have their system dialog, so there it stays the only
//! chooser; this one opens only when the host reports the dialog unavailable, and the host logs why.
use super::GpuiOnboardingWindow;
use super::install_guide::{modal_actions, modal_p, popup};
use super::interact;
use super::primitives::*;
use super::stage::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div,
};
use gpui_component::input::{Input, InputEvent, InputState};
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::time::Instant;

pub(crate) struct FolderChooser {
    pub(crate) opened_at: Instant,
    input: Entity<InputState>,
    /// Why the system dialog could not open, shown under the title.
    reason: String,
    /// The folder whose subfolders are listed, and those subfolders' names.
    listed_dir: Option<PathBuf>,
    folders: Vec<String>,
    /// Bumped per listing request, so a slow listing never replaces a newer one.
    listing_generation: u64,
    error: Option<String>,
    _subscription: Subscription,
}

fn is_separator(character: char) -> bool {
    character == '/' || character == MAIN_SEPARATOR
}

/// `~` and `~/…` name the home folder, as in a shell.
pub(crate) fn expand_home(typed: &str, home: &Path) -> String {
    let typed = typed.trim();
    if typed == "~" {
        return home.to_string_lossy().into_owned();
    }
    match typed.strip_prefix('~') {
        Some(rest) if rest.starts_with(is_separator) => {
            format!("{}{rest}", home.to_string_lossy())
        }
        _ => typed.to_string(),
    }
}

/// Shows a path under the home folder as `~/…`.
pub(crate) fn display_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~{MAIN_SEPARATOR}{}", rest.to_string_lossy()),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// The folder to list for what is typed, and the start of the name being typed in it:
/// `~/dev/gh` lists `~/dev` for names starting with `gh`; `~/dev/` lists `~/dev` whole.
/// `None` until the text is an absolute path.
pub(crate) fn split_typed(typed: &str, home: &Path) -> Option<(PathBuf, String)> {
    let expanded = expand_home(typed, home);
    if !Path::new(&expanded).is_absolute() {
        return None;
    }
    match expanded.rfind(is_separator) {
        Some(index) => {
            let (dir, prefix) = expanded.split_at(index);
            let dir = if dir.is_empty() || dir.ends_with(':') {
                &expanded[..=index]
            } else {
                dir
            };
            Some((PathBuf::from(dir), prefix[1..].to_string()))
        }
        None => None,
    }
}

/// The listed folders whose names start with `prefix`, sorted case-insensitively. Hidden folders
/// show only once the typed name starts with a dot.
pub(crate) fn matching_folders<'a>(folders: &'a [String], prefix: &str) -> Vec<&'a str> {
    let mut matches: Vec<&str> = folders
        .iter()
        .map(String::as_str)
        .filter(|name| name.starts_with(prefix))
        .filter(|name| prefix.starts_with('.') || !name.starts_with('.'))
        .collect();
    matches.sort_by_key(|name| name.to_lowercase());
    matches
}

/// A folder as the field shows it: under the home folder as `~/…` when the user typed it that way,
/// always ending in a separator so the field lists its subfolders.
pub(crate) fn folder_text(path: &Path, home: &Path, tilde: bool) -> String {
    let mut text = if tilde {
        display_path(path, home)
    } else {
        path.to_string_lossy().into_owned()
    };
    if !text.ends_with(is_separator) {
        text.push(MAIN_SEPARATOR);
    }
    text
}

/// Tab: the typed name completed as far as every matching folder agrees, plus a separator when
/// only one folder matches. `None` when nothing matches or nothing would change.
pub(crate) fn complete(typed: &str, folders: &[String], home: &Path) -> Option<String> {
    let (dir, prefix) = split_typed(typed, home)?;
    let matches = matching_folders(folders, &prefix);
    let first = matches.first()?;
    let mut common = first.to_string();
    for name in &matches[1..] {
        let shared = common
            .chars()
            .zip(name.chars())
            .take_while(|(a, b)| a == b)
            .count();
        common = common.chars().take(shared).collect();
    }
    let path = dir.join(&common);
    let tilde = typed.trim_start().starts_with('~');
    let completed = if matches.len() == 1 {
        folder_text(&path, home, tilde)
    } else if tilde {
        display_path(&path, home)
    } else {
        path.to_string_lossy().into_owned()
    };
    (completed != typed).then_some(completed)
}

/// The folder "Use this folder" picks: the typed path, which must be an existing folder.
pub(crate) fn chosen_folder(typed: &str, home: &Path) -> Result<PathBuf, String> {
    let expanded = expand_home(typed, home);
    if expanded.is_empty() {
        return Err("Type a folder path, or click a folder below.".to_string());
    }
    // `~/dev/` and `~/dev` are the same folder; the project path keeps no trailing separator.
    let trimmed = expanded.trim_end_matches(is_separator);
    let path = if trimmed.is_empty() || trimmed.ends_with(':') {
        PathBuf::from(&expanded)
    } else {
        PathBuf::from(trimmed)
    };
    if !path.is_absolute() {
        return Err("Type a full path, starting with / or ~.".to_string());
    }
    if !path.is_dir() {
        return Err(format!("{expanded} is not a folder on this computer."));
    }
    Ok(path)
}

/// The subfolders of `dir`, by name. Unreadable entries are skipped.
fn list_folders(dir: &Path) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("Could not open {}: {error}", dir.display()))?;
    Ok(entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect())
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::path::MAIN_SEPARATOR_STR))
}

impl GpuiOnboardingWindow {
    /// The host could not open the system folder dialog: choose the folder here instead.
    pub(crate) fn open_folder_chooser(
        &mut self,
        reason: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let home = home_dir();
        let start = self
            .picked_folder
            .clone()
            .filter(|folder| Path::new(folder).is_dir())
            .map(|folder| folder_text(Path::new(&folder), &home, true))
            .unwrap_or_else(|| folder_text(&home, &home, true));
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(format!("~{MAIN_SEPARATOR}projects{MAIN_SEPARATOR}my-app"))
        });
        input.update(cx, |input, cx| {
            input.set_value(start, window, cx);
            input.focus(window, cx);
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    if let Some(chooser) = this.folder_chooser.as_mut() {
                        chooser.error = None;
                    }
                    this.refresh_folder_listing(cx);
                }
                InputEvent::PressEnter { .. } => this.use_chosen_folder(window, cx),
                _ => {}
            },
        );
        self.folder_chooser = Some(FolderChooser {
            opened_at: Instant::now(),
            input,
            reason,
            listed_dir: None,
            folders: Vec::new(),
            listing_generation: 0,
            error: None,
            _subscription: subscription,
        });
        self.refresh_folder_listing(cx);
        cx.notify();
    }

    fn folder_chooser_text(&self, cx: &gpui::App) -> String {
        self.folder_chooser
            .as_ref()
            .map(|chooser| chooser.input.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// Lists the folder the typed path points into, off the main thread, unless it is listed already.
    fn refresh_folder_listing(&mut self, cx: &mut Context<Self>) {
        let home = home_dir();
        let typed = self.folder_chooser_text(cx);
        let Some(chooser) = self.folder_chooser.as_mut() else {
            return;
        };
        let Some((dir, _)) = split_typed(&typed, &home) else {
            cx.notify();
            return;
        };
        if chooser.listed_dir.as_deref() == Some(dir.as_path()) {
            cx.notify();
            return;
        }
        chooser.listing_generation += 1;
        let generation = chooser.listing_generation;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let listed = dir.clone();
            let folders = background.spawn(async move { list_folders(&listed) }).await;
            let _ = this.update(cx, |this, cx| {
                let Some(chooser) = this.folder_chooser.as_mut() else {
                    return;
                };
                if chooser.listing_generation != generation {
                    return;
                }
                chooser.listed_dir = Some(dir);
                match folders {
                    Ok(folders) => chooser.folders = folders,
                    Err(_) => chooser.folders.clear(),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn set_folder_chooser_text(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(chooser) = self.folder_chooser.as_ref() {
            let input = chooser.input.clone();
            input.update(cx, |input, cx| {
                input.set_value(text, window, cx);
                input.focus(window, cx);
            });
        }
        self.refresh_folder_listing(cx);
    }

    /// Tab in the path field.
    fn complete_folder_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let home = home_dir();
        let typed = self.folder_chooser_text(cx);
        let Some(chooser) = self.folder_chooser.as_ref() else {
            return;
        };
        let listed = split_typed(&typed, &home)
            .is_some_and(|(dir, _)| chooser.listed_dir.as_deref() == Some(dir.as_path()));
        if !listed {
            return;
        }
        if let Some(completed) = complete(&typed, &chooser.folders, &home) {
            self.set_folder_chooser_text(completed, window, cx);
        }
    }

    fn use_chosen_folder(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.folder_chooser_text(cx);
        match chosen_folder(&typed, &home_dir()) {
            Ok(path) => {
                self.folder_chooser = None;
                self.receive_picked_folder(path.to_string_lossy().into_owned(), cx);
            }
            Err(error) => {
                if let Some(chooser) = self.folder_chooser.as_mut() {
                    chooser.error = Some(error);
                }
                cx.notify();
            }
        }
    }

    pub(super) fn render_folder_chooser(
        &mut self,
        s: S,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.folder_chooser.as_ref()?;
        let home = home_dir();
        let typed = self.folder_chooser_text(cx);
        let close = self.popup_close(s, cx);
        let chooser = self.folder_chooser.as_ref()?;
        let opened_at = chooser.opened_at;
        let input = chooser.input.clone();
        let reason = chooser.reason.clone();
        let error = chooser.error.clone();
        let split = split_typed(&typed, &home);
        let listed_dir = chooser.listed_dir.clone();
        let current = split
            .as_ref()
            .filter(|(dir, _)| listed_dir.as_deref() == Some(dir.as_path()));
        let (dir, rows): (Option<PathBuf>, Vec<String>) = match current {
            Some((dir, prefix)) => (
                Some(dir.clone()),
                matching_folders(&chooser.folders, prefix)
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            ),
            None => (None, Vec::new()),
        };
        let focused = input.read(cx).focus_handle(cx).is_focused(window);
        let field = div()
            .id("folder-chooser-field")
            .flex()
            .items_center()
            .gap(s.px(12.0))
            .h(s.px(46.0))
            .px(s.px(14.0))
            .rounded(s.px(10.0))
            .border_1()
            .border_color(if focused {
                rgba(110, 150, 255, 0.55)
            } else {
                white(0.1)
            })
            .bg(white(0.03))
            .child(icon(s, "folder", 18.0, 1.6, hex(0x8e97a8)))
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&input)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .w_full()
                        .px(gpui::px(0.0))
                        .py(gpui::px(0.0))
                        .text_size(s.px(15.0))
                        .text_color(hex(0xeef1f7)),
                ),
            )
            .capture_action(cx.listener(|this, _: &super::FocusNext, window, cx| {
                cx.stop_propagation();
                this.complete_folder_path(window, cx);
            }));
        let row = |id: SharedString, icon_name: &'static str, text: String| {
            let hover =
                interact::hover_color(&id, "bg", white(0.0), white(0.06), interact::BUTTON_MS);
            div()
                .id(id)
                .flex()
                .flex_none()
                .items_center()
                .gap(s.px(10.0))
                .h(s.px(34.0))
                .px(s.px(10.0))
                .rounded(s.px(7.0))
                .bg(hover)
                .cursor_pointer()
                .child(icon(s, icon_name, 16.0, 1.6, hex(0x9aa3b4)))
                .child(
                    sans(s, 14.0, 400.0, hex(0xdfe4ee))
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(text),
                )
        };
        let mut list = div()
            .id("folder-chooser-list")
            .flex()
            .flex_col()
            .gap(s.px(2.0))
            .h(s.px(300.0))
            .overflow_y_scroll();
        if let Some(dir) = &dir {
            if let Some(parent) = dir.parent() {
                let target = folder_text(parent, &home, true);
                let up = interact::watch_hover(
                    row("folder-chooser-up".into(), "chevL", "Up one folder".into()),
                    "folder-chooser-up",
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.set_folder_chooser_text(target.clone(), window, cx);
                }));
                list = list.child(up);
            }
            for name in &rows {
                let id = SharedString::from(format!("folder-chooser-{name}"));
                let target = folder_text(&dir.join(name), &home, true);
                let entry = interact::watch_hover(row(id.clone(), "folder", name.clone()), id)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_folder_chooser_text(target.clone(), window, cx);
                    }));
                list = list.child(entry);
            }
            if rows.is_empty() {
                list = list.child(
                    ss(s, 13.0)
                        .px(s.px(10.0))
                        .py(s.px(8.0))
                        .child("No folders here. Use this folder, or go up one."),
                );
            }
        } else {
            list = list.child(
                ss(s, 13.0)
                    .px(s.px(10.0))
                    .py(s.px(8.0))
                    .child("Type a full path, starting with / or ~."),
            );
        }
        let actions = modal_actions(s)
            .child(self.control(
                s,
                ghost(s, "folder-chooser-cancel", "Cancel", 15.5, false),
                "folder-chooser-cancel",
                interact::Ring::new(0.0, 0.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| {
                    this.folder_chooser = None;
                    cx.notify();
                },
            ))
            .child(self.control(
                s,
                cta(
                    s,
                    "folder-chooser-use",
                    "Use this folder",
                    true,
                    false,
                    false,
                    CtaSize::Small,
                ),
                "folder-chooser-use",
                interact::Ring::new(10.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                |this, window, cx| this.use_chosen_folder(window, cx),
            ));
        let body = vec![
            modal_p(s)
                .child(format!(
                    "{reason} Type the folder's path (Tab completes it) or click through the folders below."
                ))
                .into_any_element(),
            div().mt(s.px(14.0)).child(field).into_any_element(),
            div()
                .min_h(s.px(22.0))
                .mt(s.px(6.0))
                .when_some(error, |this, error| {
                    this.child(sans(s, 13.0, 400.0, hex(0xff6b62)).child(error))
                })
                .into_any_element(),
            div()
                .mt(s.px(4.0))
                .p(s.px(6.0))
                .rounded(s.px(10.0))
                .border_1()
                .border_color(white(0.07))
                .bg(white(0.02))
                .child(list)
                .into_any_element(),
            actions.into_any_element(),
        ];
        let this = cx.entity().downgrade();
        Some(popup(
            s,
            "Choose a project folder",
            640.0,
            opened_at,
            now,
            close,
            body,
            move |_, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.folder_chooser = None;
                    cx.notify();
                });
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/home/me")
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn tilde_expands_to_home() {
        assert_eq!(expand_home("~", &home()), "/home/me");
        assert_eq!(expand_home("~/dev", &home()), "/home/me/dev");
        assert_eq!(expand_home(" /opt ", &home()), "/opt");
        assert_eq!(expand_home("~other", &home()), "~other");
    }

    #[test]
    fn typed_path_splits_into_folder_and_name() {
        assert_eq!(
            split_typed("~/dev/gh", &home()),
            Some((PathBuf::from("/home/me/dev"), "gh".to_string()))
        );
        assert_eq!(
            split_typed("~/dev/", &home()),
            Some((PathBuf::from("/home/me/dev"), String::new()))
        );
        assert_eq!(
            split_typed("/", &home()),
            Some((PathBuf::from("/"), String::new()))
        );
        assert_eq!(
            split_typed("/wo", &home()),
            Some((PathBuf::from("/"), "wo".to_string()))
        );
        assert_eq!(split_typed("relative/path", &home()), None);
        assert_eq!(split_typed("", &home()), None);
    }

    #[test]
    fn hidden_folders_show_only_for_a_dot() {
        let folders = names(&["src", ".git", "Docs", "dist"]);
        assert_eq!(matching_folders(&folders, ""), vec!["dist", "Docs", "src"]);
        assert_eq!(matching_folders(&folders, "d"), vec!["dist"]);
        assert_eq!(matching_folders(&folders, "."), vec![".git"]);
    }

    #[test]
    fn tab_completes_the_shared_start_then_the_only_match() {
        let folders = names(&["ghostex", "ghostty", "notes"]);
        assert_eq!(
            complete("~/dev/g", &folders, &home()),
            Some("~/dev/ghost".to_string())
        );
        assert_eq!(
            complete("~/dev/ghoste", &folders, &home()),
            Some("~/dev/ghostex/".to_string())
        );
        assert_eq!(
            complete("/home/me/dev/n", &folders, &home()),
            Some("/home/me/dev/notes/".to_string())
        );
        assert_eq!(complete("~/dev/ghost", &folders, &home()), None);
        assert_eq!(complete("~/dev/zz", &folders, &home()), None);
    }

    #[test]
    fn only_an_existing_absolute_folder_can_be_chosen() {
        let dir = std::env::temp_dir();
        assert_eq!(
            chosen_folder(&dir.to_string_lossy(), &home()),
            Ok(dir.clone())
        );
        let with_separator = format!(
            "{}{MAIN_SEPARATOR}",
            dir.to_string_lossy().trim_end_matches(MAIN_SEPARATOR)
        );
        assert!(
            !chosen_folder(&with_separator, &home())
                .unwrap()
                .to_string_lossy()
                .ends_with(MAIN_SEPARATOR)
        );
        assert!(chosen_folder("", &home()).is_err());
        assert!(chosen_folder("relative", &home()).is_err());
        assert!(chosen_folder("/definitely/not/a/folder/here", &home()).is_err());
    }

    #[test]
    fn folder_text_ends_in_one_separator() {
        assert_eq!(folder_text(Path::new("/"), &home(), true), "/");
        assert_eq!(folder_text(Path::new("/home/me"), &home(), true), "~/");
        assert_eq!(
            folder_text(Path::new("/home/me/dev"), &home(), false),
            "/home/me/dev/"
        );
    }

    #[test]
    fn home_paths_display_with_a_tilde() {
        assert_eq!(display_path(Path::new("/home/me"), &home()), "~");
        assert_eq!(display_path(Path::new("/home/me/dev"), &home()), "~/dev");
        assert_eq!(display_path(Path::new("/opt/x"), &home()), "/opt/x");
    }
}
