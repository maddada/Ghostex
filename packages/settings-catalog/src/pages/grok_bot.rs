use crate::rows::{row, section, Page, Section};

/// Settings › Grok Bot: the Grok Bot Integration built-in extension's own page, right below
/// Integrations. Every row is a UI row with no stored value except the switch: gxserver keeps the
/// connection (server/src/grokbot/), and the page walks the user through the setup.
///
/// CDXC:GrokBot 2026-11-19 SEE-ALSO:
/// apps/desktop/src/app/window/settings_modal/tabs/grok_bot.rs draws these rows (the checklist of
/// Q4206 = A); `ghostex grokbot` does the same from a terminal.
pub(crate) fn page() -> Page {
    Page {
        id: "grokBot",
        title: "Grok Bot",
        sections: vec![switch(), setup(), connection()],
    }
}

pub(crate) fn switch() -> Section {
    section(
        "switch",
        "Grok Bot Integration",
        vec![row("grokBotHidden", "Turn off Grok Bot Integration", "Turn off Grok Bot Integration: Ghostex stops sending your agents' notes to Grok Bot, refuses connect and test, and stops telling agents about it; the saved connection is kept. It pauses notes only: the bot can still run ghostex commands on this computer through the Grok Bot desktop app until you set Execution on this computer to Never allow in Grok Bot. On by default.")],
    )
}

pub(crate) fn setup() -> Section {
    section(
        "setup",
        "Set up",
        vec![
            row("grokBotDesktopApp", "Grok Bot desktop app", "Install the Grok Bot desktop app from x.ai/bot, sign in and leave it running: the bot runs Ghostex's commands on this computer through it. Ghostex ticks this off when it finds the app."),
            row("grokBotMakeBot", "Add the Ghostex bot", "Add the ready-made Ghostex bot to Grok Bot (Add the Ghostex bot to Grok Bot opens grok-bot.directory/bots/ghostex), or make a bot named Ghostex and paste the instructions (Copy instructions). Send it any message; when it asks to run ghostex grokbot brief, choose Allow once. Ghostex ticks this off when the bot reaches this computer."),
            row("grokBotWebhook", "Paste the webhook", "The bot makes a routine called Ghostex reports in its first reply. Open it in the Grok Bot desktop app and paste its POST to address and its key here. Ghostex keeps the key in an owner-only file and never shows it again."),
            row("grokBotTestNote", "Check the test note", "Ghostex sends the bot a test note; say whether it showed up in the bot's chat."),
        ],
    )
}

pub(crate) fn connection() -> Section {
    section(
        "connection",
        "Connection",
        vec![
            row("grokBotConnection", "Connected", "How many notes your agents sent to Grok Bot and when the last one went, with Send a test note, Change (a new webhook address and key), Copy instructions and Disconnect."),
        ],
    )
}
