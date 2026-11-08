//! Settings › Dictation's rows about our retired local engine and the key a cloud engine needs:
//! the one-time notice for a computer that used the local engine (`localRetired`), the old
//! models it can delete (`oldModels`), and the OpenAI or xAI key right under the engine picker.
//! SEE-ALSO: server/src/voice_agent/speech/retired.rs (CDXC:Dictate 2026-11-08), the Voice Agent
//! page's key row in voice_agent.rs.

use super::super::voice_agent::Provider;
use super::*;

impl VoiceAgentTab {
    /// "Ghostex now dictates with Hex": the notice in full and Got it; the Hex row under the
    /// engine has Install Hex and the steps.
    pub(super) fn local_retired_row(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let answer = self.dictate.answer.as_ref()?;
        let message = answer["localRetired"]["message"].as_str()?.to_string();
        let got_it = settings_button(
            p,
            "dictate-retired-got-it",
            "Got it",
            None,
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, _window, cx| {
                if let Some(answer) = page.dictate.answer.as_mut() {
                    answer["localRetired"] = Value::Null;
                }
                page.set_dictate(json!({ "localRetiredNotice": false }), cx);
            },
            cx,
        );
        Some(setting_row(
            p,
            "dictateLocalRetired",
            RowSpec::new("Ghostex now dictates with Hex").wide(),
            None,
            note_with_controls(p, message, vec![got_it]),
            cx,
        ))
    }

    /// The models the retired engine downloaded, with their size and Delete.
    pub(super) fn old_models_row(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let old = &self.dictate.answer.as_ref()?["oldModels"];
        let bytes = old["bytes"].as_u64()?;
        let (title, subtitle, _) = Self::catalog_row(OLD_MODELS).unwrap_or_default();
        let place = if self.dictate.machine.is_some() {
            "that computer"
        } else {
            "this computer"
        };
        let note = format!(
            "{} on {place}. Ghostex no longer uses them.",
            size_label(bytes)
        );
        let control = settings_button(
            p,
            "dictate-old-models-delete",
            if self.dictate.old_models_busy {
                "Deleting…"
            } else {
                "Delete"
            },
            None,
            ButtonVariant::Outline,
            self.dictate.old_models_busy,
            None,
            |page: &mut Self, _window, cx| page.delete_old_models(cx),
            cx,
        );
        Some(setting_row(
            p,
            OLD_MODELS,
            RowSpec::new(title).description(subtitle).wide(),
            None,
            note_with_controls(p, note, vec![control]),
            cx,
        ))
    }

    fn delete_old_models(&mut self, cx: &mut Context<Self>) {
        if self.dictate.old_models_busy {
            return;
        }
        self.dictate.old_models_busy = true;
        self.dictate.action_error = None;
        let machine = self.picked_machine(cx);
        let page = cx.entity().downgrade();
        store_machine_gxserver_rpc(
            &self.store,
            machine.as_deref(),
            "/api/deleteOldSpeechModels",
            json!({}),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.dictate.old_models_busy = false;
                    match result {
                        Ok(answer) => page.take_dictate_answer(answer, cx),
                        Err(error) => page.dictate.action_error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

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
