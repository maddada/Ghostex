---
name: ghostex-browser-use
description: >-
  Use this skill when an agent needs to inspect or automate web content in
  Chrome, Chromium, Edge, or a supported Electron app. You are running inside
  Ghostex: it sends you to Trycua's `cua-driver` skill (its BROWSER.md) and the
  `cua-driver` CLI's typed browser tools. Use ghostex-embedded-browser-use
  instead for browser panes built into Ghostex.
disable-model-invocation: true
---

# ghostex-browser-use

You are running inside Ghostex. For browser use (web pages in Chrome,
Chromium, Edge or Electron), read and follow Trycua's `cua-driver` skill, above
all its `BROWSER.md`, and act through the `cua-driver` CLI.

<!--
CDXC:AgentSkills 2026-11-19 DECISION:
User: "I think these 2 should just mention we're in ghostex and refer to cua-driver skill and ask agent to use that for browser and for computer use basically." This file only points at the cua-driver skill plus the Ghostex-only lines it cannot know; never copy the driver's own guidance back in.
-->

## Read the cua-driver skill

Load `$cua-driver` if your agent offers it; otherwise read its `SKILL.md`, then
`BROWSER.md` and the other files it links:

```bash
cat "$(cua-driver skills path)/SKILL.md" "$(cua-driver skills path)/BROWSER.md"
```

From WSL (Ghostex on Windows), the driver lives on the Windows side:

```bash
dir="$(wslpath -u "$(powershell.exe -NoProfile -Command 'cua-driver skills path' | tr -d '\r')")"
cat "$dir/SKILL.md" "$dir/BROWSER.md"
```

If the skill is missing, run `cua-driver skills install` (through
`powershell.exe` from WSL). If the `cua-driver` driver itself is missing,
install Fast Computer & Browser Use from Ghostex Settings > Integrations,
which installs the driver and its skill.

## What Ghostex adds

- Use the `cua-driver` CLI (`cua-driver <tool> '<JSON>'`) with one named
  `session` on every call, not MCP. Do not register or start a Trycua MCP
  server for this workflow.
- If the daemon is not running (macOS), start it with `open -n -g -a
  CuaDriver --args serve`. Do not use `cua-driver permissions grant`: it
  walks through the macOS permission prompts.
- From WSL, call every tool as `powershell.exe -NoProfile -Command
  "cua-driver <tool> '<JSON>'"` (escape the JSON's double quotes as `\"`), and
  pass Windows paths for uploads and downloads (`wslpath -w`, `wslpath -u`).
- Browser panes built into Ghostex: `$ghostex-embedded-browser-use`. Browser
  chrome, native dialogs, Safari and Firefox: `$ghostex-computer-use`.
- Attach to the user's own signed-in browser profile only after the user
  says yes: it exposes their live pages, cookies and storage. Restart the
  driver with the grant (`cua-driver stop`, then `open -n -g -a CuaDriver
  --args serve --grant existing-profile`) only after that yes, never on your
  own, even where BROWSER.md says to.
- If the task needs the user's logins, say so in your first reply and ask
  before any browser work, so they are at the computer for the prompts;
  never start isolated and ask at a login wall.
- macOS "no vendor-signed system Chromium executable is available": a
  drag-installed Chrome often carries Finder info that fails `codesign --verify
  --strict`. Strip it with `xattr -r -d com.apple.FinderInfo "/Applications/Google
  Chrome.app"`; never work around it with a hand-started debugging-port Chrome.
- Windows "launch token was granted delete on …": report the refusal and the
  folder it names to the user and stop. It is their machine's permission
  problem; do not change permissions or launch another browser around it.
