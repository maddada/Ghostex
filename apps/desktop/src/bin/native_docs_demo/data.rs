//! The demo's view for a file that is not Markdown: a CSV/TSV table, a JSON tree, or the code
//! editor (with Format or Minify applied, or a grammar to colour with).
//!
//!     GHOSTEX_NATIVE_DOCS_DEMO_MODE=table|tree|raw|format|minify   (default: table for CSV/TSV, raw otherwise)
//!     GHOSTEX_NATIVE_DOCS_DEMO_LANG=<editor language>              (raw: e.g. dockerfile, powershell, hcl, json)
//!     GHOSTEX_NATIVE_DOCS_DEMO_SELECT=<row>,<col>                  (table: a selected cell)
//!     GHOSTEX_NATIVE_DOCS_DEMO_SEARCH=<text>                       (tree: typed into its search box)

use std::path::Path;
use std::rc::Rc;

use gpui::{
    App, AppContext as _, Bounds, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, WindowBounds, WindowOptions,
    div, point, px, size,
};
use gpui_component::Root;
use gpui_component::input::{Editor, EditorState};
use gpui_component::table::TableState;

use crate::app::native_docs::data_table::{self, DocsDataTable};
use crate::app::native_docs::json_view::{self, DocsJsonTree};
use crate::app::native_docs::{grammars, palette::DocsPalette};
use crate::{assets, env, register_mono, ui_fonts};

enum Body {
    Table(Entity<TableState<DocsDataTable>>, Option<String>),
    Tree(Entity<DocsJsonTree>),
    Code(Entity<EditorState>),
}

impl Body {
    fn clone_code(&self) -> Option<Entity<EditorState>> {
        match self {
            Body::Code(editor) => Some(editor.clone()),
            _ => None,
        }
    }
}

struct DataDemo {
    body: Body,
    palette: DocsPalette,
    title: String,
}

impl Render for DataDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette.clone();
        let copy: Rc<dyn Fn(String, &mut App)> =
            Rc::new(|text, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)));
        let body = match &self.body {
            Body::Table(table, notice) => div()
                .size_full()
                .flex()
                .flex_col()
                .children(
                    notice
                        .clone()
                        .map(|notice| data_table::render_table_notice(notice, &p)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .child(data_table::render_data_table(table, &p, copy, cx)),
                )
                .into_any_element(),
            Body::Tree(tree) => {
                tree.update(cx, |tree, _| tree.palette = p.clone());
                div().size_full().child(tree.clone()).into_any_element()
            }
            Body::Code(editor) => div()
                .size_full()
                // The Files view's own Option+Up / Option+Down handler (`native_docs_key_down`), on
                // the ancestor of the focused editor as in the app.
                .on_key_down({
                    let editor = editor.clone();
                    move |event, window, cx| {
                        if let Some(down) =
                            crate::app::native_docs::move_lines::move_lines_key(event)
                        {
                            editor.update(cx, |state, cx| {
                                crate::app::native_docs::move_lines::move_lines_in_code(
                                    state, down, window, cx,
                                )
                            });
                            cx.stop_propagation();
                        }
                    }
                })
                .font_family(p.mono_font.clone())
                .text_size(px(12.0))
                .child(
                    Editor::new(editor)
                        .appearance(false)
                        .h_full()
                        .font_family(p.mono_font.clone())
                        .text_sm(),
                )
                .into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.page)
            .font_family(p.font.clone())
            .text_color(p.text)
            .child(
                div()
                    .flex_none()
                    .h(px(35.0))
                    .pl(px(13.0))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(p.border)
                    .bg(p.chrome)
                    .text_size(px(12.0))
                    .child(SharedString::from(self.title.clone())),
            )
            .child(div().relative().flex_1().min_h_0().child(body))
    }
}

pub(crate) fn run(file: &Path, text: String, light: bool, width: f32, height: f32) {
    let path = file.to_string_lossy().to_string();
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let delimiter = data_table::delimiter_for(&path, &text);
    let mode = match env("GHOSTEX_NATIVE_DOCS_DEMO_MODE") {
        mode if !mode.is_empty() => mode,
        _ if delimiter.is_some() => "table".to_string(),
        _ => "raw".to_string(),
    };
    gpui_platform::application()
        .with_assets(assets::GhostexAssets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            grammars::register();
            gpui_component::Theme::change(
                if light {
                    gpui_component::ThemeMode::Light
                } else {
                    gpui_component::ThemeMode::Dark
                },
                None,
                cx,
            );
            data_table::apply_table_theme(gpui_component::Theme::global_mut(cx), light);
            ui_fonts::register(cx);
            register_mono(cx);
            let window_size = size(px(width), px(height));
            let bounds = cx
                .primary_display()
                .map(|display| Bounds::centered_at(display.bounds().center(), window_size))
                .unwrap_or_else(|| Bounds::new(point(px(120.0), px(80.0)), window_size));
            let background = env("GHOSTEX_NATIVE_MODAL_DEMO_BACKGROUND") == "1";
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                focus: !background,
                show: true,
                ..Default::default()
            };
            let window = cx
                .open_window(options, move |window, cx| {
                    window.set_window_title("Native Docs demo");
                    let palette = DocsPalette::resolve(
                        false,
                        light,
                        ui_fonts::UI_FONT.to_string(),
                        gpui::rgb(if light { 0xffffff } else { 0x0e0e0e }).into(),
                    );
                    let copy: Rc<dyn Fn(String, &mut App)> = Rc::new(|text, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text))
                    });
                    let (body, title) = match (mode.as_str(), delimiter) {
                        ("table", Some(delimiter)) => {
                            let read = data_table::read_delimited(&text, delimiter);
                            let notice = data_table::delimited_notice(&read);
                            let table =
                                data_table::new_data_table(read.headers, read.rows, window, cx);
                            if let Some((row, col)) = env("GHOSTEX_NATIVE_DOCS_DEMO_SELECT")
                                .split_once(',')
                                .and_then(|(r, c)| Some((r.parse().ok()?, c.parse().ok()?)))
                            {
                                table.update(cx, |table, cx| table.set_selected_cell(row, col, cx));
                            }
                            (Body::Table(table, notice), format!("{name}  ·  Table"))
                        }
                        ("tree", _) => {
                            let palette = palette.clone();
                            let tree =
                                cx.new(|cx| DocsJsonTree::new(&text, palette, copy, window, cx));
                            let search = env("GHOSTEX_NATIVE_DOCS_DEMO_SEARCH");
                            if !search.is_empty() {
                                tree.update(cx, |tree, cx| tree.search_for(&search, window, cx));
                            }
                            (Body::Tree(tree), format!("{name}  ·  Tree"))
                        }
                        (mode, _) => {
                            let shown = match mode {
                                "format" | "minify" => json_view::reformat(&text, mode == "format")
                                    .unwrap_or_else(|error| error),
                                _ => text.clone(),
                            };
                            let language = match env("GHOSTEX_NATIVE_DOCS_DEMO_LANG") {
                                language if !language.is_empty() => language,
                                _ => "text".to_string(),
                            };
                            let title = format!("{name}  ·  {mode}  ·  {language}");
                            let editor = cx.new(|cx| {
                                EditorState::new(window, cx)
                                    .language(language)
                                    .line_number(true)
                                    .default_value(shown)
                            });
                            (Body::Code(editor), title)
                        }
                    };
                    let view = cx.new(|_| DataDemo {
                        body,
                        palette,
                        title,
                    });
                    if !background {
                        window.activate_window();
                    }
                    // The code editor starts focused so keystrokes reach it.
                    if let Some(editor) = view.read(cx).body.clone_code() {
                        editor.update(cx, |editor, cx| editor.focus(window, cx));
                    }
                    cx.new(|cx| Root::new(view, window, cx))
                })
                .expect("open the demo window");
            // Keystrokes through the keymap, as a keyboard would send them.
            let keys = env("GHOSTEX_NATIVE_DOCS_DEMO_KEYS");
            if !keys.is_empty() {
                let handle: gpui::AnyWindowHandle = window.into();
                let os = cx.platform_info().os;
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(3))
                        .await;
                    for key in keys.split_whitespace() {
                        let Ok(keystroke) = gpui::Keystroke::parse_with_os(key, os) else {
                            continue;
                        };
                        let _ = cx.update_window(handle, |_, window, cx| {
                            window.dispatch_keystroke(keystroke, cx);
                        });
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(120))
                            .await;
                    }
                })
                .detach();
            }
            cx.activate(!background);
        });
}
