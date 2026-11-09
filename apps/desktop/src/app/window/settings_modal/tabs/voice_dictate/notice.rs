//! Settings › Dictation's rows about the key a cloud engine needs: the OpenAI or xAI key right
//! under the engine picker, and where a remote computer's missing key is added.
//! SEE-ALSO: the Voice Agent page's key row in voice_agent.rs.

use super::super::voice_agent::Provider;
use super::*;

impl VoiceAgentTab {
    /// A remote computer's OpenAI or xAI engine without its key: the keys are that computer's own,
    /// so this computer can't take one; the row says where to add it (it used to be a tooltip on
    /// the engine row, so nothing showed).
    pub(super) fn remote_key_missing_row(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.dictate.machine.as_ref()?;
        let answer = self.dictate.answer.as_ref()?;
        let name = match answer["settings"]["engine"].as_str()? {
            "openai" => "OpenAI",
            "xai" => "xAI",
            _ => return None,
        };
        let code = answer["engine"]["problem"]["code"].as_str()?;
        if !matches!(code, "noKey" | "keyUnreadable") {
            return None;
        }
        let title = if code == "noKey" {
            format!("{name} needs an API key")
        } else {
            format!("{name}'s saved key can't be read")
        };
        Some(setting_row(
            p,
            "dictateRemoteKey",
            RowSpec::new(title).wide(),
            None,
            note_with_controls(
                p,
                "API keys stay on the computer that uses them. Add it in Settings › Dictation on that computer.",
                Vec::new(),
            ),
            cx,
        ))
    }

    /// The key the picked cloud engine needs, right under the picker (Q1711). Keys are this
    /// computer's, so a remote computer's engine says where to add it instead (`engine_row`).
    pub(super) fn engine_key_row(
        &mut self,
        p: &SettingsPalette,
        visible: &dyn Fn(&str) -> bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.dictate.machine.is_some() {
            return None;
        }
        let engine = self.dictate.answer.as_ref()?["settings"]["engine"].clone();
        let (provider, row_key) = match engine.as_str()? {
            "openai" => (Provider::OpenAi, OPENAI_KEY),
            "xai" => (Provider::Xai, XAI_KEY),
            _ => return None,
        };
        if !visible(row_key) && !visible(ENGINE) {
            return None;
        }
        let description = match provider {
            Provider::OpenAi => {
                "Get one at platform.openai.com under API keys. The Voice Agent uses this key too, on GPT Realtime."
            }
            Provider::Xai => {
                "Get one at console.x.ai under API Keys. The Voice Agent uses this key too, on Grok Voice."
            }
        };
        self.key_row(p, provider, row_key, description, window, cx)
    }
}

/// A wide row's body: a muted sentence (rows show no description text, CDXC:Settings 2026-09-09
/// in fields/row.rs, so what the user must read sits here) over its buttons.
pub(super) fn note_with_controls(
    p: &SettingsPalette,
    note: impl Into<SharedString>,
    controls: Vec<AnyElement>,
) -> AnyElement {
    v_flex()
        .w_full()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(18.5))
                .text_color(hsla(p.muted))
                .child(note.into()),
        )
        .when(!controls.is_empty(), |this| {
            this.child(h_flex().gap(px(8.0)).items_center().children(controls))
        })
        .into_any_element()
}
