//! Dictate's microphone: the input devices Settings › Voice's picker offers, and the one a
//! recording opens (`dictateMicrophone`, empty for the system default).
//!
//! CDXC:VoiceMode 2026-11-05 DECISION: User (Q1264) picked a microphone picker among Dictate's
//! extras. The choice belongs to this computer (the phone records with its own microphone), is
//! saved by device name on every system (CoreAudio's name on macOS, cpal's on Windows and Linux),
//! and a device that is not connected falls back to the default instead of failing the recording.
//! The voice agent's calls keep the default device, which their echo canceller is paired with.

pub(crate) const MICROPHONE_SETTING: &str = "dictateMicrophone";

/// The picked microphone's name, or `None` for the system default.
pub(crate) fn chosen() -> Option<String> {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get(MICROPHONE_SETTING)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// Every input device's name and the default one's. Blocking (it asks the sound system).
pub(crate) fn input_names() -> (Vec<String>, Option<String>) {
    #[cfg(target_os = "macos")]
    {
        super::audio_mac::input_names()
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        super::audio_cpal::input_names()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        (Vec::new(), None)
    }
}

impl crate::GhostexGpuiApp {
    /// Settings › Voice asked for the microphones and whether macOS takes Ctrl+Alt+Space:
    /// answered as `dictateDevices`.
    pub(crate) fn answer_dictate_devices(&mut self, cx: &mut gpui::Context<Self>) {
        let listed = cx.background_executor().spawn(async {
            let (microphones, default) = input_names();
            let conflict = super::dictate_hotkey::macos_input_source_conflict();
            (microphones, default, conflict)
        });
        cx.spawn(async move |this, cx| {
            let (microphones, default, conflict) = listed.await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(
                    serde_json::json!({
                        "type": "dictateDevices",
                        "microphones": microphones,
                        "defaultMicrophone": default,
                        "inputSourceConflict": conflict,
                    }),
                    cx,
                );
            });
        })
        .detach();
    }

    /// Show in Finder (Explorer, the file manager) for this computer's speech models, the folder
    /// gxserver keeps them in (`server/src/voice_agent/speech/models.rs`).
    pub(crate) fn reveal_dictate_models_folder(&mut self, cx: &mut gpui::Context<Self>) {
        let folder = ghostex_paths::GhostexPaths::resolve()
            .gxserver_data_dir()
            .join("speech-models");
        let opened = std::fs::create_dir_all(&folder)
            .map_err(|error| error.to_string())
            .and_then(|_| crate::app::helpers::gpui_open_path(&folder));
        if let Err(message) = opened {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Could not open the models folder",
                &message,
                cx,
            );
        }
    }
}
