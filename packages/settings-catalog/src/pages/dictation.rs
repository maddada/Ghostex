use crate::json::opt;
use crate::rows::{row, section, Page, Section};

/// Settings › Dictation: speech to text in the chat box, with its engine (Hex, Superwhisper, OpenAI or xAI) and
/// the key a cloud engine needs right under it, and the Dictation history.
///
/// CDXC:VoiceMode 2026-11-08 DECISION:
/// User: "we need to call Voice page in settings "Dictation" and have "Voice Agent" as a separate page" (Q1710: Voice Agent sits right under Dictation in the rail; Q1711 "Key where it is needed": each page shows the key its current pick needs right under the picker, both pages save the same stored key and say the other uses it too, and Hex needs none). Supersedes Q1269's single Voice page (CDXC:VoiceMode 2026-11-05 in voice_agent.rs). Dictation takes the new id `dictation`; `voice` opens it too, and `voiceAgent` opens the Voice Agent page. Dictate's engine and language belong to the computer whose gxserver transcribes (server/src/voice_agent/speech/settings.rs), so their rows are UI rows with no stored value; the hotkey mode, the microphone and Insert with spacing are this app's (`dictateHotkeyMode`, `dictateMicrophone`, `dictateInsertSpacing`).
pub(crate) fn page() -> Page {
    Page {
        id: "dictation",
        title: "Dictation",
        sections: vec![dictate(), dictations()],
    }
}

/// Dictate's engine, the key a cloud engine needs, how it records and how the text goes in. The
/// engine and language are gxserver's (`/api/readDictateSettings`), per computer. Insert with
/// spacing sits last here since the Clean-up section went (CDXC:Dictate 2026-11-10 in
/// server/src/voice_agent/transcribe.rs).
pub(crate) fn dictate() -> Section {
    section(
        "dictate",
        "Speech to text",
        vec![
            row("dictateEngine", "Engine", "Which service turns your recording into text: Hex, a free dictation app that runs on this computer with no key and works offline (macOS on Apple silicon, Windows, Linux), Superwhisper when it is installed on a Linux computer (Omarchy comes with it), or OpenAI or xAI with your API key. Hex is recommended; Set up Hex installs it and walks you through it. A new install picks your saved key (xAI, then OpenAI) and Hex when there is no key; after that it stays on what you pick. Each computer has its own engine, and the phone changes it per computer."),
            row("dictateOpenaiKey", "OpenAI API key", "Shown under the engine when it is OpenAI: paste your key from platform.openai.com. It is the same key the Voice Agent uses on GPT Realtime, kept encrypted on this computer."),
            row("dictateXaiKey", "xAI API key", "Shown under the engine when it is xAI: paste your key from console.x.ai. It is the same key the Voice Agent uses on Grok Voice, kept encrypted on this computer."),
            row("dictateLanguage", "Language", "Auto works for most people. Pick one when short phrases come out in the wrong language."),
            row("dictateHotkey", "Hotkey", "Dictate into the chat box you are in. Change it in Hotkeys."),
            row("dictateHotkeyMode", "Hotkey mode", "Auto: tap to start and tap again to stop, or hold for 300 ms or more and it stops when you let go. Toggle: every press starts or stops. Hold: it records only while you hold the keys. Escape cancels.").options(&[
                opt("Auto", "auto"), opt("Toggle", "toggle"), opt("Hold", "hold"),
            ]),
            row("dictateMicrophone", "Microphone", "The microphone this computer records dictation from. System default follows the input device chosen in your sound settings."),
            row("dictateInsertSpacing", "Insert with spacing", "Adds a space before or after the text when the characters next to the caret need one."),
        ],
    )
}

/// CDXC:VoiceMode 2026-10-20 SEE-ALSO:
/// Dictate's recordings are kept by gxserver (server/src/voice_agent/recordings.rs); this UI row is the page's Dictation history section (`/api/listDictations`), with no stored value.
pub(crate) fn dictations() -> Section {
    section(
        "dictations",
        "Past recordings",
        vec![
            row("voiceAgentDictationHistory", "Dictation history", "Every recording you made with the dictation microphone, newest first, with its text and the engine that made it. Play, download or delete a recording, copy its text, recover one you cancelled, or transcribe one that failed (the text is copied to your clipboard too). Recordings stay on this computer until you delete them."),
        ],
    )
}
