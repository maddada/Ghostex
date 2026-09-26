//! Cursor + structural edits over the model. GUI-free.
//!
//! The cursor is a path of [`Step`]s descending from the top row into nested slots,
//! plus an `index` between atoms in the target row. Editing inserts/removes atoms and
//! navigates left/right — descending into structures (fraction / root / delimiter /
//! script slots), walking a structure's slots in order, and ascending out at
//! boundaries, MathQuill-style.
//!
//! A super/subscript ([`Atom::SupSub`]) attaches to the preceding row atom — its base —
//! so the base stays an editable symbol; navigating a script visits its present
//! sub/sup slots.

use crate::editor::model::{Atom, Row};

/// Which child row of a structural atom a [`Step`] descends into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Num,
    Den,
    Radicand,
    Index,
    Body,
    /// An accent's base row.
    Base,
    Sup,
    Sub,
    /// A matrix cell at (row, column).
    Cell(usize, usize),
}

/// One descent: into `slot` of the structural atom at `atom` in the parent row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub atom: usize,
    pub slot: Slot,
}

/// A position in the model: a descent path + an index between atoms in the target row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cursor {
    pub path: Vec<Step>,
    pub index: usize,
}

/// A structural atom's child rows, in left-to-right navigation order. Leaves have none.
pub(crate) fn nav_slots(atom: &Atom) -> Vec<Slot> {
    match atom {
        Atom::Frac { .. } => vec![Slot::Num, Slot::Den],
        Atom::Sqrt { index, .. } => {
            if index.is_some() {
                vec![Slot::Index, Slot::Radicand]
            } else {
                vec![Slot::Radicand]
            }
        }
        Atom::Delim { .. } => vec![Slot::Body],
        Atom::Accent { .. } => vec![Slot::Base],
        Atom::Matrix { rows } => (0..rows.len())
            .flat_map(|r| (0..rows[r].len()).map(move |c| Slot::Cell(r, c)))
            .collect(),
        Atom::SupSub { sup, sub } => {
            // Present scripts only; subscript first (matching the serialized order).
            let mut s = Vec::new();
            if sub.is_some() {
                s.push(Slot::Sub);
            }
            if sup.is_some() {
                s.push(Slot::Sup);
            }
            s
        }
        Atom::Sym(_) => vec![],
    }
}

fn slot_row(atom: &Atom, slot: Slot) -> &Row {
    match (atom, slot) {
        (Atom::Frac { num, .. }, Slot::Num) => num,
        (Atom::Frac { den, .. }, Slot::Den) => den,
        (Atom::Sqrt { radicand, .. }, Slot::Radicand) => radicand,
        (Atom::Sqrt { index: Some(i), .. }, Slot::Index) => i,
        (Atom::Delim { body, .. }, Slot::Body) => body,
        (Atom::Accent { base, .. }, Slot::Base) => base,
        (Atom::SupSub { sub: Some(r), .. }, Slot::Sub) => r,
        (Atom::SupSub { sup: Some(r), .. }, Slot::Sup) => r,
        (Atom::Matrix { rows }, Slot::Cell(r, c)) => &rows[r][c],
        _ => unreachable!("cursor invariant: slot {slot:?} does not exist on this atom"),
    }
}

fn slot_row_mut(atom: &mut Atom, slot: Slot) -> &mut Row {
    match (atom, slot) {
        (Atom::Frac { num, .. }, Slot::Num) => num,
        (Atom::Frac { den, .. }, Slot::Den) => den,
        (Atom::Sqrt { radicand, .. }, Slot::Radicand) => radicand,
        (Atom::Sqrt { index: Some(i), .. }, Slot::Index) => i,
        (Atom::Delim { body, .. }, Slot::Body) => body,
        (Atom::Accent { base, .. }, Slot::Base) => base,
        (Atom::SupSub { sub: Some(r), .. }, Slot::Sub) => r,
        (Atom::SupSub { sup: Some(r), .. }, Slot::Sup) => r,
        (Atom::Matrix { rows }, Slot::Cell(r, c)) => &mut rows[r][c],
        _ => unreachable!("cursor invariant: slot {slot:?} does not exist on this atom"),
    }
}

/// Resolve a path to the target row (immutable).
fn resolve<'a>(row: &'a Row, path: &[Step]) -> &'a Row {
    match path.split_first() {
        None => row,
        Some((step, rest)) => resolve(slot_row(&row.atoms[step.atom], step.slot), rest),
    }
}

/// Resolve a path to the target row (mutable).
fn resolve_mut<'a>(row: &'a mut Row, path: &[Step]) -> &'a mut Row {
    match path.split_first() {
        None => row,
        Some((step, rest)) => resolve_mut(slot_row_mut(&mut row.atoms[step.atom], step.slot), rest),
    }
}

/// Collapse two positions into a selection in their deepest COMMON row: each end
/// climbs out of any structure the other doesn't share, taking that structure in
/// whole (MathQuill's drag-selection semantics). Returns `(row_path, lo, hi)`;
/// `lo == hi` when both ends resolve to the same spot.
pub fn common_row_selection(a: &Cursor, b: &Cursor) -> (Vec<Step>, usize, usize) {
    let common = a
        .path
        .iter()
        .zip(b.path.iter())
        .take_while(|(x, y)| x == y)
        .count();
    // Each end's position at the common depth: the atom of the structure it's still
    // inside, or its own index when it lives in the common row.
    let pos = |c: &Cursor| c.path.get(common).map(|s| s.atom).unwrap_or(c.index);
    let (pa, pb) = (pos(a), pos(b));
    let path = a.path[..common].to_vec();
    if a.path.len() > common && b.path.len() > common && pa == pb {
        // Both inside the same structure but on divergent branches (e.g. a
        // fraction's num vs den): the selection is that structure, whole.
        return (path, pa, pa + 1);
    }
    // The left end includes its structure from its left edge (the atom itself);
    // the right end includes through its right edge (atom + 1).
    let edge = |c: &Cursor, p: usize, rightward: bool| {
        if c.path.len() > common {
            if rightward { p + 1 } else { p }
        } else {
            c.index
        }
    };
    if pa <= pb {
        (path, edge(a, pa, false), edge(b, pb, true))
    } else {
        (path, edge(b, pb, false), edge(a, pa, true))
    }
}

impl Cursor {
    /// Cursor at the start of the top-level row.
    pub fn start() -> Self {
        Self::default()
    }

    /// The row the cursor is currently in.
    pub fn row<'a>(&self, top: &'a Row) -> &'a Row {
        resolve(top, &self.path)
    }

    /// Insert `atom` at the cursor. A structure descends into its first slot (type `/`
    /// → cursor in the numerator); a leaf steps past.
    pub fn insert(&mut self, top: &mut Row, atom: Atom) {
        let descend = nav_slots(&atom).first().copied();
        resolve_mut(top, &self.path).atoms.insert(self.index, atom);
        match descend {
            Some(slot) => {
                self.path.push(Step {
                    atom: self.index,
                    slot,
                });
                self.index = 0;
            }
            None => self.index += 1,
        }
    }

    /// Delete the atom before the cursor. An accent peels — the wrapper goes, its base
    /// stays in place (`\hat{x}` → `x`, MathQuill's convention) — since the base is
    /// user content a plain remove would silently destroy. At a slot start, ascend out
    /// to before the structure (deleting an empty structure outright is a later
    /// refinement).
    pub fn backspace(&mut self, top: &mut Row) {
        if self.index > 0 {
            let row = resolve_mut(top, &self.path);
            let at = self.index - 1;
            if let Atom::Accent { base, .. } = &mut row.atoms[at] {
                let inner = std::mem::take(&mut base.atoms);
                self.index = at + inner.len();
                row.atoms.splice(at..=at, inner);
            } else {
                row.atoms.remove(at);
                self.index = at;
            }
        } else if let Some(step) = self.path.pop() {
            self.index = step.atom;
        }
    }

    /// Move right: past a leaf, into a structure's first slot, on to the next slot, or
    /// out after the structure.
    pub fn move_right(&mut self, top: &Row) {
        let len = self.row(top).atoms.len();
        if self.index < len {
            match nav_slots(&self.row(top).atoms[self.index]).first().copied() {
                Some(slot) => {
                    self.path.push(Step {
                        atom: self.index,
                        slot,
                    });
                    self.index = 0;
                }
                None => self.index += 1,
            }
        } else if let Some(step) = self.path.last().copied() {
            let slots =
                nav_slots(&resolve(top, &self.path[..self.path.len() - 1]).atoms[step.atom]);
            let pos = slots.iter().position(|s| *s == step.slot).unwrap();
            if pos + 1 < slots.len() {
                self.path.last_mut().unwrap().slot = slots[pos + 1];
                self.index = 0;
            } else {
                self.path.pop();
                self.index = step.atom + 1;
            }
        }
    }

    /// Move left: mirror of [`Cursor::move_right`].
    pub fn move_left(&mut self, top: &Row) {
        if self.index > 0 {
            match nav_slots(&self.row(top).atoms[self.index - 1])
                .last()
                .copied()
            {
                Some(slot) => {
                    self.path.push(Step {
                        atom: self.index - 1,
                        slot,
                    });
                    self.index = self.row(top).atoms.len();
                }
                None => self.index -= 1,
            }
        } else if let Some(step) = self.path.last().copied() {
            let slots =
                nav_slots(&resolve(top, &self.path[..self.path.len() - 1]).atoms[step.atom]);
            let pos = slots.iter().position(|s| *s == step.slot).unwrap();
            if pos > 0 {
                self.path.last_mut().unwrap().slot = slots[pos - 1];
                self.index = self.row(top).atoms.len();
            } else {
                self.path.pop();
                self.index = step.atom;
            }
        }
    }

    /// Move up to the vertically-stacked sibling slot (denominator → numerator,
    /// subscript → superscript). No-op outside such a slot.
    pub fn move_up(&mut self, top: &Row) {
        self.move_vert(top, true);
    }

    /// Move down to the vertically-stacked sibling slot (numerator → denominator,
    /// superscript → subscript). No-op outside such a slot.
    pub fn move_down(&mut self, top: &Row) {
        self.move_vert(top, false);
    }

    fn move_vert(&mut self, top: &Row, up: bool) {
        let Some(step) = self.path.last().copied() else {
            return;
        };
        // Matrix cells move between rows of the same column.
        if let Slot::Cell(r, c) = step.slot {
            let Some(tr) = (if up { r.checked_sub(1) } else { Some(r + 1) }) else {
                return;
            };
            let parent = resolve(top, &self.path[..self.path.len() - 1]);
            if let Atom::Matrix { rows } = &parent.atoms[step.atom]
                && tr < rows.len()
                && c < rows[tr].len()
            {
                self.path.last_mut().unwrap().slot = Slot::Cell(tr, c);
                let len = self.row(top).atoms.len();
                self.index = self.index.min(len);
            }
            return;
        }
        let target = match (step.slot, up) {
            (Slot::Den, true) => Slot::Num,
            (Slot::Num, false) => Slot::Den,
            (Slot::Sub, true) => Slot::Sup,
            (Slot::Sup, false) => Slot::Sub,
            _ => return,
        };
        let parent = resolve(top, &self.path[..self.path.len() - 1]);
        if !nav_slots(&parent.atoms[step.atom]).contains(&target) {
            return; // the sibling slot isn't present (e.g. a sup-only script)
        }
        self.path.last_mut().unwrap().slot = target;
        let len = self.row(top).atoms.len();
        self.index = self.index.min(len);
    }

    /// Add an empty row below the cursor's matrix row (no-op outside a matrix); the caret
    /// moves to the new row's first cell.
    pub fn matrix_add_row(&mut self, top: &mut Row) {
        let Some(&Step {
            atom,
            slot: Slot::Cell(r, _),
        }) = self.path.last()
        else {
            return;
        };
        let parent = resolve_mut(top, &self.path[..self.path.len() - 1]);
        if let Atom::Matrix { rows } = &mut parent.atoms[atom] {
            let ncols = rows.first().map_or(0, |row| row.len());
            rows.insert(r + 1, vec![Row::new(); ncols]);
            self.path.last_mut().unwrap().slot = Slot::Cell(r + 1, 0);
            self.index = 0;
        }
    }

    /// Add an empty column after the cursor's matrix column; the caret moves into it.
    pub fn matrix_add_col(&mut self, top: &mut Row) {
        let Some(&Step {
            atom,
            slot: Slot::Cell(r, c),
        }) = self.path.last()
        else {
            return;
        };
        let parent = resolve_mut(top, &self.path[..self.path.len() - 1]);
        if let Atom::Matrix { rows } = &mut parent.atoms[atom] {
            for row in rows.iter_mut() {
                row.insert(c + 1, Row::new());
            }
            self.path.last_mut().unwrap().slot = Slot::Cell(r, c + 1);
            self.index = 0;
        }
    }

    /// Remove the cursor's matrix row (kept if it's the only one); the caret clamps to a
    /// surviving row.
    pub fn matrix_remove_row(&mut self, top: &mut Row) {
        let Some(&Step {
            atom,
            slot: Slot::Cell(r, c),
        }) = self.path.last()
        else {
            return;
        };
        let parent = resolve_mut(top, &self.path[..self.path.len() - 1]);
        if let Atom::Matrix { rows } = &mut parent.atoms[atom]
            && rows.len() > 1
        {
            rows.remove(r);
            self.path.last_mut().unwrap().slot = Slot::Cell(r.min(rows.len() - 1), c);
            self.index = 0;
        }
    }

    /// Remove the cursor's matrix column (kept if it's the only one); the caret clamps to a
    /// surviving column.
    pub fn matrix_remove_col(&mut self, top: &mut Row) {
        let Some(&Step {
            atom,
            slot: Slot::Cell(r, c),
        }) = self.path.last()
        else {
            return;
        };
        let parent = resolve_mut(top, &self.path[..self.path.len() - 1]);
        if let Atom::Matrix { rows } = &mut parent.atoms[atom]
            && rows.first().is_some_and(|row| row.len() > 1)
        {
            for row in rows.iter_mut() {
                row.remove(c);
            }
            let ncols = rows.first().map_or(1, |row| row.len());
            self.path.last_mut().unwrap().slot = Slot::Cell(r, c.min(ncols - 1));
            self.index = 0;
        }
    }

    /// Delete atoms `lo..hi` of the cursor's current row (a selection), leaving the caret at
    /// `lo`. `hi` is clamped to the row length; an empty range is a no-op.
    pub fn delete_range(&mut self, top: &mut Row, lo: usize, hi: usize) {
        let row = resolve_mut(top, &self.path);
        let hi = hi.min(row.atoms.len());
        if lo < hi {
            row.atoms.drain(lo..hi);
            self.index = lo;
        }
    }

    /// Replace atoms `lo..hi` of the cursor's row with one atom that `make` builds from the
    /// drained atoms (as a `Row`). Returns the new atom's index, or `None` (model untouched)
    /// when the range is empty or out of bounds. The caller positions the caret.
    fn wrap_range(
        &mut self,
        top: &mut Row,
        lo: usize,
        hi: usize,
        make: impl FnOnce(Row) -> Atom,
    ) -> Option<usize> {
        let row = resolve_mut(top, &self.path);
        if lo >= hi || hi > row.atoms.len() {
            return None;
        }
        let body: Vec<Atom> = row.atoms.drain(lo..hi).collect();
        row.atoms.insert(lo, make(Row { atoms: body }));
        Some(lo)
    }

    /// Wrap a selection (`lo..hi`) in auto-growing delimiters `\left<open> … \right<close>` —
    /// e.g. `(`/`)`, `[`/`]`, `\{`/`\}`, `|`/`|`, `\langle`/`\rangle`. Caret lands just after
    /// the new delimiter.
    pub fn wrap_delim(&mut self, top: &mut Row, lo: usize, hi: usize, open: &str, close: &str) {
        if let Some(at) = self.wrap_range(top, lo, hi, |body| Atom::Delim {
            open: open.to_string(),
            body,
            close: close.to_string(),
        }) {
            self.index = at + 1;
        }
    }

    /// Wrap a selection (`lo..hi`) under an accent (`\hat`, `\bar`, …) — the selection
    /// becomes the base. Caret lands just after the accent.
    pub fn wrap_accent(&mut self, top: &mut Row, lo: usize, hi: usize, accent: &str) {
        if let Some(at) = self.wrap_range(top, lo, hi, |base| Atom::Accent {
            accent: accent.to_string(),
            base,
        }) {
            self.index = at + 1;
        }
    }

    /// Wrap a selection (`lo..hi`) under a square root. Caret lands just after the root.
    pub fn wrap_sqrt(&mut self, top: &mut Row, lo: usize, hi: usize) {
        if let Some(at) = self.wrap_range(top, lo, hi, |radicand| Atom::Sqrt {
            radicand,
            index: None,
        }) {
            self.index = at + 1;
        }
    }

    /// Wrap a selection (`lo..hi`) under an nth-root — the selection becomes the radicand and
    /// an empty degree (index) box is added. Caret descends into the index, ready to type the
    /// degree (e.g. `3` for a cube root).
    pub fn wrap_nth_root(&mut self, top: &mut Row, lo: usize, hi: usize) {
        if let Some(at) = self.wrap_range(top, lo, hi, |radicand| Atom::Sqrt {
            radicand,
            index: Some(Row::new()),
        }) {
            self.path.push(Step {
                atom: at,
                slot: Slot::Index,
            });
            self.index = 0;
        }
    }

    /// Make a selection (`lo..hi`) the numerator of a new fraction. Caret descends into the
    /// (empty) denominator, ready to type it.
    pub fn wrap_fraction(&mut self, top: &mut Row, lo: usize, hi: usize) {
        if let Some(at) = self.wrap_range(top, lo, hi, |num| Atom::Frac {
            num,
            den: Row::new(),
        }) {
            self.path.push(Step {
                atom: at,
                slot: Slot::Den,
            });
            self.index = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::{Atom, Row};

    fn sym(c: &str) -> Atom {
        Atom::Sym(c.into())
    }
    fn empty_frac() -> Atom {
        Atom::Frac {
            num: Row::new(),
            den: Row::new(),
        }
    }

    fn empty_matrix(rows: usize, cols: usize) -> Atom {
        Atom::Matrix {
            rows: vec![vec![Row::new(); cols]; rows],
        }
    }

    fn matrix_dims(top: &Row) -> (usize, usize) {
        match &top.atoms[0] {
            Atom::Matrix { rows } => (rows.len(), rows.first().map_or(0, |r| r.len())),
            _ => (0, 0),
        }
    }

    #[test]
    fn matrix_grows_and_shrinks() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(&mut top, empty_matrix(2, 2)); // 2x2, caret in (0,0)
        cur.matrix_add_row(&mut top);
        assert_eq!(matrix_dims(&top), (3, 2));
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Cell(1, 0)
            }]
        );
        cur.matrix_add_col(&mut top);
        assert_eq!(matrix_dims(&top), (3, 3));
        cur.matrix_remove_row(&mut top);
        assert_eq!(matrix_dims(&top), (2, 3));
        cur.matrix_remove_col(&mut top);
        assert_eq!(matrix_dims(&top), (2, 2));
    }

    #[test]
    fn flat_insert_and_backspace() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(&mut top, sym("a"));
        cur.insert(&mut top, sym("b"));
        assert_eq!(top.to_latex(), "a b");
        assert_eq!(cur.index, 2);
        cur.backspace(&mut top);
        assert_eq!(top.to_latex(), "a");
        assert_eq!(cur.index, 1);
    }

    #[test]
    fn vertical_nav_between_fraction_slots() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(&mut top, empty_frac()); // into the numerator
        cur.insert(&mut top, sym("a"));
        cur.move_down(&top); // -> denominator
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Den
            }]
        );
        cur.insert(&mut top, sym("b"));
        assert_eq!(top.to_latex(), r"\frac{a}{b}");
        cur.move_up(&top); // -> numerator
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Num
            }]
        );
    }

    #[test]
    fn flat_left_right() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for c in ["a", "b", "c"] {
            cur.insert(&mut top, sym(c));
        }
        assert_eq!(cur.index, 3);
        cur.move_left(&top);
        cur.move_left(&top);
        cur.move_left(&top);
        assert_eq!(cur.index, 0);
        cur.move_left(&top); // at start — no-op
        assert_eq!(cur.index, 0);
        cur.move_right(&top);
        assert_eq!(cur.index, 1);
    }

    #[test]
    fn insert_fraction_descends_into_numerator() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(&mut top, empty_frac());
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Num
            }]
        );
        assert_eq!(cur.index, 0);
        cur.insert(&mut top, sym("a"));
        assert_eq!(top.to_latex(), r"\frac{a}{\square}");
    }

    #[test]
    fn right_walks_num_den_then_out() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(&mut top, empty_frac()); // in numerator
        cur.insert(&mut top, sym("a")); // num = a
        cur.move_right(&top); // num end -> denominator
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Den
            }]
        );
        assert_eq!(cur.index, 0);
        cur.insert(&mut top, sym("b")); // den = b
        assert_eq!(top.to_latex(), r"\frac{a}{b}");
        cur.move_right(&top); // den end -> out, after the fraction
        assert_eq!(cur.path, vec![]);
        assert_eq!(cur.index, 1);
    }

    #[test]
    fn backspace_at_slot_start_ascends() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(&mut top, empty_frac()); // empty numerator, index 0
        cur.backspace(&mut top); // ascend out
        assert_eq!(cur.path, vec![]);
        assert_eq!(cur.index, 0);
    }

    #[test]
    fn right_enters_existing_fraction() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(&mut top, empty_frac()); // descends into numerator
        cur.backspace(&mut top); // ascend to top, before the (still-present) fraction
        assert_eq!(cur.index, 0);
        cur.move_right(&top); // enter the fraction's numerator
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Num
            }]
        );
        assert_eq!(cur.index, 0);
    }

    #[test]
    fn script_descend_and_traverse() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(&mut top, sym("x")); // the base
        cur.insert(
            &mut top,
            Atom::SupSub {
                sup: Some(Row::new()),
                sub: Some(Row::new()),
            },
        );
        // insert descends into the first nav slot — the subscript
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 1,
                slot: Slot::Sub
            }]
        );
        cur.insert(&mut top, sym("0")); // sub = 0
        cur.move_right(&top); // sub end -> superscript
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 1,
                slot: Slot::Sup
            }]
        );
        cur.insert(&mut top, sym("2")); // sup = 2
        assert_eq!(top.to_latex(), "x _{0}^{2}");
        cur.move_right(&top); // sup end -> out after the script
        assert_eq!(cur.path, vec![]);
        assert_eq!(cur.index, 2);
    }

    #[test]
    fn accent_descend_type_and_exit() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(
            &mut top,
            Atom::Accent {
                accent: r"\hat".into(),
                base: Row::new(),
            },
        );
        // insert descends into the base slot
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Base
            }]
        );
        cur.insert(&mut top, sym("X"));
        assert_eq!(top.to_latex(), r"\hat{X}");
        cur.move_right(&top); // base end -> out after the accent
        assert_eq!(cur.path, vec![]);
        assert_eq!(cur.index, 1);
    }

    #[test]
    fn common_row_selection_reconciles_divergent_ends() {
        let step = |atom, slot| Step { atom, slot };
        // Ends inside two different structures of the top row: both climb out,
        // each structure taken whole (atoms 1 and 3 → 1..4).
        let a = Cursor {
            path: vec![step(1, Slot::Base)],
            index: 0,
        };
        let b = Cursor {
            path: vec![step(3, Slot::Base)],
            index: 1,
        };
        assert_eq!(common_row_selection(&a, &b), (vec![], 1, 4));
        // Order-independent.
        assert_eq!(common_row_selection(&b, &a), (vec![], 1, 4));
        // One end deep, one in the row: the structure is included whole.
        let c = Cursor {
            path: vec![],
            index: 0,
        };
        assert_eq!(common_row_selection(&c, &a), (vec![], 0, 2));
        // Divergent branches of the SAME structure select just that structure.
        let n = Cursor {
            path: vec![step(2, Slot::Num)],
            index: 0,
        };
        let d = Cursor {
            path: vec![step(2, Slot::Den)],
            index: 1,
        };
        assert_eq!(common_row_selection(&n, &d), (vec![], 2, 3));
        // Both in one nested row: a plain range there.
        let e = Cursor {
            path: vec![step(2, Slot::Num)],
            index: 2,
        };
        assert_eq!(
            common_row_selection(&n, &e),
            (vec![step(2, Slot::Num)], 0, 2)
        );
    }

    #[test]
    fn backspace_after_accent_peels_the_wrapper() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(
            &mut top,
            Atom::Accent {
                accent: r"\hat".into(),
                base: Row::new(),
            },
        ); // descends into the base
        cur.insert(&mut top, sym("x"));
        cur.insert(&mut top, sym("y"));
        cur.move_right(&top); // base end -> out, after the accent
        assert_eq!(top.to_latex(), r"\hat{x y}");
        assert_eq!((cur.path.clone(), cur.index), (vec![], 1));
        cur.backspace(&mut top); // peel: the base stays, the hat goes
        assert_eq!(top.to_latex(), "x y");
        assert_eq!(cur.index, 2);
        cur.backspace(&mut top); // plain delete resumes
        assert_eq!(top.to_latex(), "x");
    }

    #[test]
    fn wrap_accent_wraps_a_range_caret_after() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for c in ["x", "y"] {
            cur.insert(&mut top, sym(c));
        }
        cur.wrap_accent(&mut top, 0, 2, r"\bar");
        assert_eq!(top.to_latex(), r"\bar{x y}");
        assert_eq!(cur.index, 1);
    }

    #[test]
    fn wrap_delim_wraps_a_range_caret_after() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for c in ["a", "b", "c"] {
            cur.insert(&mut top, sym(c));
        }
        cur.wrap_delim(&mut top, 0, 2, "(", ")"); // wrap a,b in parens
        assert_eq!(top.to_latex(), r"\left( a b \right) c");
        assert_eq!(cur.path, vec![]);
        assert_eq!(cur.index, 1); // after the delimiter, before c
    }

    #[test]
    fn wrap_delim_supports_brackets_and_braces() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(&mut top, sym("x"));
        cur.wrap_delim(&mut top, 0, 1, "[", "]");
        assert_eq!(top.to_latex(), r"\left[ x \right]");

        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(&mut top, sym("x"));
        cur.wrap_delim(&mut top, 0, 1, r"\{", r"\}");
        assert_eq!(top.to_latex(), r"\left\{ x \right\}");
    }

    #[test]
    fn wrap_sqrt_wraps_a_range() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for c in ["x", "y"] {
            cur.insert(&mut top, sym(c));
        }
        cur.wrap_sqrt(&mut top, 0, 2);
        assert_eq!(top.to_latex(), r"\sqrt{x y}");
        assert_eq!(cur.index, 1);
    }

    #[test]
    fn wrap_nth_root_wraps_radicand_caret_in_degree() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for c in ["x", "y"] {
            cur.insert(&mut top, sym(c));
        }
        cur.wrap_nth_root(&mut top, 0, 2);
        assert_eq!(top.to_latex(), r"\sqrt[\square]{x y}");
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Index
            }]
        );
        assert_eq!(cur.index, 0);
    }

    #[test]
    fn wrap_fraction_makes_selection_the_numerator_caret_in_denominator() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for c in ["x", "+", "1"] {
            cur.insert(&mut top, sym(c));
        }
        cur.wrap_fraction(&mut top, 0, 3); // whole row -> numerator
        assert_eq!(top.to_latex(), r"\frac{x + 1}{\square}");
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Den
            }]
        );
        assert_eq!(cur.index, 0);
    }

    #[test]
    fn wrap_empty_range_is_a_noop() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(&mut top, sym("a"));
        cur.wrap_delim(&mut top, 1, 1, "(", ")");
        assert_eq!(top.to_latex(), "a");
    }

    #[test]
    fn delete_range_removes_selected_atoms() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for c in ["a", "b", "c", "d"] {
            cur.insert(&mut top, sym(c));
        }
        cur.delete_range(&mut top, 1, 3); // remove b,c
        assert_eq!(top.to_latex(), "a d");
        assert_eq!(cur.index, 1);
    }

    #[test]
    fn matrix_cell_navigation() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        cur.insert(
            &mut top,
            Atom::Matrix {
                rows: vec![vec![Row::new(), Row::new()], vec![Row::new(), Row::new()]],
            },
        );
        // insert descends into cell (0,0)
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Cell(0, 0)
            }]
        );
        cur.move_right(&top); // -> (0,1)
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Cell(0, 1)
            }]
        );
        cur.move_down(&top); // -> (1,1)
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Cell(1, 1)
            }]
        );
        cur.move_up(&top); // -> (0,1)
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Cell(0, 1)
            }]
        );
    }
}
