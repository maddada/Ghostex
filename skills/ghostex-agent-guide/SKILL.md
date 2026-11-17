---
name: ghostex-agent-guide
description: >-
  You are running inside Ghostex. Ghostex can show Ghostex pages (gx page:
  HTML), Ghostex forms (gx form: questions) and Ghostex visuals (gx visuals:
  charts, graphs) in this chat, open pages you build in its own browser, run
  other agents, and install Ghostex skills for the user (such as
  ghostex-visuals or ghostex-embedded-browser-use). Use this skill when the
  user asks for one of these, or about Ghostex, and when a step only the user
  can do (a sudo command, a sign-in, a tap on their phone) blocks your work;
  otherwise don't bring it up.
# CDXC:AgentSkills 2026-11-17 DECISION:
# User (Q1 "C"): no Ghostex skill is picked up by a model on its own, in any session. A message that
# names the skill (`/name` in Claude, `$name` in Codex) still loads it: Ghostex names it where a
# session needs it, and its launch note and `ghostex agent-guide` carry the rest.
disable-model-invocation: true
---

# ghostex-agent-guide

Run `ghostex agent-guide` and follow it. It says how to show Ghostex pages,
Ghostex forms and Ghostex visuals in the chat (a user may still call them
inline HTML, inline questions and inline graphs), which browser to open a
page in (the user's saved choice, or ask them once), how to work with other
agents, how to ask for a secret, how to leave the user a to-do for a step only
they can do (`ghostex needs-you add`; an older Ghostex without it: list the
step under Needs you in your reply), which Ghostex skills you can install for
the user, and how to ask Ghostex Help (`ghostex help ask "<question>"`).

If `ghostex agent-guide` is an unknown command, this Ghostex is older: run
`ghostex guide` and `ghostex --help` instead.
