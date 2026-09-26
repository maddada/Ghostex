//! LaTeX → edit-tree parsing — the inverse of [`Row::to_latex`]. Rather than hand-write a
//! LaTeX tokenizer, we reuse RaTeX's own parser (`ratex_parser::parse` → `ParseNode` AST)
//! and walk that tree into our `Row`/`Atom` model. Best-effort: constructs the model doesn't
//! represent (fonts, colors, spacing, …) degrade to their inner content or are dropped,
//! so an existing `$$…$$` block round-trips for editing and never errors on input.

use crate::editor::model::{Atom, Row};
use ratex_parser::{ParseNode, parse};

/// Parse `latex` into a [`Row`]. Unparseable (or empty) input yields an empty row.
pub fn parse_latex(latex: &str) -> Row {
    match parse(latex) {
        Ok(nodes) => nodes_to_row(&nodes),
        Err(_) => Row::new(),
    }
}

/// Walk a sequence of nodes into a row.
fn nodes_to_row(nodes: &[ParseNode]) -> Row {
    let mut atoms = Vec::new();
    for node in nodes {
        push_node(&mut atoms, node);
    }
    Row { atoms }
}

/// One node as a standalone row (a `{…}` group unwraps to its body; a leaf → a 1-atom row).
fn node_to_row(node: &ParseNode) -> Row {
    let mut atoms = Vec::new();
    push_node(&mut atoms, node);
    Row { atoms }
}

/// Append a node's atom(s) to `atoms`.
fn push_node(atoms: &mut Vec<Atom>, node: &ParseNode) {
    match node {
        // Symbols. `\square` is our empty-slot placeholder — drop it so the slot stays empty.
        ParseNode::MathOrd { text, .. }
        | ParseNode::TextOrd { text, .. }
        | ParseNode::Atom { text, .. }
        | ParseNode::OpToken { text, .. }
            if !is_placeholder(text) =>
        {
            atoms.push(Atom::Sym(text.clone()));
        }
        // An operator: a symbol op (\int, \sum) keeps its command name; a body op recurses.
        ParseNode::Op { name, body, .. } => {
            if let Some(name) = name {
                atoms.push(Atom::Sym(name.clone()));
            } else if let Some(body) = body {
                for n in body {
                    push_node(atoms, n);
                }
            }
        }
        // A `{…}` group flattens into the row (our model has no group atom).
        ParseNode::OrdGroup { body, .. } => {
            for n in body {
                push_node(atoms, n);
            }
        }
        // A super/subscript: emit the base atom(s), then a postfix SupSub bound to it.
        ParseNode::SupSub { base, sup, sub, .. } => {
            if let Some(base) = base {
                push_node(atoms, base);
            }
            atoms.push(Atom::SupSub {
                sup: sup.as_deref().map(node_to_row),
                sub: sub.as_deref().map(node_to_row),
            });
        }
        // A fraction: the bar-line kind is `\frac`; a barless one (\binom) keeps its parts.
        ParseNode::GenFrac {
            numer,
            denom,
            has_bar_line,
            ..
        } => {
            if *has_bar_line {
                atoms.push(Atom::Frac {
                    num: node_to_row(numer),
                    den: node_to_row(denom),
                });
            } else {
                push_node(atoms, numer);
                push_node(atoms, denom);
            }
        }
        ParseNode::Sqrt { body, index, .. } => atoms.push(Atom::Sqrt {
            radicand: node_to_row(body),
            index: index.as_deref().map(node_to_row),
        }),
        ParseNode::LeftRight {
            body, left, right, ..
        } => {
            // `pmatrix` parses as `\left( <array> \right)` — collapse that back to a Matrix
            // (our serializer emits matrices via `\begin{pmatrix}…`).
            if left.as_str() == "("
                && right.as_str() == ")"
                && let [ParseNode::Array { body: rows, .. }] = body.as_slice()
            {
                atoms.push(Atom::Matrix {
                    rows: rows
                        .iter()
                        .map(|row| row.iter().map(node_to_row).collect())
                        .collect(),
                });
            } else {
                atoms.push(Atom::Delim {
                    open: left.clone(),
                    body: nodes_to_row(body),
                    close: right.clone(),
                });
            }
        }
        ParseNode::Array { body, .. } => atoms.push(Atom::Matrix {
            rows: body
                .iter()
                .map(|row| row.iter().map(node_to_row).collect())
                .collect(),
        }),
        // Unrepresented wrappers: keep the inner content, drop the decoration. (Array cells
        // arrive wrapped in `styling`, so this is also what fills matrix cells.)
        ParseNode::OperatorName { body, .. }
        | ParseNode::MClass { body, .. }
        | ParseNode::Phantom { body, .. }
        | ParseNode::Styling { body, .. }
        | ParseNode::Sizing { body, .. }
        | ParseNode::Color { body, .. }
        | ParseNode::Text { body, .. } => {
            for n in body {
                push_node(atoms, n);
            }
        }
        // An accent keeps its base editable (#77) — previously it degraded to the bare
        // base, so opening `\hat{X}` for editing silently rewrote it as `X`.
        ParseNode::Accent { label, base, .. } => atoms.push(Atom::Accent {
            accent: label.clone(),
            base: node_to_row(base),
        }),
        ParseNode::AccentUnder { base, .. } => push_node(atoms, base),
        ParseNode::Overline { body, .. }
        | ParseNode::Underline { body, .. }
        | ParseNode::VPhantom { body, .. }
        | ParseNode::Smash { body, .. }
        | ParseNode::Font { body, .. } => push_node(atoms, body),
        // Spacing, kerns, rules, and anything else → drop.
        _ => {}
    }
}

/// Whether `text` is our empty-slot placeholder (`\square` / □), which should parse back to
/// an empty slot rather than a literal symbol.
fn is_placeholder(text: &str) -> bool {
    matches!(text, "\\square" | "\u{25A1}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(row: Row) {
        let latex = row.to_latex();
        let parsed = parse_latex(&latex);
        assert_eq!(
            parsed, row,
            "round-trip changed the tree (LaTeX: {latex:?})"
        );
    }

    #[test]
    fn empty_latex_is_empty_row() {
        assert_eq!(parse_latex(""), Row::new());
        // `\square` (our empty-slot marker) parses back to empty.
        assert_eq!(parse_latex(r"\square"), Row::new());
    }

    #[test]
    fn roundtrips_symbols() {
        roundtrip(Row::syms("abc"));
    }

    #[test]
    fn roundtrips_fraction() {
        roundtrip(Row {
            atoms: vec![Atom::Frac {
                num: Row::syms("a"),
                den: Row::syms("b"),
            }],
        });
    }

    #[test]
    fn roundtrips_sqrt_with_index() {
        roundtrip(Row {
            atoms: vec![Atom::Sqrt {
                radicand: Row::syms("x"),
                index: Some(Row::syms("3")),
            }],
        });
    }

    #[test]
    fn roundtrips_accent() {
        // The #77 data-loss guard: `\hat{X}` must survive parse → serialize unchanged.
        roundtrip(Row {
            atoms: vec![Atom::Accent {
                accent: r"\hat".into(),
                base: Row::syms("X"),
            }],
        });
    }

    /// Every accent the editor can insert must round-trip through RaTeX and lay out —
    /// guards against shipping a `Command::Accent` the engine can't parse.
    #[test]
    fn accent_variants_roundtrip_and_lay_out() {
        for accent in [
            r"\hat",
            r"\widehat",
            r"\bar",
            r"\vec",
            r"\tilde",
            r"\widetilde",
            r"\dot",
            r"\ddot",
            r"\check",
            r"\breve",
        ] {
            let row = Row {
                atoms: vec![Atom::Accent {
                    accent: accent.into(),
                    base: Row::syms("x"),
                }],
            };
            let latex = row.to_latex();
            assert_eq!(parse_latex(&latex), row, "{latex} round-trips");
            let nodes = parse(&latex).unwrap_or_else(|_| panic!("RaTeX parses {latex}"));
            let lbox = ratex_layout::layout(&nodes, &ratex_layout::LayoutOptions::default());
            assert!(lbox.width > 0.0, "{latex} lays out to a non-empty box");
        }
    }

    #[test]
    fn roundtrips_matrix() {
        roundtrip(Row {
            atoms: vec![Atom::Matrix {
                rows: vec![
                    vec![Row::syms("a"), Row::syms("b")],
                    vec![Row::syms("c"), Row::syms("d")],
                ],
            }],
        });
    }

    #[test]
    fn parses_a_realistic_formula() {
        // Not an exact round-trip target — just confirms a typical block parses to a
        // non-empty structured tree (frac + sqrt) without erroring.
        let row = parse_latex(r"\frac{1}{2} + \sqrt{x}");
        assert!(row.atoms.iter().any(|a| matches!(a, Atom::Frac { .. })));
        assert!(row.atoms.iter().any(|a| matches!(a, Atom::Sqrt { .. })));
    }
}
