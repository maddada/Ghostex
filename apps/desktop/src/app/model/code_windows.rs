//! The extra VS Code windows owned by a project's Code view, shown in its window-tab row above
//! the Code page (`app/workarea/code_windows.rs`).

use gpui::SharedString;

/// One extra Code window, for the life of the app: ids are never reused and never saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CodeWindowId(pub(crate) u64);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CodeWindow {
    pub(crate) id: CodeWindowId,
    /// The path and query of the window's page on the Code view's code-server origin
    /// (`/?folder=…`, `/?workspace=…`, `/?ew=true`): where it opened, then where its own script
    /// reports it went, so a page built again comes back to the folder the window has open.
    location: String,
    /// The folder or workspace the window has open, or "New Window" (VS Code's name for a window
    /// with nothing open), worked out when `location` changes so drawing the tab parses nothing.
    label: SharedString,
    /// VS Code's window title, as its own script reports it.
    pub(crate) title: Option<SharedString>,
}

impl CodeWindow {
    pub(crate) fn new(id: CodeWindowId, location: String) -> Self {
        Self {
            id,
            label: code_window_label(&location).into(),
            location,
            title: None,
        }
    }

    pub(crate) fn location(&self) -> &str {
        &self.location
    }

    pub(crate) fn label(&self) -> SharedString {
        self.label.clone()
    }

    /// Records where the window's page went; false when it was already there.
    pub(crate) fn set_location(&mut self, location: &str) -> bool {
        if self.location == location {
            return false;
        }
        self.label = code_window_label(location).into();
        self.location = location.to_owned();
        true
    }
}

/// A project's extra Code windows and which one the Code view is showing.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CodeWindows {
    pub(crate) windows: Vec<CodeWindow>,
    /// `None` is the project's own Code window.
    pub(crate) active: Option<CodeWindowId>,
}

impl CodeWindows {
    pub(crate) fn window(&self, id: CodeWindowId) -> Option<&CodeWindow> {
        self.windows.iter().find(|window| window.id == id)
    }

    pub(crate) fn window_mut(&mut self, id: CodeWindowId) -> Option<&mut CodeWindow> {
        self.windows.iter_mut().find(|window| window.id == id)
    }

    /// Removes the window; when it was the one on screen, the window before it takes over, else the
    /// project's own window.
    pub(crate) fn remove(&mut self, id: CodeWindowId) -> bool {
        let Some(index) = self.windows.iter().position(|window| window.id == id) else {
            return false;
        };
        self.windows.remove(index);
        if self.active == Some(id) {
            self.active = index
                .checked_sub(1)
                .and_then(|previous| self.windows.get(previous))
                .map(|window| window.id);
        }
        true
    }
}

/// A switch between two Code windows still in progress: the outgoing page stays on screen until
/// the incoming one has drawn (`begin_code_window_handoff` in app/workarea/code_windows.rs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CodeWindowHandoff {
    pub(crate) outgoing: crate::app::model::ProjectWorkareaCefSurfaceSlotKey,
    pub(crate) incoming: crate::app::model::ProjectWorkareaCefSurfaceSlotKey,
    /// Which switch this is, so the timer of an earlier switch to the same window (A to B, then
    /// C to B) cannot raise or finish a later one.
    pub(crate) generation: u64,
}

/// The address of the VS Code window `url` opens, when it is one: the workbench (`/` with its
/// `?folder=`, `?workspace=` or `?ew=` query) on `origin`. Another path on that origin is a page
/// code-server serves (`/proxy/<port>/`, a static file), not a window.
pub(crate) fn code_window_location(url: &url::Url, origin: &url::Origin) -> Option<String> {
    if &url.origin() != origin || url.path() != "/" {
        return None;
    }
    Some(match url.query() {
        Some(query) => format!("/?{query}"),
        None => "/".to_string(),
    })
}

/// What a window at `location` has open (`folder` or `workspace`, and its path), `None` for an
/// empty window: two windows with the same one are the same VS Code window.
pub(crate) fn code_window_target(location: &str) -> Option<(String, String)> {
    let url = url::Url::parse(&format!("http://code-window{location}")).ok()?;
    url.query_pairs().find_map(|(key, value)| {
        matches!(key.as_ref(), "folder" | "workspace").then(|| {
            let path = value.trim_end_matches(['/', '\\']);
            (key.into_owned(), path.to_owned())
        })
    })
}

fn code_window_label(location: &str) -> String {
    let Ok(url) = url::Url::parse(&format!("http://code-window{location}")) else {
        return "New Window".to_string();
    };
    let last_segment = |path: &str| {
        path.trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    };
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "folder" => {
                if let Some(name) = last_segment(&value) {
                    return name;
                }
            }
            "workspace" => {
                if let Some(name) = last_segment(&value) {
                    let name = name.strip_suffix(".code-workspace").unwrap_or(&name);
                    return format!("{name} (Workspace)");
                }
            }
            _ => {}
        }
    }
    "New Window".to_string()
}
