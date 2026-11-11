use super::{
    appearance::SidebarAppearance,
    model::{NativeSidebarGroup, NativeSidebarSnapshot},
};
use crate::GhostexGpuiApp;
use gpui::{AnyElement, Bounds, IntoElement, ParentElement, Pixels, Styled, deferred, div, px};

impl GhostexGpuiApp {
    pub(crate) fn record_native_project_bounds(
        &mut self,
        id: &str,
        bounds: Bounds<Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.native_sidebar.group_bounds.get(id) != Some(&bounds) {
            self.native_sidebar
                .group_bounds
                .insert(id.to_owned(), bounds);
            cx.notify();
        }
    }

    /// The project header pinned at the top of the list, with its own row's bounds and the y its
    /// pinned copy sits at, measured from last frame's recorded rows.
    fn native_sticky_project_placement<'a>(
        &self,
        snapshot: &'a NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
    ) -> Option<(&'a NativeSidebarGroup, Bounds<Pixels>, Pixels)> {
        let viewport = self.native_sidebar.scroll.bounds();
        let height = px(30.0 * appearance.scale);
        let visible: std::collections::HashSet<&str> = snapshot
            .order
            .iter()
            .flat_map(|item| {
                if item.kind == "project" {
                    vec![item.id.as_str()]
                } else {
                    snapshot
                        .collections
                        .iter()
                        .find(|collection| {
                            collection.collection_id == item.id && !collection.collapsed
                        })
                        .map(|collection| collection.group_ids.iter().map(String::as_str).collect())
                        .unwrap_or_default()
                }
            })
            .collect();
        let (group, bounds) = snapshot
            .groups
            .iter()
            .filter(|group| !group.collapsed && visible.contains(group.group_id.as_str()))
            .filter_map(|group| {
                self.native_sidebar
                    .group_bounds
                    .get(&group.group_id)
                    .map(|bounds| (group, *bounds))
            })
            .filter(|(_, bounds)| bounds.top() < viewport.top() && bounds.bottom() > viewport.top())
            .max_by_key(|(_, bounds)| bounds.top())?;
        let y = viewport
            .top()
            .min(bounds.bottom() - height)
            .max(viewport.top() - height);
        Some((group, bounds, y))
    }

    /// The group whose header is pinned at the top of the list, if one is.
    pub(crate) fn native_sticky_project_group_id(
        &self,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
    ) -> Option<String> {
        self.native_sticky_project_placement(snapshot, appearance)
            .map(|(group, _, _)| group.group_id.clone())
    }

    /// Where the list must stop painting under window glass: the pinned header is see-through
    /// there, so rows scrolling beneath it would show through its text.
    pub(crate) fn native_sticky_project_clip_top(
        &self,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
    ) -> Option<Pixels> {
        if !appearance.glass {
            return None;
        }
        self.native_sticky_project_placement(snapshot, appearance)
            .map(|(_, _, y)| y + px(30.0 * appearance.scale))
    }

    pub(crate) fn render_native_sticky_project(
        &self,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        window: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let height = px(30.0 * appearance.scale);
        let (group, bounds, y) = self.native_sticky_project_placement(snapshot, appearance)?;
        let root = self.native_sidebar.bounds;
        // CDXC:Projects 2026-10-06 WHY:
        // The recorded bounds are the header box's, which already include the chevron slot at its left edge (the box reaches left by a negative margin). The pinned wrapper starts at those bounds, pads its left by that same reach so the header's negative margin lands on the wrapper's edge instead of being clipped, and keeps the header's 3px right margin.
        let margin = px(3.0 * appearance.scale);
        let reach = px(super::project_header::PROJECT_HEADER_CHEVRON_INSET * appearance.scale);
        let pinned = div()
            .absolute()
            .left(bounds.left() - root.left())
            .top(y - root.top())
            .w(bounds.size.width + margin)
            .pl(reach)
            .h(height)
            .overflow_hidden()
            // Under window glass the list stops at this header's bottom edge
            // (`native_sticky_project_clip_top`), so it needs no fill to hide rows.
            .bg(if appearance.glass {
                gpui::transparent_black()
            } else {
                crate::app::helpers::titlebar_background()
            })
            .child(self.render_native_project_header(group, &snapshot.hud, appearance, true, cx));
        let pinned = self.sidebar_focus_mirror(
            pinned,
            super::focus_order::project_header_stop_key(&group.group_id),
            appearance,
            window,
            cx,
        );
        Some(deferred(pinned).with_priority(5).into_any_element())
    }
}
