---
name: ghostex-computer-use
description: >-
  Use this skill when the user asks for Ghostex Computer Use, Desktop Control,
  or native app automation. You are running inside Ghostex: it sends you to
  Trycua's `cua-driver` skill and the `cua-driver` CLI, so the user does not
  need to remember `$cua-driver`.
# CDXC:AgentSkills 2026-09-13 DECISION:
# User: Ghostex Help and all other Ghostex skills must not be agent-invokable, matching the Shepherd skill's explicit-invocation policy.
disable-model-invocation: true
---

# ghostex-computer-use

You are running inside Ghostex. For computer use (driving native apps), read
and follow Trycua's `cua-driver` skill, and act through the `cua-driver` CLI.

<!--
CDXC:AgentSkills 2026-11-19 DECISION:
User: "I think these 2 should just mention we're in ghostex and refer to cua-driver skill and ask agent to use that for browser and for computer use basically." This file only points at the cua-driver skill plus the Ghostex-only lines it cannot know; never copy the driver's own guidance back in.
-->

## Read the cua-driver skill

Load `$cua-driver` if your agent offers it; otherwise read its `SKILL.md` and
the files it links:

```bash
cat "$(cua-driver skills path)/SKILL.md"
```

From WSL (Ghostex on Windows), the driver lives on the Windows side:

```bash
cat "$(wslpath -u "$(powershell.exe -NoProfile -Command 'cua-driver skills path' | tr -d '\r')")/SKILL.md"
```

If the skill is missing, run `cua-driver skills install` (through
`powershell.exe` from WSL). If the `cua-driver` driver itself is missing,
install Fast Computer & Browser Use from Ghostex Settings > Integrations,
which installs the driver and its skill.

## What Ghostex adds

- Use the `cua-driver` CLI (`cua-driver <tool> '<JSON>'`), not MCP. Do not
  register or start a Trycua MCP server for this workflow.
- Check permissions read-only: `cua-driver check_permissions
  '{"prompt":false}'`.
- If the daemon is not running (macOS), start it with `open -n -g -a
  CuaDriver --args serve`. Do not use `cua-driver permissions grant`: it
  walks through the macOS permission prompts.
- From WSL, call every tool as `powershell.exe -NoProfile -Command
  "cua-driver <tool> '<JSON>'"` (escape the JSON's double quotes as `\"`), and
  pass Windows paths (`wslpath -w`, `wslpath -u`). A failing `which
  cua-driver` inside WSL does not mean it is missing.
- Web page content in Chrome, Chromium, Edge or Electron: `$ghostex-browser-use`.
  Browser panes built into Ghostex: `$ghostex-embedded-browser-use`.
- To use Mac apps on a hidden screen, leaving the user's screen, pointer and
  focus alone, Ghostex ships `$ghostex-spaceo`.

<!--
CDXC:CuaPermissions 2026-05-29-12:21:
State checks must be read-only. The default Trycua permission check can ask macOS to show missing-grant prompts, so the Ghostex wrapper uses prompt:false and only sends users to System Settings when grants are actually missing.
-->
