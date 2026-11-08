use crate::json::opt;
use crate::rows::{row, section, Page, Section};

/// Settings › Voice Agent: the voice agent's provider with the key it needs right under it, its
/// voice, speed and call mode, and its memory. Dictation is its own page (dictation.rs).
///
/// CDXC:VoiceMode 2026-11-08 SEE-ALSO: the split from Q1269's single Voice page, Q1710 and Q1711 are in dictation.rs (CDXC:VoiceMode 2026-11-08). The page keeps the `voiceAgent` id, so deep links, `ghostex settings open --tab voiceAgent` and the help catalog keep working.
///
/// CDXC:VoiceMode 2026-10-25 DECISION:
/// User: "voice agent should be an extension so ppl can disable it's button and not see it top of the app if they want". Supersedes the 2026-10-20 "built into the app, not an extension": the voice agent is a built-in extension (official_extensions.rs), on by default. Since the 2026-11-08 split the keys Dictation needs and the Dictation history are on the Dictation page, so this page leaves the rail while the extension is off; its call rows (provider, voices, speed, call mode, memory) are the extension's `settingKeys`.
/// CDXC:VoiceMode 2026-10-20 DECISION:
/// User: "on the voice mode screen, the user should be able to click on Eve to change the voice, and click buttons to change the voice agent's speed, with options 1, 1.2, 1.4, 1.6, 1.8, and 2." `voiceAgentSpeed` (default 1) is that speed; the voice panel's speed menu, the phone's speed buttons and this row save the same setting.
/// CDXC:VoiceMode 2026-10-24 DECISION:
/// User, asked "Should Push-to-talk become the default call mode?", picked "No, keep Regular as the default" ("The voice agent is built for talking while you drive or walk, and that wants hands-free. Push-to-talk stays one tap away in the gear menu"). `voiceAgentCallMode` stays `regular` by default.
/// CDXC:VoiceMode 2026-10-20 SEE-ALSO:
/// gxserver reads `voiceAgentProvider` and the two voice keys (server/src/voice_agent/keys.rs, which seals the API keys in its vault), and answers `/api/readVoiceAgent` and `/api/setVoiceAgentKey`; the key rows are UI rows with no stored value. Client contract: docs/2026-10-20/voice-mode/voice-agent-client-contract.md.
pub(crate) fn page() -> Page {
    Page {
        id: "voiceAgent",
        title: "Voice Agent",
        sections: vec![provider(), memory()],
    }
}

pub(crate) fn provider() -> Section {
    section(
        "provider",
        "Provider and voice",
        vec![
            row("voiceAgentProvider", "Voice provider", "Which realtime voice model you talk to: xAI Grok Voice or OpenAI GPT Realtime. Both are billed by the provider while a conversation is open. It doesn't follow Dictation's engine: dictation can stay on Hex while calls use xAI or OpenAI.").options(&[opt("xAI Grok Voice", "xai"), opt("OpenAI GPT Realtime", "openai")]),
            row("voiceAgentXaiKey", "xAI API key", "Shown under the provider when it is xAI Grok Voice. Dictation uses the same key when its engine is xAI. Kept encrypted on this computer; your phone uses it through this computer and never stores it. With a 1Password account added in Settings > Secrets, paste an op://vault/item/field reference instead and the key is read from 1Password each time."),
            row("voiceAgentOpenaiKey", "OpenAI API key", "Shown under the provider when it is OpenAI GPT Realtime. Dictation uses the same key when its engine is OpenAI. Kept encrypted on this computer and never shown again after you save it. With a 1Password account added in Settings > Secrets, paste an op://vault/item/field reference instead."),
            row("voiceAgentXaiVoice", "Grok voice", "The voice Grok Voice speaks with.").options(&[
                opt("Eve", "eve"), opt("Ara", "ara"), opt("Rex", "rex"), opt("Sal", "sal"), opt("Leo", "leo"),
                opt("Altair", "altair"), opt("Atlas", "atlas"), opt("Aurora", "aurora"), opt("Carina", "carina"),
                opt("Castor", "castor"), opt("Celeste", "celeste"), opt("Cosmo", "cosmo"), opt("Helios", "helios"),
                opt("Helix", "helix"), opt("Iris", "iris"), opt("Kepler", "kepler"), opt("Liora", "liora"),
                opt("Lumen", "lumen"), opt("Luna", "luna"), opt("Lux", "lux"), opt("Naksh", "naksh"),
                opt("Orion", "orion"), opt("Perseus", "perseus"), opt("Rigel", "rigel"), opt("Sirius", "sirius"),
                opt("Ursa", "ursa"), opt("Zagan", "zagan"), opt("Zenith", "zenith"),
            ]),
            row("voiceAgentOpenaiVoice", "OpenAI voice", "The voice GPT Realtime speaks with. Marin and Cedar sound the most natural.").options(&[
                opt("Marin", "marin"), opt("Cedar", "cedar"), opt("Alloy", "alloy"), opt("Ash", "ash"),
                opt("Ballad", "ballad"), opt("Coral", "coral"), opt("Echo", "echo"), opt("Sage", "sage"),
                opt("Shimmer", "shimmer"), opt("Verse", "verse"),
            ]),
            row("voiceAgentSpeed", "Speaking speed", "How fast the voice agent talks, from normal speed to twice as fast. The voice keeps its pitch, and the speed menu next to the voice in the voice panel changes it too, even mid-sentence.").options(&[
                opt("1×", "1"), opt("1.2×", "1.2"), opt("1.4×", "1.4"), opt("1.6×", "1.6"), opt("1.8×", "1.8"), opt("2×", "2"),
            ]),
            row("voiceAgentCallMode", "Call mode", "Regular calls listen hands-free; on Grok they're billed for the whole time the call is connected, quiet and muted time included (about $0.08 a minute). Push-to-talk calls listen only while you hold the talk button next to Mute (or Space, or the Hold to Talk hotkey); on Grok you pay only for the audio you send and the agent's replies. In a muted Regular call, holding it unmutes you for as long as you hold. GPT Realtime bills only audio in both modes. A change applies to your next call; the gear in the voice panel changes it too.").options(&[
                opt("Regular", "regular"), opt("Push-to-talk", "pushToTalk"),
            ]),
        ],
    )
}

/// CDXC:VoiceMode 2026-10-20 SEE-ALSO:
/// The remembered last conversation is kept by gxserver (server/src/voice_agent/memory.rs); this UI row shows it and forgets it through `/api/forgetVoiceAgentMemory`, like the voice panel's Forget.
pub(crate) fn memory() -> Section {
    section(
        "memory",
        "Memory",
        vec![
            row("voiceAgentMemory", "Last conversation", "A short summary of your last voice conversation, kept on this computer until the next conversation replaces it. Press Continue in the voice panel, or say \"pick up where we left off\", to carry on from it. Forget deletes it."),
        ],
    )
}
