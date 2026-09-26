//! The structural math model — a MathQuill-style edit tree. GUI-free.
//!
//! A [`Row`] is a horizontal list of [`Atom`]s; structural atoms (fractions, scripts,
//! roots, delimiters) hold child `Row`s, which are the editable **slots**. The model
//! serializes to LaTeX, which RaTeX then typesets — so editing manipulates the 2-D
//! structure, never raw LaTeX text. An empty slot serializes to `\square` (a visible
//! placeholder box).

/// A horizontal list of atoms — the editable unit ("slot"). Empty = a placeholder box.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub atoms: Vec<Atom>,
}

/// One element of a [`Row`]: a leaf symbol, or a structure with child rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Atom {
    /// A single symbol, stored as its LaTeX: `"x"`, `"4"`, `"+"`, `"\\alpha"`, `"\\int"`.
    Sym(String),
    /// `\frac{num}{den}` — a bar with a box above and below.
    Frac { num: Row, den: Row },
    /// A super/subscript attached to the **preceding** atom in the row, which is its
    /// base (`x^2`, `\int_0^1`). MathQuill-style: the base stays an editable row atom,
    /// so it never owns a sub-atom and the serializer never has to brace it.
    SupSub { sup: Option<Row>, sub: Option<Row> },
    /// `\sqrt{radicand}` or `\sqrt[index]{radicand}`.
    Sqrt { radicand: Row, index: Option<Row> },
    /// An accent over an editable base: `\hat{base}`, `\bar{base}`, `\vec{base}`, ….
    /// `accent` is the command (`"\\hat"`).
    Accent { accent: String, base: Row },
    /// Auto-growing delimiters: `\left<open> body \right<close>`.
    Delim {
        open: String,
        body: Row,
        close: String,
    },
    /// A matrix: a rectangular grid of cells (`rows[r][c]`), rendered with parentheses.
    Matrix { rows: Vec<Vec<Row>> },
}

impl Row {
    pub fn new() -> Self {
        Self { atoms: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }

    /// Build a row from a string of single-character symbols (test/convenience helper).
    pub fn syms(s: &str) -> Self {
        Self {
            atoms: s.chars().map(|c| Atom::Sym(c.to_string())).collect(),
        }
    }

    /// Serialize to LaTeX. An empty row emits `\square` so RaTeX renders a visible,
    /// layout-occupying placeholder. Atoms are space-joined so a command symbol
    /// (`\alpha`) can't fuse with the next token — LaTeX math collapses the spaces.
    pub fn to_latex(&self) -> String {
        if self.atoms.is_empty() {
            return r"\square".to_string();
        }
        self.atoms
            .iter()
            .map(Atom::to_latex)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Atom {
    pub fn to_latex(&self) -> String {
        match self {
            // A bare brace is TeX group syntax, not a glyph — escape it so a stray
            // `{` Sym can never emit an unbalanced group (#77).
            Atom::Sym(s) if s == "{" => r"\{".to_string(),
            Atom::Sym(s) if s == "}" => r"\}".to_string(),
            Atom::Sym(s) => s.clone(),
            Atom::Frac { num, den } => {
                format!(r"\frac{{{}}}{{{}}}", num.to_latex(), den.to_latex())
            }
            Atom::SupSub { sup, sub } => {
                // No base here — the preceding row atom is the base (the row serializer
                // joins them). Sub before sup is KaTeX's canonical order.
                let mut out = String::new();
                if let Some(sub) = sub {
                    out.push_str(&format!("_{{{}}}", sub.to_latex()));
                }
                if let Some(sup) = sup {
                    out.push_str(&format!("^{{{}}}", sup.to_latex()));
                }
                out
            }
            Atom::Sqrt { radicand, index } => match index {
                Some(idx) => format!(r"\sqrt[{}]{{{}}}", idx.to_latex(), radicand.to_latex()),
                None => format!(r"\sqrt{{{}}}", radicand.to_latex()),
            },
            Atom::Accent { accent, base } => format!("{}{{{}}}", accent, base.to_latex()),
            Atom::Delim { open, body, close } => {
                format!(r"\left{} {} \right{}", open, body.to_latex(), close)
            }
            Atom::Matrix { rows } => {
                let body = rows
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(Row::to_latex)
                            .collect::<Vec<_>>()
                            .join(" & ")
                    })
                    .collect::<Vec<_>>()
                    .join(r" \\ ");
                format!(r"\begin{{pmatrix}} {body} \end{{pmatrix}}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_row_is_placeholder() {
        assert_eq!(Row::new().to_latex(), r"\square");
    }

    #[test]
    fn symbols_space_join() {
        assert_eq!(Row::syms("4x").to_latex(), "4 x");
    }

    #[test]
    fn fraction() {
        let f = Atom::Frac {
            num: Row::syms("a"),
            den: Row::syms("b"),
        };
        assert_eq!(f.to_latex(), r"\frac{a}{b}");
    }

    #[test]
    fn empty_fraction_slots_render_boxes() {
        let f = Atom::Frac {
            num: Row::new(),
            den: Row::new(),
        };
        assert_eq!(f.to_latex(), r"\frac{\square}{\square}");
    }

    #[test]
    fn supsub_sub_then_sup() {
        // A SupSub serializes alone — no base; sub is emitted before sup.
        let s = Atom::SupSub {
            sup: Some(Row::syms("2")),
            sub: Some(Row::syms("i")),
        };
        assert_eq!(s.to_latex(), r"_{i}^{2}");
    }

    #[test]
    fn supsub_attaches_to_preceding_operator() {
        // ∫ with limits: the operator is a plain preceding atom, so RaTeX keeps it an
        // operator and the script binds its limits — no bracing needed.
        let row = Row {
            atoms: vec![
                Atom::Sym(r"\int".into()),
                Atom::SupSub {
                    sup: Some(Row::syms("1")),
                    sub: Some(Row::syms("0")),
                },
            ],
        };
        assert_eq!(row.to_latex(), r"\int _{0}^{1}");
    }

    #[test]
    fn supsub_after_fraction() {
        // (a/b)^2 — the script attaches to the fraction (the preceding atom).
        let row = Row {
            atoms: vec![
                Atom::Frac {
                    num: Row::syms("a"),
                    den: Row::syms("b"),
                },
                Atom::SupSub {
                    sup: Some(Row::syms("2")),
                    sub: None,
                },
            ],
        };
        assert_eq!(row.to_latex(), r"\frac{a}{b} ^{2}");
    }

    #[test]
    fn sqrt_with_index() {
        let s = Atom::Sqrt {
            radicand: Row::syms("x"),
            index: Some(Row::syms("3")),
        };
        assert_eq!(s.to_latex(), r"\sqrt[3]{x}");
    }

    #[test]
    fn accent_wraps_its_base() {
        let a = Atom::Accent {
            accent: r"\hat".into(),
            base: Row::syms("X"),
        };
        assert_eq!(a.to_latex(), r"\hat{X}");
        let nodes = ratex_parser::parse(&a.to_latex()).expect("RaTeX parses accents");
        let lbox = ratex_layout::layout(&nodes, &ratex_layout::LayoutOptions::default());
        assert!(lbox.width > 0.0);
    }

    #[test]
    fn bare_brace_syms_escape() {
        // A stray brace Sym must never emit TeX group syntax (#77).
        assert_eq!(Atom::Sym("{".into()).to_latex(), r"\{");
        assert_eq!(Atom::Sym("}".into()).to_latex(), r"\}");
    }

    #[test]
    fn delimiters() {
        let d = Atom::Delim {
            open: "(".into(),
            body: Row::syms("x"),
            close: ")".into(),
        };
        assert_eq!(d.to_latex(), r"\left( x \right)");
    }

    /// The serializer's output must actually parse + lay out in RaTeX (the round-trip
    /// that keeps `model → LaTeX → RaTeX` honest).
    #[test]
    fn serialized_latex_parses_and_lays_out() {
        // ∫_0^1 \frac{a}{b} — operator limits (postfix script) + a fraction.
        let row = Row {
            atoms: vec![
                Atom::Sym(r"\int".into()),
                Atom::SupSub {
                    sup: Some(Row::syms("1")),
                    sub: Some(Row::syms("0")),
                },
                Atom::Frac {
                    num: Row::syms("a"),
                    den: Row::syms("b"),
                },
            ],
        };
        let latex = row.to_latex();
        let nodes = ratex_parser::parse(&latex).expect("RaTeX should parse our LaTeX");
        let lbox = ratex_layout::layout(&nodes, &ratex_layout::LayoutOptions::default());
        assert!(lbox.width > 0.0, "lays out to a non-empty box");
    }

    #[test]
    fn matrix_serializes_and_lays_out() {
        let m = Atom::Matrix {
            rows: vec![
                vec![Row::syms("a"), Row::syms("b")],
                vec![Row::syms("c"), Row::syms("d")],
            ],
        };
        assert_eq!(
            m.to_latex(),
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}"
        );
        let nodes = ratex_parser::parse(&m.to_latex()).expect("RaTeX parses pmatrix");
        let lbox = ratex_layout::layout(&nodes, &ratex_layout::LayoutOptions::default());
        assert!(lbox.width > 0.0);
    }

    /// Every delimiter the editor can wrap a selection in must round-trip through RaTeX —
    /// guards against shipping an `open`/`close` (e.g. a norm or angle bracket) the engine
    /// can't parse or lay out.
    #[test]
    fn delimiter_variants_parse_and_lay_out() {
        for (open, close) in [
            ("(", ")"),
            ("[", "]"),
            (r"\{", r"\}"),
            ("|", "|"),
            (r"\|", r"\|"),
            (r"\langle", r"\rangle"),
            (r"\lfloor", r"\rfloor"),
            (r"\lceil", r"\rceil"),
        ] {
            let d = Atom::Delim {
                open: open.into(),
                body: Row::syms("x"),
                close: close.into(),
            };
            let latex = d.to_latex();
            let nodes =
                ratex_parser::parse(&latex).unwrap_or_else(|_| panic!("RaTeX parses {latex}"));
            let lbox = ratex_layout::layout(&nodes, &ratex_layout::LayoutOptions::default());
            assert!(lbox.width > 0.0, "{latex} lays out to a non-empty box");
        }
    }
}
