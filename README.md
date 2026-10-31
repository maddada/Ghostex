<h1 align="center">
  <a href="https://ghostex.dev"><img src="media/ghostex-marketplace-icon.png" alt="Ghostex" width="72" valign="middle" /></a> Ghostex
  <br>
  The best native all-in-one agents workspace
</h1>

<p align="center">
<a href="https://discord.gg/df7b3G92CS"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/Discord-Join%20the%20community.svg?variant=branded&logo=discord&mode=dark" /><img src="https://shieldcn.dev/badge/Discord-Join%20the%20community.svg?variant=branded&logo=discord&mode=light" alt="Join the Discord" /></picture></a>
</p>

<p align="center">
  <a href="https://github.com/maddada/Ghostex/stargazers"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/github/stars/maddada/Ghostex.svg?variant=secondary&mode=dark" /><img src="https://shieldcn.dev/github/stars/maddada/Ghostex.svg?variant=secondary&mode=light" alt="GitHub stars" /></picture></a>
  <a href="https://github.com/maddada/Ghostex/releases/latest"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/github/release/maddada/Ghostex.svg?variant=secondary&mode=dark" /><img src="https://shieldcn.dev/github/release/maddada/Ghostex.svg?variant=secondary&mode=light" alt="Latest release" /></picture></a>
  <a href="LICENSE"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/github/license/maddada/Ghostex.svg?variant=secondary&mode=dark" /><img src="https://shieldcn.dev/github/license/maddada/Ghostex.svg?variant=secondary&mode=light" alt="License" /></picture></a>
  <br />
  <a href="#install"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/platforms-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux%20%C2%B7%20Android%20%C2%B7%20iOS.svg?variant=secondary&mode=dark" /><img src="https://shieldcn.dev/badge/platforms-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux%20%C2%B7%20Android%20%C2%B7%20iOS.svg?variant=secondary&mode=light" alt="Supported platforms: macOS, Windows, Linux, Android, iOS" /></picture></a>
</p>

<h3 align="center"><a href="#install"><ins>Download Ghostex</ins></a> &nbsp;·&nbsp; <a href="https://ghostex.dev">Website</a> &nbsp;·&nbsp; <a href="https://youtu.be/QzjFB4J6-8E">Watch the 3-minute tour</a>  &nbsp;·&nbsp; <a href="https://www.youtube.com/watch?v=cJgogrS4mao">Watch the Demo</a></h3>

<p align="center">
  <a href="https://github.com/user-attachments/assets/74906504-c9b1-42e8-aa06-04f2f0439045">
    <picture>
      <img width="960" src="https://github.com/user-attachments/assets/74906504-c9b1-42e8-aa06-04f2f0439045" />
    </picture>
  </a>
</p>

<p align="center">
  <a href="https://github.com/user-attachments/assets/d4914393-4e5c-4fbf-b5b3-9d3ab4dbcc9d">
    <picture>
          <img width="960" src="https://github.com/user-attachments/assets/d4914393-4e5c-4fbf-b5b3-9d3ab4dbcc9d" />
    </picture>
  </a>
</p>

<p align="center">
  <a href="https://github.com/user-attachments/assets/fc9b2632-51dd-444e-a113-6936e498fa06">
    <picture>
      <img width="960" src="https://github.com/user-attachments/assets/fc9b2632-51dd-444e-a113-6936e498fa06" />
    </picture>
  </a>
</p>

Ghostex is built for developers who keep many agents alive at once. A chat view for every agent, a native Rust/GPUI shell, embedded Chromium panes, and a mobile app share one workspace, and every session survives restarts.

## Features

<h3 align="center">📱 iOS and Android apps with Easy Connect&nbsp;or&nbsp;Tailscale</h3>

<p align="center">Your agents in your pocket. Easy Connect pairs your phone with a single scan, no extra accounts needed, or join through your Tailscale tailnet if you already have one. Read transcripts, send follow-ups, preview localhost pages, and get a push when an&nbsp;agent&nbsp;finishes.</p>

<p align="center"><img src="media/readme/gx-phone.gif" alt="Pairing the phone app, then sessions and a chat on the phone" width="70%" /></p>

<br/>

<h3 align="center">💬 Chat &amp;&nbsp;CLI&nbsp;views</h3>

<p align="center">Talk to Claude Code, Codex, or any other agent in a proper chat GUI: clickable images, readable diffs, queued prompts, sub-agents in view, and a full editor for long messages. If you ever need the raw CLI, it is one hotkey away and your draft comes&nbsp;with&nbsp;you.</p>

<p align="center"><img src="media/readme/gx-diff.png" alt="A chat showing the agent's code change as a diff" width="70%" /></p>

<br/>

<h3 align="center">🔀 Any agent, swap on&nbsp;the&nbsp;fly</h3>

<p align="center">Claude Code, Codex, OpenCode, Pi, Gemini, Grok, Cursor, and more. Pick the model and effort from the chat box, or hand a session from one agent to&nbsp;another&nbsp;mid‑task.</p>

<p align="center"><img src="media/readme/gx-agent-picker.png" alt="The model picker switching between agents" width="70%" /></p>

<br/>

<h3 align="center">📝 Annotate Markdown, HTML&nbsp;and&nbsp;diagrams</h3>

<p align="center">Open plans, HTML prototypes, and Excalidraw diagrams next to your agent, select anything, and leave a note or mark it Looks good, Clarify, or Needs tests. One click sends every note to the agent's prompt as numbered feedback with&nbsp;line&nbsp;numbers.</p>

<p align="center"><img src="media/readme/gx-annotate.png" alt="A Markdown plan in the Files view with a comment, a Clarify mark and a Needs tests mark in the Annotations list" width="70%" /></p>

<br/>

<h3 align="center">🤖 Agents that&nbsp;run&nbsp;agents</h3>

<p align="center">Agents can open sessions, send prompts, and read replies from other agents through the <code>ghostex</code> command. Ask Claude Code to spin up Codex sub-agents and steer them, or hand the work to a coordinator that keeps track of them&nbsp;for&nbsp;you.</p>

<p align="center"><img src="media/readme/gx-threads.png" alt="A coordinator with its threads in the sidebar and the Workers panel" width="70%" /></p>

<br/>

<h3 align="center">📊 Charts and tables in&nbsp;the&nbsp;chat</h3>

<p align="center">Ask for <code>$ghostex-visuals</code> and agents answer with charts, tables, and stat tiles drawn right in the chat, or publish an HTML page that opens as a card. Same look on desktop, web,&nbsp;and&nbsp;phone.</p>

<p align="center"><img src="media/readme/gx-visuals.png" alt="A chat reply with stat tiles, a bar chart and a table of failing tests" width="70%" /></p>

<br/>

<h3 align="center">⏭️ Queue your&nbsp;next&nbsp;prompts</h3>

<p align="center">Keep typing while the agent works. Press <kbd>Tab</kbd> to queue a prompt and it waits above the input until the agent is free; drag to reorder, or press play on a message you already sent to interrupt and deliver it&nbsp;right&nbsp;away.</p>

<p align="center"><img src="media/readme/gx-queue.png" alt="An agent at work with two queued prompts waiting above the chat input" width="70%" /></p>

<br/>

<h3 align="center">🔎 Find any&nbsp;past&nbsp;session</h3>

<p align="center">Fuzzy search every prompt you ever sent, across all your agents, and press <kbd>Enter</kbd> to resume the conversation. Star favourites and filter by agent or project. Also available as <code>gx f</code> for&nbsp;command‑line&nbsp;fans.</p>

<p align="center"><img src="media/readme/gx-search.png" alt="Searching every past prompt" width="70%" /></p>

### Also in the box

- **Built-in IDE**: an optional VS Code editor that loads on demand and sleeps when you are not using it.
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
  <a href="https://maddada.com/download/linux-appimage-x64"><picture><source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/badge/Linux-AppImage.svg?variant=secondary&logo=linux&mode=dark" /><img src="https://shieldcn.dev/badge/Linux-AppImage.svg?variant=secondary&logo=linux&mode=light" alt="Linux AppImage" /></picture></a>
</p>

```bash
# Arch Linux
yay -S ghostex-bin

# Any other x64 distribution: the tarball is a prefix-preserving /opt/ghostex tree
sudo tar -xpf ghostex-*-linux-x64.tar.zst -C /

# No package manager: the AppImage copies itself into ~/.local/share/ghostex/appimage on first start
chmod +x ghostex-*-linux-x64.AppImage && ./ghostex-*-linux-x64.AppImage
```

ARM64 packages and the ARM64 AppImage are on the [release page](https://github.com/maddada/Ghostex/releases/latest). To update an AppImage, download the new one and open it. Without FUSE the AppImage says "Cannot mount AppImage": install your distribution's `fuse3` package, or start it with `--appimage-extract-and-run`.

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

## License

Ghostex is free and open source under the [MIT License](LICENSE).
