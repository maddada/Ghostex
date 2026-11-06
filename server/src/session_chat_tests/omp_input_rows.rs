//! OMP's input with rows drawn under it (`session_chat_composer_input`'s OMP grammar). Every
//! composer shape takes any rows OMP draws there (its status bar, extension statuses, extension
//! widgets, the slash-command list) and refuses rows that open with a frame or another shape's
//! chrome, or a second list cursor. The screens follow OMP 18.13.3's composer shapes
//! (`packages/tui/src/components/composer/`), the gap row it keeps above every editor but the
//! band's (`packages/tui/src/prompt/editor-top-gap.ts`), the list it hangs under the editor
//! (`packages/tui/src/components/editor.ts`), widgets padded by one cell and capped at ten lines
//! (`packages/coding-agent/src/modes/controllers/extension-ui-controller.ts`) and the frames of
//! its symbol presets (`packages/tui/src/theme/symbols.ts`); they are written out here, not
//! captured.

use crate::session_chat_composer::{
    detect_session_chat_composer_readiness, session_chat_composer_input,
    SessionChatComposerReadiness, SessionChatComposerState,
};
use crate::session_chat_notice::classify_session_chat_terminal_notice;
use crate::session_chat_omp_blocking::detect_omp_blocking_screen;

#[derive(Clone, Copy, Debug)]
enum Shape {
    Box,
    Band,
    Claude,
    Pi,
    Borderless,
    Rule,
    Field,
    Rail,
}

const SHAPES: [Shape; 8] = [
    Shape::Box,
    Shape::Band,
    Shape::Claude,
    Shape::Pi,
    Shape::Borderless,
    Shape::Rule,
    Shape::Field,
    Shape::Rail,
];

/// The transcript ends with a tool's output block whose foot carries its exit code: a rounded box
/// of the same family as the box composer.
const TRANSCRIPT: &str = "Ran the build.\n\
\n\
╭─── bash ─────────────────────────────╮\n\
│ cargo build                          │\n\
╰─── exit 0 ───────────────────────────╯\n";

const DRAFT: &str = "hello there";
const COMMAND: &str = "/mo";
const STATUS_BAR: &str = "π  gpt-5 · ~/work/app · main · 12%";
const HOOK_STATUSES: [&str; 3] = ["lint clean", "2 jobs queued", "deploy watcher idle"];

/// The input with its chrome, after the gap row OMP keeps above it.
fn composer(shape: Shape, draft: &str) -> String {
    match shape {
        Shape::Box => format!(
            "\n╭── π  gpt-5 ──────────────────────────╮\n╰─ {draft:<34} ─╯\n"
        ),
        Shape::Band => format!("\nπ  gpt-5 · ~/work/app · main\n╰─ {draft}\n"),
        Shape::Claude => format!(
            "\n─────────────────────────────── main ─\n❯ {draft}\n──────────────────────────────────────\n"
        ),
        Shape::Pi => format!(
            "\n──────────────────────────────────────\n {draft}\n──────────────────────────────────────\n"
        ),
        Shape::Borderless => format!("\n❯ {draft}\n"),
        Shape::Rule => format!("\n─────────────────────────────── main ─\n❯ {draft}\n"),
        Shape::Field => format!("\n▐ {draft:<35}▌\n"),
        Shape::Rail => format!("\n▎ {draft}\n"),
    }
}

/// The rule, field and rail shapes keep a blank row between the input and the status rows.
fn status_gap(shape: Shape) -> bool {
    matches!(shape, Shape::Rule | Shape::Field | Shape::Rail)
}

/// What OMP draws under the input: the slash list right under it, extension widgets, then the
/// status host (the status bar and one row per extension status).
#[derive(Default)]
struct Under {
    list: Vec<String>,
    widget: Vec<String>,
    status: Vec<String>,
}

fn screen(shape: Shape, draft: &str, under: &Under) -> String {
    screen_after(TRANSCRIPT, shape, draft, under)
}

fn screen_after(transcript: &str, shape: Shape, draft: &str, under: &Under) -> String {
    let mut screen = format!("{transcript}{}", composer(shape, draft));
    for row in under.list.iter().chain(&under.widget) {
        screen.push_str(row);
        screen.push('\n');
    }
    if status_gap(shape) && !under.status.is_empty() {
        screen.push('\n');
    }
    for row in &under.status {
        screen.push_str(row);
        screen.push('\n');
    }
    screen
}

fn rows(rows: &[&str]) -> Vec<String> {
    rows.iter().map(|row| row.to_string()).collect()
}

fn status_bar() -> Vec<String> {
    rows(&[STATUS_BAR])
}

fn status_bar_and_hooks() -> Vec<String> {
    let mut status = status_bar();
    status.extend(rows(&HOOK_STATUSES));
    status
}

/// A string-array widget: OMP draws each line with one cell of padding.
fn ten_line_widget() -> Vec<String> {
    (1..=10)
        .map(|line| format!(" widget line {line}"))
        .collect()
}

/// One widget line too long for the pane, wrapped by OMP onto three padded rows.
fn wrapped_widget() -> Vec<String> {
    rows(&[
        " A widget line long enough that OMP",
        " wraps it onto a second row and then",
        " onto a third one",
    ])
}

/// The slash list for `/mo` with its `selected` row drawn by a symbol preset's `nav.cursor` and
/// the others with two spaces.
fn command_list(cursor: &str, selected: usize) -> Vec<String> {
    [
        "model       Switch the model",
        "models      List the models",
    ]
    .iter()
    .enumerate()
    .map(|(index, item)| {
        if index == selected {
            format!("{cursor} {item}")
        } else {
            format!("  {item}")
        }
    })
    .collect()
}

/// The verdict the send path acts on: the input's readiness with OMP's blocking notice as a veto.
fn readiness(screen: &str) -> SessionChatComposerReadiness {
    let notice = classify_session_chat_terminal_notice(Some("omp"), screen);
    detect_session_chat_composer_readiness(Some("omp"), screen, notice.as_ref())
}

/// The draft as read from the screen, each row trimmed: what the send path compares and clears.
fn draft(screen: &str) -> Option<String> {
    session_chat_composer_input("omp", screen).map(|input| {
        input
            .text
            .lines()
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    })
}

fn assert_ready(shape: Shape, label: &str, screen: &str, expected_draft: &str) {
    assert_eq!(
        readiness(screen).state,
        SessionChatComposerState::Ready,
        "{shape:?} with {label} should be ready:\n{screen}"
    );
    assert_eq!(
        draft(screen).as_deref(),
        Some(expected_draft),
        "{shape:?} with {label} should read its own input:\n{screen}"
    );
}

fn assert_not_ready(shape: Shape, label: &str, screen: &str) {
    assert!(
        readiness(screen).blocks_message_for(Some("omp")),
        "{shape:?} with {label} should hold the message:\n{screen}"
    );
}

#[test]
fn every_composer_shape_takes_the_rows_omp_draws_under_its_input() {
    let states: [(&str, Under); 5] = [
        ("nothing under the input", Under::default()),
        (
            "the status bar",
            Under {
                status: status_bar(),
                ..Under::default()
            },
        ),
        (
            "the status bar and three extension statuses",
            Under {
                status: status_bar_and_hooks(),
                ..Under::default()
            },
        ),
        (
            "a ten-line widget",
            Under {
                widget: ten_line_widget(),
                status: status_bar(),
                ..Under::default()
            },
        ),
        (
            "a wrapped widget",
            Under {
                widget: wrapped_widget(),
                status: status_bar(),
                ..Under::default()
            },
        ),
    ];
    for shape in SHAPES {
        for (label, under) in &states {
            assert_ready(shape, label, &screen(shape, DRAFT, under), DRAFT);
        }
    }
}

#[test]
fn the_slash_list_counts_in_every_symbol_preset_with_or_without_rows_under_it() {
    for shape in SHAPES {
        for cursor in ["❯", "\u{f054}", ">"] {
            for selected in [0, 1] {
                for statuses in [&[][..], &HOOK_STATUSES[..]] {
                    let under = Under {
                        list: command_list(cursor, selected),
                        status: rows(statuses),
                        ..Under::default()
                    };
                    let label = format!(
                        "the slash list ({cursor}, item {selected} selected, {} statuses)",
                        statuses.len()
                    );
                    let screen = screen(shape, COMMAND, &under);
                    // Under a rule or borderless input, an unselected item above the selected one
                    // lines up like a draft row would, so the input waits for the list to close.
                    if selected > 0 && matches!(shape, Shape::Rule | Shape::Borderless) {
                        assert_not_ready(shape, &label, &screen);
                    } else {
                        assert_ready(shape, &label, &screen, COMMAND);
                    }
                }
            }
        }
    }
}

#[test]
fn a_draft_row_lined_up_with_the_list_above_its_selected_item_holds_the_message() {
    // OMP's mid-prompt completion list under a two-row draft whose second row puts `/mo` in the
    // list's description column: the screen shows it like an unselected list item.
    let list = [
        "❯ skill:model   First skill",
        "  skill:models  Second skill",
    ];
    for shape in [Shape::Rule, Shape::Borderless] {
        for (second_row, lined_up) in [("x             /mo", true), ("x              /mo", false)] {
            let mut screen = format!(
                "Earlier reply.\n{}",
                multiline_composer(shape, &["first", second_row])
            );
            for row in list.iter().chain(&HOOK_STATUSES) {
                screen.push_str(row);
                screen.push('\n');
            }
            if lined_up {
                assert_not_ready(shape, "a draft row lined up with the list", &screen);
            } else {
                assert_ready(
                    shape,
                    "a draft row one column off the list",
                    &screen,
                    &format!("first\n{second_row}"),
                );
            }
        }
    }
}

#[test]
fn the_hold_follows_the_lists_own_description_column() {
    // OMP's select list collapses a label's whitespace to single spaces, pads it by at least two
    // cells and starts the description at the same terminal column on every row, a wide label or
    // an emoji sequence counting the two cells it is drawn in. An icon cell before the label is
    // padded to the widest icon. A list without descriptions has no description column.
    let row = |label: &str, cells: usize, column: usize, description: &str| {
        format!("{label}{}{description}", " ".repeat(column - 2 - cells))
    };
    let families: [(&str, [String; 2], bool); 7] = [
        (
            "labels with spaces, no descriptions",
            ["one file.txt".into(), "two file.txt".into()],
            false,
        ),
        (
            "labels with spaces and descriptions",
            [
                row("short name", 10, 34, "First detail"),
                row("longer name", 11, 34, "Second detail"),
            ],
            true,
        ),
        (
            "a wide label and descriptions",
            [
                row("模型", 4, 34, "First detail"),
                row("model", 5, 34, "Second detail"),
            ],
            true,
        ),
        (
            "an emoji sequence label and descriptions",
            [
                row("👩\u{200d}💻", 2, 34, "First detail"),
                row("model", 5, 34, "Second detail"),
            ],
            true,
        ),
        (
            "an emoji with a skin tone and descriptions",
            [
                row("👍🏽", 2, 34, "First detail"),
                row("model", 5, 34, "Second detail"),
            ],
            true,
        ),
        (
            "icons, no descriptions",
            ["f  alpha".into(), "🚀 beta".into()],
            false,
        ),
        (
            "icons of two widths and descriptions",
            [
                row("f  alpha", 8, 37, "First detail"),
                row("🚀 beta", 7, 37, "Second detail"),
            ],
            true,
        ),
    ];
    for shape in [Shape::Rule, Shape::Borderless] {
        for (label, items, described) in &families {
            for selected in [0, 1] {
                let mut screen =
                    format!("Earlier reply.\n{}", multiline_composer(shape, &["/model"]));
                for (index, item) in items.iter().enumerate() {
                    let marker = if index == selected { "❯ " } else { "  " };
                    screen.push_str(&format!("{marker}{item}\n"));
                }
                let case = format!("{label}, item {selected} selected");
                if *described && selected > 0 {
                    assert_not_ready(shape, &case, &screen);
                } else {
                    assert_eq!(
                        readiness(&screen).state,
                        SessionChatComposerState::Ready,
                        "{shape:?} with {case} should be ready:\n{screen}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_borderless_input_under_a_tool_output_block_is_read_as_the_input() {
    // The output block is a complete rounded box with text in its foot, so the box grammar, tried
    // first, could take it for a box composer; the borderless input under it is the real one.
    for (label, under) in [
        (
            "the status bar",
            Under {
                status: status_bar(),
                ..Under::default()
            },
        ),
        (
            "the status bar and three extension statuses",
            Under {
                status: status_bar_and_hooks(),
                ..Under::default()
            },
        ),
        (
            "a ten-line widget",
            Under {
                widget: ten_line_widget(),
                status: status_bar(),
                ..Under::default()
            },
        ),
    ] {
        let screen = screen(Shape::Borderless, DRAFT, &under);
        assert_ready(Shape::Borderless, label, &screen, DRAFT);
    }
}

#[test]
fn a_second_list_cursor_under_the_input_holds_the_message() {
    for shape in SHAPES {
        for cursor in ["❯", "\u{f054}", ">"] {
            for (label, list) in [
                (
                    "the first item selected",
                    vec![
                        format!("{cursor} model       Switch the model"),
                        "  models      List the models".to_string(),
                        format!("{cursor} mode        Switch the mode"),
                    ],
                ),
                (
                    "a later item selected",
                    vec![
                        "  model       Switch the model".to_string(),
                        format!("{cursor} models      List the models"),
                        format!("{cursor} mode        Switch the mode"),
                    ],
                ),
            ] {
                let under = Under {
                    list,
                    status: rows(&HOOK_STATUSES),
                    ..Under::default()
                };
                assert_not_ready(
                    shape,
                    &format!("two list cursors ({cursor}, {label})"),
                    &screen(shape, COMMAND, &under),
                );
            }
            // The same list under a draft whose last row is empty.
            let mut screen = format!(
                "Earlier reply.\n{}",
                multiline_composer(shape, &[COMMAND, ""])
            );
            for row in [
                format!("{cursor} model       Switch the model"),
                format!("{cursor} mode        Switch the mode"),
            ] {
                screen.push_str(&row);
                screen.push('\n');
            }
            assert_not_ready(
                shape,
                &format!("two list cursors ({cursor}) under an empty draft row"),
                &screen,
            );
        }
    }
}

#[test]
fn a_framed_row_under_the_input_holds_the_message() {
    for shape in SHAPES {
        for frame in [
            rows(&[
                "╭─ Extension ─────────╮",
                "│ framed widget row   │",
                "╰─────────────────────╯",
            ]),
            rows(&["│ framed widget row   │"]),
            rows(&[
                "+- Extension ---------+",
                "| framed widget row   |",
                "+---------------------+",
            ]),
            rows(&["| framed widget row   |"]),
        ] {
            let under = Under {
                widget: frame.clone(),
                status: status_bar(),
                ..Under::default()
            };
            assert_not_ready(
                shape,
                &format!("a framed row ({})", frame[0]),
                &screen(shape, DRAFT, &under),
            );
        }
    }
}

#[test]
fn another_shapes_input_row_right_under_the_input_holds_the_message() {
    // A component widget can draw any row; one drawn like the field or the rail shape's input row,
    // right under a different shape's input, is a widget. Under the same shape it continues the
    // input, so each shape gets only the other shape's cap.
    const FIELD_ROW: &str = "▐ other                              ▌";
    const RAIL_ROW: &str = "▎ other";
    for shape in SHAPES {
        let cap_rows: &[&str] = match shape {
            Shape::Field => &[RAIL_ROW],
            Shape::Rail => &[FIELD_ROW],
            _ => &[FIELD_ROW, RAIL_ROW],
        };
        for row in cap_rows {
            let under = Under {
                widget: rows(&[row]),
                ..Under::default()
            };
            assert_not_ready(
                shape,
                &format!("a widget row drawn like another shape's input ({row})"),
                &screen(shape, DRAFT, &under),
            );
        }
    }
}

#[test]
fn an_overlay_with_a_solid_foot_over_the_rows_under_the_input_holds_the_message() {
    let overlay = rows(&[
        "     ╭─ Switch model ──────────────╮",
        "     │ ❯ gpt-5                     │",
        "     │   claude-sonnet             │",
        "     │ ⏎ select · ⎋ cancel         │",
        "     ╰─────────────────────────────╯",
    ]);
    for shape in SHAPES {
        let mut status = status_bar();
        status.extend(overlay.clone());
        let under = Under {
            status,
            ..Under::default()
        };
        assert_not_ready(shape, "an overlay", &screen(shape, DRAFT, &under));
    }
}

/// A 40-column selector overlay, framed the way OMP's overlay box draws it.
fn selector_overlay() -> Vec<String> {
    let mut overlay = vec![format!("╭─ Deploy target {}╮", "─".repeat(22))];
    for row in [
        "",
        "  ❯ staging",
        "    production",
        "",
        " ↑/↓ navigate  ⏎ select  ⎋ cancel",
    ] {
        overlay.push(format!("│{row:<38}│"));
    }
    overlay.push(format!("╰{}╯", "─".repeat(38)));
    overlay
}

/// Writes `overlay` over the screen's rows from `top` at `column`, as OMP's compositor does: the
/// rows keep their text left of the overlay and are padded with spaces to reach it.
fn composite(screen: &str, overlay: &[String], top: usize, column: usize) -> String {
    let mut lines: Vec<String> = screen.lines().map(str::to_string).collect();
    lines.resize(lines.len().max(top + overlay.len()), String::new());
    for (offset, row) in overlay.iter().enumerate() {
        let mut cells: Vec<char> = lines[top + offset].chars().collect();
        let width = row.chars().count();
        cells.resize(cells.len().max(column + width), ' ');
        cells.splice(column..column + width, row.chars());
        lines[top + offset] = cells.into_iter().collect();
    }
    lines.join("\n") + "\n"
}

#[test]
fn an_overlay_composited_across_the_composer_or_the_rows_under_it_holds_the_message() {
    // The composer's first row sits after the transcript and the gap row above the editor.
    let composer_top = TRANSCRIPT.lines().count() + 1;
    for shape in SHAPES {
        let under = Under {
            widget: ten_line_widget(),
            status: status_bar_and_hooks(),
            ..Under::default()
        };
        let screen = screen(shape, DRAFT, &under);
        for top in [composer_top, composer_top + 3] {
            assert_not_ready(
                shape,
                &format!("a selector composited from row {top}"),
                &composite(&screen, &selector_overlay(), top, 20),
            );
        }
    }
}

/// A select dialog docked in the editor slot with OMP's Unicode symbols.
const DOCKED_DIALOG: &str = "╭─ Deploy target ──────────────────────╮\n\
│ Where should the build go?           │\n\
├──────────────────────────────────────┤\n\
│ ❯ staging                            │\n\
│   production                         │\n\
├──────────────────────────────────────┤\n\
│ ⏎ select · ⎋ cancel                  │\n\
╰──────────────────────────────────────╯\n";

/// The same dialog with OMP's ASCII symbols.
const DOCKED_DIALOG_ASCII: &str = "+- Deploy target ----------------------+\n\
| Where should the build go?           |\n\
+--------------------------------------+\n\
| > staging                            |\n\
|   production                         |\n\
+--------------------------------------+\n\
| Enter select · Esc cancel            |\n\
+--------------------------------------+\n";

/// A dialog replaces the editor and keeps the status rows under it.
fn docked(dialog: &str) -> String {
    format!(
        "{TRANSCRIPT}\n{dialog}{}\n",
        status_bar_and_hooks().join("\n")
    )
}

#[test]
fn a_dialog_docked_in_the_editor_slot_holds_the_message() {
    for dialog in [DOCKED_DIALOG, DOCKED_DIALOG_ASCII] {
        let screen = docked(dialog);
        assert!(
            readiness(&screen).blocks_message_for(Some("omp")),
            "a docked dialog should hold the message:\n{screen}"
        );
    }
    let screen = docked(DOCKED_DIALOG);
    assert!(
        detect_omp_blocking_screen(&screen).is_some(),
        "a docked dialog with status rows under it should stay a blocking screen:\n{screen}"
    );
}

#[test]
fn a_selector_dismissed_above_the_returned_input_does_not_hold_the_message() {
    // The selector's frame is left above the gap row OMP keeps over the editor.
    let dismissed = format!("{TRANSCRIPT}{DOCKED_DIALOG}");
    for shape in SHAPES {
        let under = Under {
            status: status_bar_and_hooks(),
            ..Under::default()
        };
        let screen = screen_after(&dismissed, shape, DRAFT, &under);
        assert_ready(shape, "a dismissed selector above it", &screen, DRAFT);
    }
}

/// OMP's Ask dialog with a choice list, as its Unicode symbols draw it.
const ASK: &str = "╭─ Ask ────────────────────────────────╮\n\
│ Where should the build go?           │\n\
├──────────────────────────────────────┤\n\
│ ❯ ○ staging                          │\n\
│   ○ production                       │\n\
├──────────────────────────────────────┤\n\
│ ⏎ select · n note · ↑/↓ move · ⎋ cancel │\n\
╰──────────────────────────────────────╯\n";

#[test]
fn a_question_with_the_draft_parked_under_it_holds_the_message() {
    // An Ask that arrives over a draft shares the editor slot with it: the draft's composer is
    // drawn directly under the Ask, with no gap row between them.
    for shape in SHAPES {
        let under = Under {
            status: status_bar_and_hooks(),
            ..Under::default()
        };
        let parked = screen_after(
            &format!("{TRANSCRIPT}\n{ASK}"),
            shape,
            "parked draft",
            &under,
        )
        .replacen(&format!("{ASK}\n"), ASK, 1);
        assert_not_ready(shape, "an Ask with the draft parked under it", &parked);
        assert_eq!(
            detect_omp_blocking_screen(&parked).map(|blocking| blocking.title),
            Some("OMP is waiting for an answer"),
            "{shape:?} with an Ask over its parked draft should stay a pending question:\n{parked}"
        );
        let answered = screen_after(
            &format!("{TRANSCRIPT}{ASK} Answered: staging\n"),
            shape,
            DRAFT,
            &under,
        );
        assert_ready(shape, "an answered Ask above it", &answered, DRAFT);
    }
}

#[test]
fn a_rule_shape_input_under_a_reply_with_a_horizontal_rule_is_read_as_the_input() {
    // OMP pads a reply's rows by one cell and draws its markdown rule full width, so the reply's
    // rule and the rule shape's top rule enclose padded rows like the pi shape's input.
    let reply = " First part.\n ─────────────────────────────────────\n Second part.\n";
    for under in [
        Under {
            status: status_bar_and_hooks(),
            ..Under::default()
        },
        Under {
            list: command_list("❯", 0),
            status: rows(&HOOK_STATUSES),
            ..Under::default()
        },
    ] {
        let draft = if under.list.is_empty() {
            DRAFT
        } else {
            COMMAND
        };
        let screen = screen_after(reply, Shape::Rule, draft, &under);
        assert_ready(Shape::Rule, "a reply's rule above it", &screen, draft);
        // Before the status line attaches, the rule shape's top rule is plain like a closing rule.
        let plain = screen.replacen(" main ─\n", "───────\n", 1);
        assert_ready(
            Shape::Rule,
            "a reply's rule above it and no status chip yet",
            &plain,
            draft,
        );
    }
}

#[test]
fn an_above_editor_widget_under_a_reply_with_a_horizontal_rule_keeps_each_input_readable() {
    // A widget placed above the editor takes the gap row's place: a blank row, the widget's rows,
    // then the editor with no row between.
    let above = " First part.\n ─────────────────────────────────────\n Second part.\n\n above editor widget";
    for shape in SHAPES {
        let under = Under {
            status: status_bar_and_hooks(),
            ..Under::default()
        };
        let screen = screen_after(above, shape, DRAFT, &under);
        assert_ready(
            shape,
            "an above-editor widget under a reply's rule",
            &screen,
            DRAFT,
        );
    }
}

#[test]
fn a_corner_glyph_in_the_draft_or_a_widget_line_is_text() {
    for shape in SHAPES {
        for glyph in ['╭', '╮', '╰', '╯'] {
            let typed = format!("describe x{glyph}y");
            assert_ready(
                shape,
                &format!("{typed:?} typed"),
                &screen(shape, &typed, &Under::default()),
                &typed,
            );
            let under = Under {
                widget: vec![format!(" step x{glyph}y")],
                status: status_bar(),
                ..Under::default()
            };
            assert_ready(
                shape,
                &format!("a widget line with {glyph} in it"),
                &screen(shape, DRAFT, &under),
                DRAFT,
            );
        }
    }
}

#[test]
fn an_indented_widget_above_the_editor_keeps_the_input_readable() {
    // OMP pads a string widget's lines by one cell, so a line that starts with spaces of its own
    // draws two-space rows above the editor, in place of the gap row.
    for widget in ["  indented widget", "   more indented widget"] {
        let above = format!("Earlier reply.\n\n{widget}");
        for shape in SHAPES {
            let under = Under {
                status: status_bar_and_hooks(),
                ..Under::default()
            };
            assert_ready(
                shape,
                &format!("{widget:?} above the editor"),
                &screen_after(&above, shape, DRAFT, &under),
                DRAFT,
            );
        }
    }
}

/// The editor's rows for a draft of several lines, as each shape draws them; an empty line is a
/// blank row, except in the box and the field, whose side chrome frames it.
fn multiline_composer(shape: Shape, draft: &[&str]) -> String {
    let (last, rows) = draft.split_last().expect("a draft row");
    let mut composer = String::from("\n");
    let rule = "──────────────────────────────────────";
    match shape {
        Shape::Box => {
            composer.push_str("╭── π  gpt-5 ──────────────────────────╮\n");
            for row in rows {
                composer.push_str(&format!("│  {row:<35}│\n"));
            }
            composer.push_str(&format!("╰─ {last:<34} ─╯\n"));
        }
        Shape::Band => {
            composer.push_str("π  gpt-5 · ~/work/app · main\n");
            for (index, row) in draft.iter().enumerate() {
                let gutter = if index == 0 { "╰─ " } else { "   " };
                composer.push_str(format!("{gutter}{row}").trim_end());
                composer.push('\n');
            }
        }
        Shape::Claude | Shape::Rule | Shape::Borderless => {
            if !matches!(shape, Shape::Borderless) {
                composer.push_str("─────────────────────────────── main ─\n");
            }
            for (index, row) in draft.iter().enumerate() {
                let gutter = if index == 0 { "❯ " } else { "  " };
                composer.push_str(format!("{gutter}{row}").trim_end());
                composer.push('\n');
            }
            if matches!(shape, Shape::Claude) {
                composer.push_str(&format!("{rule}\n"));
            }
        }
        Shape::Pi => {
            composer.push_str(&format!("{rule}\n"));
            for row in draft {
                composer.push_str(format!(" {row}").trim_end());
                composer.push('\n');
            }
            composer.push_str(&format!("{rule}\n"));
        }
        Shape::Field => {
            for row in draft {
                composer.push_str(&format!("▐ {row:<35}▌\n"));
            }
        }
        Shape::Rail => {
            for row in draft {
                composer.push_str(format!("▎ {row}").trim_end());
                composer.push('\n');
            }
        }
    }
    composer
}

#[test]
fn a_multi_line_draft_with_the_completion_list_open_is_read_whole() {
    // OMP opens its completion list under the editor's last row, whichever draft row the cursor
    // is on: for a command on a later row, and for a draft whose last row is empty.
    let cases: [(&[&str], [&str; 2], &str); 2] = [
        (
            &["first", "/mo"],
            [
                "❯ skill:model   First skill",
                "  skill:models  Second skill",
            ],
            "first\n/mo",
        ),
        (
            &["/mo", ""],
            ["❯ model       Switch model", "  models      List models"],
            "/mo",
        ),
    ];
    for shape in SHAPES {
        for (draft_rows, list, expected) in &cases {
            for statuses in [&[][..], &HOOK_STATUSES[..]] {
                let mut screen =
                    format!("Earlier reply.\n{}", multiline_composer(shape, draft_rows));
                for row in list.iter().chain(statuses) {
                    screen.push_str(row);
                    screen.push('\n');
                }
                assert_ready(
                    shape,
                    &format!("the draft {draft_rows:?} and its completion list"),
                    &screen,
                    expected,
                );
            }
        }
    }
}

#[test]
fn an_ascii_composer_under_a_unicode_output_block_is_not_read_as_the_block() {
    // A tool's output block drawn before a switch to ASCII symbols stays above an ASCII pi
    // composer, whose rules are `-` rows. The block is not the input, so its text is never the draft.
    let ascii_rule = "--------------------------------------";
    let screen = format!(
        "{TRANSCRIPT}\n{ascii_rule}\n {DRAFT}\n{ascii_rule}\n{}\n",
        status_bar_and_hooks().join("\n")
    );
    let ready = readiness(&screen).state == SessionChatComposerState::Ready;
    assert!(
        !ready || draft(&screen).as_deref() == Some(DRAFT),
        "an ASCII pi composer under a Unicode output block should not read the block as its draft:\n{screen}"
    );
}

/// The rows of a composer the ASCII symbol preset draws, for the shapes Ghostex reads with it (the
/// claude and rule shapes through their `❯ ` row), with no gap row above.
const ASCII_COMPOSERS: [(Shape, &str); 6] = [
    (Shape::Band, "π  gpt-5 · main\n╰─ parked draft\n"),
    (
        Shape::Claude,
        "-------------------------------- main -\n❯ parked draft\n--------------------------------------\n",
    ),
    (Shape::Borderless, "❯ parked draft\n"),
    (
        Shape::Rule,
        "-------------------------------- main -\n❯ parked draft\n",
    ),
    (Shape::Field, "▐ parked draft                      ▌\n"),
    (Shape::Rail, "▎ parked draft\n"),
];

/// OMP's Ask with its ASCII symbols.
const ASCII_ASK: &str = "+- Ask ---------------------------------+\n\
| Where should the build go?             |\n\
+----------------------------------------+\n\
| > ( ) staging                          |\n\
|   ( ) production                       |\n\
+----------------------------------------+\n\
| Enter select · Up/Down move · Esc cancel |\n\
+----------------------------------------+\n";

#[test]
fn an_ascii_frame_on_the_composer_holds_the_message() {
    let statuses = status_bar_and_hooks().join("\n");
    for (shape, composer) in ASCII_COMPOSERS {
        // A live Ask mounted on the parked draft, directly above the composer.
        let parked = format!("Earlier reply.\n\n{ASCII_ASK}{composer}{statuses}\n");
        assert_not_ready(shape, "an ASCII Ask on its parked draft", &parked);
        // An ASCII selector composited from the composer's first row; its title can hold a `+`.
        let screen = format!("Earlier reply.\n\n{composer}{statuses}\n");
        let top = "Earlier reply.\n\n".lines().count();
        for title in ["Deploy target", "C++ target", "A+B target"] {
            let mut overlay: Vec<String> = selector_overlay()
                .iter()
                .map(|row| {
                    row.replace(['╭', '╮', '╰', '╯', '├', '┤'], "+")
                        .replace(['─'], "-")
                        .replace('│', "|")
                })
                .collect();
            overlay[0] = format!("+- {title} {}+", "-".repeat(36 - title.len()));
            assert_not_ready(
                shape,
                &format!("an ASCII selector titled {title:?} composited over it"),
                &composite(&screen, &overlay, top, 20),
            );
        }
        // The same Ask left above the gap row is not on the composer.
        let answered =
            format!("Earlier reply.\n{ASCII_ASK} Answered: staging\n\n{composer}{statuses}\n");
        assert_ready(
            shape,
            "an answered ASCII Ask above it",
            &answered,
            "parked draft",
        );
    }
}

#[test]
fn an_ascii_frame_border_inside_a_multi_line_draft_is_text() {
    let draft_rows = ["first", "a +---+ table", "last"];
    for shape in SHAPES {
        let screen = format!(
            "Earlier reply.\n{}{}\n",
            multiline_composer(shape, &draft_rows),
            status_bar().join("\n")
        );
        assert_ready(
            shape,
            "a `+---+` in a draft row between its first and last rows",
            &screen,
            &draft_rows.join("\n"),
        );
    }
}
