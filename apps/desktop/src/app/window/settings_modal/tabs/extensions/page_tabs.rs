//! The Extensions page's tabs (CDXC:Extensions 2026-11-09 in filter_bar.rs `render_page_tabs`):
//! which built-in categories, Store groups and sections each one shows.

/// One tab of the Extensions page, in the strip's order (Experimental last).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageTab {
    BuiltIn,
    Views,
    PanelsMenus,
    Installed,
    Community,
    Experimental,
}

impl PageTab {
    pub(crate) const ALL: [PageTab; 6] = [
        PageTab::BuiltIn,
        PageTab::Views,
        PageTab::PanelsMenus,
        PageTab::Installed,
        PageTab::Community,
        PageTab::Experimental,
    ];

    /// The id saved as `extensionsTab` in `settingsModalNavigation`.
    pub(crate) fn id(self) -> &'static str {
        match self {
            PageTab::BuiltIn => "builtIn",
            PageTab::Views => "views",
            PageTab::PanelsMenus => "panelsMenus",
            PageTab::Installed => "installed",
            PageTab::Community => "community",
            PageTab::Experimental => "experimental",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.id() == id)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            PageTab::BuiltIn => "Built-in",
            PageTab::Views => "Views",
            PageTab::PanelsMenus => "Panels & menus",
            PageTab::Installed => "Installed",
            PageTab::Community => "Community",
            PageTab::Experimental => "Experimental",
        }
    }

    /// The built-in categories (the catalog's `GHOSTEX_OFFICIAL_EXTENSION_CATEGORIES` ids) it shows.
    pub(crate) fn categories(self) -> &'static [&'static str] {
        match self {
            PageTab::BuiltIn => &["features"],
            PageTab::Views => &["project-websites", "code-and-files", "planning"],
            PageTab::PanelsMenus => &["header-buttons", "menus-and-panels"],
            PageTab::Experimental => &["experimental"],
            PageTab::Installed | PageTab::Community => &[],
        }
    }

    /// The tab a built-in category is listed under.
    pub(crate) fn of_category(category: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|tab| tab.categories().contains(&category))
            .unwrap_or(PageTab::BuiltIn)
    }
}
