pub(crate) mod native_chat {
    /// The picture measuring of `native_chat/image_viewer/render.rs`; the real file needs the chat.
    pub(crate) mod image_viewer {
        pub(crate) mod render {
            pub(crate) fn header_size(picture: &gpui::Image) -> Option<gpui::Size<gpui::Pixels>> {
                let reader =
                    image::ImageReader::new(std::io::Cursor::new(picture.bytes.as_slice()))
                        .with_guessed_format()
                        .ok()?;
                let (width, height) = reader.into_dimensions().ok()?;
                (width > 0 && height > 0)
                    .then(|| gpui::size(gpui::px(width as f32), gpui::px(height as f32)))
            }
        }
    }
    pub(crate) mod fonts {
        pub(crate) const CHAT_MONO: &str = "JetBrainsMono Nerd Font";
    }
    pub(crate) mod markdown_style {
        use gpui_component::highlighter::HighlightTheme;
        use std::sync::{Arc, LazyLock};

        #[derive(serde::Deserialize)]
        struct CodeThemes {
            dark: HighlightTheme,
            light: HighlightTheme,
        }

        static THEMES: LazyLock<(Arc<HighlightTheme>, Arc<HighlightTheme>)> = LazyLock::new(|| {
            let themes: CodeThemes = serde_json::from_str(include_str!(
                "../../../../../packages/gx-chat-core/visual/code-theme.json"
            ))
            .expect("shared code theme");
            (Arc::new(themes.dark), Arc::new(themes.light))
        });

        pub(crate) fn highlight_theme(light: bool) -> Arc<HighlightTheme> {
            if light {
                THEMES.1.clone()
            } else {
                THEMES.0.clone()
            }
        }
    }
}
#[path = "helpers.rs"]
pub(crate) mod helpers;
#[path = "native_docs.rs"]
pub(crate) mod native_docs;
#[path = "window.rs"]
pub(crate) mod window;
