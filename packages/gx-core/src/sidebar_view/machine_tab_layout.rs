//! How the machine tabs share the sidebar's width, as one pure rule.
//!
//! CDXC:RemoteMachines 2026-10-31 DECISION:
//! User, for randomsand on Discord (hundreds of projects, 4 remotes, "my remotes on top very cramped"): every machine has to stay reachable and tell-apart-able at any sidebar width, with this computer always easy to find, and nothing changes for one or two machines. Tabs share the row exactly as before while each gets a readable width. Past that, this computer's tab stays pinned at the left, the other tabs keep a readable width and scroll sideways, and the row always shows part of the next tab so it reads as scrollable with no extra control. That beats moving the overflow into a menu, which would hide machines' names and status behind a press, and it keeps one tab-shaped control for every machine however many there are. Picking a tab scrolls it into view with room to see its neighbour, so pressing the peeking tab walks along the row.
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/machines.rs, apps/mobile/app/src/components/sessions/machineTabLayout.ts (the phone's twin of this rule).

/// The narrowest a tab may get: its icon, padding and about nine characters of label.
pub const MACHINE_TAB_MIN_WIDTH: f32 = 92.0;
/// This computer's tab while the others scroll: its icon and a short name.
pub const MACHINE_TAB_PINNED_WIDTH: f32 = 64.0;
/// How much of the next tab a scrolling row aims to show, in tab widths.
pub const MACHINE_TAB_PEEK: f32 = 0.4;
/// The least of it the row may show, so a row never ends flush on a tab and hides that it scrolls.
const MACHINE_TAB_MIN_PEEK: f32 = 0.25;

/// How the row is laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MachineTabLayout {
    /// Every tab shares the row equally, as it always has.
    Fit,
    /// The first tab (this computer) is pinned; the rest sit in `viewport` at `tab_width` each.
    Pinned {
        pinned_width: f32,
        viewport: f32,
        tab_width: f32,
        /// They need more room than `viewport`, so the row scrolls.
        scrolls: bool,
    },
}

/// The row's layout for `count` tabs in `row_width`. `scale` is the sidebar's interface size.
pub fn machine_tab_layout(count: usize, row_width: f32, scale: f32) -> MachineTabLayout {
    let min = MACHINE_TAB_MIN_WIDTH * scale;
    if count <= 2 || row_width / count as f32 >= min {
        return MachineTabLayout::Fit;
    }
    let pinned_width = MACHINE_TAB_PINNED_WIDTH * scale;
    let viewport = row_width - pinned_width;
    let others = (count - 1) as f32;
    if viewport / others >= min {
        return MachineTabLayout::Pinned {
            pinned_width,
            viewport,
            tab_width: viewport / others,
            scrolls: false,
        };
    }
    let whole = (viewport / min - MACHINE_TAB_MIN_PEEK).floor().max(1.0);
    MachineTabLayout::Pinned {
        pinned_width,
        viewport,
        // The minimum wins over the aimed-for peek.
        tab_width: (viewport / (whole + MACHINE_TAB_PEEK)).max(min.min(viewport)),
        scrolls: true,
    }
}

/// The scroll offset that shows tab `index` with room for its neighbours, moving as little as it
/// can from `offset`.
pub fn machine_tab_reveal_offset(
    index: usize,
    count: usize,
    tab_width: f32,
    viewport: f32,
    offset: f32,
) -> f32 {
    let room = (tab_width * MACHINE_TAB_PEEK).min((viewport - tab_width).max(0.0) / 2.0);
    let left = index as f32 * tab_width;
    let right = left + tab_width;
    let mut next = offset;
    if left - room < next {
        next = left - room;
    }
    if right + room > next + viewport {
        next = right + room - viewport;
    }
    next.clamp(0.0, (count as f32 * tab_width - viewport).max(0.0))
}

/// Which tabs are scrolled out of view, as the ones before and the ones after: a tab whose middle
/// is off the row can't show its status dots, so the row's edge shows them for it.
pub fn machine_tabs_out_of_view(
    count: usize,
    tab_width: f32,
    viewport: f32,
    offset: f32,
) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
    let middle = |index: usize| (index as f32 + 0.5) * tab_width;
    let first = (0..count)
        .find(|index| middle(*index) >= offset)
        .unwrap_or(count);
    let after = (first..count)
        .find(|index| middle(*index) > offset + viewport)
        .unwrap_or(count);
    (0..first, after..count)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NORMAL: f32 = 275.0;
    const NARROW: f32 = 190.0;

    fn scroll_parts(layout: MachineTabLayout) -> (f32, f32, bool) {
        match layout {
            MachineTabLayout::Pinned {
                viewport,
                tab_width,
                scrolls,
                ..
            } => (viewport, tab_width, scrolls),
            MachineTabLayout::Fit => panic!("expected the pinned layout"),
        }
    }

    #[test]
    fn one_and_two_machines_never_change_at_any_width() {
        for width in [120.0, NARROW, NORMAL, 600.0] {
            assert_eq!(machine_tab_layout(1, width, 1.0), MachineTabLayout::Fit);
            assert_eq!(machine_tab_layout(2, width, 1.0), MachineTabLayout::Fit);
        }
    }

    #[test]
    fn tabs_share_the_row_while_each_is_readable() {
        assert_eq!(machine_tab_layout(3, 300.0, 1.0), MachineTabLayout::Fit);
        assert_eq!(machine_tab_layout(4, 400.0, 1.0), MachineTabLayout::Fit);
    }

    #[test]
    fn four_and_eight_machines_pin_this_computer_and_keep_readable_tabs() {
        for count in [4, 8] {
            for width in [NARROW, NORMAL] {
                let (viewport, tab_width, scrolls) =
                    scroll_parts(machine_tab_layout(count, width, 1.0));
                assert!(scrolls, "{count} in {width}");
                assert!(tab_width >= MACHINE_TAB_MIN_WIDTH, "{count} in {width}");
                assert_eq!(viewport, width - MACHINE_TAB_PINNED_WIDTH);
            }
        }
    }

    #[test]
    fn a_scrolling_row_always_shows_part_of_the_next_tab() {
        for width in (190..=500).step_by(7) {
            let (viewport, tab_width, scrolls) =
                scroll_parts(machine_tab_layout(12, width as f32, 1.0));
            assert!(scrolls);
            assert!(tab_width >= MACHINE_TAB_MIN_WIDTH, "{width}");
            let fraction = (viewport / tab_width).fract();
            assert!(fraction >= 0.25 - 0.001, "{width}: {fraction}");
            // Never a lone tab where two would fit at the minimum.
            assert!(
                (viewport / tab_width).floor() >= (viewport / MACHINE_TAB_MIN_WIDTH - 0.25).floor(),
                "{width}"
            );
        }
    }

    #[test]
    fn four_machines_at_the_normal_width_show_this_computer_and_two_more_and_a_peek() {
        let (viewport, tab_width, _) = scroll_parts(machine_tab_layout(4, NORMAL, 1.0));
        assert_eq!((viewport / tab_width).floor(), 2.0);
    }

    #[test]
    fn three_machines_that_no_longer_share_pin_without_scrolling_when_the_rest_fit() {
        let (viewport, tab_width, scrolls) = scroll_parts(machine_tab_layout(3, NORMAL, 1.0));
        assert!(!scrolls);
        assert_eq!(tab_width, viewport / 2.0);
    }

    #[test]
    fn the_sidebars_interface_size_scales_the_rule() {
        assert_eq!(machine_tab_layout(3, 300.0, 1.0), MachineTabLayout::Fit);
        assert_ne!(machine_tab_layout(3, 300.0, 1.5), MachineTabLayout::Fit);
    }

    #[test]
    fn picking_a_tab_reveals_it_and_its_neighbour_and_moves_no_more_than_it_must() {
        let (tab, viewport) = (90.0, 203.0);
        // Already in view with room: nothing moves.
        assert_eq!(machine_tab_reveal_offset(0, 7, tab, viewport, 0.0), 0.0);
        // The second one needs a little room for the tab after it.
        assert!((machine_tab_reveal_offset(1, 7, tab, viewport, 0.0) - 13.0).abs() < 0.001);
        // The peeking tab: it comes in with part of the next one showing.
        let next = machine_tab_reveal_offset(2, 7, tab, viewport, 0.0);
        assert!((next - (270.0 + 36.0 - 203.0)).abs() < 0.001);
        // Walking left does the same.
        let back = machine_tab_reveal_offset(3, 7, tab, viewport, 400.0);
        assert!((back - (270.0 - 36.0)).abs() < 0.001);
        // Never past the ends.
        assert_eq!(machine_tab_reveal_offset(0, 7, tab, viewport, 90.0), 0.0);
        assert_eq!(
            machine_tab_reveal_offset(6, 7, tab, viewport, 0.0),
            630.0 - viewport
        );
    }

    #[test]
    fn pressing_the_peeking_tab_again_and_again_reaches_every_machine() {
        let (viewport, tab, _) = scroll_parts(machine_tab_layout(8, NARROW, 1.0));
        let others = 7;
        let mut offset = 0.0;
        for index in 0..others {
            offset = machine_tab_reveal_offset(index, others, tab, viewport, offset);
            let left = index as f32 * tab;
            assert!(left >= offset - 0.001 && left + tab <= offset + viewport + 0.001);
            let (_, after) = machine_tabs_out_of_view(others, tab, viewport, offset);
            // The next one's middle may be off the row, but its edge is showing.
            if index + 1 < others {
                assert!(((index + 1) as f32 * tab) < offset + viewport, "{index}");
                assert!(after.start > index);
            }
        }
    }

    #[test]
    fn tabs_out_of_view_are_the_ones_whose_middle_is_off_the_row() {
        let (tab, viewport) = (100.0, 240.0);
        assert_eq!(
            machine_tabs_out_of_view(6, tab, viewport, 0.0),
            (0..0, 2..6)
        );
        assert_eq!(
            machine_tabs_out_of_view(6, tab, viewport, 120.0),
            (0..1, 4..6)
        );
        assert_eq!(
            machine_tabs_out_of_view(6, tab, viewport, 360.0),
            (0..4, 6..6)
        );
    }
}
