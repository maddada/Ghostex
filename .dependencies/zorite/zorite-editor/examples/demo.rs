//! Standalone demo for the `zorite-editor` crate.
//!
//! Run with: `cargo run -p zorite-editor --example demo`.
//!
//! Wires the editor to the real OS spell checker (M6): misspelled words get red
//! squiggles, and right-clicking one offers the system's suggestions. Type to
//! watch the squiggles update live — the editor emits [`EditorEvent::Changed`]
//! on each edit, and we re-run the checker in response.

use gpui::{
    App, AppContext, Bounds, Context, Entity, Focusable, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement,
    Styled, Subscription, Window, WindowBounds, WindowOptions, actions, div, font, hsla, px, rgb,
    size,
};
use os_spellcheck::SpellChecker;
use zorite_editor::{Diagnostic, EditorEvent, EditorState, SyntaxStyle};

actions!(demo, [Quit]);

struct Demo {
    editor: Entity<EditorState>,
    /// Held so the change subscription keeps firing for the window's lifetime.
    _spell_sub: Subscription,
    /// Lets the seeded content (taller than the window) scroll.
    scroll: ScrollHandle,
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x1e1e22))
            .text_color(rgb(0xe6e6e6))
            .text_size(px(16.))
            .child(
                div()
                    .id("demo-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .p(px(28.))
                    .child(
                        div()
                            .bg(rgb(0x111114))
                            .border_1()
                            .border_color(rgb(0x333338))
                            .rounded(px(8.))
                            .p(px(14.))
                            .child(self.editor.clone()),
                    ),
            )
    }
}

/// Run the OS spell checker over `text` and turn the misspellings into editor
/// diagnostics (the byte ranges + suggestions map across one-to-one).
/// A dark-theme palette for the live-preview markdown styling.
fn demo_markdown_style() -> SyntaxStyle {
    SyntaxStyle {
        block_label: None,
        block_label_gen: 0,
        block_ref_count: None,
        marker: hsla(0., 0., 0.5, 0.55), // dimmed gray syntax markers
        code: hsla(0.09, 0.6, 0.72, 1.), // warm inline code text
        code_bg: hsla(0., 0., 1., 0.06), // faint code chip background
        link: hsla(0.58, 0.75, 0.66, 1.), // blue links / wiki-links
        tag: hsla(0.33, 0.45, 0.62, 1.), // green tags
        quote: hsla(0., 0., 0.6, 1.),    // muted blockquote text/border
        alert_note: hsla(0.58, 0.9, 0.62, 1.), // GitHub alert blues/greens…
        alert_tip: hsla(0.36, 0.5, 0.48, 1.),
        alert_important: hsla(0.74, 0.85, 0.73, 1.),
        alert_warning: hsla(0.12, 0.7, 0.48, 1.),
        alert_caution: hsla(0.01, 0.9, 0.63, 1.),
        alert_icons: None,
        rule: hsla(0., 0., 1., 0.18),                // `---` divider
        mark_bg: hsla(0.13, 1., 0.5, 0.4),           // yellow <mark> highlight
        popover_bg: hsla(0., 0., 0.16, 1.),          // dark menu surface
        popover_border: hsla(0., 0., 0.28, 1.),      // menu border
        popover_fg: hsla(0., 0., 0.9, 1.),           // menu text
        popover_hover: hsla(0.58, 0.75, 0.66, 0.16), // soft accent tint
        popover_divider: hsla(0., 0., 1., 0.18),     // group divider
        popover_danger: gpui::rgb(0xE5484D).into(),  // destructive rows
        mono: font("Menlo"),
        property_icon: None,
    }
}

fn diagnostics_for(text: &str) -> Vec<Diagnostic> {
    SpellChecker::new()
        .check(text)
        .into_iter()
        .map(|range| Diagnostic { range })
        .collect()
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        zorite_editor::bind_keys(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx: &mut App| cx.quit());

        let bounds = Bounds::centered(None, size(px(760.), px(520.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let text = "# Heading 1\n## Heading 2\n\nHeadings get bigger as you type (W2). \
                            **Bold**, *italic*, ~~strike~~, `inline code`, a \
                            [link](https://example.com), a [[Wiki Page]], a #tag and a \
                            namespaced #area/sub tag, plus a bare https://example.com/auto \
                            autolink.\n\n> [!NOTE] GitHub alerts render with a colored bar \
                            and label\n> across their lines.\n\n> [!WARNING]\n> The classic \
                            two-line form works too.\n\nA fenced code block (W4b):\n\n\
                            ```rust\nfn main() {\n    \
                            println!(\"hello, world\");\n}\n```\n\nA table (W4c):\n\n| Name | \
                            Role | Score |\n| :-- | :--: | --: |\n| Ada | Engineer | 99 |\n\
                            | Linus | Kernel | 88 |\n\n> A blockquote, *muted* with a left \
                            border.\n\n- First bullet\n- Second bullet\n  - Nested bullet\n\n\
                            1. First step\n2. Second step\n\n- [x] Done task\n- [ ] Pending \
                            task\n\n![](docs/report.pdf)\n\n---\n\nA footnote reference[^1], a \
                            [reference link][ref], and <mark>highlighted</mark> text.\n\n\
                            [^1]: The footnote definition, shown muted.\n\
                            [ref]: https://example.com\n\nSpell-check still flags mispelled \
                            wrds; right-click one for suggestions.\n\nStriped:\n\
                            <!-- table:striped -->\n| Name | Role | Score |\n| :-- | :--: | --: |\n\
                            | Ada | Engineer | 99 |\n| Linus | Kernel | 88 |\n\
                            | Grace | Compiler | 95 |\n\nHeader:\n<!-- table:header -->\n\
                            | Name | Role |\n| :-- | :-- |\n| Ada | Engineer |\n| Linus | Kernel |\
                            \n\nMinimal:\n<!-- table:minimal -->\n| Name | Role |\n| :-- | :-- |\n\
                            | Ada | Engineer |\n| Linus | Kernel |";
                let editor = cx.new(|cx| {
                    EditorState::new(window, cx)
                        .with_placeholder("Type here…")
                        .with_text(text)
                });
                window.focus(&editor.read(cx).focus_handle(cx), cx);

                // Lazy suggestion provider (consulted on right-click) + an
                // initial detection pass over the seeded text.
                editor.update(cx, |editor, cx| {
                    editor.on_suggest(|word| SpellChecker::new().suggestions(word));
                    editor.set_markdown_style(demo_markdown_style(), cx);
                    // A toy syntax highlighter, to demo the hook: real hosts
                    // plug in an engine (Zorite passes gpui-component's
                    // tree-sitter highlighter) — the editor only wants
                    // `(lang, code) -> sorted styled ranges`.
                    editor.set_code_highlighter(|_lang, code| {
                        let mut out = Vec::new();
                        for kw in ["fn", "let", "println!"] {
                            let mut from = 0;
                            while let Some(i) = code[from..].find(kw) {
                                let at = from + i;
                                out.push((
                                    at..at + kw.len(),
                                    gpui::HighlightStyle {
                                        color: Some(hsla(0.83, 0.6, 0.7, 1.)),
                                        ..Default::default()
                                    },
                                ));
                                from = at + kw.len();
                            }
                        }
                        out.sort_by_key(|(r, _)| r.start);
                        out
                    });
                    // Treat a `![](*.pdf)` as a clickable chip (label = file name).
                    editor.set_block_chip_provider(|src| {
                        src.ends_with(".pdf")
                            .then(|| SharedString::from(src.rsplit('/').next().unwrap_or(src)))
                    });
                    editor.set_diagnostics(diagnostics_for(text), cx);
                });

                // Re-check on every edit; log chip opens.
                let editor_handle = editor.clone();
                cx.new(|cx| {
                    let _spell_sub = cx.subscribe(
                        &editor_handle,
                        |_demo: &mut Demo, editor, event: &EditorEvent, cx| {
                            if let EditorEvent::OpenLink(src) = event {
                                eprintln!("open link: {src}");
                                return;
                            }
                            let text = editor.read(cx).text().to_string();
                            let diagnostics = diagnostics_for(&text);
                            editor.update(cx, |editor, cx| editor.set_diagnostics(diagnostics, cx));
                        },
                    );
                    Demo {
                        editor,
                        _spell_sub,
                        scroll: ScrollHandle::new(),
                    }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
