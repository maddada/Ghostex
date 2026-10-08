<h1 align="center">
  <a href="https://ghostex.dev"><img src="media/ghostex-marketplace-icon.png" alt="Ghostex" width="72" valign="middle" /></a> Ghostex
</h1>

<p align="center">
  <a href="https://github.com/maddada/Ghostex/stargazers"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/github/stars/maddada/Ghostex.svg?variant=secondary&mode=dark" /><img src="https://shieldcn.dev/github/stars/maddada/Ghostex.svg?variant=secondary&mode=light" alt="GitHub stars" /></picture></a>
  <a href="https://github.com/maddada/Ghostex/releases/latest"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/github/release/maddada/Ghostex.svg?variant=secondary&mode=dark" /><img src="https://shieldcn.dev/github/release/maddada/Ghostex.svg?variant=secondary&mode=light" alt="Latest release" /></picture></a>
  <a href="LICENSE"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/github/license/maddada/Ghostex.svg?variant=secondary&mode=dark" /><img src="https://shieldcn.dev/github/license/maddada/Ghostex.svg?variant=secondary&mode=light" alt="License" /></picture></a>
  <br />
  <a href="#install"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/platforms-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux%20%C2%B7%20Android%20%C2%B7%20iOS.svg?variant=secondary&mode=dark" /><img src="https://shieldcn.dev/badge/platforms-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux%20%C2%B7%20Android%20%C2%B7%20iOS.svg?variant=secondary&mode=light" alt="Supported platforms: macOS, Windows, Linux, Android, iOS" /></picture></a>
</p>

<p align="center">
<a href="https://discord.gg/df7b3G92CS"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/Discord-Join%20the%20community.svg?variant=branded&logo=discord&mode=dark" /><img src="https://shieldcn.dev/badge/Discord-Join%20the%20community.svg?variant=branded&logo=discord&mode=light" alt="Join the Discord" /></picture></a>
</p>

<p align="center">
  <strong>The native desktop app for Claude Code, Codex, OpenCode, and every other coding agent.</strong><br/>
  Chat with several agents side by side, review their work as they go, and keep steering from your phone.
</p>

<h3 align="center"><a href="#install"><ins>Download Ghostex</ins></a> &nbsp;·&nbsp; <a href="https://ghostex.dev">Website</a> &nbsp;·&nbsp; <a href="https://youtu.be/QzjFB4J6-8E">Watch the 3-minute tour</a> &nbsp;·&nbsp; <a href="https://cdn.angles.video/videos/213c9683-d490-4177-80da-51d04c8c521b.mp4">Watch the 30s demo</a></h3>

<p align="center">
  <a href="media/readme/gx-hero-dark.jpg"><picture><source media="(prefers-color-scheme: dark)" srcset="media/readme/gx-hero-dark.gif" /><img src="media/readme/gx-hero-light.gif" alt="Ghostex with see-through glass over a moving meadow: projects and agent sessions in the sidebar, a chat in the middle and the view picker on the right" width="960" /></picture></a>
</p>

<div align="center">
  <table>
    <tr>
      <td width="50%" align="center"><a href="#a-real-chat-view-for-every-agent"><img src="media/readme/gx-diff.jpg" alt="A chat showing the agent's code change as a diff" width="100%" /></a></td>
      <td width="50%" align="center"><a href="#agents-that-run-agents"><img src="media/readme/gx-threads.jpg" alt="A coordinator with its threads in the sidebar and the Threads panel" width="100%" /></a></td>
    </tr>
    <tr>
      <td width="50%" align="center"><a href="#any-agent-swap-on-the-fly"><img src="media/readme/gx-agent-picker.gif" alt="The model picker switching between agents" width="100%" /></a></td>
      <td width="50%" align="center"><a href="#embedded-chromium-browser"><img src="media/readme/gx-browser-pane.jpg" alt="A chat next to the built-in browser" width="100%" /></a></td>
    </tr>
    <tr>
      <td width="50%" align="center"><a href="#ios-and-android-apps-with-easy-connect-or-tailscale"><img src="media/readme/gx-phone.gif" alt="Pairing the phone app, then sessions and a chat on the phone" width="100%" /></a></td>
      <td width="50%" align="center"><a href="#find-any-past-session"><img src="media/readme/gx-search.gif" alt="Searching every past prompt" width="100%" /></a></td>
    </tr>
    <tr>
      <td width="50%" align="center"><a href="#a-real-chat-view-for-every-agent"><img src="media/readme/gx-question.jpg" alt="An agent's question shown as a card with options" width="100%" /></a></td>
      <td width="50%" align="center"><a href="#also-in-the-box"><img src="media/readme/gx-views.jpg" alt="The view picker with Code, Browser, Kanban, Automate, Files, Terminal, Linear and Jira" width="100%" /></a></td>
    </tr>
  </table>
</div>

Ghostex is built for developers who keep many agents alive at once. A chat view for every agent, a native Rust/GPUI shell, embedded Chromium panes, and a mobile app share one workspace, and every session survives restarts.

## Features

<table>
<tr>
<td width="50%" valign="middle">

<br/>

### A real chat view for every agent

Talk to Claude Code, Codex, or any other agent in a proper chat GUI: clickable images, readable diffs, queued prompts, sub-agents in view, and a full editor for long messages. If you ever need the raw CLI, it is one hotkey away and your draft comes with you.

<br/>

</td>
<td width="50%">
  <img src="media/readme/gx-diff.jpg" alt="A chat showing the agent's code change as a diff" width="100%" />
</td>
</tr>
<tr>
<td width="50%" valign="middle">

<br/>

### iOS and Android apps with Easy Connect or Tailscale

Your agents in your pocket. Easy Connect pairs your phone with a single scan, no extra accounts needed, or join through your Tailscale tailnet if you already have one. Read transcripts, send follow-ups, preview localhost pages, and get a push when an agent finishes.

<br/>

</td>
<td width="50%">
  <img src="media/readme/gx-phone.gif" alt="Pairing the phone app, then sessions and a chat on the phone" width="100%" />
</td>
</tr>
<tr>
<td width="50%" valign="middle">

<br/>

### Any agent, swap on the fly

Claude Code, Codex, OpenCode, Pi, Gemini, Grok, Cursor, and more. Pick the model and effort from the chat box, or hand a session from one agent to another mid-task.

<br/>

</td>
<td width="50%">
  <img src="media/readme/gx-agent-picker.gif" alt="The model picker switching between agents" width="100%" />
</td>
</tr>
<tr>
<td width="50%" valign="middle">

<br/>

### Embedded Chromium browser

Click any element, type what should change, and the note lands in the agent's prompt. Agents can drive your tabs too, through the built-in browser-use skill.

<br/>

</td>
<td width="50%">
  <img src="media/readme/gx-browser-pane.jpg" alt="A chat next to the built-in browser" width="100%" />
</td>
</tr>
<tr>
<td width="50%" valign="middle">

<br/>

### Kanban board on Beads

Dump your thoughts on the board, then let an agent pick up the tickets. Runs on the [Beads](https://github.com/gastownhall/beads) `bd` CLI, so agents and humans share one backlog.

<br/>

</td>
<td width="50%">
  <img src="media/readme/gx-kanban-board.jpg" alt="The Kanban board with Backlog, Todo, In Progress and Test lanes" width="100%" />
</td>
</tr>
<tr>
<td width="50%" valign="middle">

<br/>

### Find any past session

Fuzzy search every prompt you ever sent, across all your agents, and press Enter to resume the conversation. Star favourites and filter by agent or project. Also available as `gx f` for command-line fans.

<br/>

</td>
<td width="50%">
  <img src="media/readme/gx-search.gif" alt="Searching every past prompt" width="100%" />
</td>
</tr>
<tr>
<td width="50%" valign="middle">

<br/>

### Agents that run agents

Agents can open sessions, send prompts, and read replies from other agents through the `ghostex` command. Ask Claude Code to spin up Codex sub-agents and steer them, or hand the work to a coordinator that keeps track of them for you.

<br/>

</td>
<td width="50%">
  <img src="media/readme/gx-threads.jpg" alt="A coordinator with its threads in the sidebar and the Threads panel" width="100%" />
</td>
</tr>
</table>

### Also in the box

- **Docs with annotations**: open Markdown, HTML prototypes, and Excalidraw diagrams next to your agent and leave notes on anything.
- **Rich prompt editor**: press Ctrl+G to edit any prompt in a full editor.
- **Built-in IDE**: a VS Code editor that loads on demand and sleeps when you are not using it.
- **Remote machines**: connect another computer with an Easy Connect code or SSH and it shows up in the sidebar.
- **Usage and accounts**: Claude and Codex limits at a glance, with automatic account switching.
- **Worktrees, splits, and spaces**: a worktree per task, split panes, Arc-style spaces.
- **Automations**: scheduled prompts, plus menu bar and sound notifications.
- **Optional plugins**: browser, Kanban, IDE, Docs, and more load only when you use them. Or write your own.

---

## Supported agents

Works with **any coding agent**. Bring the one you already use.

<p>
  <a href="https://docs.anthropic.com/claude/docs/claude-code"><kbd><img src="https://www.google.com/s2/favicons?domain=anthropic.com&sz=64" alt="" width="16" valign="middle" /> Claude Code</kbd></a> &nbsp;
  <a href="https://github.com/openai/codex"><kbd><img src="https://www.google.com/s2/favicons?domain=openai.com&sz=64" alt="" width="16" valign="middle" /> Codex</kbd></a> &nbsp;
  <a href="https://opencode.ai"><kbd><img src="https://www.google.com/s2/favicons?domain=opencode.ai&sz=64" alt="" width="16" valign="middle" /> OpenCode</kbd></a> &nbsp;
  <a href="https://pi.dev"><kbd><img src="https://pi.dev/favicon.svg" alt="" width="16" valign="middle" /> Pi</kbd></a> &nbsp;
  <a href="https://omp.sh"><kbd><img src="https://omp.sh/favicon.svg" alt="" width="16" valign="middle" /> oh-my-pi</kbd></a> &nbsp;
  <a href="https://github.com/google-gemini/gemini-cli"><kbd><img src="https://www.google.com/s2/favicons?domain=gemini.google.com&sz=64" alt="" width="16" valign="middle" /> Gemini CLI</kbd></a> &nbsp;
  <a href="https://x.ai/cli"><kbd><img src="https://www.google.com/s2/favicons?domain=x.ai&sz=64" alt="" width="16" valign="middle" /> Grok</kbd></a> &nbsp;
  <a href="https://cursor.com/cli"><kbd><img src="https://www.google.com/s2/favicons?domain=cursor.com&sz=64" alt="" width="16" valign="middle" /> Cursor</kbd></a> &nbsp;
  <a href="https://github.com/features/copilot/cli"><kbd><img src="https://www.google.com/s2/favicons?domain=github.com&sz=64" alt="" width="16" valign="middle" /> Copilot CLI</kbd></a> &nbsp;
  <kbd>+ many more</kbd>
</p>

---

## Install

### macOS

```bash
brew install ghostex
```

Or download the app directly:

<p>
  <a href="https://maddada.com/download/macos-arm64"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/macOS-Apple%20Silicon%20DMG.svg?variant=secondary&logo=apple&mode=dark" /><img src="https://shieldcn.dev/badge/macOS-Apple%20Silicon%20DMG.svg?variant=secondary&logo=apple&mode=light" alt="macOS Apple Silicon DMG" /></picture></a>
</p>

### Windows

<p>
  <a href="https://maddada.com/download/windows-x64"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/Windows-x64%20Setup.svg?variant=secondary&logo=windows&mode=dark" /><img src="https://shieldcn.dev/badge/Windows-x64%20Setup.svg?variant=secondary&logo=windows&mode=light" alt="Windows x64 installer" /></picture></a>
  <a href="https://maddada.com/download/windows-arm64"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/Windows-ARM64%20Setup.svg?variant=secondary&logo=windows&mode=dark" /><img src="https://shieldcn.dev/badge/Windows-ARM64%20Setup.svg?variant=secondary&logo=windows&mode=light" alt="Windows ARM64 installer" /></picture></a>
</p>

Agents run in native PowerShell by default. Prefer Linux? Switch to WSL in Settings > General > Terminal > Windows Environment. Updates arrive automatically.

### Linux

<p>
  <a href="https://maddada.com/download/linux-deb-x64"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/Linux-.deb.svg?variant=secondary&logo=debian&mode=dark" /><img src="https://shieldcn.dev/badge/Linux-.deb.svg?variant=secondary&logo=debian&mode=light" alt="Linux DEB package" /></picture></a>
  <a href="https://maddada.com/download/linux-rpm-x64"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/Linux-.rpm.svg?variant=secondary&logo=redhat&mode=dark" /><img src="https://shieldcn.dev/badge/Linux-.rpm.svg?variant=secondary&logo=redhat&mode=light" alt="Linux RPM package" /></picture></a>
  <a href="https://aur.archlinux.org/packages/ghostex-bin"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/AUR-ghostex--bin.svg?variant=secondary&logo=archlinux&mode=dark" /><img src="https://shieldcn.dev/badge/AUR-ghostex--bin.svg?variant=secondary&logo=archlinux&mode=light" alt="Arch Linux AUR package" /></picture></a>
  <a href="https://maddada.com/download/linux-tar-x64"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/Linux-tar.zst.svg?variant=secondary&logo=linux&mode=dark" /><img src="https://shieldcn.dev/badge/Linux-tar.zst.svg?variant=secondary&logo=linux&mode=light" alt="Linux tarball" /></picture></a>
</p>

```bash
# Arch Linux
yay -S ghostex-bin

# Any other x64 distribution: the tarball is a prefix-preserving /opt/ghostex tree
sudo tar -xpf ghostex-*-linux-x64.tar.zst -C /
```

Ghostex offers to install Chromium the first time you open the browser or code editor.

### Mobile

<p>
  <a href="https://github.com/maddada/Ghostex/releases/latest/download/ghostex-android.apk"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/Android-APK.svg?variant=secondary&logo=android&mode=dark" /><img src="https://shieldcn.dev/badge/Android-APK.svg?variant=secondary&logo=android&mode=light" alt="Android APK" /></picture></a>
  <a href="https://discord.gg/df7b3G92CS"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/iOS-TestFlight.svg?variant=secondary&logo=apple&mode=dark" /><img src="https://shieldcn.dev/badge/iOS-TestFlight.svg?variant=secondary&logo=apple&mode=light" alt="iOS TestFlight" /></picture></a>
</p>

The iOS TestFlight runs through the [Discord](https://discord.gg/df7b3G92CS). Post in the iOS channel to get in.

## Comparison

| Feature                   | Ghostex | ChatGPT app | cmux |
| ------------------------- | ------- | ----------- | ---- |
| macOS support             | Yes     | Yes         | Yes  |
| Windows support           | Yes     | Yes         | No   |
| Linux support             | Yes     | Yes         | No   |
| Open source               | Yes     | -           | Yes  |
| Ghostty terminal          | Yes     | -           | Yes  |
| Chromium Browser          | Yes     | Yes         | No   |
| Chat GUI view             | Yes     | Yes         | No   |
| Fully featured IDE        | Yes     | -           | -    |
| Built-in Computer use     | Yes     | Yes         | -    |
| Built-in Browser use      | Yes     | Yes         | Yes  |
| Use any model             | Yes     | -           | Yes  |
| Cross Model Orchestration | Yes     | -           | Yes  |
| Rich Prompt Editor        | Yes     | N/A         | -    |
| iOS                       | Yes     | Yes         | Yes  |
| Android                   | Yes     | Yes         | Yes  |
| Automations               | Yes     | Yes         | -    |

---

## Community

- **Discord:** [discord.gg/df7b3G92CS](https://discord.gg/df7b3G92CS) for help, TestFlight access, and feature talk.
- **Issues:** [Report a bug or request a feature](https://github.com/maddada/Ghostex/issues).
- **Contributing:** Ghostex moves fast and help is welcome on platform ports, agent integrations, docs, and polish. Start with [CONTRIBUTING.md](CONTRIBUTING.md).

<p align="center">
  <a href="https://github.com/maddada/Ghostex/graphs/contributors"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/contributors/maddada/Ghostex.svg?bg=transparent&border=false&mode=dark" /><img src="https://shieldcn.dev/contributors/maddada/Ghostex.svg?bg=transparent&border=false&mode=light" alt="Ghostex contributors" /></picture></a>
</p>

## Credits

Ghostex builds on open source work from these projects and communities:

- [Ghostty](https://github.com/ghostty-org/ghostty) and [Zed / GPUI](https://github.com/zed-industries/zed) for the terminal and the native shell
- [CEF](https://github.com/chromiumembedded/cef) for embedded Chromium panes
- [Trycua](https://github.com/trycua/cua) for built-in Computer Use
- [VS Code](https://github.com/microsoft/vscode) and [code-server](https://github.com/coder/code-server) for the embedded IDE
- [Beads](https://github.com/gastownhall/beads) by [Steve Yegge](https://github.com/steveyegge) and [Beads Viewer](https://github.com/Dicklesworthstone/beads_viewer) for the Kanban board
- [OpenUsage](https://github.com/robinebers/openusage) for Claude and Codex usage stats
- [Agentation](https://github.com/benjitaylor/agentation) for browser annotation tooling
- [cmux](https://github.com/manaflow-ai/cmux) for agent hook and notification patterns
- [zehn](https://github.com/al3rez/zehn) by [al3rez](https://github.com/al3rez) for prompt-history search
- [vvterm](https://github.com/vivy-company/vvterm) and [Termux](https://github.com/termux/termux-app) for mobile terminal components
- [Pierre](https://github.com/pierrecomputer/pierre) for diff and file rendering components

The moving background behind the screenshots is [Time lapse of a green meadow](https://mixkit.co/free-stock-video/time-lapse-of-a-green-meadow-4070/) from Mixkit, used under the Mixkit Stock Video Free License.

## License

Ghostex is free and open source under the [MIT License](LICENSE).
