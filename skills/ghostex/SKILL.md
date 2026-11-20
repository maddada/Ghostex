---
name: ghostex
description: >-
  You are running inside Ghostex. Ghostex can show Live pages (HTML; once
  called Ghostex pages, gx page or inline HTML), Ghostex forms (gx form:
  questions) and Ghostex visuals (gx visuals: charts, graphs) in this chat,
  open pages you build in its own browser, run other agents and coordinators,
  ask the user for a secret, and explain or change its own settings. Use this
  skill when the user names it or asks about Ghostex. It sends you to the
  `ghostex` CLI, whose guides always match the Ghostex you run on.
# CDXC:AgentSkills 2026-11-20 DECISION:
# User (Q4262 "B"): Ghostex's own skills are retired for CLI help, keeping this one tiny skill
# (once `ghostex-agent-guide`) whose only job is to send the agent to `ghostex agent-guide` and the
# `ghostex <area> guide` or `--help` it names. Like every Ghostex skill it never loads on its own
# (2026-11-17, Q1 "C"): it loads when the user types /ghostex (Codex: $ghostex).
disable-model-invocation: true
---

# ghostex

Run `ghostex agent-guide` and follow it. It says how to show Live pages,
Ghostex forms and Ghostex visuals in the chat, which browser to open a page
in, how to work with other agents and coordinators, how to ask for a secret,
how to leave the user a to-do for a step only they can do, and which guide to
read for each job:

- `ghostex visual guide visuals|forms|pages` for the chat's visuals, forms and Live pages
- `ghostex agents guide` for other agents, reading a thread, renaming a session
- `ghostex coordinator guide` if you are a coordinator
- `ghostex help guide` to explain Ghostex or change its settings for the user
- `ghostex board guide`, `ghostex secret guide`, `ghostex spaceo guide`, `ghostex browser --help`
- `ghostex --help` for every command, `ghostex <command> --help` for one area

For pages in Chrome, Edge or an Electron app, and for desktop apps, read the
`cua-driver` skill instead.

If `ghostex agent-guide` is an unknown command, this Ghostex is older: run
`ghostex guide` and `ghostex --help` instead.
