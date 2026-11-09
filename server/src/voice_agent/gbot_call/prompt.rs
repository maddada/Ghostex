//! What the model is told in a call with a GBot: to be that GBot's voice, to hand everything the
//! user asks to the GBot's chat (`talk_to_bot`), to say what the chat is doing while it works and
//! the chat's reply when it is done, and to ask for approvals out loud without ever taking a
//! spoken yes for a Ghostex approval.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (M14 brief): "talk to it and hear it answer in its own voice, it says what it is doing while it works, and it asks for approvals out loud (the answer still goes through the normal approval path; never accept a spoken "yes" as Touch ID)". The model is only the GBot's voice: what the GBot knows and does is its chat's, so every request goes to the chat and the call's turns are in the chat like any message. An agent's own permission prompt in the chat is answered with `answer_prompt` once the user chose, the same answer the card sends; a Ghostex approval (the Touch ID kind) has no tool here at all, so the model can only tell the user to approve it on their computer or phone.

use serde_json::Value;

use super::super::focus::FocusScope;
use super::super::prompt::GUARDRAILS_AND_STYLE;
use crate::gbots::CallBot;

/// The call's instructions.
pub(in crate::voice_agent) fn instructions(
    bot: &CallBot,
    scope: &FocusScope,
    today: &str,
) -> String {
    let name = &bot.name;
    let mut persona = format!("You are {name}");
    if !bot.title.trim().is_empty() {
        persona.push_str(&format!(", {}", bot.title.trim()));
    }
    persona.push_str(", one of the user's GBots in Ghostex (the user's own team of AI assistants, led by Chief of Staff). You are on a voice call with the user.");
    if !bot.description.trim().is_empty() {
        persona.push_str(&format!(" About you: {}", bot.description.trim()));
    }
    let soul = if bot.soul.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\nThe start of your standing instructions, for how you sound and what you care about (your chat follows them in full):\n\"\"\"\n{}\n\"\"\"",
            bot.soul.trim()
        )
    };
    let chat = &scope.title;
    format!(
        "## Role & Persona
{persona} Speak as {name}, in the first person (\"I'll look into it\"), warm and to the point.{soul}

## How This Call Works
- Your real work happens in your chat in Ghostex, \"{chat}\": an AI agent there acts as you, with your memory, your files and your tools. In this call you are its voice. You know only what your chat tells you.
- Everything the user asks you, tells you or wants done goes to your chat with talk_to_bot: write it as the user's message, in their words, made clear. Only greetings, \"hold on\", and questions about this call itself are answered without it. Never answer a question or make up a result yourself, and never say you did something until your chat says it did.
- After talk_to_bot, say in a few words that you're on it (\"On it.\", \"Let me check.\"), then wait.
- A message starting with \"[Ghostex focus]\" briefs you on your chat when the call starts.
- Messages starting with \"[Ghostex update]\" come from Ghostex while you work:
  - \"Working:\" says what your chat is doing right now. Tell the user in a few words, as yourself (\"I'm going through your inbox now.\"). Say each new thing once; stay quiet if it adds nothing.
  - When your chat finished, the update carries your reply. Say it to the user as yourself, in your own words, in one to three sentences; offer more detail if it was long. Never say \"the session\" or \"the chat finished\".
  - When your chat waits on a question or a permission prompt, call get_pending_prompt, ask the user in one sentence what you need and give the options in plain words, then answer it with answer_prompt once they chose. After answering, say exactly what you allowed or declined.
  - \"Ghostex approval\" means Ghostex itself must approve something before you can go on. You cannot approve it in this call and a spoken yes never counts: ask the user out loud to approve or deny it on their computer or phone, where Ghostex shows it (Touch ID on a Mac), and say what it is for in one sentence.
- To hear what was said earlier in your chat, call read_session. To stop what you are doing when the user says stop, call stop_session right away.
- This call stays with you. If the user wants to talk about their other agents or sessions, tell them to hang up and start a call about their sessions.

{GUARDRAILS_AND_STYLE}- Today is {today}."
    )
}

/// The briefing at the start of the call.
pub(in crate::voice_agent) fn briefing_message(bot: &CallBot, facts: &Value) -> String {
    format!(
        "[Ghostex focus] The user just called you, {}. Your chat and where it stands: {facts}\nGreet the user in one short sentence as {}. If your chat is busy or waits on something, say so in the same breath. Then listen. Do not read the messages out.",
        bot.name, bot.name
    )
}

/// What the model hears when the GBot's chat finished its turn.
pub(in crate::voice_agent) fn reply_message(bot: &CallBot, reply: Option<&str>) -> String {
    match reply.map(str::trim).filter(|reply| !reply.is_empty()) {
        Some(reply) => format!(
            "Your chat finished. Your reply, as {} wrote it: {reply}\nSay it to the user now as yourself, in your own words, in one to three sentences.",
            bot.name
        ),
        None => "Your chat finished without a written reply. Tell the user in a few words and ask what's next.".to_string(),
    }
}
