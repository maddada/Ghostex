//! The typed-input interpreter — "natural typing" that maps keystrokes to structural
//! edits. GUI-free; the view feeds it characters and repaints.
//!
//! Triggers: `/` opens a fraction (cursor into the numerator), `^`/`_` open a
//! super/subscript on the preceding atom, `(` opens a delimiter pair (cursor inside),
//! `)` hops out of it, space exits the current structure, and any other character is
//! inserted as a literal symbol. Multi-character `\commands` are a later milestone.

use crate::editor::cursor::{Cursor, Slot, Step};
use crate::editor::model::{Atom, Row};

/// Apply one typed character as a structural edit at the cursor.
pub fn type_char(top: &mut Row, cursor: &mut Cursor, ch: char) {
    match ch {
        '/' => cursor.insert(
            top,
            Atom::Frac {
                num: Row::new(),
                den: Row::new(),
            },
        ),
        '^' => insert_script(
            top,
            cursor,
            Atom::SupSub {
                sup: Some(Row::new()),
                sub: None,
            },
        ),
        '_' => insert_script(
            top,
            cursor,
            Atom::SupSub {
                sup: None,
                sub: Some(Row::new()),
            },
        ),
        '(' => cursor.insert(
            top,
            Atom::Delim {
                open: "(".into(),
                body: Row::new(),
                close: ")".into(),
            },
        ),
        ')' => close_delim(cursor),
        // A typed brace is the literal `\{…\}` pair, mirroring `(` — a bare `{` Sym
        // would be TeX group syntax and render as nothing (#77).
        '{' => cursor.insert(
            top,
            Atom::Delim {
                open: r"\{".into(),
                body: Row::new(),
                close: r"\}".into(),
            },
        ),
        '}' => close_delim(cursor),
        ' ' => exit_structure(cursor),
        _ => cursor.insert(top, Atom::Sym(ch.to_string())),
    }
}

/// A script needs a base — the atom just before the cursor. With none (a row/slot
/// start), the keystroke is dropped rather than producing baseless `^{}`.
fn insert_script(top: &mut Row, cursor: &mut Cursor, script: Atom) {
    if cursor.index == 0 {
        return;
    }
    cursor.insert(top, script);
}

/// `)` hops out of the innermost delimiter body, to just after the delimiter.
fn close_delim(cursor: &mut Cursor) {
    if let Some(&Step {
        slot: Slot::Body,
        atom,
    }) = cursor.path.last()
    {
        cursor.path.pop();
        cursor.index = atom + 1;
    }
}

/// Space exits the current structure (MathQuill convention): hop up one level, to just
/// after the structure. A no-op at the top level.
fn exit_structure(cursor: &mut Cursor) {
    if let Some(step) = cursor.path.pop() {
        cursor.index = step.atom + 1;
    }
}

// ---------------------------------------------------------------------------
// `\command` entry — the keyboard path to the TeX symbol/structure long tail.
// ---------------------------------------------------------------------------

/// What a `\command` expands to.
#[derive(Clone, Copy)]
enum Command {
    /// A symbol, inserted as a leaf with this LaTeX.
    Sym(&'static str),
    /// A fraction (caret into the numerator).
    Frac,
    /// A square root (caret into the radicand).
    Sqrt,
    /// An nth-root — a root with an editable degree box (caret into the degree, e.g. `3`
    /// for a cube root).
    NthRoot,
    /// A matched delimiter pair given as `(open, close)` LaTeX — parentheses, brackets,
    /// braces, bars, etc. With a selection it wraps it; otherwise an empty pair (caret
    /// inside the body).
    Delim(&'static str, &'static str),
    /// A big operator with empty lower + upper limit boxes (∫, ∑, ∏); caret into the lower.
    OpLimits(&'static str),
    /// A big operator with only a lower limit box (lim); caret into it.
    OpSub(&'static str),
    /// A matrix — a 2×2 grid of empty cells (caret into the top-left).
    Matrix,
    /// An accent (`\hat`, `\bar`, …) over an editable base. With a selection it wraps
    /// it; otherwise an empty base (caret inside).
    Accent(&'static str),
}

/// The known `\commands`. Table order is the autocomplete order.
#[rustfmt::skip]
const COMMANDS: &[(&str, Command)] = &[
    // structures
    ("frac", Command::Frac), ("sqrt", Command::Sqrt), ("nthroot", Command::NthRoot),
    ("matrix", Command::Matrix),
    // delimiters (wrap a selection, or insert an empty pair)
    ("paren", Command::Delim("(", ")")), ("bracket", Command::Delim("[", "]")),
    ("brace", Command::Delim(r"\{", r"\}")), ("abs", Command::Delim("|", "|")),
    ("norm", Command::Delim(r"\|", r"\|")), ("angle", Command::Delim(r"\langle", r"\rangle")),
    ("floor", Command::Delim(r"\lfloor", r"\rfloor")), ("ceil", Command::Delim(r"\lceil", r"\rceil")),
    // accents (wrap a selection, or insert an empty base)
    ("hat", Command::Accent(r"\hat")), ("widehat", Command::Accent(r"\widehat")),
    ("bar", Command::Accent(r"\bar")), ("vec", Command::Accent(r"\vec")),
    ("tilde", Command::Accent(r"\tilde")), ("widetilde", Command::Accent(r"\widetilde")),
    ("dot", Command::Accent(r"\dot")), ("ddot", Command::Accent(r"\ddot")),
    ("check", Command::Accent(r"\check")), ("breve", Command::Accent(r"\breve")),
    // operators / functions
    ("int", Command::OpLimits(r"\int")), ("iint", Command::OpLimits(r"\iint")),
    ("oint", Command::OpLimits(r"\oint")), ("sum", Command::OpLimits(r"\sum")),
    ("prod", Command::OpLimits(r"\prod")), ("lim", Command::OpSub(r"\lim")),
    ("log", Command::Sym(r"\log")), ("ln", Command::Sym(r"\ln")),
    ("sin", Command::Sym(r"\sin")), ("cos", Command::Sym(r"\cos")),
    ("tan", Command::Sym(r"\tan")),
    // relations
    ("le", Command::Sym(r"\le")), ("leq", Command::Sym(r"\leq")),
    ("ge", Command::Sym(r"\ge")), ("geq", Command::Sym(r"\geq")),
    ("ne", Command::Sym(r"\ne")), ("approx", Command::Sym(r"\approx")),
    ("equiv", Command::Sym(r"\equiv")), ("sim", Command::Sym(r"\sim")),
    ("propto", Command::Sym(r"\propto")),
    // binary operators
    ("times", Command::Sym(r"\times")), ("div", Command::Sym(r"\div")),
    ("cdot", Command::Sym(r"\cdot")), ("pm", Command::Sym(r"\pm")),
    ("mp", Command::Sym(r"\mp")), ("ast", Command::Sym(r"\ast")),
    ("circ", Command::Sym(r"\circ")),
    // arrows
    ("to", Command::Sym(r"\to")), ("rightarrow", Command::Sym(r"\rightarrow")),
    ("leftarrow", Command::Sym(r"\leftarrow")), ("Rightarrow", Command::Sym(r"\Rightarrow")),
    ("leftrightarrow", Command::Sym(r"\leftrightarrow")), ("mapsto", Command::Sym(r"\mapsto")),
    // sets / logic
    ("in", Command::Sym(r"\in")), ("notin", Command::Sym(r"\notin")),
    ("subset", Command::Sym(r"\subset")), ("subseteq", Command::Sym(r"\subseteq")),
    ("supset", Command::Sym(r"\supset")), ("cup", Command::Sym(r"\cup")),
    ("cap", Command::Sym(r"\cap")), ("emptyset", Command::Sym(r"\emptyset")),
    ("forall", Command::Sym(r"\forall")), ("exists", Command::Sym(r"\exists")),
    ("neg", Command::Sym(r"\neg")),
    // misc
    ("infty", Command::Sym(r"\infty")), ("partial", Command::Sym(r"\partial")),
    ("nabla", Command::Sym(r"\nabla")), ("angle", Command::Sym(r"\angle")),
    ("cdots", Command::Sym(r"\cdots")), ("ldots", Command::Sym(r"\ldots")),
    // greek lowercase
    ("alpha", Command::Sym(r"\alpha")), ("beta", Command::Sym(r"\beta")),
    ("gamma", Command::Sym(r"\gamma")), ("delta", Command::Sym(r"\delta")),
    ("epsilon", Command::Sym(r"\epsilon")), ("varepsilon", Command::Sym(r"\varepsilon")),
    ("zeta", Command::Sym(r"\zeta")), ("eta", Command::Sym(r"\eta")),
    ("theta", Command::Sym(r"\theta")), ("iota", Command::Sym(r"\iota")),
    ("kappa", Command::Sym(r"\kappa")), ("lambda", Command::Sym(r"\lambda")),
    ("mu", Command::Sym(r"\mu")), ("nu", Command::Sym(r"\nu")),
    ("xi", Command::Sym(r"\xi")), ("pi", Command::Sym(r"\pi")),
    ("rho", Command::Sym(r"\rho")), ("sigma", Command::Sym(r"\sigma")),
    ("tau", Command::Sym(r"\tau")), ("phi", Command::Sym(r"\phi")),
    ("varphi", Command::Sym(r"\varphi")), ("chi", Command::Sym(r"\chi")),
    ("psi", Command::Sym(r"\psi")), ("omega", Command::Sym(r"\omega")),
    // greek uppercase
    ("Gamma", Command::Sym(r"\Gamma")), ("Delta", Command::Sym(r"\Delta")),
    ("Theta", Command::Sym(r"\Theta")), ("Lambda", Command::Sym(r"\Lambda")),
    ("Xi", Command::Sym(r"\Xi")), ("Pi", Command::Sym(r"\Pi")),
    ("Sigma", Command::Sym(r"\Sigma")), ("Phi", Command::Sym(r"\Phi")),
    ("Psi", Command::Sym(r"\Psi")), ("Omega", Command::Sym(r"\Omega")),
];

fn lookup_command(name: &str) -> Option<Command> {
    COMMANDS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// Commit a typed `\name` at the cursor. Returns `false` if it isn't a known command
/// (the caller decides the fallback — e.g. drop it or insert the literal letters).
pub fn commit_command(top: &mut Row, cursor: &mut Cursor, name: &str) -> bool {
    match lookup_command(name) {
        Some(Command::Sym(latex)) => cursor.insert(top, Atom::Sym(latex.to_string())),
        Some(Command::Frac) => cursor.insert(
            top,
            Atom::Frac {
                num: Row::new(),
                den: Row::new(),
            },
        ),
        Some(Command::Sqrt) => cursor.insert(
            top,
            Atom::Sqrt {
                radicand: Row::new(),
                index: None,
            },
        ),
        // An empty degree box first (caret lands there), then the radicand — `nav_slots`
        // orders `[Index, Radicand]`, so `insert` descends into the degree.
        Some(Command::NthRoot) => cursor.insert(
            top,
            Atom::Sqrt {
                radicand: Row::new(),
                index: Some(Row::new()),
            },
        ),
        Some(Command::OpLimits(op)) => {
            cursor.insert(top, Atom::Sym(op.to_string()));
            cursor.insert(
                top,
                Atom::SupSub {
                    sub: Some(Row::new()),
                    sup: Some(Row::new()),
                },
            );
        }
        Some(Command::OpSub(op)) => {
            cursor.insert(top, Atom::Sym(op.to_string()));
            cursor.insert(
                top,
                Atom::SupSub {
                    sub: Some(Row::new()),
                    sup: None,
                },
            );
        }
        Some(Command::Matrix) => cursor.insert(
            top,
            Atom::Matrix {
                rows: vec![vec![Row::new(), Row::new()], vec![Row::new(), Row::new()]],
            },
        ),
        Some(Command::Delim(open, close)) => cursor.insert(
            top,
            Atom::Delim {
                open: open.to_string(),
                body: Row::new(),
                close: close.to_string(),
            },
        ),
        Some(Command::Accent(accent)) => cursor.insert(
            top,
            Atom::Accent {
                accent: accent.to_string(),
                base: Row::new(),
            },
        ),
        None => return false,
    }
    true
}

/// Commit a palette/`\command` `name` with a possible selection `sel` (an atom range in the
/// cursor's row). A wrap-capable structure — a fraction or a root — wraps the selection
/// (it becomes the numerator / radicand); every other command just inserts at the caret,
/// leaving the selection's atoms in place (the caller clears the selection highlight). With
/// no selection this is plain [`commit_command`].
pub fn commit_command_selecting(
    top: &mut Row,
    cursor: &mut Cursor,
    name: &str,
    sel: Option<(usize, usize)>,
) -> bool {
    match (lookup_command(name), sel) {
        (Some(Command::Frac), Some((lo, hi))) => {
            cursor.wrap_fraction(top, lo, hi);
            true
        }
        (Some(Command::Sqrt), Some((lo, hi))) => {
            cursor.wrap_sqrt(top, lo, hi);
            true
        }
        (Some(Command::NthRoot), Some((lo, hi))) => {
            cursor.wrap_nth_root(top, lo, hi);
            true
        }
        (Some(Command::Delim(open, close)), Some((lo, hi))) => {
            cursor.wrap_delim(top, lo, hi, open, close);
            true
        }
        (Some(Command::Accent(accent)), Some((lo, hi))) => {
            cursor.wrap_accent(top, lo, hi, accent);
            true
        }
        _ => commit_command(top, cursor, name),
    }
}

/// The delimiter pair a typed bracket/brace/bar wraps a selection in — `(open, close)`
/// LaTeX — or `None` if `c` isn't a wrapping delimiter. The keyboard wrap path uses this so
/// `[`, `{`, `|` over a selection behave like `(`.
pub fn delim_pair(c: char) -> Option<(&'static str, &'static str)> {
    match c {
        '(' => Some(("(", ")")),
        '[' => Some(("[", "]")),
        '{' => Some((r"\{", r"\}")),
        '|' => Some(("|", "|")),
        _ => None,
    }
}

/// The known command names that start with `prefix`, in table order (for autocomplete).
pub fn command_matches(prefix: &str) -> Vec<&'static str> {
    COMMANDS
        .iter()
        .filter(|(n, _)| n.starts_with(prefix))
        .map(|(n, _)| *n)
        .collect()
}

/// A curated click-to-insert palette: `(display glyph, command name)`. The command name
/// feeds [`commit_command`], so the palette and `\command` typing share one source.
#[rustfmt::skip]
pub const PALETTE: &[(&str, &str)] = &[
    ("x/y", "frac"), ("√", "sqrt"), ("ⁿ√", "nthroot"), ("▦", "matrix"),
    // delimiters — wrap the selection, or insert an empty pair
    ("()", "paren"), ("[]", "bracket"), ("{}", "brace"),    ("||", "abs"),
    ("‖‖", "norm"),  ("⟨⟩", "angle"),   ("⌊⌋", "floor"),    ("⌈⌉", "ceil"),
    ("∫", "int"),    ("∑", "sum"),     ("∏", "prod"),   ("∞", "infty"),
    ("π", "pi"),     ("θ", "theta"),   ("α", "alpha"),  ("β", "beta"),
    ("γ", "gamma"),  ("δ", "delta"),   ("λ", "lambda"), ("μ", "mu"),
    ("σ", "sigma"),  ("φ", "phi"),     ("ω", "omega"),  ("Δ", "Delta"),
    ("≤", "le"),     ("≥", "ge"),      ("≠", "ne"),     ("≈", "approx"),
    ("×", "times"),  ("÷", "div"),     ("·", "cdot"),   ("±", "pm"),
    ("→", "to"),     ("∂", "partial"), ("∇", "nabla"),  ("∈", "in"),
    // accents — wrap the selection, or insert an empty base (#77)
    ("x\u{302}", "hat"), ("x\u{304}", "bar"), ("x\u{20D7}", "vec"), ("x\u{303}", "tilde"), ("x\u{307}", "dot"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> Row {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for ch in s.chars() {
            type_char(&mut top, &mut cur, ch);
        }
        top
    }

    #[test]
    fn typed_braces_are_a_literal_pair() {
        // `{x}` types as the `\{…\}` delimiter, `}` hops out — never a bare group (#77).
        assert_eq!(typed("{x}y").to_latex(), r"\left\{ x \right\} y");
    }

    #[test]
    fn hat_command_descends_into_base() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        assert!(commit_command(&mut top, &mut cur, "hat"));
        type_char(&mut top, &mut cur, 'X');
        assert_eq!(top.to_latex(), r"\hat{X}");
    }

    #[test]
    fn command_inserts_symbol() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        assert!(commit_command(&mut top, &mut cur, "alpha"));
        assert_eq!(top.to_latex(), r"\alpha");
    }

    #[test]
    fn command_inserts_structure_and_descends() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        assert!(commit_command(&mut top, &mut cur, "sqrt"));
        assert_eq!(top.to_latex(), r"\sqrt{\square}"); // caret descended into the radicand
    }

    #[test]
    fn unknown_command_is_rejected() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        assert!(!commit_command(&mut top, &mut cur, "definitelynotacommand"));
        assert!(top.is_empty());
    }

    #[test]
    fn command_matches_by_prefix() {
        let m = command_matches("al");
        assert!(m.contains(&"alpha"));
        assert!(!m.contains(&"beta"));
    }

    #[test]
    fn op_with_limits_inserts_boxes() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        assert!(commit_command(&mut top, &mut cur, "int"));
        assert_eq!(top.to_latex(), r"\int _{\square}^{\square}");
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 1,
                slot: Slot::Sub
            }]
        );
    }

    #[test]
    fn nth_root_inserts_a_degree_box_caret_in_it() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        assert!(commit_command(&mut top, &mut cur, "nthroot"));
        assert_eq!(top.to_latex(), r"\sqrt[\square]{\square}");
        // The caret descends into the degree (index) slot, ready for e.g. `3`.
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Index
            }]
        );
    }

    #[test]
    fn nth_root_wraps_a_selection_as_the_radicand() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for c in ["x", "y"] {
            cur.insert(&mut top, Atom::Sym(c.into()));
        }
        // Select both atoms (0..2) and pick the nth-root command.
        assert!(commit_command_selecting(
            &mut top,
            &mut cur,
            "nthroot",
            Some((0, 2))
        ));
        assert_eq!(top.to_latex(), r"\sqrt[\square]{x y}");
        assert_eq!(
            cur.path,
            vec![Step {
                atom: 0,
                slot: Slot::Index
            }]
        );
    }

    #[test]
    fn delim_command_inserts_empty_pair_and_wraps_selection() {
        // No selection → an empty bracket pair (caret descends into the body).
        let mut top = Row::new();
        let mut cur = Cursor::start();
        assert!(commit_command(&mut top, &mut cur, "bracket"));
        assert_eq!(top.to_latex(), r"\left[ \square \right]");

        // With a selection → wrap it (here, in bars).
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for c in ["a", "b"] {
            cur.insert(&mut top, Atom::Sym(c.into()));
        }
        assert!(commit_command_selecting(
            &mut top,
            &mut cur,
            "abs",
            Some((0, 2))
        ));
        assert_eq!(top.to_latex(), r"\left| a b \right|");
    }

    #[test]
    fn delim_pair_maps_typed_brackets() {
        assert_eq!(delim_pair('('), Some(("(", ")")));
        assert_eq!(delim_pair('['), Some(("[", "]")));
        assert_eq!(delim_pair('{'), Some((r"\{", r"\}")));
        assert_eq!(delim_pair('|'), Some(("|", "|")));
        assert_eq!(delim_pair('x'), None);
    }

    #[test]
    fn plain_symbols() {
        assert_eq!(typed("4x").to_latex(), "4 x");
    }

    #[test]
    fn slash_makes_a_fraction_and_enters_numerator() {
        // "1/2": '1' -> sym; '/' -> fraction, cursor into the numerator; '2' -> numerator.
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for ch in "1/2".chars() {
            type_char(&mut top, &mut cur, ch);
        }
        assert_eq!(top.to_latex(), r"1 \frac{2}{\square}");
    }

    #[test]
    fn caret_makes_a_superscript() {
        assert_eq!(typed("x^2").to_latex(), "x ^{2}");
    }

    #[test]
    fn underscore_makes_a_subscript() {
        assert_eq!(typed("a_i").to_latex(), "a _{i}");
    }

    #[test]
    fn script_with_no_base_is_dropped() {
        // '^' at the row start has no base, so it's ignored; '2' lands as a symbol.
        assert_eq!(typed("^2").to_latex(), "2");
    }

    #[test]
    fn parens_wrap_then_close_exits() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for ch in "(x)".chars() {
            type_char(&mut top, &mut cur, ch);
        }
        assert_eq!(top.to_latex(), r"\left( x \right)");
        assert_eq!(cur.path, vec![]); // exited the body
        assert_eq!(cur.index, 1); // sitting just after the delimiter
    }

    #[test]
    fn space_exits_a_script() {
        let mut top = Row::new();
        let mut cur = Cursor::start();
        for ch in "x^2".chars() {
            type_char(&mut top, &mut cur, ch);
        }
        type_char(&mut top, &mut cur, ' '); // exit the superscript
        assert_eq!(cur.path, vec![]);
        type_char(&mut top, &mut cur, '3'); // continues after the script
        assert_eq!(top.to_latex(), "x ^{2} 3");
    }
}
