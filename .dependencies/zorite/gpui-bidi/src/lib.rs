//! Bidirectional-text index↔x mapping for gpui shaped lines.
//!
//! # Why this exists
//!
//! The platform shapers already do bidi correctly. Shape `"سلام دنیا"` and the
//! glyphs come back in **visual** order carrying **logical** byte indices —
//! for that string, indices `15, 13, 11, 9, 8, 6, 2, 0` as x ascends. That is
//! exactly what UAX #9 asks for, and it means RTL text *renders* right today.
//!
//! What is wrong is how those glyphs get read back. gpui's
//! `LineLayout::x_for_index` returns the first glyph whose `index >= target`,
//! which assumes byte index and x rise together. In an RTL line the first
//! glyph carries the *highest* index, so every offset in the line resolves to
//! the same glyph and the caret collapses onto x = 0. `index_for_x` and
//! `closest_index_for_x` fail the same way, and mixed content is worse: in
//! `"hello سلام world"` the four offsets 6, 8, 10, 12 all map to one x.
//!
//! So this crate is mostly **not** an implementation of the bidi algorithm —
//! the shaper did that, and this reads its already-reordered glyph table
//! correctly. The one thing the glyph table cannot tell you is a glyph's
//! embedding LEVEL: at the edge of an embedded run the glyph beside it belongs
//! to the other run and carries a misleading index, so direction inferred from
//! geometry is wrong exactly there. Those levels come from `unicode-bidi`,
//! given the text — see [`VisualMap::with_levels`].
//!
//! # Shape of the API
//!
//! [`VisualMap`] is built from plain `(logical byte index, x)` pairs, so every
//! rule here is unit-testable without a window, a GPU, or a font. Hosts get
//! those pairs from a shaped line — see [`VisualMap::from_glyphs`] and the
//! `gpui` adapter in [`shaped`].
//!
//! # What a caller still owns
//!
//! - **Paragraph direction** (which side a line starts on) is the host's, from
//!   the first strong character. This crate only maps within a shaped line.
//! - **Logical caret movement.** Left/right arrows moving in *logical* order
//!   needs nothing from here. For the *visual* movement platforms actually do
//!   in bidi text, see [`VisualMap::step_visual`].

use std::ops::Range;

/// One glyph as this crate needs it: where it came from in the source, and
/// where it was laid out.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Glyph {
    /// Byte offset into the shaped text — *logical* order.
    pub index: usize,
    /// Laid-out horizontal position of the glyph's leading edge, in the
    /// line's own coordinate space (0 = line start) — *visual* order.
    pub x: f32,
}

/// A shaped line's glyphs, indexed so logical offsets and visual positions can
/// be converted in both directions regardless of writing direction.
///
/// Glyphs are stored in visual (x-ascending) order — the order a shaper
/// returns them — and a parallel table gives their logical order, so neither
/// direction of the mapping has to scan.
#[derive(Clone, Debug)]
pub struct VisualMap {
    /// Visual order, x ascending. Always sorted by `x`.
    glyphs: Vec<Glyph>,
    /// Indices into `glyphs`, sorted by the glyph's logical `index`.
    by_logical: Vec<usize>,
    /// Total advance width of the line.
    width: f32,
    /// Byte length of the shaped text — the offset one past the last glyph.
    len: usize,
    /// UAX #9 embedding level per byte of the shaped text, when the caller
    /// supplied the text ([`Self::with_levels`]). Odd = right-to-left.
    levels: Option<Vec<u8>>,
}

impl VisualMap {
    /// Build from a line's glyphs plus its total `width` and text byte length.
    ///
    /// `glyphs` may arrive in any order; it is sorted by `x` here, so callers
    /// can hand over a shaper's runs concatenated without pre-sorting.
    pub fn from_glyphs(glyphs: impl IntoIterator<Item = Glyph>, width: f32, len: usize) -> Self {
        let mut glyphs: Vec<Glyph> = glyphs.into_iter().collect();
        glyphs.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut by_logical: Vec<usize> = (0..glyphs.len()).collect();
        by_logical.sort_by_key(|&i| glyphs[i].index);
        Self {
            glyphs,
            by_logical,
            width,
            len,
            levels: None,
        }
    }

    /// Attach the text these glyphs were shaped from, so glyph direction comes
    /// from the UAX #9 embedding levels rather than being inferred.
    ///
    /// Inference from `(index, x)` alone is exact in the middle of a run and
    /// WRONG at its edges: the glyph visually beside the last letter of a Latin
    /// word embedded in Persian belongs to the surrounding RTL run and carries
    /// a lower index, which reads as "descending, therefore RTL". The caret
    /// then sits on that letter's far edge — one glyph out, which is what a
    /// reader sees as the caret skipping a character.
    pub fn with_levels(mut self, text: &str) -> Self {
        let info = unicode_bidi::BidiInfo::new(text, None);
        self.levels = Some(info.levels.iter().map(|l| l.number()).collect());
        self
    }

    /// Whether any glyph sits out of logical order — i.e. the line contains
    /// right-to-left text. A pure-LTR line answers `false`, and callers can
    /// use that to skip straight to gpui's own (cheaper) lookups.
    pub fn is_bidi(&self) -> bool {
        self.glyphs.windows(2).any(|w| w[0].index > w[1].index)
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    /// The x where a caret sitting *before* byte `offset` belongs.
    ///
    /// "Before" is in reading order, so for a glyph in an RTL run that is its
    /// **right** edge, not its left — which is the whole reason gpui's version
    /// can't be reused. An offset past the end of the text resolves to the
    /// line's trailing edge: the right edge for LTR, x = 0 for a line that
    /// ends in RTL.
    pub fn x_for_index(&self, offset: usize) -> f32 {
        if self.glyphs.is_empty() {
            return 0.0;
        }
        if offset >= self.len {
            return self.trailing_edge();
        }
        // The glyph this offset falls inside: the last one whose logical index
        // is <= offset. Several glyphs can share an index (a ligature or a
        // combining sequence); the first of those owns the caret.
        match self.glyph_at_logical(offset) {
            Some(gi) => {
                let g = self.glyphs[gi];
                if self.is_rtl_at(gi) {
                    self.right_edge(gi)
                } else {
                    g.x
                }
            }
            // Before the first logical glyph — the line's leading edge.
            None => self.leading_edge(),
        }
    }

    /// The byte offset a click at `x` should place the caret at.
    ///
    /// Picks the nearer edge of whichever glyph `x` lands on, so clicking a
    /// glyph's trailing half puts the caret after it in *reading* order — past
    /// it on the left for RTL, on the right for LTR.
    pub fn index_for_x(&self, x: f32) -> usize {
        if self.glyphs.is_empty() {
            return 0;
        }
        // Which glyph's advance contains x (they're x-sorted, so the last one
        // starting at or before x).
        let gi = match self.glyphs.iter().rposition(|g| g.x <= x) {
            Some(i) => i,
            // Left of every glyph: the leading edge of the visually-first one.
            None => return self.edge_offset(0, false),
        };
        let past_middle = x >= (self.glyphs[gi].x + self.right_edge(gi)) / 2.0;
        self.edge_offset(gi, past_middle)
    }

    /// The visual rectangles covering the logical byte range `range`, as
    /// `(start x, end x)` pairs in the line's coordinate space.
    ///
    /// A logically contiguous selection can be **visually split** — selecting
    /// across a direction change in `"hello سلام world"` covers two or three
    /// separate stretches — so this returns however many runs it takes.
    /// Adjacent runs are merged, and the result is sorted left to right.
    pub fn rects_for_range(&self, range: Range<usize>) -> Vec<(f32, f32)> {
        if range.is_empty() || self.glyphs.is_empty() {
            return Vec::new();
        }
        // Collect the visual span of every glyph whose logical index is in
        // range, then coalesce the ones that touch.
        let mut spans: Vec<(f32, f32)> = self
            .glyphs
            .iter()
            .enumerate()
            .filter(|(_, g)| range.contains(&g.index))
            .map(|(i, g)| (g.x, self.right_edge(i)))
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out: Vec<(f32, f32)> = Vec::new();
        for (start, end) in spans {
            match out.last_mut() {
                // `>=` so glyphs that merely abut still merge into one rect.
                Some(last) if start <= last.1 => last.1 = last.1.max(end),
                _ => out.push((start, end)),
            }
        }
        out
    }

    // --- internals ---

    /// The entry in `glyphs` that owns logical `offset`: the last glyph whose
    /// index is <= offset, preferring the first of a group sharing an index.
    fn glyph_at_logical(&self, offset: usize) -> Option<usize> {
        // `by_logical` is sorted by logical index — partition to the first
        // entry whose index exceeds `offset`, then step back one.
        let p = self
            .by_logical
            .partition_point(|&i| self.glyphs[i].index <= offset);
        (p > 0).then(|| self.by_logical[p - 1])
    }

    /// Whether the glyph at visual position `gi` belongs to an RTL run, judged
    /// by its neighbours: in RTL the logical index *decreases* as x rises.
    ///
    /// Both sides are checked, not just one. A run's LAST glyph has no
    /// descending neighbour to its right — the next glyph belongs to the
    /// following LTR run and has a higher index — so looking right alone
    /// misclassifies it and puts the caret on the wrong edge. Its left
    /// neighbour still carries the descent.
    ///
    /// A lone RTL glyph between two LTR runs has neither, and reads as LTR.
    /// Visually that is a single glyph either way; the caret lands on its
    /// left edge rather than its right, which is a one-glyph discrepancy in a
    /// case the shaper itself renders ambiguously.
    fn is_rtl_at(&self, gi: usize) -> bool {
        let here = self.glyphs[gi].index;
        // The levels know, when we have them: no inference, and correct at the
        // edges of an embedded run where inference cannot be.
        if let Some(levels) = &self.levels {
            return levels.get(here).is_some_and(|l| l % 2 == 1);
        }
        let descends_right = self.glyphs.get(gi + 1).is_some_and(|n| n.index < here);
        let descends_left = gi > 0 && self.glyphs[gi - 1].index > here;
        descends_right || descends_left
    }

    /// The x just past the glyph at visual position `gi` — the next glyph's
    /// leading edge, or the line's width for the last one.
    fn right_edge(&self, gi: usize) -> f32 {
        self.glyphs.get(gi + 1).map_or(self.width, |next| next.x)
    }

    /// The logical offset one caret step to the visual left/right of `offset`.
    ///
    /// `None` when there is no glyph that way — the caret is leaving the line,
    /// and the caller moves it to the neighbouring row instead.
    ///
    /// Arrow keys move visually, and inside a line that is NOT a matter of
    /// "this line is RTL, so flip left and right". A Latin word or a URL
    /// embedded in Persian runs the other way, and the caret has to flow
    /// through it in ITS direction, not jump over it to the far end. Stepping
    /// by x rather than by byte gets that for free: the glyph table is already
    /// in visual order, so "the next caret position to the right" is just the
    /// next distinct x.
    pub fn step_visual(&self, offset: usize, right: bool) -> Option<usize> {
        let stops = self.caret_stops();
        // Where the caret is now in that order. An offset with no stop of its
        // own (mid-glyph, say) has no neighbour to step to.
        let pos = stops.iter().position(|&(_, o)| o == offset)?;
        if right {
            stops.get(pos + 1).map(|&(_, o)| o)
        } else {
            pos.checked_sub(1).map(|i| stops[i].1)
        }
    }

    /// Every caret position on the line as `(x, offset)`, ordered left to
    /// right.
    ///
    /// Ordering by x ALONE is not enough: at a direction change two different
    /// offsets sit at the same x — the trailing edge of one run and the leading
    /// edge of the next — and stepping by "the next bigger x" makes one of them
    /// unreachable, which shows up as the caret skipping a character or two at
    /// the edge of an embedded word. Ties break on the offset, so the order is
    /// total and every stop can be reached.
    fn caret_stops(&self) -> Vec<(f32, usize)> {
        let mut stops: Vec<(f32, usize)> = (0..self.glyphs.len())
            .flat_map(|gi| [self.edge_offset(gi, false), self.edge_offset(gi, true)])
            .map(|off| (self.x_for_index(off), off))
            .collect();
        stops.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        stops.dedup();
        stops
    }

    /// Does the byte range read right-to-left?
    ///
    /// From the UAX #9 levels when they're available (see
    /// [`Self::with_levels`]); otherwise from the glyph order, which is right
    /// in the middle of a run and unreliable at its edges.
    pub fn is_rtl_range(&self, range: Range<usize>) -> bool {
        if let Some(levels) = &self.levels {
            return levels
                .get(range.start..range.end.min(levels.len()))
                .is_some_and(|s| s.iter().any(|l| l % 2 == 1));
        }
        (0..self.glyphs.len())
            .filter(|&gi| range.contains(&self.glyphs[gi].index))
            .any(|gi| self.is_rtl_at(gi))
    }

    /// The byte offset at one edge of the glyph at visual position `gi`.
    /// `trailing` means the edge further along in *reading* order.
    fn edge_offset(&self, gi: usize, trailing: bool) -> usize {
        let g = self.glyphs[gi];
        let rtl = self.is_rtl_at(gi);
        // Reading-order "after this glyph" is the next logical offset; for RTL
        // that is still index+1 in bytes, because indices are logical.
        let after = self.next_logical_offset(g.index);
        match (rtl, trailing) {
            // LTR: leading edge = this offset, trailing = the next one.
            (false, false) => g.index,
            (false, true) => after,
            // RTL: x rises against reading order, so a click past the visual
            // middle is *earlier* in reading order.
            (true, false) => after,
            (true, true) => g.index,
        }
    }

    /// The logical offset immediately after `index` — the next glyph's index
    /// in logical order, or the text length past the last one.
    fn next_logical_offset(&self, index: usize) -> usize {
        let p = self
            .by_logical
            .partition_point(|&i| self.glyphs[i].index <= index);
        self.by_logical
            .get(p)
            .map_or(self.len, |&i| self.glyphs[i].index)
    }

    /// x where reading starts: 0 for a line beginning LTR, the width for one
    /// beginning RTL.
    fn leading_edge(&self) -> f32 {
        if self.starts_rtl() { self.width } else { 0.0 }
    }

    /// x where reading ends — the opposite of [`Self::leading_edge`].
    fn trailing_edge(&self) -> f32 {
        if self.ends_rtl() { 0.0 } else { self.width }
    }

    /// Whether the logically-first glyph sits at the right — an RTL line.
    fn starts_rtl(&self) -> bool {
        self.by_logical.first().is_some_and(|&i| self.is_rtl_at(i))
    }

    /// Whether the logically-last glyph belongs to an RTL run.
    fn ends_rtl(&self) -> bool {
        self.by_logical.last().is_some_and(|&i| self.is_rtl_at(i))
    }
}

pub mod paragraph;
pub mod shaped;

#[cfg(test)]
mod tests {
    use super::*;

    /// `"hello"` — every glyph 8px wide, indices ascending with x.
    fn ltr() -> VisualMap {
        VisualMap::from_glyphs(
            (0..5).map(|i| Glyph {
                index: i,
                x: i as f32 * 8.0,
            }),
            40.0,
            5,
        )
    }

    /// A pure-RTL line of 4 two-byte characters, the shape the probe measured:
    /// glyph indices descend as x rises (6, 4, 2, 0), text length 8.
    fn rtl() -> VisualMap {
        VisualMap::from_glyphs(
            (0..4).map(|k| Glyph {
                index: 6 - k * 2,
                x: k as f32 * 10.0,
            }),
            40.0,
            8,
        )
    }

    /// `"ab" + RTL(2 chars) + "cd"`: logical 0,1 | 2,4 | 6,7 laid out as
    /// a b [rtl2 rtl1] c d.
    fn mixed() -> VisualMap {
        VisualMap::from_glyphs(
            [
                Glyph { index: 0, x: 0.0 },
                Glyph { index: 1, x: 10.0 },
                Glyph { index: 4, x: 20.0 }, // second RTL char, drawn first
                Glyph { index: 2, x: 30.0 }, // first RTL char, drawn second
                Glyph { index: 6, x: 40.0 },
                Glyph { index: 7, x: 50.0 },
            ],
            60.0,
            8,
        )
    }

    #[test]
    fn ltr_behaves_exactly_like_a_plain_line() {
        let m = ltr();
        assert!(!m.is_bidi());
        for i in 0..5 {
            assert_eq!(m.x_for_index(i), i as f32 * 8.0);
        }
        // Past the end → the right edge.
        assert_eq!(m.x_for_index(5), 40.0);
        // Clicks: leading half picks this glyph, trailing half the next.
        assert_eq!(m.index_for_x(0.0), 0);
        assert_eq!(m.index_for_x(7.0), 1);
        assert_eq!(m.index_for_x(9.0), 1);
        assert_eq!(m.index_for_x(39.0), 5);
    }

    #[test]
    fn rtl_caret_walks_right_to_left_instead_of_collapsing() {
        let m = rtl();
        assert!(m.is_bidi());
        // This is the bug the crate exists for: gpui returns 0.0 for every one
        // of these. Reading starts at the RIGHT edge and moves left.
        assert_eq!(m.x_for_index(0), 40.0, "first char sits at the right edge");
        assert_eq!(m.x_for_index(2), 30.0);
        assert_eq!(m.x_for_index(4), 20.0);
        assert_eq!(m.x_for_index(6), 10.0);
        // Past the end of an RTL line is the LEFT edge.
        assert_eq!(m.x_for_index(8), 0.0);
        // Every position is distinct — the collapse is gone.
        let xs: Vec<f32> = (0..4).map(|k| m.x_for_index(k * 2)).collect();
        let mut sorted = xs.clone();
        sorted.sort_by(f32::total_cmp);
        sorted.dedup();
        assert_eq!(sorted.len(), xs.len(), "caret positions must be distinct");
    }

    #[test]
    fn a_neutral_run_takes_the_direction_of_the_text_around_it() {
        // "سلام [[x]] دنیا" — the bracket pair is neutral, so UAX #9 gives it
        // the paragraph's direction (RTL). Painting a styled row re-shapes each
        // run on its own, which loses that context and would render `[[` in
        // LTR order; the run's direction has to come from the levels.
        let text = "سلام [[x]] دنیا";
        let m = VisualMap::from_glyphs([Glyph { index: 0, x: 0.0 }], 10.0, text.len())
            .with_levels(text);
        let open = text.find("[[").unwrap();
        assert!(
            m.is_rtl_range(open..open + 2),
            "the brackets read RTL, like the prose around them"
        );
        let x = text.find('x').unwrap();
        assert!(!m.is_rtl_range(x..x + 1), "the Latin between them does not");
    }

    #[test]
    fn levels_fix_the_edges_of_an_embedded_run() {
        // "سلام Demo x" — a Latin word inside Persian. Shaped, the Latin runs
        // left-to-right while the Persian around it runs the other way. Build
        // the glyph table by hand in VISUAL order to mimic that.
        //
        // Bytes: "سلام" = 0..8 (4 chars, 2 bytes each), ' ' = 8, "Demo" = 9..13,
        // ' ' = 13, "x" = 14.
        let text = "سلام Demo x";
        let m = VisualMap::from_glyphs(
            [
                // Persian, drawn right to left: it sits at the RIGHT, so its
                // glyphs take the higher x's. Latin sits to its left.
                Glyph { index: 14, x: 0.0 },  // x
                Glyph { index: 13, x: 10.0 }, // space
                Glyph { index: 9, x: 20.0 },  // D
                Glyph { index: 10, x: 30.0 }, // e
                Glyph { index: 11, x: 40.0 }, // m
                Glyph { index: 12, x: 50.0 }, // o
                Glyph { index: 8, x: 60.0 },  // space
                Glyph { index: 6, x: 70.0 },
                Glyph { index: 4, x: 80.0 },
                Glyph { index: 2, x: 90.0 },
                Glyph { index: 0, x: 100.0 },
            ],
            110.0,
            text.len(),
        )
        .with_levels(text);

        // The caret before 'o' (byte 12) belongs on 'o's LEFT edge, because 'o'
        // is left-to-right. Inference put it on the right edge — the glyph to
        // 'o's right is the Persian space, whose lower index reads as
        // "descending, therefore RTL" — and the caret appeared to skip a
        // character at the end of the embedded word.
        assert_eq!(m.x_for_index(12), 50.0, "caret before 'o' sits left of it");
        assert_eq!(m.x_for_index(9), 20.0, "and before 'D' at the run's start");
        // Stepping right through the Latin run advances one letter at a time.
        assert_eq!(m.step_visual(9, true), Some(10));
        assert_eq!(m.step_visual(10, true), Some(11));
        assert_eq!(m.step_visual(11, true), Some(12));
        // Persian around it still reads right-to-left.
        assert_eq!(m.x_for_index(0), 110.0, "first Persian char at the right");
    }

    #[test]
    fn visual_steps_flow_through_an_embedded_run_instead_of_jumping_it() {
        // `mixed()`: LTR bytes 0,1 — an RTL pair at 2,4 drawn in reverse — then
        // LTR 6,7. Walking right from the line's left edge must visit every
        // caret stop in x order, INCLUDING both ends of the embedded run. The
        // bug this covers: keying the step off "the line is RTL" made the caret
        // skip to the far side of an embedded Latin word or URL.
        let m = mixed();
        let mut seen = vec![0usize];
        let mut at = 0usize;
        while let Some(next) = m.step_visual(at, true) {
            seen.push(next);
            at = next;
            assert!(seen.len() < 20, "stepping right must terminate");
        }
        // Never moves backwards, and never lands on the same stop twice —
        // co-located stops at a direction change must each be reachable, which
        // is what "the caret skipped the last two characters" was.
        let xs: Vec<f32> = seen.iter().map(|&o| m.x_for_index(o)).collect();
        assert!(
            xs.windows(2).all(|w| w[1] >= w[0]),
            "no step moves left: {xs:?} from {seen:?}"
        );
        let mut uniq = seen.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(uniq.len(), seen.len(), "no stop visited twice: {seen:?}");
        // Every caret stop on the line is reachable by stepping.
        assert_eq!(seen.len(), m.caret_stops().len(), "all stops visited");
        // And it reaches the line's right edge rather than stopping early at
        // the direction change.
        assert_eq!(*xs.last().unwrap(), 60.0);

        // Walking back left returns to where it started.
        let mut back = at;
        let mut steps = 0;
        while let Some(prev) = m.step_visual(back, false) {
            back = prev;
            steps += 1;
            assert!(steps < 20, "stepping left must terminate");
        }
        assert_eq!(m.x_for_index(back), 0.0, "back at the left edge");
    }

    #[test]
    fn rtl_clicks_map_back_to_the_right_characters() {
        let m = rtl();
        // Clicking the rightmost glyph's right half selects the FIRST
        // character in reading order.
        assert_eq!(m.index_for_x(39.0), 0);
        // …and its left half moves one character along in reading order.
        assert_eq!(m.index_for_x(31.0), 2);
        // Leftmost glyph, left half → the last character's trailing edge.
        assert_eq!(m.index_for_x(0.0), 8);
        // Round-trip: every caret x maps back to the offset it came from.
        for k in 0..4 {
            let off = k * 2;
            let x = m.x_for_index(off);
            // Nudge just inside the glyph so we're not exactly on a boundary.
            assert_eq!(m.index_for_x(x - 1.0), off, "round trip at offset {off}");
        }
    }

    #[test]
    fn mixed_runs_keep_their_own_direction() {
        let m = mixed();
        assert!(m.is_bidi());
        // LTR head.
        assert_eq!(m.x_for_index(0), 0.0);
        assert_eq!(m.x_for_index(1), 10.0);
        // RTL middle: offset 2 is the FIRST rtl char, drawn at the RIGHT of
        // the run, so its caret is at that glyph's right edge (40.0).
        assert_eq!(m.x_for_index(2), 40.0);
        assert_eq!(m.x_for_index(4), 30.0);
        // LTR tail resumes.
        assert_eq!(m.x_for_index(6), 40.0);
        assert_eq!(m.x_for_index(7), 50.0);
        // The four offsets gpui collapsed onto one x are distinct again.
        assert_ne!(m.x_for_index(1), m.x_for_index(2));
        assert_ne!(m.x_for_index(2), m.x_for_index(4));
    }

    #[test]
    fn a_selection_across_a_direction_change_is_visually_split() {
        let m = mixed();
        // Selecting the LTR head alone is one rect.
        assert_eq!(m.rects_for_range(0..2), vec![(0.0, 20.0)]);
        // Selecting the RTL middle alone is one rect too (its glyphs abut).
        assert_eq!(m.rects_for_range(2..6), vec![(20.0, 40.0)]);
        // Selecting head + middle merges, because they touch visually.
        assert_eq!(m.rects_for_range(0..6), vec![(0.0, 40.0)]);
        // The whole line is one rect.
        assert_eq!(m.rects_for_range(0..8), vec![(0.0, 60.0)]);
        // An empty or reversed range selects nothing.
        assert!(m.rects_for_range(3..3).is_empty());
    }

    #[test]
    fn selection_can_need_more_than_one_rect() {
        // Visual order 0, 4, 2, 6 — a shape where one *contiguous* logical
        // range lands in two separate places on screen. This is the case a
        // single-rect-per-row painter gets wrong.
        let split = VisualMap::from_glyphs(
            [
                Glyph { index: 0, x: 0.0 },
                Glyph { index: 4, x: 10.0 },
                Glyph { index: 2, x: 20.0 },
                Glyph { index: 6, x: 30.0 },
            ],
            40.0,
            8,
        );
        // Logical 0..3 covers glyphs 0 (x 0-10) and 2 (x 20-30); glyph 4 sits
        // between them visually but is outside the range.
        assert_eq!(split.rects_for_range(0..3), vec![(0.0, 10.0), (20.0, 30.0)]);
        // Widening to 0..5 pulls in glyph 4 and the three merge into one.
        assert_eq!(split.rects_for_range(0..5), vec![(0.0, 30.0)]);
        // A tail-only range is a single rect at the far right.
        let m = mixed();
        assert_eq!(m.rects_for_range(6..8), vec![(40.0, 60.0)]);
    }

    #[test]
    fn degenerate_lines_do_not_panic() {
        let empty = VisualMap::from_glyphs([], 0.0, 0);
        assert!(empty.is_empty());
        assert_eq!(empty.x_for_index(0), 0.0);
        assert_eq!(empty.x_for_index(99), 0.0);
        assert_eq!(empty.index_for_x(10.0), 0);
        assert!(empty.rects_for_range(0..5).is_empty());
        assert!(!empty.is_bidi());

        let one = VisualMap::from_glyphs([Glyph { index: 0, x: 0.0 }], 12.0, 1);
        assert_eq!(one.x_for_index(0), 0.0);
        assert_eq!(one.x_for_index(1), 12.0);
        assert_eq!(one.index_for_x(0.0), 0);
        assert_eq!(one.index_for_x(11.0), 1);
    }
}
