//! One row of a sidebar menu, and the JSON the renderer draws it from.
//!
//! SEE-ALSO: packages/shared/native-sidebar.ts (deleted 2026-10-01) (`NativeSidebarMenuItem`) and
//! apps/desktop/src/app/native_sidebar/menus.rs, which is the one reader of this shape.

use serde_json::{Map, Value};

use super::commands::MenuCommand;

/// Which half of a split header button a row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSplit {
    /// The action itself.
    Start,
    /// The chevron that opens the action's menu.
    End,
}

impl MenuSplit {
    fn as_str(self) -> &'static str {
        match self {
            MenuSplit::Start => "start",
            MenuSplit::End => "end",
        }
    }
}

/// The small secondary button a row can carry on its right (the agent launcher's account count).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuSecondary {
    pub icon: String,
    pub label: String,
    pub command: MenuCommand,
}

/// One menu row: a label with an action, a submenu, a heading, or a separator.
///
/// CDXC:ContextMenus 2026-09-20 WHY:
/// `checked` and `disabled` are written on EVERY row, true or false. An open panel is refreshed in
/// place by `SidebarMenuState::refresh` (apps/desktop/src/app/native_sidebar/menu_state.rs), which
/// copies a key only when the newly built item HAS it, so a key left out when the value is false
/// can add a tick and grey a row but can never take either back. Leaving them out made unticking a
/// tag filter keep its tick, both sort modes read as ticked, and a project menu that was open
/// while its last session went idle refuse the Close Inactive click it was still showing greyed.
/// The other flags are written only when true because nothing refreshes them: a row's label,
/// tick and enabled state are the only three that change under an open panel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuItem {
    pub label: Option<String>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    /// A colour swatch drawn instead of an icon (the collection colour page).
    pub color: Option<String>,
    /// The second line of an agent launcher account row.
    pub detail: Option<String>,
    pub suffix: Option<String>,
    pub image_data_url: Option<String>,
    pub agent_icon: Option<String>,
    /// Set by the host when it opens a panel whose contents arrive later.
    pub menu_owner: Option<String>,
    pub split: Option<MenuSplit>,
    /// Dispatched when the panel this row opens is shown.
    pub on_open: Option<MenuCommand>,
    pub command: Option<MenuCommand>,
    pub secondary: Option<MenuSecondary>,
    pub children: Option<Vec<MenuItem>>,
    pub supports_chat: bool,
    pub keep_open: bool,
    pub heading: bool,
    pub checked: bool,
    pub disabled: bool,
    pub danger: bool,
    pub separator: bool,
    /// `presentation: 'page'`: the submenu replaces the panel rather than opening beside it.
    pub page: bool,
    /// `presentation: 'beside'`: the submenu opens beside the menu, level with this row, where
    /// the host has room for it there (`with_beside`).
    pub beside: bool,
    /// `menuStyle: 'agentLauncher'`, read from the first row of a panel.
    pub agent_launcher: bool,
    /// The last-used agent, drawn highlighted.
    pub primary: bool,
    /// The hotkey action id (`ghostex-hotkeys.ts`) that runs the same action; the renderer shows
    /// its current binding in the button's tooltip.
    pub hotkey: Option<String>,
    /// Text behind an (i) icon at the row's end, shown as a tooltip while it is hovered (a tap
    /// shows it on the phone).
    pub info: Option<String>,
}

impl MenuItem {
    /// A row with a label, an icon and a command.
    pub(crate) fn row(label: &str, icon: &str, command: MenuCommand) -> Self {
        Self {
            label: Some(label.to_string()),
            icon: Some(icon.to_string()),
            command: Some(command),
            ..Self::default()
        }
    }

    /// A row with a label and a submenu.
    pub(crate) fn submenu(label: &str, icon: &str, children: Vec<MenuItem>) -> Self {
        Self {
            label: Some(label.to_string()),
            icon: Some(icon.to_string()),
            children: Some(children),
            ..Self::default()
        }
    }

    pub(crate) fn separator() -> Self {
        Self {
            separator: true,
            ..Self::default()
        }
    }

    pub(crate) fn heading(label: &str) -> Self {
        Self {
            label: Some(label.to_string()),
            heading: true,
            ..Self::default()
        }
    }

    pub(crate) fn with_danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub(crate) fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn with_checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub(crate) fn with_hotkey(mut self, action_id: &str) -> Self {
        self.hotkey = Some(action_id.to_string());
        self
    }

    pub(crate) fn with_page(mut self) -> Self {
        self.page = true;
        self
    }

    /// The submenu opens beside the menu rather than over it, so the rows under this one stay
    /// in reach while it is open.
    pub(crate) fn with_beside(mut self) -> Self {
        self.beside = true;
        self
    }

    /// Applies a tag's glyph and colour, the way the TypeScript spreads `nativeTagPresentation`
    /// over the row it just built.
    pub(crate) fn with_tag_presentation(
        mut self,
        presentation: Option<&crate::sidebar_view::tags::TagPresentation>,
    ) -> Self {
        if let Some(presentation) = presentation {
            self.icon = Some(presentation.icon.clone());
            self.icon_color = Some(presentation.icon_color.clone());
        }
        self
    }

    /// The JSON the native renderer reads.
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        let mut text = |key: &str, value: &Option<String>| {
            if let Some(value) = value {
                object.insert(key.to_string(), Value::String(value.clone()));
            }
        };
        text("label", &self.label);
        text("icon", &self.icon);
        text("iconColor", &self.icon_color);
        text("color", &self.color);
        text("detail", &self.detail);
        text("suffix", &self.suffix);
        text("imageDataUrl", &self.image_data_url);
        text("agentIcon", &self.agent_icon);
        text("menuOwner", &self.menu_owner);
        text("hotkey", &self.hotkey);
        text("info", &self.info);
        let mut flag = |key: &str, value: bool| {
            if value {
                object.insert(key.to_string(), Value::Bool(true));
            }
        };
        flag("supportsChat", self.supports_chat);
        flag("keepOpen", self.keep_open);
        flag("heading", self.heading);
        flag("danger", self.danger);
        flag("separator", self.separator);
        flag("primary", self.primary);
        // Always present: see the type's comment. A refresh of an open panel reads these two and
        // the label, and can only copy a key it finds.
        object.insert("checked".to_string(), Value::Bool(self.checked));
        object.insert("disabled".to_string(), Value::Bool(self.disabled));
        if self.page {
            object.insert(
                "presentation".to_string(),
                Value::String("page".to_string()),
            );
        } else if self.beside {
            object.insert(
                "presentation".to_string(),
                Value::String("beside".to_string()),
            );
        }
        if self.agent_launcher {
            object.insert(
                "menuStyle".to_string(),
                Value::String("agentLauncher".to_string()),
            );
        }
        if let Some(split) = self.split {
            object.insert(
                "split".to_string(),
                Value::String(split.as_str().to_string()),
            );
        }
        if let Some(command) = &self.on_open {
            object.insert("onOpen".to_string(), command.to_json());
        }
        if let Some(command) = &self.command {
            object.insert("command".to_string(), command.to_json());
        }
        if let Some(secondary) = &self.secondary {
            object.insert(
                "secondary".to_string(),
                serde_json::json!({
                    "icon": secondary.icon,
                    "label": secondary.label,
                    "command": secondary.command.to_json(),
                }),
            );
        }
        if let Some(children) = &self.children {
            object.insert(
                "children".to_string(),
                Value::Array(children.iter().map(MenuItem::to_json).collect()),
            );
        }
        Value::Object(object)
    }
}

/// A whole menu as the renderer's JSON array.
pub fn menu_to_json(items: &[MenuItem]) -> Value {
    Value::Array(items.iter().map(MenuItem::to_json).collect())
}
