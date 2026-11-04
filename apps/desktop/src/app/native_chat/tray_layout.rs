//! Where the new chat's breadcrumb puts its parts: one row while they fit, else two rows on a grid
//! (project and computer over where and detail, the columns lined up), centred while the bar is
//! narrow, with the widest column cut short first. Widths in, placement out, so it is tested
//! without a window; `worktree_tray.rs` measures the parts and draws what this decides. The phone
//! follows the same rule in `apps/mobile/app/src/chat/native/composer/trayLayout.ts`.
//!
//! CDXC:Worktrees 2026-11-04 DECISION:
//! User (Q1227 note): "when the screen is narrow center it if it didn't fill the area ... maybe
//! it's better on narrow screen to actually show the bread crumbs on 2 levels but they must be
//! aligned well please like a grid i hate when stuff is alll over the place". Two rows are a grid:
//! each column is as wide as its widest part, so the second row's parts start where the first
//! row's do, and nothing ever wraps on its own. A part that still does not fit is cut short with
//! its full words in its tooltip.

/// One part's width with all of its words, and the least it may be cut to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PartSize {
    pub natural: f32,
    pub min: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TrayLayout {
    /// The where row sits under the place row instead of after it.
    pub two_rows: bool,
    /// The parts sit in the middle of the bar instead of at its start.
    pub centered: bool,
    /// Each part's width, row by row as given: the place row, then the where row.
    pub widths: [Vec<f32>; 2],
    /// Two rows: each column's width, which every part of that column is drawn in.
    pub columns: Vec<f32>,
}

/// Below this width (before the chat's zoom) the bar counts as narrow: what fits is centred.
pub(super) const NARROW_WIDTH: f32 = 520.0;

/// Places the place row (project, computer) and the where row (where, detail) in `available`
/// pixels with `separator` between neighbours. `available` of zero or less is a bar not measured
/// yet: one row, every part whole.
pub(super) fn tray_layout(
    rows: &[Vec<PartSize>; 2],
    separator: f32,
    available: f32,
    narrow: f32,
) -> TrayLayout {
    let natural = |row: &[PartSize]| row.iter().map(|part| part.natural).collect::<Vec<_>>();
    let all: Vec<PartSize> = rows.iter().flatten().copied().collect();
    let one_row = sum(&natural(&all)) + separator * all.len().saturating_sub(1) as f32;
    let measured = available > 0.0;
    if !measured || one_row <= available || rows[1].is_empty() || rows[0].is_empty() {
        // One row: whole when it fits, else cut down (there is no second row to move to).
        let widths = if measured && one_row > available {
            let room = available - separator * all.len().saturating_sub(1) as f32;
            shrink(
                &natural(&all),
                &all.iter().map(|part| part.min).collect::<Vec<_>>(),
                room,
            )
        } else {
            natural(&all)
        };
        let first = rows[0].len();
        return TrayLayout {
            two_rows: false,
            centered: measured && available < narrow,
            widths: [widths[..first].to_vec(), widths[first..].to_vec()],
            columns: Vec::new(),
        };
    }
    let count = rows[0].len().max(rows[1].len());
    let column = |index: usize, value: fn(&PartSize) -> f32| {
        rows.iter()
            .filter_map(|row| row.get(index))
            .map(value)
            .fold(0.0, f32::max)
    };
    let wanted: Vec<f32> = (0..count)
        .map(|index| column(index, |part| part.natural))
        .collect();
    let mins: Vec<f32> = (0..count)
        .map(|index| column(index, |part| part.min))
        .collect();
    let room = available - separator * (count - 1) as f32;
    let columns = if sum(&wanted) > room {
        shrink(&wanted, &mins, room)
    } else {
        wanted
    };
    let widths = rows.clone().map(|row| {
        row.iter()
            .zip(&columns)
            .map(|(part, column)| part.natural.min(*column))
            .collect()
    });
    TrayLayout {
        two_rows: true,
        centered: true,
        widths,
        columns,
    }
}

fn sum(widths: &[f32]) -> f32 {
    widths.iter().sum()
}

/// Cuts the widest down first, to one common level, until they fit in `room`, never below each
/// one's `min` (they overflow at their mins when even those do not fit).
fn shrink(widths: &[f32], mins: &[f32], room: f32) -> Vec<f32> {
    let at = |level: f32| -> Vec<f32> {
        widths
            .iter()
            .zip(mins)
            .map(|(width, min)| width.min(level.max(*min)))
            .collect()
    };
    let (mut low, mut high) = (0.0_f32, widths.iter().copied().fold(0.0, f32::max));
    if sum(&at(low)) > room {
        return at(low);
    }
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if sum(&at(middle)) > room {
            high = middle;
        } else {
            low = middle;
        }
    }
    at(low)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(natural: f32) -> PartSize {
        PartSize { natural, min: 40.0 }
    }

    fn breadcrumb() -> [Vec<PartSize>; 2] {
        // Ghostex › Local › This folder › main
        [vec![part(80.0), part(60.0)], vec![part(100.0), part(50.0)]]
    }

    #[test]
    fn a_wide_bar_keeps_one_row_at_the_start() {
        let layout = tray_layout(&breadcrumb(), 16.0, 700.0, NARROW_WIDTH);
        assert!(!layout.two_rows);
        assert!(!layout.centered);
        assert_eq!(layout.widths, [vec![80.0, 60.0], vec![100.0, 50.0]]);
    }

    #[test]
    fn a_narrow_bar_that_still_fits_centres_its_one_row() {
        // 290 + 3 separators = 338.
        let layout = tray_layout(&breadcrumb(), 16.0, 360.0, NARROW_WIDTH);
        assert!(!layout.two_rows);
        assert!(layout.centered);
        assert_eq!(layout.widths, [vec![80.0, 60.0], vec![100.0, 50.0]]);
    }

    #[test]
    fn what_does_not_fit_goes_on_two_rows_with_lined_up_columns() {
        let layout = tray_layout(&breadcrumb(), 16.0, 300.0, NARROW_WIDTH);
        assert!(layout.two_rows);
        assert!(layout.centered);
        // Each column is as wide as its widest part: Ghostex over This folder, Local over main.
        assert_eq!(layout.columns, vec![100.0, 60.0]);
        assert_eq!(layout.widths, [vec![80.0, 60.0], vec![100.0, 50.0]]);
    }

    #[test]
    fn a_long_worktree_name_is_cut_first_and_the_short_parts_stay_whole() {
        let rows = [vec![part(80.0), part(60.0)], vec![part(90.0), part(260.0)]];
        let layout = tray_layout(&rows, 16.0, 250.0, NARROW_WIDTH);
        assert!(layout.two_rows);
        // 250 - one separator = 234 for the columns: the first keeps its 90, the second gets 144.
        assert_eq!(layout.columns[0], 90.0);
        assert!(
            (layout.columns[1] - 144.0).abs() < 0.01,
            "{:?}",
            layout.columns
        );
        assert_eq!(layout.widths[0], vec![80.0, 60.0]);
        assert_eq!(layout.widths[1][0], 90.0);
        assert!((layout.widths[1][1] - 144.0).abs() < 0.01);
    }

    #[test]
    fn no_part_is_cut_below_its_least_width() {
        let layout = tray_layout(&breadcrumb(), 16.0, 60.0, NARROW_WIDTH);
        assert!(layout.two_rows);
        assert!(layout.columns.iter().all(|column| *column >= 40.0));
    }

    #[test]
    fn without_a_computer_the_where_row_still_lines_up_under_the_project() {
        let rows = [vec![part(80.0)], vec![part(100.0), part(50.0)]];
        let layout = tray_layout(&rows, 16.0, 200.0, NARROW_WIDTH);
        assert!(layout.two_rows);
        assert_eq!(layout.columns, vec![100.0, 50.0]);
        assert_eq!(layout.widths, [vec![80.0], vec![100.0, 50.0]]);
    }

    #[test]
    fn a_folder_with_nothing_to_choose_cuts_its_one_row() {
        let rows = [vec![part(300.0), part(60.0)], vec![]];
        let layout = tray_layout(&rows, 16.0, 200.0, NARROW_WIDTH);
        assert!(!layout.two_rows);
        assert!(layout.widths[0][0] < 300.0);
        assert_eq!(layout.widths[0][1], 60.0);
        assert!((layout.widths[0].iter().sum::<f32>() - 184.0).abs() < 0.01);
    }

    #[test]
    fn a_bar_not_measured_yet_draws_one_whole_row() {
        let layout = tray_layout(&breadcrumb(), 16.0, 0.0, NARROW_WIDTH);
        assert!(!layout.two_rows);
        assert!(!layout.centered);
        assert_eq!(layout.widths, [vec![80.0, 60.0], vec![100.0, 50.0]]);
    }
}
