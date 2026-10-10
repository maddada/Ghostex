# Changelog

## Unreleased

## 10.19.0 - 2026-10-10

**Ghostex 10.19.0 is out.** A new Skills menu shows Ghostex's skills in the chat and terminal, the Work page gets two-line rows, Group by and a one-row filter bar, each workspace window keeps to its own sessions, browser sign-ins in workspaces stay saved, and Colourfulness gets finer steps.

### 💬 Chat and terminal
- **Skills in the ⋯ menus:** the chat's More actions and the terminal's ⋯ menu list the Ghostex skills the session's agent has installed; pick one to put it in the chat box or type it into the agent, and Configure / Install more opens Settings > Integrations > Agent skills.
- **Branches moved into More actions:** the floating "This conversation has N branches" button is gone; Branches at the top of the Chat section opens the same list.
- **Agents' HTML pages open in a floating window over the chat.**
- **A send waits out a busy agent** instead of retyping into it.

### 🧩 Workspaces and Work Mode
- **Two-line Work rows** keep every part in the same place, the ticket ID, PR number, repo and project names open their pages, and the owner comes first.
- **Group by** sorts the Work list by status, repo, project, assignee, type, pull request or last update, with folding headers.
- **One-row filter bar:** All / issues / PRs and Assigned to me on the left, Filter and View menus on the right, and the page's menus stay inside the page.
- **A window shows only its own workspace's sessions,** and opening a session of another workspace goes to the window showing it.
- **The workspace tile is a split button:** one click switches workspace, the chevron opens the menu.
- **A workspace's browser keeps its sign-ins** after Ghostex closes (sign in once more after updating).
- **A PR's details find its ticket** whichever chip opened them, plus fixes from live testing.

### 🌐 Browser
- **Links open in the empty New Tab** instead of leaving it beside the page, everywhere the app opens a link in its browser.
- **Links that open another app never leave an error page;** a tab opened only for that link closes once you answer.

### 🎨 Theme and views
- **Colourfulness moves in half steps** (33 positions) from Subtle to Vivid, showing the nearest name, and the Custom colour's depth sliders follow.
- **Files, Kanban, Automate and extension views say "Loading sessions…"** while Ghostex starts, instead of "unavailable".
- **The Files view's comment box takes typing on macOS** with transparency on.

### 📱 Phone
- **The Spaces bar shows its Space buttons again,** the workspace tile is a split button, and the sessions list starts at the screen's edge.
- **Skills** is in the chat's More actions and the terminal menu, and the chat's Branches item replaces the floating button.
- **The unused Sidebar projects opacity setting is gone** from Advanced.

## 10.18.0 - 2026-10-10

**Ghostex 10.18.0 is out.** Coordinators are now called Orchestrators, failed sends fix themselves, the phone app stops crashing and reaches your Windows computer again, and Windows hooks, cloning and helper windows behave.

### 🧭 Orchestrators
- **Coordinators are now called Orchestrators** across the app, the phone, Help and the CLI; `ghostex orchestrator` is the new command and `ghostex coordinator` still works.

### 💬 Chat
- **Failed sends fix themselves:** Ghostex redraws a stuck screen, retypes once, or restarts the agent on its own conversation and delivers your message, and shows a card with one Fix it button only when that fails. It never restarts an orchestrator or a session with background work.
- **A Claude session that quit before its first message starts fresh** instead of failing to resume.
- **Messages from other agents show their real text,** and Not sent appears only when a send really failed.
- **Claude Code's internal suggestion queries no longer show up as questions in the chat,** and its faint guess at your next message is no longer mistaken for typed text.
- **Sliding a panel no longer re-wraps or rebuilds the chat every frame,** and the question card's buttons have an outline.

### 🪟 Windows
- **Agent hooks no longer time out on Windows** for Claude, Codex, Grok, OpenClaude and OpenCode, thanks to @gvastethecreator.
- **No more console windows flashing** from anything Ghostex starts in the background.
- **Window glass uses Windows 11's own blur,** so sidebar and panel slides run about three times faster, menus share the panels' bluish fill, and toasts no longer show a square blur; new installs on Windows still start with glass off.

### 📂 Projects and Files
- **Add Project's clone shows git's progress,** can be cancelled, never hangs on a password prompt, and starts at your drives on Windows.
- **Project websites like GitHub, Linear and Jira open straight to the page** instead of a command screen.
- **Confirm delete in the files list is bold red,** and text buttons in Settings and the Agents Hub have an outline.
- **The hooks card shows Install all only when needed,** with Uninstall all (now confirmed first) in a ⋯ menu and Refresh as an icon, and long extension descriptions show in full on hover.

### 📱 Phone
- **The Android app no longer crashes** and can send to a Windows computer again; Ghostex on the computer also listens where older phone apps look.

### 🧩 Workspaces and Work Mode
- **Work Mode adds team Linear keys and GitHub as a workspace's tracker,** with work cards on the phone (Settings > Extensions > Workspaces and Work Mode).

## 10.17.0 - 2026-10-09

**Ghostex 10.17.0 is out.** Workspaces and Work Mode arrive as an extension you can turn on, a chat's history no longer disappears after Escape, dialogs keep their focus on Windows, and the Files editor gets find, Open with and proper Windows editing keys.

### 🧩 Workspaces and Work Mode
- **Workspaces and Work Mode are a built-in extension, off by default:** turn it on in Settings > Extensions to get workspaces with their own browser sign-ins, a Work page with Linear and GitHub items, Link to and Create Linear ticket on sessions, and your team's Slack flow.

### 💬 Chat
- **A chat no longer loses its earlier history** after you press Escape and send again.
- **Long messages show their first lines with Show more,** and scrolling past a very long message no longer lags the app.
- **Send failures say what actually went wrong** (the message never appeared, appeared twice, or was kept by the agent), and Claude sessions on a saved account are no longer reported as stopped.
- **Short hex colors like `#abc` show a swatch,** while all-digit numbers like `#1234` stay issue links.
- **The loading notice stays on one line** when there's room.

### 📂 Files
- **Ctrl+F (Cmd+F) finds in the open file,** with next and previous match.
- **Open with works on Windows,** listing your apps and browsers plus Choose another app.
- **Windows editing keys work in the Markdown editor:** Ctrl+arrows by word, Shift+Home/End, Ctrl+Home/End and Ctrl+Backspace.
- **File list tooltips sit below the pointer and wrap,** and hovering one no longer closes the floating list.

### 🪟 Windows and dialogs
- **Dialogs keep their focus:** Rename keeps its selected text and History stays open instead of the main window taking focus back; clicking away closes Rename without renaming.
- **The floating chat stays open** with two windows, and a second window no longer hides the first window's menu.
- **Tooltips never block clicks,** and a pane's blue attention line shows along its top edge.
- **The terminal view's Switch Account opens its account list,** to the right when there's room.

### 🗂️ Sessions
- **Starting a new session no longer closes your other empty sessions;** turn it back on for Chat agents with Settings > General > Chat > Close empty sessions when starting a new one.

### 📱 Phone
- **The phone's session menu matches the desktop's Copy, Work Mode and Parked menus,** and its chat connection recovers from a dropped link instead of showing raw errors.

## 10.16.0 - 2026-10-09

**Ghostex 10.16.0 is out.** Chat View no longer starts a terminal it doesn't need, the phone opens chats at once, custom agents can keep their own Claude or Codex login, and the Files editor handles wide tables, selections and long code lines properly.

### ⚡ Speed
- **Chat View no longer runs a terminal in the background:** a session's terminal starts the first time you switch to it, and your last three stay warm. Terminal View works exactly as before.
- **On Windows, switching between Chat and Terminal resizes the agent straight away** instead of waiting for a size check.
- **Settings > General > Faster rendering (experimental) redraws only what changed,** which uses less CPU while agents stream and you scroll; GPUI is updated to the latest upstream.

### 💬 Chat
- **`#123` issue and PR numbers link to the project's GitHub,** and only real 6- and 8-digit hex colors show a color swatch.
- **Long code blocks are capped** with an expand button and scroll inside, charts draw in, and list markers and finished tasks are quieter.
- **A message is never sent twice** when the computer is slow, and its preview bubble always clears.
- **Codex's weekly limit shows in the status line** on plans that only have a weekly limit.
- **View and Switch Account in More actions open on click and close on a second click.**
- **A pane shows its blue attention outline** whenever its session's sidebar dot is blue.

### 👤 Accounts
- **Custom agents that use their own login keep it:** an agent that sets `CLAUDE_CONFIG_DIR` or `CODEX_HOME`, or runs its own wrapper, runs as-is, and Ghostex shows "Uses its own login" instead of offering accounts, thanks to @landygg.
- **Continue automatically is on by default,** and at a limit Ghostex uses another account.
- **An account whose subscription access was disabled is skipped,** and a message to one of its sessions first moves it to an account with usage; Settings > Accounts says why it turned Manual.
- **The terminal view's ⋯ menu offers Switch Account** for Claude and Codex, with each account's usage.

### 📂 Files
- **Wide tables fit the editor like the chat's,** wrapping long columns and scrolling sideways when they still can't fit, with Open in window, Copy as Markdown and Copy as CSV on hover.
- **Typing over a selection replaces it,** wrapped code lines keep their background, and the editor's scrollbar is always shown.
- **The floating files list opens only from its button** and slides in from the right, without moving the bar behind it.
- **Files opened from outside the project before the last update open again** instead of asking to be reopened from a chat link.

### 🪟 Windows and dialogs
- **Menus with hotkeys widen to fit Windows' spelled-out shortcuts,** and modal scrollbars sit 1px from the window edge.

### 📱 Phone
- **Chats open at once:** a Chat View session no longer connects its terminal until you switch to it, and chat uses one warm connection per computer instead of a new SSH command per request.

## 10.15.5 - 2026-10-08

**Ghostex 10.15.5 is out.** Windows close when you click away, every conversation keeps its own model, `!` commands get a cleaner card, and the app waits for its background service instead of asking you to click.

### 💬 Chat
- **Each conversation remembers its own model and effort:** waking or resuming it comes back on what it last used, including a choice for that session only or a `/model` typed in Claude's terminal, and the chat box shows it right away.
- **`!` commands show as `! command`** with their output in its own bubble below; click the header to fold it, even while it runs.
- **While Ghostex is still starting, the chat shows its loading skeleton with the normal chat box** and fills in by itself.

### 🗂️ Sidebar
- **"Loading sessions…" retries every 2 seconds on its own** while Ghostex's background service starts; Try now is still there.
- **A dragged session card stays under your cursor,** and a pane from another project dims and says it can't split there.

### 🪟 Windows and dialogs
- **Clicking away closes Quick Access, Settings, Search by Prompt, the viewers and the Agents Hub,** unless you're in the middle of typing something.
- **Resizing the window closes floating panels and menus,** such as the Files list.
- **The Settings scrollbar drags again** and sits against the window's right edge.
- **Files opened from outside the project show their name and a plain path in Open Files.**

### 📱 Phone
- **The sessions list retries every 2 seconds** while Ghostex on the computer isn't answering, instead of showing a raw error.

## 10.15.0 - 2026-10-08

**Ghostex 10.15.0 is out.** Empryo joins as a built-in agent, the chat feels instant (sending, the model name, `!` shell commands), Windows terminals use PowerShell 7, and Easy Connect pairs Windows administrator accounts.

### 🤖 Agents
- **Empryo is a built-in agent:** chat, questions and approvals as cards, its model and effort picker, forks, resume, coordinators and threads, and its prompts in Search by Prompt and on the phone. It installs on Windows too and starts switched off in Settings > Agents, thanks to @banozz0.
- **Haiku 5.5 replaces Haiku 4.5** in the Claude Code and Cursor model pickers, and auto titles and other background helpers run on Haiku 5.5.
- **Chat messages reach OMP in all eight of its prompt layouts.**
- **Codex's own update works again inside Ghostex sessions on Windows** (it failed with "Get-FileHash is not recognized").

### 💬 Chat
- **Sending is instant:** your message leaves the composer and shows in the chat the moment you press Enter, images included.
- **The model name shows right away,** and new sessions start on the model you last picked.
- **`!` shell commands show as a Shell card** with their output streaming live.
- **Scrolling up past a long reply no longer jumps to its top.**
- The loading notice sits in the middle of an empty chat, tree guide lines line up with the icon above, the answer field's pills always match its text, and the Tasks panel reopens the way you left it.

### 🧭 Coordinators
- **Clicking a thread in the Threads panel opens it,** resuming a closed one.
- **The Threads panel shows working threads and those active in the last 2 hours,** and remembers whether you folded it.

### 🗂️ Sidebar and Files
- **An expanded project shows only New agent, the agent picker and a ⋯ menu;** everything else moved into the menu.
- **Clicking the file name at the top of the Files view copies its full path.**

### 🪟 Windows
- **Terminals use PowerShell 7 wherever it is installed** (Program Files, your user folder, the Microsoft Store or PATH) and Windows PowerShell 5.1 until then; Settings > General > Terminal offers **Install PowerShell 7** when only 5.1 is present.
- **Easy Connect pairs a phone with a Windows administrator account** after one Windows admin prompt.
- **Every build shows the real Ghostex version,** and About shows the app icon.

### 📱 Phone
- **A simpler sessions list,** Empryo in chat and Find, and pairing waits for the computer's admin prompt.

## 10.14.0 - 2026-10-07

**Ghostex 10.14.0 is out.** Agents can show charts and pages right in the chat, Cursor and ZCode work properly on Windows, Settings > Agents gets a simple switch per agent, coordinators show one tidy list of threads, and the phone opens file links and attaches pictures reliably.

### 🤖 Agents
- **Settings > Agents has a switch on every agent.** Turn agents on or off instead of deleting them; agents you used but turned off stay dimmed in place, and agents you never used wait under More agents, where one click turns one on and offers its install. Add custom agent is the last row, and a single line with Fix all lists only real problems.
- **Cursor works end to end on Windows:** sign-in, its plan approval and mode-switch choices as chat cards, model switching from the chat, questions answered from the chat, and Stop that really stops it.
- **A ZCode session that exits shows a red card with Restart ZCode and Open terminal,** and opens in Chat View even when its hooks never connected, thanks to @Ni7e.
- **Pi starts with the model and thinking level you pick, gets its own status line, and resumes by its session ID;** OMP and Amp get the same turn handling on Windows.
- **Windows: agent CLIs installed through npm or mise (ZCode, Codex, Gemini, Copilot) no longer show as not installed** or fail their version check.

### 💬 Chat
- **Agents can show charts, tables, stat tiles and short layouts in the chat, and open small HTML pages as cards,** on the computer, the web and the phone. Ask for it with `$ghostex-visuals`; install the skill in Settings > Integrations.
- **An answered question no longer flashes back as answerable** before the answer appears.
- **The `# Changelog

, `/` and `@` menus scroll and follow the pointer without lag,** even with a long chat.
- **Links to Windows paths such as `C:\Users\you\.claude\…` open the right file.**
- **A message sent to a session that hasn't started yet starts it,** instead of waiting forever.

### 🧭 Coordinators
- **The Threads panel is one simple list:** working threads first, then the three most recent, and "N more" shows every thread, closed ones included, each still one tap away. A thread stuck on a permission only you can give says Needs your approval.
- **Under an expanded coordinator the sidebar shows threads working or active in the last 2 hours,** with an "N older threads" row for the rest.
- **Hovering a coordinator turns its crown into the fold chevron.**

### 🗂️ Sidebar and Spaces
- **Go to Space 1–9 hotkeys:** Cmd+Option+Shift+1–9 on macOS, Ctrl+Alt+Shift+1–9 on Windows and Linux, following the order of your Space row.
- **Spaces follow the machine you're looking at:** a remote computer shows its own Spaces when it has them turned on.
- **Dragging projects works everywhere:** above the first project, over another project's sessions and threads, and the dragged project stays under the pointer. The project chevron is now part of its header button.

### 👥 Accounts
- **Auto-redeem uses a banked reset 5 minutes before it expires.** Using one as soon as you hit a usage limit in its last 24 hours is a separate switch, off by default.

### 📱 On the phone
- **Tapping a link to a file on your computer opens it:** HTML pages, Markdown, text and code, and pictures.
- **Android: attaching a picture no longer fails with "open failed".**
- **Charts and tables from agents, and the simpler coordinator Threads list, show on the phone too.**

### 🪟 Windows
- **Thin overlay scrollbars in the Files view, browser tabs, the Code view and extension views.**
- **Shutting down Windows no longer shows an error dialog for every session.**
- **The setup intro video plays inside the app.**
- **The Windows app can install Ghostex on Linux remote machines,** which used to show Machine unsupported.
- **Status line commands no longer leave hundreds of stray processes running.**

### 🧩 Extensions and integrations
- **Cloud Boxes is now a built-in extension on macOS and Linux, off by default.** Turn it on in Settings > Extensions; your boxes and logins are kept.
- **Installing Computer Use or Browser Use also installs the Cua Driver skill.**

### 🩹 Fixes and polish
- **macOS: `brew upgrade` no longer fails and uninstalls Ghostex;** Ghostex now leaves Homebrew's `ghostex` link alone and repairs it (#203).
- **Starting a new session no longer closes a session you were using** (#204).
- **Windows: opening Settings while it's already open no longer closes it.**
- **The titlebar Update button is rounded.**

## 10.13.0 - 2026-10-06

**Ghostex 10.13.0 is out.** Ghostex can spend a banked Claude or Codex reset before it expires, Windows gets real system notifications, working sessions can fold into a Working section, the built-in browser can open, save and copy links and images, and Pi works much better.

### 👥 Accounts
- **Ghostex can spend a banked Claude or Codex reset for you before it expires.** It only uses a reset that would otherwise be lost. Turn it on per provider in Settings > Accounts > Auto-redeem expiring resets (off by default).
- **The bell warns in red when a banked reset expires within 3 days and again within 24 hours,** with an optional system notification (Settings > General > Notifications > Reset Expiry Notifications).

### 🔔 Notifications
- **Windows: attention alerts and reset warnings show as real Windows notifications,** and clicking one brings Ghostex forward on that session. Linux shows them through the desktop's notifications.

### 🗂️ Sidebar
- **Group working sessions moves sessions into a collapsed Working section under their project while their agent works.** The session you have open and sessions with an unanswered question stay in Sessions, and a session goes back on its own when its agent stops. Turn it on in Settings > Sidebar or the sidebar's More > Sort & Filter (off by default).
- **Drag sideways with the mouse on an empty part of the sidebar list to switch Spaces,** like a sideways trackpad swipe; one switch per drag, with a grabbing hand cursor.
- **Sidebar setting changes show right away** instead of after a delay.

### 🧭 Coordinators
- **The Threads panel lists the 3 most recently finished threads,** with older ones folded into "N more done", on the computer and the phone.
- **Threads keep the title their coordinator gives them.**
- **A message to an agent is typed at most once,** even when the send is retried after a timeout or a restart.

### 🤖 Pi
- **Pi works on native Windows and reports its state accurately:** a finished turn only when nothing more is coming, Esc as an interrupt, and its dialogs as needing you.
- **The chat shows Pi's thinking, links each tool call to its result, and shows provider errors and Pi's edit diffs.**

### 🌐 Browser
- **Right-click a link or an image in the built-in browser** to open it in a new tab, save it, or copy it or its address. Downloads a page starts ask where to save instead of being dropped.

### 🧩 Integrations
- **Desktop control is now called Fast Computer & Browser Use,** links to the trycua/cua project, and installs the driver's own Cua Driver skill, which the Agent skills list can also install or remove.

### 🪟 Windows
- **The Dev servers panel lists this computer's servers in PowerShell mode** instead of saying it could not inspect WSL ports.
- **`ghostex ports` works on native Windows,** so the phone's Web preview lists this computer's servers with their page titles.
- **Clicking the title bar no longer leaves the chat selecting text as the mouse moves.**
- **The "Paste potentially unsafe text?" confirmation opens above browser tabs and the Files view** instead of being hidden behind them.

### 📱 On the phone
- **Android: the status bar shows the Ghostex flower** instead of a terminal icon.

### 🩹 Fixes and polish
- **macOS: arrow keys move the cursor in the VS Code view** instead of typing red FS/GS/RS/US characters.
- **Every tooltip has rounded corners.**
- **Opening a General setting by name lands on General with the setting found,** instead of an empty search on the last page you used.
- **A `ghostex` command that times out says so** instead of telling you to start Ghostex.

## 10.12.0 - 2026-10-05

**Ghostex 10.12.0 is out.** Make any session a coordinator without interrupting it, run coordinators on ZCode, switch Claude accounts on Windows, copy whole tool calls from the chat, and a new session tidies away the project's empty ones.

### 🧭 Coordinators
- **Make Coordinator turns an existing session into a coordinator without interrupting it.** Right-click a Claude, Codex or ZCode session, open Advanced and choose Make Coordinator, or use the session's menu on the phone. It keeps its conversation, gets the crown right away, and picks up the coordinator playbook once its current turn ends. From a terminal: `ghostex coordinator promote <session>`.
- **Coordinators can run on ZCode, with a model picker** in New Coordinator…, thanks to @Ni7e.
- **Messages between agents name the sender after the message,** so a new thread titles itself from its task instead of from the agent that sent it.

### 👥 Accounts
- **Windows: Ghostex confirms a Claude account switch by checking which account the restarted agent really uses.** Switching on Windows also needs a Claude Swap version with Windows history sharing, which is on its way upstream.
- **Switching the account of a session you never sent a message to works,** instead of failing with "No conversation found".
- **A failed account switch card has a close (X) button** that dismisses it on the computer and the phone.
- **Add account in Settings > Accounts always opens the sign-in form,** and long email addresses in menus are shortened instead of stretching the menu.
- **Account chips fill their row in the sidebar.**

### 💬 Chat
- **Tool calls in the chat are selectable and have a copy-all button** that copies the tool's name, input and result, on the computer, the web and the phone.
- **The spinning working icon no longer shows flickering square lines.**

### ⌨️ Terminal
- **Shift+Insert pastes** in terminals and in text fields such as the chat composer and dialogs, thanks to @rgruenewald.
- **Windows: Shift+Enter inserts a newline in Claude Code.**
- **Windows: no more leftover characters at the left edge of a terminal,** for example after Claude's /usage.

### 🗂️ Sessions and sidebar
- **Starting a new session closes the project's older sessions that are still completely empty:** nothing sent, no chat draft, nothing queued and no text in the agent's input box. This applies to the hotkey, the project's agent button and menu, the New Thread picker and the phone.
- **Windows: a session can no longer take on another session's identity** when Windows reuses a process ID or an agent runs `claude -p` inside it, and a title taken from the wrong conversation corrects itself.
- **Search and Notifications stay in the sidebar's top row;** only Feedback moves into the sidebar menu when the sidebar is narrow.
- **Windows: the sidebar's minimum width is 200 px (300 px on a 150% screen),** so its top row always fits.

### 📱 On the phone
- **SVG project icons show on the phone,** such as a project's favicon.svg.
- **Image chips in the phone's composer no longer leave a stray line after them.**
- **Windows: Easy Connect pairing with the phone works,** because the pairing code now names the port the Ghostex server really listens on.

### 🩹 Fixes and polish
- **Model pickers show this release's model lineup right after an update** instead of the previous release's, thanks to @Ni7e.

## 10.11.1 - 2026-10-04

**Ghostex 10.11.1 is out.** On Windows, Ghostex's small pop-ups now stay with Ghostex instead of floating over other apps, and the agent menu keeps all its rows.

### 🪟 Windows
- **The chat's scroll-down and Esc pills, tooltips, toasts and menus no longer stay on top of other apps** after you switch away from Ghostex, including when one opens while Ghostex is in the background.
- **The agent menu keeps its last rows,** New Coordinator… and Configure, when it opens right after a smaller menu.

## 10.11.0 - 2026-10-04

**Ghostex 10.11.0 is out.** Send feedback straight from the sidebar, new threads fill in and agents start sooner, Freebuff and seven more agents get a status line, and on Windows the window drags smoothly with its dialogs, menus and toasts following along.

### 📣 Feedback
- **New: Send Feedback from the sidebar.** The chat-bubble button in the sidebar's top row opens a pop-up where you describe what's broken, confusing or missing and paste screenshots, review the exact GitHub issue before it goes out, and get a link to the issue once it's filed. If you hit the sending limit, it tells you when you can send again.

### ⚡ Faster starts
- **A new thread's composer fills in much sooner:** its mode and the model chosen at launch show right away, and a model or mode you pick in the composer is never replaced.
- **New agents start sooner,** without a full hook check before every launch and without waiting for the chat to load first.
- **Windows: Claude's status line updates no longer start PowerShell,** so each one arrives in a fraction of the time.

### 🤖 Agents
- **Freebuff sessions drop the "Freebuff:" prefix from their titles,** and the sidebar shows when Freebuff is working.
- **Agents without a status line of their own get one showing the repo and branch:** Freebuff, Pi, OMP, Grok Build, Antigravity, OpenCode, ZCode and OpenClaude.
- **Chat offers "Switch model in CLI" for agents whose models it can't list,** on the computer and the phone.
- **Windows: sessions are no longer named "Windows PowerShell".**

### 🪟 Windows
- **Dragging the window is smoother,** because Ghostex no longer rebuilds a hidden window on every step of the move.
- **Drag the window from the empty top of the sidebar,** and double-click there to maximize it.
- **Overlay windows follow the window when it moves:** the maximized composer, dialogs and their close X, dropdowns, toasts and the Docs drawer.
- **Clicking a dialog's close X no longer flashes the main window.**
- **Tooltips and toasts no longer show a square box behind their rounded corners.**
- **The chat's scroll-down and Esc pills show with transparency on,** including at 125% and 150% display scaling.
- **Transparency is off by default on Windows,** so the window is fast and solid. Turn it on in Settings > Theme > Enable transparency.

### 🩹 Fixes and polish
- **Transparency Strength defaults to 10,** and Blur shows only where it changes something: on Windows once What shows behind the glass is no longer Desktop and windows, and Menu blur only on macOS.
- **macOS: app dialogs move with the main window,** and the Automate dialog behaves like the other dialogs.
- **A Codex question card can still be answered after Codex closed its question box,** thanks to @rgruenewald.
- **The Extensions page has a flatter filter row,** and its count includes only the extensions it offers.

## 10.10.0 - 2026-10-04

**Ghostex 10.10.0 is out.** Coordinators check that the work they hand to a thread actually arrives, Freebuff joins the agents with chat, the phone can start a coordinator and nests its threads, embedded pages can open apps like Okta Verify, and Windows gets a working account sign-in, a red Close button and more reliable remote setup.

### 🧭 Coordinators
- **Coordinators check that a thread received its work.** A new thread reports started only once its brief has arrived, and the coordinator hears about any message that never reached a thread.
- **`ghostex agents send` says whether a message was delivered,** pending, accepted or queued, and control characters in a message no longer act as keys.
- **New Coordinator… is first in a project's agent menu,** and a coordinator keeps the name you gave it.
- **The crew badge counts working threads, else waiting ones, else all of them,** with a tint that matches.

### 🤖 Agents
- **Freebuff joins the agents with simple chat support:** messages, replies, tool results and its questions as chat cards. Its chat appears after the first message, and sign-in and `/model` stay in its terminal.
- **macOS: agents start for fish users whose agent command is only on fish's PATH,** and they keep Ghostex's prompt editor, thanks to @escottalexander.
- **A chat message sent while the computer is busy is no longer lost** when its paste arrives late.

### 📱 On the phone
- **Start a coordinator from the phone's agent menu,** and its threads nest under it as they do on the computer.
- **A Handoff's new conversation opens with the handover link in its chat box.**

### 🪟 Windows
- **Signing in to an account from Settings > Accounts works on Windows.**
- **The window buttons reach the top-right corner, and Close turns red on hover.**
- **Remote setup over SSH finds the SSH server reliably** and says clearly what blocks it, such as missing admin rights, a pending restart or a policy block.
- **The Ghostex server never runs your sessions from an SSH or background session,** and a server started over SSH keeps running when nobody is signed in.
- **A new agent created in another project gets focus.**

### 🌐 Browser and extensions
- **Embedded pages can open other apps and reach local apps after asking,** so sign-ins that use Okta Verify work. Ghostex asks before opening an app or letting a site connect to apps on this computer.
- **Prerender Buddy is in the extension store:** open your Prerender Buddy workspace beside your agent to review AI visibility, crawler access, website health and articles.

### 🩹 Fixes and polish
- **Full Reload keeps focus on the session it reloads.**
- **The chat keeps following new output** at the bottom, and the scroll-down button shows exactly when it doesn't.
- **The chat history rail is shorter and starts below the fork button.**
- **A modal's close X sits on its top-right corner** and shows whenever the pointer is in the modal's top 80px.
- **Dragging a session no longer reveals hidden sidebar sections.**
- **Experimental terminal shaders skip terminals they can't draw on,** thanks to @vespillo-tech.

## 10.9.1 - 2026-10-02

**Ghostex 10.9.1 is out, and it brings 10.9.0 to macOS:** Cloud Boxes, more windows for other monitors and Spaces, menus and dialogs that open on the monitor you are using, and everything else in the 10.9.0 notes below. Every platform also gets files that open in the browser, highlighted chat search and a clear message when a remote computer can't connect.

### 📂 Files
- **Open HTML, Markdown and drawings in your browser** with the open-in-browser button in the Files header. Markdown opens as a rendered page with its diagrams, and a drawing opens as an editor that saves back to the file. The button's Open With arrow, and Open Externally and Open With on a file's right-click menu, open the file in another browser or app.
- **Pinch to zoom an Excalidraw drawing** with the trackpad.
- **The Show files button is the last button in the header row,** so it no longer covers the other buttons, and the Open Files close-all x sits in a circle.

### 💬 Chat
- **Search highlights the words it found,** yellow for every match and orange for the selected one, and scrolls the selected match into view, on the computer and the phone.
- **Cmd+F opens chat search from the chat box,** and typing in the search field keeps its focus when another session switches to Chat.

### 🌐 Remote computers
- **A remote computer that can't connect says why** in its sidebar tab, with Reconnect and Remote Settings buttons, and its cloud turns red.
- **A wrong SSH password is no longer retried every minute,** so the account is not locked out. Saving the new password reconnects.

### 🩹 Fixes and polish
- **The Search by Prompt placeholder is never cut off** by the filter buttons in a narrow window.
- **A modal's close button sits in its corner** and appears when the pointer comes near it.
- **New Folder in Add Project looks like a button you can click.**

## 10.9.0 - 2026-10-01

**Ghostex 10.9.0 is out.** Run agent threads in Cloud Boxes on this computer, in the cloud or on your own server, open more Ghostex windows for other monitors and Spaces, turn Actions, Open In and Spaces on or off as built-in extensions, and get wrapping tables, Antigravity questions in chat and native Find on the phone.

### 📦 Cloud Boxes
- **Run an agent thread in an isolated box instead of on this computer.** Boxes run locally with Docker, in the cloud on Hetzner, Vercel, Daytona, E2B or DigitalOcean, or on your own server over SSH, and the session keeps working like any other: sleep and wake, messages, notifications and its terminal.
- **Set it up from Settings > Cloud Boxes,** or press Set It Up for Me and an agent installs and configures everything, asking before anything that costs money.
- **Pick where a thread runs from the Run on row** in New Thread or above a new chat thread's message box, on the computer, the phone and in the browser. The box starts with your first message.

### 🪟 More windows
- **File > New Window (Cmd+Shift+N) opens another full Ghostex window** for another monitor or Space, also from Quick Access and the sidebar's More menu. Each window keeps its own selection, tabs, panes and views, and every open window reopens at launch.
- **Every window gets the Code view, glass and Keep Awake,** a Window menu lists the open windows, and Cmd+` cycles them.
- **Menus, dialogs and toasts open on the monitor of the Ghostex window you are using,** so the chat's model menu and its other popups no longer appear on the main display. Floating Capture works on every display too.

### 🧩 Built-in extensions
- **Actions, Open In and Spaces each have an on/off switch** under Features in Settings > Extensions. Turning one off hides its pages, buttons, hotkeys, menus and Quick Access rows, and your saved actions, open targets and Spaces are kept for when you turn it back on.
- **SpaceO installs from Settings > Integrations,** giving agents their own headless displays on Apple Silicon computers with macOS 14 or later.

### 💬 Chat
- **Wide tables wrap to fit the chat,** and expanded tables and pictures open over the whole window, on the computer and the phone.
- **Antigravity questions are answered from the chat card,** and a mode pill switches between Default, Accept edits and Plan.
- **A picture that can't be shown says why in a compact card** with its shortened folder.
- **Typing in a long chat is faster,** because the conversation is no longer laid out again on every key.
- **OMP sessions are named OMP in chat,** and messages reach OMP whatever symbol theme it uses.

### 🧭 Coordinators
- **Fold a coordinator's threads with the chevron beside its crown,** and the crown is white on dark themes and black on light ones, on the computer and the phone.
- **The crew badge shows one number,** orange while a thread works and light blue while one waits, and resolving a finished thread closes its session while keeping its conversation to reopen later.
- **Threads for work in another folder start in the coordinator's own project** with that folder in their brief.

### 📱 On the phone
- **Find Prompts is native screens** that open with the search field focused and the keyboard up.
- **The phone chat gets Simple mode, Saved prompts and Switch Account,** and text fields and dialog buttons stay above the keyboard everywhere.
- **The phone's app icon matches the desktop app icon.**

### 🩹 Fixes and polish
- **Experimental terminal shader effects on macOS.** Turn on Enable Experimental Features in Settings > Advanced, then Custom shaders at the bottom of the Terminal section runs the custom shaders from your Ghostty config. Thanks to @vespillo-tech.
- **First-run setup opens on the Ghostex intro video,** with setup continuing as usual when you are offline.
- **The mouse Back and Forward buttons walk the Settings pages again,** and a sign-in that finishes after you leave Settings reopens it at Accounts.
- **Settings > Accounts shows its accounts at once,** and the account editor saves edits as you make them.
- **Settings, Search by Prompt, the Agents Hub, Commit Changes and File Diff open 10% bigger.**
- **A custom agent starts on a remote computer,** and a refused remote start says why.
- **Sessions dropped between pinned rows pin at that spot,** and drop lines show exactly where a row will land.
- **An Excalidraw drawing no longer reloads after its own autosaves,** and the Files comment box opens on the Files view's monitor.

## 10.8.1 - 2026-10-01

**Ghostex 10.8.1 is out.** Coordinators start, brief and supervise a crew of agent sessions for you, Pi and OMP get the model picker, Escape asks twice before it interrupts a working agent, the phone can search every computer's sessions, and Linux gets 10.7.0's features together with its own fixes.

### 🧭 Coordinators
- **A coordinator is one agent you talk to about a stream of work that hands every real task to a thread,** an ordinary agent session it starts and briefs, so it stays free to talk while the threads work in parallel. Start one from a project's Select Agent menu with New Coordinator, then pick Claude or Codex, a model, and an optional goal and first request.
- **Threads sit under their coordinator in the sidebar and in a Threads panel above its chat box,** grouped under Waiting on you, Working and Finished, on the computer, the phone and in the browser. Click a thread to watch it or answer its questions directly.
- **Reports come back by themselves.** When a thread finishes or waits on a question, the coordinator is told, checks the work and tells you what needs you. It can give each thread its own worktree, and it remembers standing instructions and notes you ask it to keep.

### 💬 Chat
- **Pi and OMP get the model picker,** listing every model they can use from the providers you are logged in to, each with only the reasoning levels it supports. Picks apply to that session only.
- **Escape asks for a second press within 2 seconds before it interrupts a working agent,** and a red Agent was interrupted notice shows for 2 seconds afterwards. Turn it off in Settings > Chat > Press Escape twice to interrupt.
- **The model menu keeps the reasoning level you picked** when the highlight moves to another model that offers it, and a model change followed by an effort change shows as one pill.
- **Every session Ghostex opens follows its agent's Default Agent View,** including created, forked, restored and resumed ones, and a name you typed shows as renamed by you.
- **A message from the session's own background agent shows as a Message from card** naming that agent.

### 📱 On the phone
- **Search every computer's sessions from the Sessions header,** with the same rules as the desktop.
- **The phone shows a coordinator's Threads panel,** and its chat and session menu match the desktop's latest changes.

### 🗃 Sidebar, sessions and windows
- **Check for Updates, Restart and Quit are in the sidebar menu and Quick Access.**
- **The session menu lists Rename, Pin, Snooze, Park, Sleep and Note, then Tag As,** on the computer and the phone, and a session waiting on an unanswered question is never put to sleep.
- **A click anywhere in a view focuses it,** so Cmd+W closes the view you clicked in, and with a Browser pane focused the mouse's back and forward buttons, swipes and the Back and Forward hotkeys walk the browser's own history.
- **A file outside the project opens by its real path in Files,** with its images, links and notes working.
- **Tooltips are darker in dark mode,** the update notice fits small screens, and the notification list keeps the 40 newest.

### 🐧 Linux and Windows
- **Linux gets everything from 10.7.0 in this release,** including Floating Capture, one-click installs and Side chat for Codex, since 10.7.0 shipped without a Linux build.
- **Linux: the interface keeps its normal size under Hyprland** even when the launcher drops its environment, and dropdowns have the same solid background as the sidebar's top menu.
- **Windows: Claude Code's status line works,** Fast Computer Use no longer shows the macOS permission rows on Windows and Linux, and the model menu shows Alt instead of the Option symbol.

### 🩹 Fixes
- **Committing a change that deletes an already staged file no longer fails** with pathspec did not match.
- **Native Settings and dialog controls are named for VoiceOver,** and the Sleeping Sessions and Highlight unanswered questions rows are back in Settings.

## 10.8.0 - 2026-10-01

**Ghostex 10.8.0 is out.** Coordinators start, brief and supervise a crew of agent sessions for you, Pi and OMP get the model picker, Escape asks twice before it interrupts a working agent, the phone can search every computer's sessions, and Linux gets 10.7.0's features together with its own fixes.

### 🧭 Coordinators
- **A coordinator is one agent you talk to about a stream of work that hands every real task to a thread,** an ordinary agent session it starts and briefs, so it stays free to talk while the threads work in parallel. Start one from a project's Select Agent menu with New Coordinator, then pick Claude or Codex, a model, and an optional goal and first request.
- **Threads sit under their coordinator in the sidebar and in a Threads panel above its chat box,** grouped under Waiting on you, Working and Finished, on the computer, the phone and in the browser. Click a thread to watch it or answer its questions directly.
- **Reports come back by themselves.** When a thread finishes or waits on a question, the coordinator is told, checks the work and tells you what needs you. It can give each thread its own worktree, and it remembers standing instructions and notes you ask it to keep.

### 💬 Chat
- **Pi and OMP get the model picker,** listing every model they can use from the providers you are logged in to, each with only the reasoning levels it supports. Picks apply to that session only.
- **Escape asks for a second press within 2 seconds before it interrupts a working agent,** and a red Agent was interrupted notice shows for 2 seconds afterwards. Turn it off in Settings > Chat > Press Escape twice to interrupt.
- **The model menu keeps the reasoning level you picked** when the highlight moves to another model that offers it, and a model change followed by an effort change shows as one pill.
- **Every session Ghostex opens follows its agent's Default Agent View,** including created, forked, restored and resumed ones, and a name you typed shows as renamed by you.
- **A message from the session's own background agent shows as a Message from card** naming that agent.

### 📱 On the phone
- **Search every computer's sessions from the Sessions header,** with the same rules as the desktop.
- **The phone shows a coordinator's Threads panel,** and its chat and session menu match the desktop's latest changes.

### 🗃 Sidebar, sessions and windows
- **Check for Updates, Restart and Quit are in the sidebar menu and Quick Access.**
- **The session menu lists Rename, Pin, Snooze, Park, Sleep and Tag As first,** with Note under Advanced, and a session waiting on an unanswered question is never put to sleep.
- **A click anywhere in a view focuses it,** so Cmd+W closes the view you clicked in, and with a Browser pane focused the mouse's back and forward buttons, swipes and the Back and Forward hotkeys walk the browser's own history.
- **A file outside the project opens by its real path in Files,** with its images, links and notes working.
- **Tooltips are darker in dark mode,** the update notice fits small screens, and the notification list keeps the 40 newest.

### 🐧 Linux and Windows
- **Linux gets everything from 10.7.0 in this release,** including Floating Capture, one-click installs and Side chat for Codex, since 10.7.0 shipped without a Linux build.
- **Linux: the interface keeps its normal size under Hyprland** even when the launcher drops its environment, and dropdowns have the same solid background as the sidebar's top menu.
- **Windows: Claude Code's status line works,** Fast Computer Use no longer shows the macOS permission rows on Windows and Linux, and the model menu shows Alt instead of the Option symbol.

### 🩹 Fixes
- **Committing a change that deletes an already staged file no longer fails** with pathspec did not match.
- **Native Settings and dialog controls are named for VoiceOver,** and the Sleeping Sessions and Highlight unanswered questions rows are back in Settings.

## 10.7.0 - 2026-09-30

**Ghostex 10.7.0 is out.** Floating Capture puts a floating button over every app for screenshots and quick prompts, every install button now works with one click on a fresh computer, Mermaid diagrams are drawn in chat, window glass gets Blur sliders, GPT 6.1 Sol and Side chat arrive for Codex, off-screen web pages sleep to free memory, and Linux and Windows get a round of fixes.

### 📸 Floating Capture
- **A floating Ghostex button over every app shows how many sessions are working, waiting for you, or asking a question.** Turn it on at the top of Settings > Integrations > Floating Capture or during setup (off by default), drag it anywhere, or drop it against a screen edge to tuck it away as a thin tab.
- **Screenshot an area, the current app or the full screen from any app,** then crop it, zoom and pan, and add arrows, text and rectangles before it goes into your prompt. Click the button or press Cmd+Ctrl+Shift+S (Alt+Ctrl+Shift+S on Windows and Linux) to open the panel, or use the same keys with A, Space, F or T to run an action straight away.
- **Write a prompt without switching to Ghostex.** The floating prompt box sends to a new session in the project you last used, or to any project or running session you pick, and a prompt you close without sending is kept as a draft in the sidebar. Ghostex switches to the session after a send without coming in front of the app you are in. Screenshots need Screen Recording permission on macOS, and Linux support is X11 only.

- **Floating Capture replaces App Shots,** which is removed together with its settings.

### 🧰 One-click installs
- **Every install button works with one click on a fresh computer and installs what it needs first,** and its tooltip says exactly how Ghostex installs it. Installing an agent that needs npm installs Node.js on its own.
- **Settings > Integrations > Tools lists the tools Ghostex can set up for you:** Node.js and npm, uv, Homebrew, Linux system tools, Beads, and the GitHub and GitLab CLIs. Your own copies are used when you have them; otherwise Ghostex downloads the official release, checks it, and gives you Update, Reinstall and Uninstall buttons.
- **Claude Swap and Codex Swap install with one click,** Install Beads appears on the Kanban notice, and Add Project offers to install the GitHub or GitLab CLI.
- **Trycua is now called Fast Computer Use** in Settings > Integrations, and it installs in the background instead of opening a terminal tab.

### 💬 Chat and agents
- **Mermaid diagrams an agent writes are drawn as diagrams in the desktop chat.** Source switches to the diagram's text, Copy copies it, and the expand button opens a larger view you can zoom and pan.
- **GPT 6.1 Sol takes the Sol spot in the Codex model picker,** and GPT 6 Sol moves under Legacy.
- **In the model picker, Enter now saves the highlighted model and level as the agent's default,** and Option+Enter uses them in this session only, on the computer and the phone.
- **Codex's Sign in with Device Code shows a Sign in to Codex card** with a clickable link and the one-time code, handy on a remote computer, and when Codex quits to the terminal the chat offers Restart Codex on the same conversation.
- **Codex chats get Side chat and `/btw`.** Codex answers in a side conversation in its terminal, so sending one switches to Terminal View. In any chat, typing `/btw` on its own and pressing Enter turns Side chat on without sending anything, and Close hides a side question card at once.
- **Messages between agents show as cards:** a message from another agent shows its first two lines, a message your session's agent sent stays closed, and a click opens either one, on the computer and the phone.

### 🎨 Window glass, windows and web pages
- **Settings > Theme > Transparency has a Blur slider** that sets how soft what shows behind the window looks, plus a Menu blur slider on macOS for menus and tooltips.
- **Settings and Search by Prompt open 30% larger,** app windows such as Settings close with a corner close button instead of a click outside, and tab rails and segmented controls no longer paint an opaque slab over frosted surfaces.
- **Off-screen web pages sleep to free memory.** A browser tab or a Files, website or extension view that has been off screen for its Auto Sleep time closes and reloads when you select it again; tabs playing sound or holding text you have not sent stay awake, and the page another Space opens on stays loaded so swiping there is instant.
- **An open menu hides the tooltips of the window behind it,** typing and shortcuts work right after a dialog closes, a newly opened view tab scrolls into view, and the project header buttons in the sidebar show their hotkeys.
- **Kanban lanes keep a minimum width and the board scrolls sideways,** and long lanes stay fast.

- **Link CLI accepts the `ghostex` command that Homebrew installed** instead of reporting it as a foreign command, and the phone finds the CLI on your computer over SSH.

### 🐧 Linux and Windows
- **Linux: the sidebar and Settings keep a normal size under XWayland** (Hyprland and Omarchy among them), and closing Settings no longer crashes the built-in browser's graphics process.
- **Linux: menus, popups and dialogs no longer get a title bar from the window manager,** they are opaque where the desktop cannot blur behind them, and a restored window keeps its saved size and position instead of drifting on each launch.
- **Windows and Linux: Ctrl+click opens terminal links,** Windows drive and backslash paths are recognised as links, and Quick Access and the Files view show Ctrl instead of the Cmd symbol.
- **Windows: Codex sessions in different projects no longer share one background server,** which could attach one project's conversation to another and leave a chat empty, agent hooks no longer flash PowerShell windows, and the mouse pointer comes back after leaving or deactivating Ghostex, thanks to @gvastethecreator. Restart Codex sessions that were already running.

## 10.6.0 - 2026-09-29

**Ghostex 10.6.0 is out.** Settings, first-run setup, Add Project, the Agents hub and Git Commit are now native windows, Chromium becomes an optional one-time install, the Files editor gains tables, Mermaid and drag and drop, Claude Opus 5.5 and Sonnet 5.5 arrive, and Windows installs get through first launch on a clean computer.

### 🧩 Ghostex without Chromium
- **Settings, first-run setup, Add Project, the Agents hub and Git Commit open as native windows,** so almost all of Ghostex works without the built-in web runtime, and the Settings colour swatches use your system's colour picker.
- **The web runtime is an optional one-time install.** Until you install it, the Browser, the Code view, website and extension views, and HTML and media files show an Install button (or Hide tab), and web links open in your system browser. Nothing is downloaded until you press Install; Settings > Extensions > Built-in > Chromium runtime installs, reinstalls or uninstalls it.

### 🗂 A richer Files editor
- **Markdown in Files gains table tools, emoji, Mermaid diagrams, smarter list keys, find and notes,** with a richer gutter and rename and conflict dialogs.
- **Drag and drop files and folders in the Files tree,** and opening a file from chat always goes to the Files view.

### 💬 Chat and agents
- **Claude Opus 5.5 and Sonnet 5.5 replace Opus 5 and Sonnet 5** in the model pickers for Claude Code and Cursor, and Opus 5.5 is a single 1M row.
- **Claude Code 2.1.284 works with every chat picker:** the effort slider knows the new Ultracode switch, session-only model and effort picks answer Claude's Switch model? prompt, the model picker works in short panes, and a 1M session keeps its 1M pill.
- **A new Claude Code install's first-run setup is answered in the chat,** with a Sign in to Claude card that tells you to paste the code your browser shows.
- **A collapsed approval shows the command it asks to run,** compaction shows its token counter, and the Note, Stash, Attach and Switch to Terminal tooltips name their hotkeys, on the computer and the phone.
- **Codex plans show above Implement this plan?,** a Codex script that runs several commands lists each one, and an interrupted turn keeps its Interrupted marker under your prompt.
- **Hermes chats keep every prompt and reply across a compaction,** Hermes /compress works like /compact, and subagent notices and errors read the way the terminal prints them, thanks to @banozz0.
- **Chat rides out a gxserver restart** instead of saying gxserver is not reachable, links take the chat's own light or dark link colour, and the scrollbar stays on the pane's right edge in a wide chat.

### 🗃 Sidebar, Spaces and sleeping sessions
- **Dim sleeping sessions and Wake sleeping sessions when selected** are new in Settings > Advanced. Turn off the second one and clicking a sleeping session opens it with its Resume button instead of waking it.
- **Grouping never moves a project out of its Space.** A new group joins its project's Space, a project taken out of a group stays in that Space, and adding a project to a group in another Space switches the sidebar there.
- **Marking a notification read clears its session's done or needs-input mark** in every sidebar, and a newly pinned session goes to the bottom of the pinned list.
- **Search by Prompt follows your theme and window glass,** and its preview text can be selected and copied.

### 🪟 Windows
- **A clean Windows install gets through first launch and onboarding** without the Visual C++ runtime, and a slow first start (often the antivirus scan) is waited for, with Retry if it fails.
- **Agent CLIs installed where new terminals cannot find them offer Add to PATH,** and Git and the GitHub CLI installed while Ghostex runs are picked up without a restart.
- **Opening Ghostex again hands off to the running window at once,** and slow first session starts no longer fail the session.

### 🩹 Fixes
- **A woken Hermes session resumes under its own profile,** even when the project's Hermes agent uses another one, thanks to @banozz0.
- **Attaching to a session after Codex's fullscreen view or Claude Code quit keeps the shell history** instead of showing an empty screen.

## 10.5.5 - 2026-09-28

**Ghostex 10.5.5 is out.** A big round of Windows fixes, from agent startup and accented typing to mouse Back and Forward, Hermes bot sessions that keep their status and chat, large files that open at any size, an optional highlight for unanswered questions, and keyboard and terminal fixes.

### 🪟 Windows
- **Agent sessions start reliably on Windows,** using each CLI's own launcher and PowerShell for Grok, and a session that fails to start says why instead of hanging, thanks to @gvastethecreator.
- **Accented characters compose correctly in the terminal.** A dead key waits for the next letter instead of typing the accent on its own, thanks to @gvastethecreator.
- **Mouse Back and Forward buttons follow your visited sessions and projects,** and inside Settings they move between the pages you visited and restore each page's scroll position, thanks to @gvastethecreator.
- **Model controls appear as soon as their data arrives,** instead of waiting on a skeleton until you hover them, thanks to @gvastethecreator.
- **Menus behave like native ones:** sidebar menus close when you switch to another app, frosted menus no longer close before your click lands, and the pointer updates correctly over pop-ups, thanks to @gvastethecreator.
- **Terminal block and border characters draw without gaps,** thanks to @gvastethecreator.

### 🤖 Bots and Hermes sessions
- **A Hermes session keeps its own status, chat and name** when its subagents or nested runs report in, and its subagents show in the Subagents card, thanks to @banozz0.
- **Hermes chats show every step message,** and `/rename` in a Hermes chat renames the conversation in Hermes too.
- **The default Hermes bot always opens as itself,** even after `hermes profile use` switched Hermes to another profile.
- **Sessions no longer blink idle mid-turn,** including while Claude compacts automatically.

### ⌨️ Keyboard and terminals
- **Typing keeps working after you click beside the terminal.** Backspace and Pinyin input no longer stop after a click on the terminal's margin or the agent bar.
- **Clicking empty space in a chat sends your typing to the chat box.**
- **The terminal find bar works on Windows and Linux:** typing, Enter, the arrows, Escape and the close button all act on the search.
- **The terminal's padding always matches its background,** in dark and light mode, and waking a session in light mode no longer flashes white.

### 🗂 Files and the sidebar
- **Files opens large files at any size.** Only Markdown keeps its 2 MB editing limit, and a larger Markdown file offers Open in Code view.
- **Highlight unanswered questions** gives sessions with a waiting question a soft pink background, thanks to @alp82. Turn it on in Settings > Sidebar with Show Advanced on; it currently covers Codex's questions.

## 10.5.0 - 2026-09-28

**Ghostex 10.5.0 is out.** Bots turns the sidebar into your Hermes agents with a feed of every scheduled run, glass backdrops come to Windows and Linux, terminals start on pure black or white, chat shows a delivery icon beside each prompt, Codex's fullscreen view works everywhere, and a large batch of Windows fixes.

### 🤖 Bots for your Hermes agents
- **Bots switches the sidebar to your Hermes agents,** one row per Hermes profile with its gateway status, Edit SOUL and Edit config, pinned Actions and a + that starts a new chat with that bot, thanks to @banozz0. Turn it on in Settings > Extensions under Planning and automation; it appears only on a computer with the Hermes CLI installed.
- **Hermes conversations started elsewhere show up under their bot.** Chats begun in a terminal, the Hermes app, Discord or Telegram are listed with the bot that had them and resume with one click.
- **Bot automations collects every Hermes cron run in one feed,** thanks to @banozz0. An Automations row at the top of Bots opens a channel per scheduled job, an all-runs channel, your own groups, unread counts and a bot filter, with failed runs marked.

### 🪟 Glass on Windows and Linux
- **Wallpaper, Picture and Live backdrops now work on Windows,** so the glass can show your wallpaper, a picture you choose or a calm animation behind the window.
- **Every transparency setting works on Linux,** thanks to @alp82.

### 💬 Chat
- **A delivery icon beside your prompt replaces Waiting and QUEUED.** A spinner means the message is waiting for the agent; a play button means it is queued behind the running turn, and clicking it interrupts the agent so it takes your message now. The phone shows the same.
- **The chat box slides closed over its draft** instead of fading, on the computer and the phone.
- **Right-click a previewed image** for Copy Image, Copy Path, Save Image, Reset Zoom and Dismiss.
- **Sending closes Claude's open side question first,** so the message is never stuck behind the side-question panel, and the side question's Fork button explains what it does.
- **Codex's fullscreen view works with Ghostex.** Command-click still opens links, Stop works while you are scrolled up or selecting text, and automations and model picks keep working with Codex 0.157 and later.

### 🖥 Terminals and accounts
- **Terminals start on pure black or white.** Settings > Terminal > Terminal background offers Black / white (the new default), Follow theme, or Custom color, and a custom colour you set before is kept.
- **Terminal panes start without side padding,** so the terminal uses the pane's full width.
- **Update, reinstall or uninstall Claude Swap and Codex Swap from Settings > Accounts.** Hover a button to see the installed and latest versions; uninstalling keeps your saved logins.
- **Reconnecting a Codex account no longer has to stop its sessions first;** when Codex still needs them stopped, Settings offers Sleep sessions and continue.
- **Find by Prompt opens in a native window** that matches the rest of the app.

### 🔧 Windows fixes
- **Chat messages send reliably on Windows** now that the background service no longer freezes on a send.
- **No more flashing console windows** when Ghostex starts helpers in the background.
- **Windows sessions take their agent's title again,** instead of staying "Claude Session".
- **Onboarding installs agent CLIs and skills correctly, and a custom install folder is found everywhere,** thanks to @gvastethecreator. Onboarding and Settings > Agents install and update Claude, Codex, Cursor and Grok, and newly installed CLIs are found without restarting.
- **Codex runs without administrator rights,** and Add Project's Local folder opens the list of drives with your home folder first.
- **Settings sliders show their thumbs** as soon as the window opens.

### 🩹 Fixes
- **The Linux browser keeps WebGL working** after a browser view closes, thanks to @alp82.
- **The Files view's Send button shows how many new notes are waiting,** and the separate Review menu is gone: Send covers the open file's notes.
- **The sidebar's Search button is always an icon,** with its label and shortcut in the tooltip.
- **Claude chats show the context size Claude reports,** and server errors show the actual reason instead of a bare error code.

## 10.4.0 - 2026-09-27

**Ghostex 10.4.0 is out.** Docs becomes Files and opens your whole project, including pictures, video and audio; side questions and Claude's panels in chat; Hermes chats that name their bot and switch models; a red dot when a model change fails; Shift+Esc for the Commands panel; and a batch of chat and Windows fixes.

### 🗂 Docs is now Files
- **The Docs view is called Files and shows your whole project.** It lists and searches every file in the project, on this computer and on remote machines, not just the docs folders.
- **Pictures, video and audio open in Files.** Images and SVGs show in place with an Edit source toggle, video and audio play with seeking, and PDFs and formats Files cannot show open in your system app.
- **Choose where media from chat opens.** Settings > Tools > File opening has Images, Videos and Audio, each set to Files or System App, for files linked in agent chat or the terminal.
- **Open any file by path.** The Open File command (set a key in Settings > Hotkeys) opens Files with its search ready: type a name or paste any path on this computer and press Enter.

### 💬 Side questions and a tidier chat
- **Ask a side question without stopping the agent.** In Claude Code chats, `/btw <question>` answers in a card above the chat box with Copy, Fork and Close, and the side question stays folded in the chat afterwards; turn on Side chat in More actions to send every message that way until you remove its pill.
- **Claude's panels read like panels.** `/status`, `/usage` and similar panels show as clickable tabs, tables and usage meters instead of terminal text.
- **Skills show the way each agent calls them.** A skill picked with `$` shows as `/name` for Claude Code and `$name` for Codex, and a skill you sent reads as your own turn before and after a reload.
- **One View menu for display modes.** More actions > View holds Simple, Verbose and Summary as separate switches, and Simple mode is now on by default.
- **Replies keep their line breaks,** and text such as `Option<String>` or `<my answer>` shows exactly as written instead of disappearing.
- **Approval cards ask about the tool that is waiting** ("Allow this edit?"), Codex 0.157's command approval shows as a real approval card, and a turn the agent wrote no reply to shows what ended it.
- **A rewind stays rewound** even when Ghostex restarts before your next prompt.
- **Zoomed chat images keep the spot you clicked under the pointer,** and you can drag a zoomed picture around.
- **The chat box always shows its blinking caret when it has focus,** including in pop-up windows and after switching back to it.
- **The phone's chat gets side questions, Claude's panels, Side chat and the View menu too.** Update the app to get them.

### 🤖 Models and Hermes
- **Hermes chats name their bot and switch models,** thanks to @banozz0. A chip before the model pill names the bot (for example Harry), the status line shows its context, cost and tokens, and the model picker lists the bot's models with Low, Medium and High, applied to this session only; the phone shows the bot's chip too.
- **A model change that fails turns the session red.** If the agent refuses a model, effort or mode you picked, or it keeps failing for 30 seconds, the session shows a red dot with the reason, and your messages wait instead of reaching the agent on the wrong model; pick a model again to clear it.
- **Opus 5.5 is one row again,** starting on the 1M context window, and the model pill names the window only when it is 200K.
- **Claude effort changes from chat work on every session,** without an extra "/effort" message in the transcript, and model changes no longer wait on the faint suggestion text in Claude's input box.
- **The phone's model picker drops the reasoning chips from its rows** for a cleaner list.

### ⌨️ Menus and hotkeys
- **Shift+Esc opens the Commands panel.** It is Open Commands Panel's new second key; Focus Chat Box no longer has a default key, so set one in Settings > Hotkeys if you used it.
- **Summary mode is Option+Ctrl+S on macOS.** Windows and Linux keep Ctrl+Alt+Shift+S.
- **Session menus are shorter.** Split Right, Copy Resume and Copy Attach are gone from the sidebar and chat menus; Option+Shift+D still moves the focused session into a pane on the right.
- **The view menu reads as three groups,** built-in views, website views and your own views, and a submenu such as Show in opens in the same place.
- **Closing a side panel's last tab now closes the panel.** Turn off Close side panel with its last tab in Settings > Sidebar to get the view picker back.
- **Saved Prompts and other dialogs stay in front** when a tooltip or menu appears behind them, and table previews, Mermaid diagrams and browser history open in native windows.

### 🔧 Windows fixes
- **Dragging the titlebar moves the window reliably,** and the project breadcrumb sits centred.
- **The Settings window comes to the front when you open it** and draws its sliders at full size.
- **Messages sent to PowerShell sessions arrive intact,** and the Code view shows its editor again, thanks to @gvastethecreator.
- **Text renders consistently with bundled fonts,** thanks to @gvastethecreator.

### 🩹 Fixes and updates
- **The built-in browser moves to Chromium 154** with its latest security fixes, the Code view to VS Code 1.139.1, and terminals to the latest Ghostty.
- **Video behind the glass is now Your video under Live.** Pick your own .mov, .mp4 or .m4v for dark and light mode; the downloadable Ghostex videos and the aerial list are gone.
- **Settings tells you when the system keeps the window opaque,** naming Reduce transparency on macOS or Transparency effects on Windows and where to change it.
- **A refreshed app icon.**
- **`ghostex rename-command` reaches sleeping sessions,** `ghostex create-session --input` runs its text on first start, and a session created in a project parked in Recent Projects brings the project back to the sidebar.
- **Ghostex web keeps the session it shows awake** instead of letting Agent Auto Sleep put it to sleep, and shows the account usage meter, thanks to @gvastethecreator.

## 10.3.0 - 2026-09-26

**Ghostex 10.3.0 is out.** Live animated glass and video behind the window, glass on Windows, a fully native chat on Android, OpenCode in Chat, seven new website views, a keyboard-driven model pop-up, flicker-free terminal switching, Summary mode that keeps every reply, and a large batch of chat, remote, Windows and Linux fixes.

### ✨ Live glass, video and a new look
- **Live glass backgrounds on macOS.** Choose Live for what the glass shows and pick one of eight calm animated styles (Aurora, Ink, Drift, Nebula, Silk, Bokeh, Waves, Mesh) drawn in your theme's colours, with Speed and Brightness controls; it loops without a seam and pauses in the background, on battery and in Low Power Mode.
- **Video behind the glass.** Play a muted, looping, blurred video behind the window: the one that comes with Ghostex, an aerial wallpaper your computer has already downloaded, or your own .mov or .mp4 file.
- **Window glass comes to Windows (beta).** The glass controls now work on Windows 10 and 11 with the system's acrylic blur; turning glass on applies at the next launch, and turning off Transparency effects in Windows Settings keeps the window opaque.
- **Frosted menus, tooltips and dialogs.** Menus, tooltips, Quick Access menus, the Docs formatting toolbar and app dialogs take a see-through frosted look under glass, and header menus open just below their button.
- **A reworked Theme page.** Sixteen colour presets per appearance, a Colourfulness setting from Subtle to Vivid, More options in each group, and Use transparency set to Dark only by default; new installs start at Transparency strength 20.
- **Split buttons and view tabs get a cleaner outline,** and tooltips that only repeated a button's label are gone.

### 📱 A native chat on Android
- **The phone's chat is now fully native,** running on the same chat engine as your computer, so replies, cards, the chat box and the light theme look and behave the same on both. Update the app from Settings in the app to get it.
- **More of the chat on the phone:** the transcript menu, Save as Markdown, Handoff, fork branches and the model menu, and session rows show a stack of status dots with sidebar-style menus.
- **Browse and open a project's Markdown and HTML docs on the phone.**
- **Settings on the phone are split into pages** (Connection, Chat, Terminal, Theme, Sounds, Keyboard, Updates, Advanced), and the web preview shows a load progress bar.
- **Drafts follow you between devices.** A draft saved on another device is offered when you open the chat, and the chat box saves your draft whenever you pause typing.

### 🧩 OpenCode in Chat
- **OpenCode sessions work in Chat.** Streamed replies, reasoning, tool results, image attachments, questions, permissions, queued prompts and rewind all work; install its hooks in Settings > Agents, then start a new OpenCode session.
- **Pick OpenCode models and modes from the chat box.** The model picker lists your OpenCode providers' models and reasoning levels, the mode control switches between Build and Plan, and a message starting with `!` runs a shell command in the session.

### 🤖 Models and accounts
- **Pick a model and effort without leaving the keyboard.** The model pop-up replaces the quick picker: type to filter, Up and Down move, Left and Right change reasoning, Enter applies to this session and Shift+Enter saves it as the default; Option+P opens it on macOS.
- **F and C switch Fast mode and the context window** while the search box is empty, Option+1 to Option+9 highlight the first rows, and Option plus a letter presses the pop-up's bottom buttons.
- **Hand a conversation to another agent** by picking one of its models in a started session, and switch between signed-in Claude or Codex accounts from the Account button; agent tabs show a badge after a handoff.
- **The same picker in terminal view.** An agent terminal's bottom bar shows the model pill; turn it off with Model picker in terminal view.
- **A Claude model pick keeps the effort you saved,** and a pick the agent refuses returns the pill to the model it is really using.
- **Claude reset credits in the usage panel.** See your reset credits and redeem a reset without leaving Ghostex.

### 💬 Chat
- **Summary mode shows every reply to a prompt,** with the newest one open, and has its own hotkey: Cmd+Ctrl+S on macOS, Ctrl+Alt+Shift+S on Windows and Linux.
- **Chat hotkeys work from anywhere in the window.** Focus Chat Box (Shift+Esc), Copy Last Code Block (Cmd+Shift+;) and Copy Last Reply (Cmd+Shift+C) act on the chat of the session you are in, and they show in Quick Access.
- **Finished work folds away.** When the agent finishes a turn its tool work folds under "Worked for Xs" above the reply; click it to open it again.
- **Every expand and collapse animates,** an opened fold keeps its header in place, and sleeping sessions open straight into chat and fade in instead of showing a skeleton.
- **The transcript minimap sits on the right,** and hovering a mark previews its prompt and reply as formatted text.
- **File and folder pills show their parent folder and name,** and a long one shortens from the start so the file name stays visible.
- **Tables start neat.** Wide tables wrap and scroll, and a larger preview window shows the whole table.
- **The chat box edits like VS Code:** move, duplicate, delete and select lines with the usual shortcuts, and cut or copy the whole line with nothing selected.
- **Messages you send while a model or mode switch is settling wait for it** instead of being refused, and a queued message that could not be delivered returns to the chat box with its reason in a tooltip.
- **The Subagents card shows Claude's workflow runs,** and phone session rows show waiting questions and background work.
- **Videos, audio and PDFs in chat open in your default app.**
- **Cursor chats show their branch and lines changed** in the status line and More details.
- **Codex question replies read as normal answers,** and Copied! appears on the control you clicked.

### 🧭 More views beside your sessions
- **Sentry, Figma, Vercel, Supabase, GitHub Actions, PostHog and Custom Website views.** Turn on the ones you want in Settings > Extensions, open one from the + menu and paste its home address; each project remembers its own, and Custom Website opens any site.
- **The view picker opens and animates like a view,** its cards show one line of description with the rest on hover, and the panel toggles stay in place.
- **A view tab's right-click menu groups its options** under Configure and Show in, where you choose whether it shows in this project or this Space.
- **Close side panel with its last tab** (Settings > Sidebar) closes the whole panel instead of showing the view picker, and the side panel's view stays awake while you move between Spaces.

### 🗂 Sessions, terminals and the sidebar
- **Switching sessions in terminal view no longer flickers,** and terminals keep a steady picture while the sidebar, chat or pane size changes, thanks to @Ni7e.
- **New Agent Session starts the agent you used last,** and a new chat you leave empty closes by itself.
- **Drag a session onto Pinned, Sessions or Parked** to pin, unpin or park it; an empty section shows its heading while you drag.
- **Forking switches you to the fork,** named "Fork: <original name>", and its chat shows the history it inherited.
- **Auto Sleep and Close After Done keep working with the window closed.** Ghostex's background service runs them now, and Auto Sleep never sleeps a session you are looking at.
- **Status dots stack under Spaces and remote machine tabs,** so you see at a glance who is working.
- **Project icons are found in nested app folders,** and projects without one show their first letter, thanks to @alp82.
- **Drop a session that has no tab yet into the middle of a pane** to show it there.
- **Settings and the other app windows open faster,** because Ghostex prepares their window in advance.
- **The Kanban board shows skeleton cards while it first loads.**

### 🌐 Remote, Windows and Linux
- **Files dropped onto a remote machine's terminal are uploaded to that machine** and pasted as references the agent there can open, instead of local paths it cannot read, thanks to @Ni7e ([#159](https://github.com/maddada/Ghostex/pull/159)).
- **Saved remote machines connect when Ghostex starts** and reconnect on their own after a dropped connection, and Project Groups and Spaces edited on a remote machine's tab are saved on that machine.
- **Remote work between Windows, Linux and macOS is more reliable,** and Add Project can browse the selected computer's drives, including Windows paths from a Linux or macOS client.
- **Windows runs Ghostex's background service and sessions with standard user rights,** even when started from an administrator terminal or SSH login, so Codex can start its background server.
- **Windows terminals keep their history** when a pane is resized or reattached, and modified keys and Unicode characters reach the terminal intact.
- **Windows app dialogs have a system border** so they stand out from the app, and paths from the command line open in Code.
- **Linux dialogs stay attached to their window and open in the right place,** thanks to @alp82, and the window reports its app name correctly.
- **Linux attachments offer Images or files and Folders** before the system picker opens.

### ⚙️ Settings and the command line
- **Update, reinstall or uninstall Trycua from Settings.** The Trycua row offers a one-click update when a new version is out, and uninstalling keeps its Accessibility and Screen Recording permissions.
- **Copy Details is always in a session's Advanced menu** and carries exactly what another agent needs to reach that session.
- **The Debugging page is simpler:** ten diagnostic log areas with one Turn logs off after picker.
- **Manage hotkeys from the command line** with `ghostex settings hotkeys`, read a whole chat thread with `ghostex read-session-chat`, and `ghostex agents send` now wakes a sleeping recipient; `ghostex sessions --json` prints a short list and `--full` the whole inventory.

### 🩹 Fixes
- **Chat sends work on computers without the ghostex command installed;** Ghostex now installs and uses its own copy instead of opening an editor.
- **A message ending in a $skill or @file mention sends on the first Enter.**
- **Claude questions are answered reliably,** including long ones Claude draws behind a side border, and Codex's Folder access trust prompt is recognized.
- **Claude interrupts, rewinds, pasted content and trust prompts** behave correctly in chat, and Claude sessions keep their identity once a transcript is written.
- **Codex questions asked in the background** show up and can be answered.
- **Terminals show their complete history** when you first attach to a running session.
- **Parking a session with a tag keeps it parked,** and older turns keep their files-changed row.
- **Sleep Inactive puts browser tabs to sleep again,** and Keep Awake and Join Discord in the More menu work.
- **An App Shot goes into the focused agent, chat or terminal,** and Delayed Send from its hotkey shows the countdown.
- **A fast Ctrl+Tab walk across projects lands on the row it counted to,** and the Loading sessions notice clears when Ghostex reconnects.
- **Browser tabs load into an empty New Tab** and every tab can be closed.
- **Cmd+F keeps the find bar focused,** the Maximize button opens the maximized chat box, chat menus show hover again, and queued message buttons show their icons.
- **The scroll-to-bottom button no longer lingers** after its chat is gone.

### ⚠️ Heads up
- **New default hotkeys.** Cmd+T now opens a browser tab, Cmd+Shift+T a terminal, Cmd+N the New Thread picker, and Cmd+Shift+O starts the agent you used last; change them in Settings > Hotkeys or with `ghostex settings hotkeys`.

## 10.2.0 - 2026-09-23

**Ghostex 10.2.0 is out.** Window glass that shows your desktop, your wallpaper or a picture of your choice through the app, a new Theme page in Settings, panels that slide open and closed, a faster native Kanban board and Automate page, drag panes by their grip, and a round of Windows and Linux improvements.

### ✨ Window glass
- **See your desktop through Ghostex.** The sidebar, sessions, terminals and chat can show a blurred view of what is behind the window. Glass is on in dark mode by default; turn it on or off with Enable Transparency in Settings > Theme.
- **Choose what the glass shows.** Desktop and windows shows everything behind Ghostex, Wallpaper only shows just your desktop picture, and Custom image blurs a picture you pick, with a separate picture for dark mode and light mode.
- **One slider sets how much shows through.** Transparency strength on the Theme page tunes the whole window at once, and Advanced still lets you set the sidebar and work area tints apart.
- **Menus, popups, toasts, tooltips and suggestions are frosted to match,** as are the scroll-to-bottom button, the floating sidebar and the chat skeleton shown while a session loads.
- **Tune how much shows through.** Sidebar tint and Work area tint set each area on its own, in dark mode and in light mode, so either can be the darker one. Always glass keeps it on in light mode too.

### 🎨 A Theme page in Settings
- **Theme has its own page, right below General.** It starts with Appearance, a card for each dark and light theme showing the window in its colours, and Enable Transparency; Advanced holds colours, chat and terminal themes, glass and the app icon.
- **Background contrast in five steps.** Pick Lowest to Highest on the Theme page, or set Sidebar contrast and Work area contrast apart under Advanced.
- **Terminals follow your theme.** The terminal background matches the theme by default, and a colour you choose only paints behind the terminal text.
- **Pick your look while getting started.** The last Get started panel offers the same Appearance, theme, contrast and transparency choices, and they apply right away.

### 🗂 Panes, views and the board
- **Panels slide open and closed.** The sidebar, side panel, Agents Panel and command pane slide in and out; set Panel animations to Off, Slow, Normal or Fast, and Reduce Motion always opens them instantly.
- **Kanban and Automate are faster and match your theme.** Both are now drawn by Ghostex itself, so they open instantly, follow your theme colours and take the keyboard like the rest of the app.
- **Move or close a pane from its grip.** While the screen is split, the focused pane has a small bar along its top: click it to close the pane (its sessions keep running) or merge all panes, or drag it onto another pane.
- **Drop a session in the middle of a pane to show it there.** Dropping on a pane's edge still opens a new split.
- **Sleeping panes show a slim pill** with the session's icon, title and Resume, instead of a large card.
- **Opening a project or restarting Ghostex no longer wakes its sleeping sessions.** They show their Resume bar until you click the pane or press a key, while clicking a sidebar row still wakes them, thanks to @banozz0.
- **Cmd+Option+Arrow moves between session panes and Commands**, skipping the view panel.
- **Middle-click an empty spot in a command pane's tab bar** to close all of its terminals at once.

### 🧭 Sidebar
- **Hide sidebar and Toggle Agents Panel sit at the top left of the sidebar on every computer,** and dragging the sidebar's top row moves the window.
- **A narrow sidebar keeps its buttons.** When the top row runs out of room, Search and Notifications move into the sidebar menu.
- **Switching Spaces fades the list** as it slides, from the Spaces row and from the menu alike.
- **Double-click the top of the sidebar or the view tab strip to zoom the window,** like the header beside them.
- **Hover the window's left edge to float a collapsed sidebar.** The hover zone is now invisible, so nothing is drawn over your work.

### 💬 Chat
- **Cursor chats get a status line and Context details.** Hover the status line and click the pen after its last item to choose what it shows, for Claude Code, Codex and Cursor.
- **The chat box collapses while you scroll again by default,** and in a short pane, such as half of a stacked split, it stays collapsed until you click it.
- **Paste images into question answers.**
- **Sessions that are still loading show the chat box and status line right away,** and the chat box takes focus as soon as the agent picker closes.
- **The account switch card uses the compact account tiles.**

### 🪟 Windows and Linux
- **Close, minimize and maximize stay at the top right on Windows,** even with the sidebar or a view open.
- **Linux computers can connect to Windows computers over SSH,** and Linux keeps saved SSH passwords in the desktop keyring.
- **Windows terminals keep their output, interrupts and sessions more reliably,** and file dialogs and popups open correctly.

### 🩹 Fixes
- **Answering Claude's questions from chat works again.** Multi-select answers with a typed note deliver the note and every pick, questions with previewed options no longer look answered while Claude still waits, and answered preview questions show the option you picked.
- **Popups and dialogs open on the display the window is on,** instead of behind it on another monitor.
- **Claude sends go through even when its chat box was partly erased on screen.**
- **Waking a Claude session never resumes a conversation from another project.**
- **A new agent started right after a restart appears straight away** instead of being replaced by a woken session.
- **Custom Claude profiles and projects on different Windows drives are told apart correctly,** thanks to @banozz0.

## 10.1.0 - 2026-09-23

**Ghostex 10.1.0 is out.** Linear, Jira, GitHub, Storybook and Terminal views, light and dark theme presets, a Trust and Remember answer for folder trust prompts, drag a session onto a pane to split, account usage that peeks on hover, and the newest Claude, Codex, Cursor and Grok models.

### 🧭 New views beside your sessions
- **Linear and Jira open as views.** Pick one from the + menu, paste the workspace, team, or board address you want as its home, and Ghostex keeps it per project; worktrees follow their parent project until you give them their own. Middle-click or Cmd-click a link to open it in a background Browser tab.
- **A GitHub view opens your repository with no setup.** It appears whenever the project has a GitHub origin, including in worktrees.
- **Storybook is built in.** Projects with Storybook get a Storybook view (press S in the view picker) that builds and serves the component workshop without keeping a server running, with Rebuild Storybook and Annotate with Agentation.
- **A Terminal view puts command terminals on the right.** It has its own tabs and splits, runs Actions, and keeps its terminals alive when you close it; the Commands pane under your sessions still works as before.
- **The view picker shows real icons.** Installed extensions use their own icon and description, and website views show their mark.
- **Sleeping browser tabs show what they hold.** The card shows the page's icon, title and site, and a Resume button wakes it.

### 🎨 Theme presets
- **Pick a preset for each appearance.** Dark theme offers Dark Gray, Black, Blue, Green, Red, Purple, or Custom, and Light theme offers Light Gray, White, Blue, Green, Pink, Orange, or Custom, under Settings > Appearance.
- **The theme reaches more of the app.** The sidebar, window chrome, sidebar menus, chat background, and chat and file change cards all follow the chosen preset, and Custom keeps your own contrast and tint.

### 🗂 The Agents Panel and sidebar
- **The sessions column is now the Agents Panel.** Its toggle sits beside Hide sidebar, even while the sidebar is collapsed.
- **Drag a session onto a pane to split.** Drop a sidebar row on the left, right, top or bottom edge of a terminal or chat pane to open it in a new split there.
- **Account usage peeks on hover.** Hover the chart button in the sidebar's Commands row to float your starred accounts' meters over the list, and click the pin to keep them there.
- **Sleep Inactive on the Space menu.** Right-click a Space icon to sleep only its sessions that are awake but neither working nor waiting on you.
- **The floating sidebar and Agents Panel slide at your chosen speed.** They follow the sidebar's Collapse animation setting, and 0 turns the slide off.
- **Quick Access opens a session in the floating panel** while chat is collapsed, and your configured hotkeys now work from the floating sessions panel.
- **Sessions with background shells or monitors still running show as working.**

### 💬 Chat
- **Trust and Remember answers folder trust prompts for good.** Choose it on the trust card once and Ghostex answers that project's trust prompt by itself from then on, for every agent.
- **Every copy says Copied!** A small bubble appears at the pointer whenever you copy something anywhere in the app.
- **The chat box stays expanded while you scroll.** Turn off Settings > Chat > Keep chat box expanded while scrolling to get the collapsing box back.
- **Switching a draft's agent keeps your model and effort.** The picked model and reasoning carry into the new agent's launch.
- **Switching accounts is safer for Claude background sessions.** Ghostex stops the conversation and its remaining jobs, then resumes it on the selected account.
- **The model picker explains itself.** Hover a row's eye to read what the model is for, unavailable buttons are dimmed and say Default or Off, Auto sits first, and a detected Fast mode shows as on.

### 🤖 Newest models, without waiting for a release
- **New models reach your open app within minutes.** The model list updates on its own, so a new model shows up and can be picked without installing a new Ghostex.
- **Claude matches Claude Code's own picker.** Opus 5.5 is the Opus model with 1M context, Opus 5 leaves the list, and the quick picker shows every Claude model with its version.
- **Codex gets GPT 6 Sol and GPT 6 Luna.** Older GPT 5.6 models move under Legacy, and titles and commit messages are now written with GPT 6 Luna.
- **Grok 4.7 and Grok 4.7 Fast for Grok Build and Cursor.**

### 🩹 Fixes
- **Linux shows new versions in the Update window.** Open download page takes you to the release; install it the same way you installed Ghostex.
- **Windows chat sends reach the terminal again,** and the prompt editor starts correctly on Windows.
- **The transcript scrollbar no longer jumps while a reply streams.**
- **A new usage limit shows even right after the old one cleared.**
- **Worktree dialogs explain why a list failed,** and pick the right source project.
- **Remote sessions attach more reliably,** retrying each session on its own.
- **The GitHub button opens the right repository** as soon as a project loads.
- **Agents Hub sync is easier to read.** It says how many agents are out of sync and lists each fix in plain words, with a To fix and In sync filter.
- **Better screen reader support** for the sidebar, chat box, transcript and terminal.

## 10.0.1 - 2026-09-22

**Ghostex 10.0.1 is out.** A new layout: the titlebar row is gone, views open as tabs beside your sessions instead of over them, and browser pages share the same tab strip. The sidebar now runs fully natively, the chat model picker gains favorites and one-click settings, and the accent color follows your background tint.

### 🪟 Views open beside your sessions
- **Your sessions stay on screen while you use a view.** Terminals and chats keep the left side, the view takes the right, and each project remembers where you left the divider; drag it to change the balance and double-click it to reset.
- **Views are tabs, and a project can keep several open.** Open another from the + at the end of the tab strip, close a tab with its x or a middle click, drag tabs to reorder them, and right-click one to Pin tab. Each project brings its own tabs back when you return to it.
- **Browser pages are tabs in the same strip.** Every open page shows its icon and title beside your views, Browser Tab at the top of the + menu opens a new one, and page icons are remembered across restarts.
- **Pop a view out or give it the whole window.** Buttons at the end of the strip open the view in its own window, expand it over the sessions column (Cmd+Ctrl+E), or expand it fully and hide the sidebar too (Cmd+Ctrl+Shift+E).
- **A slimmer header replaces the titlebar row.** It holds the project name, Start, Open and Commit, and a ⋯ menu with Ask Ghostex, Tips & Tricks, Resources, Dev servers, Extensions and Customize; Cmd+Option+B shows or hides the view panel.
- **An Open a view picker when nothing is open.** It lists every view you can open in the project; press C, B, K, U or D for Code, Browser, Kanban, Automate or Docs.

### 🦀 The sidebar is fully native
- **Every sidebar action runs in the app itself.** Clicking, renaming, notes, forking, sleep and wake, pinning, snoozing, Spaces, Project Groups, and the Cmd+1 through 9 session hotkeys no longer go through a web page, so the list responds faster and stays in step with what you do.
- **Remote computers draw in the same list.** A connected remote computer's projects and sessions sit alongside this computer's, and an offline one shows the sessions it had last time.
- **Tooltips and menus fit better.** Row tooltips stay inside the sidebar and match their card's width, menus size to their labels, and the More menu no longer repeats Settings and Hotkeys.
- **The collapsed sidebar's edge reveals what you point at.** Hover the window's left edge to float the sidebar back, or the sessions column when a view fills the window, and the revealed pane takes focus.
- **Quick Access rows get an actions menu with hotkeys.** Open sessions show a blue status dot there.

### 💬 Chat
- **Star models to keep them at the top everywhere.** The model picker has a Favorites tab shared by every open session; click a model to set it for this session and as the default, or right-click to change only this session.
- **Pick another agent's model to start a Handoff.** The Handoff dialog opens with that agent and model already chosen.
- **Reasoning, context window and Fast mode are one-click buttons.** They sit along the bottom of the model picker.
- **Option-click Send to compact and send.** The gesture is named on the button.
- **Armed Delayed Send and Close After Done show on the composer.** Click one to open Delayed Actions.
- **Resuming a Claude session reattaches to its background agent** when one is still running.
- **Rewind and question answers are more reliable.** Rewind waits for Claude's input box before acting, pasted text shows as you typed it, and a file change card opens with one click anywhere on it.

### 🎨 Look and feel
- **The accent color follows your background tint.** The separate accent setting is gone; a neutral tint keeps the sky-blue accent.
- **Clearer status colors.** Every working status uses the same orange, git diff counts read well in light and dark themes, and Spaces get a theme-aware Gray.
- **The Quick Actions button shows the name of the last Action you ran.**
- **The terminal scrollbar appears while you scroll,** and the scroll wheel moves the same distance as in Ghostty.
- **Browser shortcuts.** Reload with F5 or Cmd+R (Ctrl+R on Windows and Linux), and Ctrl+F finds on the page on Windows and Linux.
- **App dialogs open faster.** Each one loads only when you first open it.

### 🩹 Fixes
- **Windows fixes.** Account helpers and install commands resolve correctly, the installer checks that the new version really landed, header dropdowns no longer steal focus from the main window, and chat saves its state again.
- **Restored custom-agent sessions keep their icon** and their chat view.
- **New sessions start as top-level agent runs,** not as nested children of another session.
- **Opening a prompt search result no longer stalls.** Pressing Enter on a result after browsing for a while used to rescan your whole history first; it now opens straight away.
- **Menu labels no longer cut off early.** Items like Browser Tab show in full instead of "Browser…" in roomy menus.

### ⚠️ Heads up
- **The previous web chat view has been removed.** Desktop chat is native only, so the Use GPUI chat setting is gone.

## 9.9.0 - 2026-09-19

**Ghostex 9.9.0 is out.** The desktop app's sidebar and chat are rebuilt from the ground up in Rust with GPUI instead of the web views they used before, which cuts memory use roughly in half in many setups (from about 1.5 GB to about 800 MB). This release also adds actions under every chat message, per-session model picks, new session hotkeys, and CLI commands that let agents launch and talk to each other.

### 🦀 A native desktop app at half the memory
- **The sidebar and chat are now native.** Both are drawn by the app itself in Rust and GPUI instead of embedded web pages, so session clicks, scrolling, and typing respond in the same frame.
- **Memory use drops by about half in many cases.** Workspaces that used around 1.5 GB now sit around 800 MB, and the built-in browser engine only starts once a page actually needs it, so launch no longer waits on web panes you are not using.
- **Switching back is instant.** Recently visited chats stay warm instead of reloading, and the previous project's terminals, chats, and view keep running after you switch away; set how long with Keep the previous project live for in Settings (10 minutes by default).
- **Views show a skeleton while they load.** Chat, Code, and Docs draw a placeholder that matches their real layout instead of an empty pane, restored browser panes stay asleep until you click them, and dark mode no longer flashes white between them.
- **The previous chat view is still one switch away.** Turn off Use GPUI chat in Settings and restart the app to go back to it.

### 🔭 Coming next: all native, and a new look
- **The rest of the main views are moving to Rust next.** Over the coming releases, the remaining views in the app will be rewritten natively like the sidebar and chat.
- **A big design revamp is on the way.** Ghostex will get a cleaner look that feels familiar to anyone coming from the ChatGPT or Claude apps.

### 💬 Chat
- **Actions and the time under every message.** Hover a reply for Copy, Reply by Annotating, Save to md, and when it arrived; hover your own message for its time, Rewind, Save prompt, and Copy.
- **Pick a model for one Claude session without changing your default.** The composer's model and effort menus apply to the current session, with an Also set as default switch; in the model picker, Enter applies to the session and Shift+Enter sets the default. Codex can only change its default, and its controls say so.
- **Identical skills show once.** Copies of the same skill installed in several places collapse into one row in the composer list.
- **Delayed Send starts on When all agents finish.** New Session Automations dialogs on desktop, web, and mobile default to that trigger; an already armed send keeps its own setting.

### ⌨️ Sidebar, titlebar, and hotkeys
- **Session hotkeys follow browser tabs.** Next and Previous Session are Ctrl+Tab and Ctrl+Shift+Tab, pane tabs move to Cmd+Option+[ and ], and Back and Forward are Cmd+[ and ]; turn on Skip sleeping sessions to jump over sleeping ones.
- **Titlebar tooltips show each button's hotkey.** Back and Forward stay in place in every view, and the mode-tab highlight slides between tabs.
- **Collapse a group from its header.** The project chevron toggles collapse, and a section's status dots show only while it is collapsed.
- **The new agent menu opens as a nested menu.** The plus button lists your agents with a back panel, like before.

### 🤖 Agents that run other agents
- **Agents can launch, message, and close other agents.** `ghostex agents` adds `whoami`, `types`, `create`, `list`, `send` (with `--interrupt` or `--queue`), and `close`; see `ghostex agents --help`.
- **Pick a worker's model and effort at launch.** `ghostex create-agent` and `ghostex board start-work` take `--model` and `--effort` for Claude and Codex, for that session only, and resume and fork keep them.
- **Ghostex Agents Orchestration replaces the Fable orchestration skill.** It walks agents through launching, messaging, reading, and closing helper sessions with the commands above.

### 🗂 Workspace
- **Two terminals side by side in the project editor companion.** The companion splits into resizable columns and remembers their widths.
- **The window reopens on the monitor you last used.** Its position is saved as you move it, so a crash or force quit keeps it too.
- **Accounts keep their last Claude usage reading when a refresh is rate-limited.** The row shows how old the reading is, and automatic switching skips that account until a fresh one arrives.
- **Search results show their match count in a floating pill.**

### 🩹 Fixes
- **Linux keeps keyboard focus in browser chrome and sidebar rename,** and the sidebar sizes to the display's scale.
- **The remote browser proxy works again,** so remote browser views connect instead of failing to configure.
- **ZCode sessions can be renamed.** The new name is saved in ZCode's own session list.
- **Dev servers keep their titles** instead of flipping to localhost.
- **Search by Prompt opens a launch in the right session.**
- **Rewind's confirmation quotes the prompt you are rewinding to.**
- **Switching sessions closes an open image preview or model picker** instead of leaving it over the next session.
- **The session you clicked away from loses its highlight right away,** even when the new session has to wake first.
- **Toasts no longer block clicks underneath them,** and tooltips that got stuck now dismiss.

## 9.8.5 - 2026-09-16

**Ghostex 9.8.5 is out.** Agent Sync in the Agents Hub keeps every agent on your computer pointed at one shared set of skills, instructions, and hooks, plus switching a draft between Claude and Codex, annotations that clear after you send them, and fixes to chat and the sidebar.

### 🔗 Agent Sync

- **Keep every agent on one set of skills, instructions, and hooks.** Open the Agents Hub (Cmd+5) and choose Agent Sync to see each agent and profile on your computer, with a dot for Skills, Instructions, and Hooks and a list of problems such as broken links or copied skill folders.
- **Review a plan before anything changes.** Sync all or sync one agent to preview the links and instruction pointers Ghostex will write; instruction files with other content are backed up first and nothing is deleted.
- **Run it from the terminal too.** `ghostex agent-sync status`, `plan`, and `apply --yes` do the same scan, plan, and apply.

### 💬 Chat and agents

- **Switch a draft between Claude and Codex before the first message.** Use Switch Agent CLI in the model menu; the new agent picks its account by your Account for new sessions rule, and your unsent text stays in place.
- **Queued prompts go out after a manual /compact in Claude.** Messages you queued no longer stay held once compaction finishes.
- **Shift+Enter adds a new line wherever focus is in the chat view.**

### 📝 Annotations

- **Copying or sending annotations clears them.** This applies in the Browser toolbar and in Docs HTML files, and the toolbar's settings panel can turn it off.

### 🗂 Sidebar

- **Reveal Session leaves room around the session.** The revealed row stops 50px from the edge, below any pinned headers.
- **The selected session blends with tinted sidebars in dark mode.** Its fill is translucent instead of a flat gray block.
- **Dragging sessions is smoother.** The sidebar does less layout work while you drag.
- **Search by Prompt stays open when started from Previous Sessions.** Its floating button now matches the chat composer's look.

## 9.8.0 - 2026-09-16

**Ghostex 9.8.0 is out.** Continue a Codex conversation that is open in another session, animated session sections with a separate dot for pending questions, one menu style across the app, Search by Prompt filters in the toolbar, and a reliable restart of the background server after an update.

### 💬 Chat and agents

- **Continue a Codex conversation that is open somewhere else.** When Codex says the conversation is in use by another session, choose Continue here (or press Cmd+Enter) and Ghostex closes the other Ghostex session, retries, and keeps your draft unsent until it succeeds.
- **Rewind lands on the prompt you picked.** Claude slash commands now appear in the rewind list, so choosing a row no longer rewinds to a different prompt.
- **Delayed Send waits for the agent to really finish.** A scheduled send no longer goes out while the agent is still writing its last turn.
- **A session whose agent changed wakes with the right agent.** A session that started with one agent and was taken over by another now resumes with its current agent instead of the old command.
- **Cursor conversations keep their place.** Late thinking steps from Cursor no longer shift where your next message is sent.

### 🗂 A clearer sidebar

- **Session sections open and close with an animation.** Status dots stay put while rows slide, and a session waiting on a question shows its own pink dot.
- **Reveal Session is easy to spot.** After the section expands, the revealed session blinks with an outline, and the titlebar button uses a hollow circle.
- **Closing a project keeps you in your Space.** Ghostex moves focus to an awake session in the next project of the same Space instead of leaving it.
- **Light mode reads better.** The active session row and its hover buttons stand out more, and resize handles and agent tab drop targets use a softer gray.

### 🎨 One look for menus and scrollbars

- **Every menu shares one panel.** Dropdowns, context menus, and titlebar popups have the same rounded panel and shadow, and submenus open on click.
- **Scrollbars are thin and match your theme everywhere.** Terminals, the code editor, chat, and the menu bar panel all use the same slim scrollbar that appears on hover.
- **Long pick lists stay inside the window.** Delayed Send and other pickers scroll to the highlighted row instead of running off screen.

### 🔍 Search by Prompt

- **Agent and project filters sit in the toolbar.** They are dropdowns at the top right, and the list shows placeholders while your history loads.
- **Open Search by Prompt from Previous Sessions.** A Search by Prompt button floats over the Sessions list.

### 📝 Docs and views

- **The annotation toolbar sits above your selection.** It moves below the text near the top of the page, and the X marks text for removal with a note.
- **The Docs header shows Send or Copy with the annotation count.** It shrinks to an icon in narrow windows.
- **Titlebar view menus offer Configure view and Wake again.** Configure view opens that custom view in Settings, and a sleeping web view can be woken from its menu.

### 🩹 Fixes

- **The background server starts on the first try after an update.** Ghostex waits for the old server to exit before starting the new one, so "gxserver failed to start" no longer appears until you reload, and the copyable diagnostics explain any start failure.

## 9.7.0 - 2026-09-16

- New Features

  - Compact before you send. Press Option+Enter on macOS or Alt+Enter on Windows and Linux in the chat box, or right-click Send and choose Compact & Send, and Ghostex sends `/compact` first and queues your written prompt to go out the moment compaction finishes. Cursor's `/summarize` and Grok Build's compaction now use the same flow, with the same compaction card above the input and the same hold on queued messages, so nothing is delivered mid-compaction.
  - Open File/Folder Location is back, everywhere a path can be copied. Right-click a file reference or file-change path in chat, an image preview, a changed file in the Git row or the commit dialog, a project, or a Docs entry and reveal it in your computer's file manager. Image previews also say Copy Path or Copy URL depending on what the picture actually is.
  - File references open in Code or Docs with one click from composer pills and transcript paths, using the same view preference in both places. Double-click a composer pill to edit its text, and a view that is switched off simply leaves the menu instead of offering a dead action.
  - Delayed Send can wait for a specific date and time. Session Automations gains a Specific time choice beside After a delay on desktop, web, and mobile, using the local time of the device you set it on. The Postpone by submenu now also has Edit delayed send and Disable delayed send, and the web sidebar can cancel a pending send.
  - Projects you add join the Space that is open at the time and appear at its top, whether you add them from the More menu, the Add Project button on an empty list or empty Space, or the empty sidebar area's right-click menu. Add a project while Other is selected to keep it out of every Space.
  - Debugging is its own Settings page, above About. It starts with Show debug UI controls, and only with that on does it load the diagnostic logging scenarios, session debugging controls, Storage usage by feature with a way to clear disposable caches, and on-disk Ghostex folder sizes with Refresh and Open Folder. The storage inspector works from any open Ghostex page, not only the one that owns the data.
  - Docs keeps the files you have not saved yet in an Open Files list beside the tree, under the same section label, so switching between them no longer risks losing work.
  - Installing the Claude Code hooks keeps your Claude transcripts on disk instead of letting Claude delete them after 30 days. A retention value you set yourself is left exactly as it is.
  - App dialogs are native windows built from one shared modal kit, so every dialog has the same controls, spacing, and theming, and the New Thread picker has been restyled to match it, with centered key-hint chips.

- Major Improvements

  - Ghostex speaks about your computer, not your Mac. Settings, notifications, remote machines, Computer Use, the first-launch guide, Agents Hub, the Docs file menu, the export dialog, and the terminal font presets no longer name Finder, the Keychain, Launch Services, Low Power Mode, or "This Mac", and describe the same things in words that fit macOS, Linux, and Windows.
  - Every shortcut hint is written in your platform's notation. Tips, the model picker, Settings, the prompt editor, Find, Add Project, Docs, saved prompts, slash commands, and session notes all render the same chord the same way: compact glyphs on macOS, key names on Windows and Linux.
  - Long conversations open faster. Completed turns before the latest prompt arrive already collapsed and expand on demand, a page hidden for a moment is kept instead of rebuilt, and switching sessions runs one reconcile instead of several.
  - Changing the app, system, or terminal theme repaints every open terminal at once, including idle terminals and terminals in parked projects, and a light window no longer flashes the dark sidebar at startup.
  - Status cards above the composer and in the transcript share one shell and one motion: compact headers, the whole header as the hover target, animated open and close, and Subagents wearing the same pending-tool card as everything else.
  - The Windows prompt editor (Ctrl+G) ships inside Windows builds again and talks to the app over a named pipe, and its window no longer shows the generic Windows icon.
  - The main pane in Code, Browser, Kanban, Automate, and Docs keeps a minimum width of 455px, and the sidebar respects that floor when you drag it.
  - Hover the model or effort to see the Model & Effort Picker shortcut, and hover the context circle to read the agent's terminal status line.
  - Clean RAM tells you what it did: a toast confirms that the diagnosis prompt is on your clipboard and asks you to paste it into an agent session.

- Minor Improvements

  - Get Started agent tiles wrap onto new rows instead of squeezing.
  - The account switch submenu closes as soon as you pick an account, and the switch backdrop covers the whole chat pane until the new account is ready.
  - Chat popups keep the chat theme, so a light chat inside a dark app shows a light menu; the scroll-to-bottom pill takes the composer's border, fill, and text color; and a disabled image menu item looks disabled.
  - Generate Name comes before Rename in the rename dialogs.
  - Context details stay usable in a narrow pane, with a real height cap and a one-row footer.
  - The sidebar shows more loading bars while a large project inventory is still arriving.
  - The collapsed composer leaves room beside More actions instead of crowding it.
  - Unread notification text is darker in light mode, and session-list dots are visible there.
  - Escape and single-key terminal controls act immediately in chat instead of waiting on your login profile.
  - Workspace state is written off the UI thread, so clicking around no longer waits on a save.
  - Annotate in the Browser toolbar now opens inside the frame that holds the page content, so a Storybook story or another framed preview can be annotated instead of only the chrome around it. It also shows up again on Ghostex's own development pages, where it used to disappear before appearing.
  - Sidebar section headings (Pinned, Sessions, Drafts, Browser, Parked, Snoozed) carry an orange dot when a session there is working and a blue dot when one is done or waiting on you, counting rows hidden by collapse or Compact mode, plus a plain dot on the section holding the active session. Section labels are also a little larger.
  - Every sidebar tooltip waits exactly as long as the delay you set in Settings, including project titles, session cards, fixed action buttons, the branch badge, and the question dot. Moving from one label to the next no longer skips the wait.

- Stabilization

  - A cancelled Claude message that was never accepted returns its text to the composer, Rewind to here does the same, and Escape can no longer let a cancelled send slip through after the text comes back.
  - Maximized composers no longer show working, tasks, or subagents cards on top of the overlay.
  - Claude completion notifications wait while Claude still reports background work running.
  - Loading earlier history survives overlapping snapshots, and the pinned section keeps its drop gap when headings are collapsed.
  - Grok's always-approve composer row is no longer mistaken for a permission card, and Cursor's compaction tool catalog stays out of the transcript.
  - A file diff deep inside a Claude Bash result can no longer become a status card's heading.
  - Worktrees whose registered parent is a bare repository now list their first linked checkout.
  - Clicking the find bar, the address bar, or terminal search while the page has focus sends your keys to that field instead of the page.
  - The Clean RAM button keeps its width while it says Copied.
  - A macOS accessibility update no longer dismisses an open popup.
  - Pairing a phone with Easy Connect works again with the current Easy Connect helper. The phone app now tunnels with the same tailcat version the computer serves, so update the phone app and scan the code on the computer again.
  - A file opened into Docs from a chat link keeps working after a reload, and opening a Code link or another chat file no longer takes its access away. Two files with the same name in different folders also stay separate.

## 9.6.0 - 2026-09-15

- New Features

  - Your phone and your Mac can now drive a Windows computer. Install Ghostex on Windows, turn on SSH, and add the Windows address with your Windows username; the connection uses whichever Windows Environment that machine is set to, either native PowerShell with Windows folders or your selected WSL distribution with Linux folders. Native PowerShell projects need Windows agent CLIs installed, and after changing the environment and restarting Windows Ghostex you reconnect the phone or desktop to pick it up.
  - Install and update agent CLIs from Settings > Agents. Expand an agent row to install or update it, see its installed version and the command output, or open its install docs. Ghostex picks an updater for installations it recognizes and asks you which one to use when it cannot tell; mise is offered for the CLIs that support it and is the default when available. Commands run on the selected computer and keep running if you close Settings.
  - Welcome to Ghostex greets you the first time the app runs. Five panels cover the agents already on your computer with one-click installs for Claude Code, Codex and Cursor Agent, which views to show, phone pairing and notifications, and your first project folder with its default agent. "I already know Ghostex" skips the rest, and you can reopen the whole thing from Tips > Setup or Quick Access > Commands > Setup.
  - ZCode joins the agents Ghostex speaks natively, with chat messages, thinking, tool results, attachments and imported conversation history. Install its hooks in Settings > Agents to connect new conversations, and switch to Terminal for its setup, model menus and permission prompts. Campfire has been removed.
  - Unsent sessions collect in a Drafts section. A new session leads Sessions for ten minutes; after that, one that still has unsent text and has never received a message drops into Drafts, below Pinned and collapsed by default. Pin a draft to move it to Pinned without sending, and sending its first message returns it to Sessions.
  - Push a scheduled send later without rescheduling it. Right-click an agent with a timed Delayed Send, choose Postpone by, and add 10 minutes, 30 minutes, 1 hour, 2 hours or 5 hours to its existing send time. Session Automations can also wait for one specific agent on the same computer to stay idle for ten seconds before pressing Enter.
  - Start a chat message with `!` to run a shell command inside a Claude Code or Codex session, for example `! pwd`, with the command and its output shown in the conversation.
  - Question cards now use the real chat composer. Paste images to get numbered references and the same clickable thumbnails the composer shows, and anything you type is saved on your computer as you edit, so switching sessions or reopening Chat keeps your half-written answer with its question.
  - Handoff and Export are one page with two clear choices. Handoff writes the file and starts the follow-up session in a single click, and Export swaps the options for the saved path when it is done.
  - Annotate a Markdown file or an agent's reply and send the notes straight to an agent. Select text in Docs to comment or mark it Looks good, Clarify or Needs tests, then Send to deliver numbered feedback with line numbers into the session you last clicked, either its chat box or its terminal. Reply by Annotating, beside an agent reply, opens that reply in Docs for the same treatment, and Docs stays on screen while the feedback goes out.
  - Long conversations open faster and stay browsable. Older history arrives with completed turns already collapsed so you can scroll through prompts and answers without wading through tool logs, and expanding a turn or its files loads that detail on demand.
  - The buttons a session card shows on hover now also lead its right-click menu, in the card's own order, so the actions you chose are in both places. Close is the exception while it is on the card, and a new switch under General > Session Cards turns the whole behavior off.
  - Ghostex is in the official Homebrew cask, so `brew install ghostex` works without adding a tap first.

- Major Improvements

  - The account switch card stays up until the new account is actually confirmed and the conversation is ready on it, then briefly says so or offers Retry switch when it fails. It shows both accounts with their usage percentages over a dimmed conversation, and uses a compact one-row-per-account layout on phones and narrow chat panes.
  - Scrollbars look the same everywhere. A thin 5px track floats over the content, appears when you are near it, and reserves no gutter, across the sidebar, chat, dialogs, menus, Docs, Find, the terminal and the embedded Code editor.
  - A brand new Claude chat shows its model and effort straight away instead of waiting up to thirty seconds for its history to resolve, and changing the effort no longer replays a model command that is already applied.
  - The Install Hooks prompt is a friendly page with the agent's own logo and color and four cards explaining what hooks give you, rather than a bare warning.
  - The phone reconnects on its own. With Auto reconnect on, it checks the connection when you return to the app or its network changes and brings interrupted agent terminals back; a red cloud or Reconnect starts a fresh connection when you want one.

- Minor Improvements

  - Hide companion sits immediately beside Hide sidebar in the titlebar and uses one clear chat-bubble icon whether the companion is showing or not.
  - Open submenus stay open while the pointer crosses other menu rows, so you can move into them without rushing.
  - Hex colors in messages, inline code and tables get a small rounded swatch beside the value on desktop, mobile and web, and copying still gives you the plain text.
  - Cursor chats offer `/compact` alongside `/summarize`.
  - Claude's default-effort pricing notice appears in chat with its original explanation and choices, so you can answer it without opening Terminal.
  - The sessions sidebar stays on the left, and Cmd+B still collapses it.
  - Sleeping sessions keep their normal title color with a dimmer last-active time, while awake sessions show a stronger one.
  - Subagent transcripts have rounded corners and have dropped a header line that no longer said anything.
  - Diff cards open against the session's own working directory, so their files resolve in worktrees and subfolders.
  - A short copy sound can play whenever you copy from a terminal, a chat message, the chat composer, a copy button or a menu. It is off by default and lives under Settings > Notifications > Sounds.
  - Native modal windows keep square corners that match the current theme.
  - Forking a session names it after the original and saves that name through the agent's own rename, so it survives reopening the conversation.
  - Light mode sidebars have darker session titles and timestamps and noticeably clearer icons.
  - A prompt with no project shows a blank project label in Find instead of the words "No project".

- Stabilization

  - Codex questions asked mid-turn stay answerable after the turn ends.
  - One dead remote tunnel can no longer stall reconnects to every other computer.
  - Claude keeps its brand color on light notification tiles.
  - Rewinding to a prompt that appears twice now lands on the one you picked.
  - Sending from follow mode snaps to the bottom instead of flashing the Scroll to bottom button.
  - The favorite star stays bright in light mode, and session hover actions stay put next to a running countdown.
  - Companion width and the split control keep their size and color when the Browser takes focus, and the command pane's Expand no longer jumps when the pane minimizes.
  - Local commands such as `/usage` no longer leave a pending bubble behind, and the chat composer's top fade clears once the text stops overflowing.
  - Chat no longer gets stuck showing "Compacting" after a pasted terminal capture.
  - Sessions started from a custom profile pick up confirmed renames instead of staying pending.
  - A manually launched Codex opens in chat before its first hook arrives, and opening a file from chat works in projects reached through a symlink.
  - Closing Ghostex no longer leaves an orphaned editor server holding its port.
  - The workspace no longer flashes "Source is unavailable" or "No running terminal" while it restores at startup.
  - Tips you have already read stay read when settings are saved from elsewhere.
  - Session card title tooltips no longer pop up over the buttons inside the card or after the pointer has left the sidebar, and no tooltip opens on top of an open sidebar right-click menu.
  - Tall dialogs are measured without losing their height cap, so their actions no longer get clipped on Windows.
  - Terminal panes hand over leadership correctly for control keys, Alt combinations and plain arrow, Home and End presses.

## 9.5.1 - 2026-09-14

- New Features

  - Light mode, everywhere. Settings > General > Theme puts App theme first: Dark Gray, Light, or System, which follows your computer. The sidebar, titlebar and its popups, Settings, modals, Docs, Kanban, Automate, the Browser chrome, tooltips and native dialogs all follow it, and open windows change with the system instead of waiting for a restart. Chat and terminals follow the app theme by default and can each override it with Light, Dark or System. New installs start on System; a theme you already picked is kept, and the dark contrast and tint come back unchanged if you switch away and back. Terminal palettes come from the Ghostty themes already configured on your computer, including separate light and dark selections, and fall back to GitHub Light and GitHub Dark when nothing is configured.
  - Windows can run without WSL. Settings > General > Terminal > Windows Environment now offers native PowerShell projects and Windows agent CLIs, and that is the default. Native sessions persist, survive closing the app or restarting gxserver, and get their own agent hooks and resume commands. Switching environments asks whether to restart now or later, and existing projects and sessions stay in the environment they started in. The Code view still needs WSL.
  - Simple mode, in More actions or Settings > Chat, calms every chat at once: tool groups without a message collapse to a count, tool rows drop their command previews, and file edits collapse to "Edited 3 files". Expand any row for the usual detail.
  - Chat remembers where you were. Cmd+P opens Recent Sessions to jump between chats across projects, and Cmd+Ctrl+[ and Cmd+Ctrl+] walk back and forward through the ones you visited. Coming back restores your reading position, which tool cards were expanded, the composer cursor, and the account badge, context usage and status line while their values refresh. Older messages load as you scroll back into the conversation.
  - Your ZCode conversations show up in Quick Access next to Claude's and Codex's. Opening one resumes it in a terminal with `zcode --resume`, as long as the ZCode CLI is installed on that computer. Deleted, archived, running and subagent conversations are left out.
  - Web Preview on the phone. From a connected computer's menu, type an address or a port like `3000`, or pick a listening port from the list, grouped into Web pages, Development tools, Services and Other ports, searchable by page title, process or port, with titles, HTTP status and favicons where the page answers. Localhost links from chat, terminals and browser actions open there instead of in the phone's browser.
  - The mobile app picks Codex models and effort straight from the chat box, including before the first message of a new draft, and waits to apply them until the agent can take them. Long-press a session to park or unpark it, with a Parked section per project.
  - Right-click a view's titlebar button (Code, Browser, Kanban, Automate, Docs) for Reload and Sleep. Reload refreshes that view, and Sleep unloads it while keeping its place; for Code it stops the editor server too. Selecting the view again wakes it.
  - Default chat zoom, in Settings > Chat, scales the whole desktop chat interface from 70% to 200% in 5% steps: messages, controls and the prompt composer together.
  - The composer gets out of the way: scrolling up collapses it, returning to the bottom expands it, and the same toolbar buttons stay visible either way.

- Major Improvements

  - Chat is much lighter on resources. Renderers are reused instead of rebuilt, conversations keep their state when you leave them, the transcript runtime is shared between chats, and account and status chrome survives a renderer being released. Switching between chats is quick even with many of them open.
  - On macOS and Linux, Ghostex can let go of terminal viewers you are not looking at and reattach them when you come back. The agent keeps running the whole time; this never sleeps a session.
  - The embedded Code view is on code-server 4.136.2 with a newer VS Code, with each web view's lifecycle now isolated so reloading or sleeping one does not disturb the others. Installed and WSL Node checks moved to Node 24.
  - Codex questions asked mid-turn can be answered through the terminal editor, so an answer lands even while the agent keeps working.
  - Compaction is clear in both agents: `/compact` shows a card above the input, with Claude's reported progress or a looping bar for Codex, and anything you send or queue during it waits quietly instead of warning about delivery.
  - The live tool card at the bottom of chat shows the current work only until the same tool appears in the conversation, and clears when the turn ends, you stop the agent, or the agent asks for input. Finished searches and commands stay in that turn's work details.
  - Assistant messages that called tools expand them underneath, from a click on the text or the chevron beside it, and keep their full text and formatting when collapsed.
  - Parked sessions are always listed most recently active first, on desktop, mobile and web, even when your other sessions use manual ordering.

- Minor Improvements

  - As the chat narrows, toolbar buttons move into More actions one at a time (Summary mode, Session note, Stash prompt, Attach, Maximize, then Terminal View). If the context ring and effort still do not fit beside the model, they move together into a Model settings section at the top of More actions, and everything comes back as space opens up. Both open on a click instead of on hover.
  - Starting a new agent from Code, Browser, Kanban, Automate or Docs keeps that view open instead of jumping to Agents. With two split companion panes, a sidebar click or a new session goes to the pane you last worked in.
  - The chat status line appears as soon as its values are known, including the first time you open a chat, and wrapped rows stay centered with separators only between items on the same row.
  - Agents Hub, Quick Access, Saved Prompts, Sessions, Add Worktree, Add a machine, Easy Connect, Browser History and the Automation editor share one tab strip: a rounded track with a raised selected tab, in both appearances.
  - Background contrast, background tint and accent color are named "Dark theme ..." and live under Show Advanced, since they do not recolor light mode.
  - Agent panes and chat companion sidepanes stop shrinking below 388px, and command pane tabs sit flush with the panel edge.
  - The sidebar Search and Commands rows are shorter, giving the session list that space back, and Projects shows a loading skeleton until its first update arrives.
  - Docs toolbars are compact and borderless, with annotation icons in darker shades of their highlight colors so they stay visible on a light page.
  - Click the Subagents header to minimize the card to its header or open it again. It starts minimized in Simple mode, and your choice is remembered per session.
  - The transcript scrollbar is a thin 5px overlay that fades out when you stop scrolling.
  - Extra-high effort is labelled xHigh.
  - Pi, OMP, Hermes and Antigravity keep single newlines in what you send.
  - The working-and-waiting dot is pink now, and a session's Last Active label steps aside when it would overlap.
  - Clicking another titlebar dropdown's button closes the open one and opens the new one in the same click.
  - Selecting a session, focusing its terminal or pressing Escape now also marks its notification read, including one you had pushed to the back of the unread queue.
  - Sign-in notices in chat offer Switch account beside Open terminal, like usage-limit notices already did.
  - The Spaces editor and the Notifications popup are smaller, and the custom tag row puts its delete button first.
  - Three new CLI commands back the mobile features: `ghostex park-session`, `ghostex select-session-chat-model`, and `ghostex ports --json --web`.
  - Ghostex Help and Computer Use no longer invoke themselves implicitly; ask for them by name. The Beads Kanban titlebar tab is just Kanban.

- Stabilization

  - Queued Codex notices stay up until the send is actually delivered.
  - Opening a session that already has a terminal tab reuses it without stealing focus.
  - The Commands pane keeps what you typed in Action across a restore and an app restart.
  - Windows: a sidebar divider no longer keeps its hover state after the pointer leaves, drafts survive switching between chat and terminal, and a stale native server is replaced without stopping the agents on it.
  - A recycled TTY can no longer hand an OMP session the wrong identity.
  - Codex composer detection tells a real input box from a menu or a setup screen, so sends do not go to the wrong place.
  - The hooks permission prompt grows with its window instead of clipping.
  - A draft recovery you dismissed stays dismissed.
  - Returning to a chat can no longer leave a second composer behind.
  - Light mode keeps its one-pixel Browser pane and sidebar lines, app modals open white from the first paint, the active session row is easier to pick out, and search boxes no longer take a black border when focused.
  - Linux and Windows find the Ghostty config in the XDG config folder instead of only the macOS location.
  - Light chat uses the shared Codex logo, and copying Context details between Claude and Codex is hidden for now while it is reworked.

## 9.4.0 - 2026-09-12

- New Features

  - A Notifications bell sits in the titlebar with an unread count. Open it for one row per session, newest first, saying whether the agent finished its turn or needs you, with the last thing it said. Click a row to jump to that session, hover a row to dismiss it, and use Next unread, Mark all read or Clear all in the header. Cmd+I opens the panel, Cmd+Shift+U jumps to the latest unread, and Cmd+Ctrl+U pushes the current session to the back of the queue and moves to the next one. Scripts and agent hooks can post their own rows with `ghostex notify --title "..."`.
  - A new setup walkthrough, from Tips > Setup in the titlebar: what lives next to your agents, the agent CLIs you already have (found for you and listed), pairing your phone, and then opening your first project. "I already know Ghostex" leaves at any point. The automatic first run keeps the older setup screen for now, which lists the agents you have connected instead of every CLI it can find.
  - Make your own session tags. In Settings > General > Sidebar > Sidebar Tags choose Add tag, then give it a name, an icon and a color; New tag at the bottom of the Tag as menu opens the same form. Custom tags sit beside the built-in Priority, Progress and Type tags in menus and filters, can be reordered, hidden or deleted, show up in the mobile app, and work with `ghostex tag-session`.
  - Snooze a session until later: 1 hour, 3 hours, Tomorrow at 9:00, or Next week on Monday at 9:00. It sleeps in its own Snoozed section and returns to its usual place when the time comes. Unsnooze brings it back early.
  - Choose what a hovered session card shows. Settings > General > Session Cards has a strip of buttons (Rename, Pin, Note, Snooze, Close After Done, Tag, Park, Sleep, Close): click one to turn it on or off, drag to reorder them, and the chevron keeps everything before it hidden until you open it. Each project remembers whether its chevron is open.
  - Codex can ask you something while it keeps working. The question appears above the composer so you can keep typing: pick a suggested answer or write your own, move between questions with the arrows, collapse the panel to answer later, or Skip it. The sidebar shows a working spinner with a blue dot until you answer.
  - Switching a running session to another account keeps the session. Ghostex exits the CLI in its own terminal and resumes the same conversation there, so the tab and the chat stay open, and a card in the middle of the chat shows both accounts, their usage and the progress, with Retry switch if something fails. Usage-limit notices in chat now offer Switch account beside Open terminal.
  - The companion and Commands panes are remembered per view: one layout for Agents and one for the wide views (Browser, Code, Docs, Kanban, Automate), the same in every project. The Commands pane also minimizes itself a minute after you leave it, with commands still running, and a Keep open lock pauses that for the current project.
  - Chat follows your computer's appearance. Chat Appearance is System by default, so chat turns light or dark with the system; Light and Dark keep it in one palette.
  - Long projects stay readable: a project with more than 13 sessions starts compact and adds a "Show all N sessions" row, its header stays pinned while you scroll, and each project remembers the mode you chose.
  - The Docs files list can be hidden, peeked and pinned from the same corner button, remembers how you left it, and opens as a drawer when the window is narrow. Cmd+F (Ctrl+F on Windows and Linux) opens Find and Replace inside a Markdown document, or the files search everywhere else.

- Major Improvements

  - The titlebar usage popup is drawn by Ghostex itself instead of an embedded page, so it opens instantly and matches the rest of the app. Clicking the same button closes it, a click anywhere else (including in chat) closes it too, and Claude's Fable limit gets its own bar.
  - Claude buttons and rows show the two tightest of the weekly, five-hour and Fable limits, so a Fable limit that is running out can no longer hide behind the others. The launcher, the New Thread picker and Settings > Accounts use those same two numbers.
  - Account for new sessions has a new default, Auto, which weighs how much limit is left against how soon it resets and picks the account that can absorb the most work.
  - Account switches are verified before anything is relabeled: the running CLI keeps its old account until the exit and the resume both succeed. A manual switch then waits for your next message, and an automatic one continues the interrupted work by itself.
  - The Ctrl+G prompt editor is the same editor as the chat composer now, with F1 commands, find and replace, undo and redo, and image previews. The bundled Monaco copy is gone, so the app is smaller and long prompts behave the same everywhere.
  - Typing goes where you are looking. Keyboard input is routed through one owner, so focus can no longer end up split between a terminal, the chat box and the sidebar.
  - The status line and Context details are cleaner: every row holds a single value, rows with no data hide themselves until it comes back (your starred choices stay saved), and the line under the chat box holds its space and fades in instead of pushing the conversation down when usage arrives.
  - Scroll Chat to Bottom is a shortcut you can rebind. Ctrl+Shift+Down scrolls the focused chat to the bottom even while you are typing, and the pill shows whichever chord is bound.

- Minor Improvements

  - Summary mode has its own button in the chat toolbar when there is room, and stays under More actions when the chat is narrow.
  - Settings rows that depend on the row above them are indented and appear only once their parent is on.
  - In the Settings table of contents, a page or section title goes straight there; only the small chevron beside it expands or collapses the entries underneath.
  - In the chat's More actions menu, Switch Account opens on a click, so passing the pointer over it no longer opens the account submenu.
  - The model picker orders Cursor and Antigravity models like the rest, and Pi and Antigravity can generate session titles.
  - Codex update cards name both versions and explain the update in words instead of printing a raw install command.
  - The History button on a project header opens Quick Access > Sessions for that project with Closed selected, ready to search what you closed.
  - Folder paths in chat open in your system file explorer, and right-clicking any path offers Copy Path.
  - Extension commands, Open Terminal, account sign-in and `ghostex://terminal` links all run in the active project's folder; a remote project says so instead of quietly starting in your home folder.
  - An empty Spaces row offers Create space, and New Space is in the Other button's context menu.
  - The Notifications panel is wider, its header buttons show their hotkeys, agent icons stay live, and rows mark themselves read as you use them.
  - The Subagents card shows a Codex child's name beside its model and effort.
  - Right-click and dropdown menus in the shell are sized to their labels, so a short Copy and Paste menu stays compact.
  - Linux can install from the AUR: the `ghostex-bin` package is documented in the README.

- Stabilization

  - Ghostex starts from the desktop entry on Wayland compositors again: a running window is found through /proc instead of needing wmctrl.
  - Linux, WSL and remote Ubuntu installs ship the bundled skills, so first-run capability setup no longer fails there.
  - Remote setup reads the hostname without relying on PATH, thanks to @rgruenewald.
  - Closing an exited tab no longer leaves its neighbour stuck on Loading Chat.
  - Codex rewind starts from the prompt you highlighted.
  - The Claude usage-limit card no longer tells you to wait it out in the terminal; sending still cancels the wait on that account.
  - The chat account panel shows each email once, and usage-limit cards stay hidden across an account switch.
  - Opening a file from chat that lives outside the project is handed to the Code view's own workbench instead of waiting forever.
  - Command actions stage their scripts on macOS and Linux and reuse a surface that is already mounted.
  - Terminal streaming retires cleanly when the reply lands as a system row.
  - Enabled hover buttons on session cards use a neutral fill, and the Subagents row text no longer resizes the card title.
  - The Spaces row keeps the same gap while the session list scrolls.

## 9.3.0 - 2026-09-11

- New Features

  - Switching Spaces brings back what each Space was last showing: its last session, in that project's remembered view, and its scroll position. A new option lets the selected Space follow whichever session becomes active.
  - Landing on another project keeps that project's view (Code, Browser, Kanban, Automate or Docs) instead of jumping to Agents. Only clicking a session inside the project you are already in opens Agents.
  - Watch Claude's reply as it is written. Long answers stream into the chat straight from the terminal and switch to the saved message the moment it lands in the transcript. The view holds at the top of the new text so you can read it as it grows; press Option+Down or the "Scroll to bottom" pill to jump to the end.
  - Choose how new sessions pick their account: Most limit remaining (the default), Soonest reset, Most used first, Same as last session, or one pinned account.
  - The titlebar usage popup is one page for Claude and Codex: live limits per account, extra model limits behind a disclosure, and a 30-day token history for the provider, read from the conversations on your computer. Codex accounts with reset credits can redeem one right there.
  - Paste almost anything into Add Project: a folder path, a `cd` command, a clone command, or a GitHub, GitLab, Bitbucket or Azure DevOps link, and it routes itself to browse or clone. A path that is already a project offers to open it, and a file inside a repository offers the repository root.
  - Docs loads folder by folder and shows a cached tree instantly when you come back, while a background index keeps search complete. Big projects show up instead of hanging.
  - Park and tag in one gesture with "Show tag menu when parking", and parked sessions unpark themselves when you send them a message (on by default).
  - Chat sends now work with Hermes, Pi and OMP: Ghostex reads their input box and clears it before pasting.

- Major Improvements

  - gxserver uses far less CPU when idle with a large session list: polls no longer load every stopped session, fork families and searches read only what they need, and one process probe serves every session.
  - The Docs window is much lighter. The editors load when first needed instead of shipping a 13.5 MB page for every Markdown file.
  - Saved Prompts opens instantly: drafts no longer rescan storage on every keystroke, Recovered shows one row per session with earlier versions behind a popover, and history loads only when its tab opens.
  - Codex rewind is dependable. If Ghostex loses track of the new thread after a rewind, the dialog stays open with Retry synchronization instead of dropping your draft, and the rewound thread is adopted without the old one resurfacing.
  - Claude and Codex name their own sessions now. Ghostex no longer runs its first-prompt rename job, so nothing blocks typing while a title is generated. Manual rename and Generate Name remain.
  - Messages sent while a session is still starting stay in the transcript with a "Waiting for agent" status, plus Retry and Remove, and survive reopening the chat or switching devices.
  - The desktop shell's right-click and dropdown menus (titlebar, tabs, panes, browser profiles, terminal Copy and Paste, Keep Awake, the pet) are drawn by Ghostex itself, so they look the same on every platform and stay above the embedded browser on Linux.
  - Sending from chat clears whatever was already typed in the agent's terminal with a method proven for each agent, and checks the box is empty before pasting.

- Minor Improvements

  - Usage rows in Context details are one value each (5h limit, 7d limit, model limit, and both resets), and the dialog has a filter bar.
  - Dragging a session shows a ghost that matches the row, and the list reorders only when you drop.
  - Space buttons have 6px corners, their counts sit lower on the icon, and there is more room under the Spaces row.
  - Hidden emails use visible mask characters instead of a blur, so nothing shows through on select or hover.
  - The compaction hint moved into an info tooltip beside the activity title.
  - A new fork keeps its "Fork:" name until it earns its own.
  - Sidebar "last active" labels share one clock that ticks only when a label would change and pauses while the window is hidden.
  - File-change cards in chat use a darker background.
  - Option+Left and Option+Right in the terminal jump by word on macOS, like Ghostty.
  - The web app gets the new Add Project flow too.

- Stabilization

  - Sending works again in Codex 0.154: the input box is recognised with or without its animated particles, and the queued-message indicator is found anywhere on screen.
  - The skill picker no longer stays empty after a failed load. It shows loading, an error with Retry, or "No skills available", and reopening tries again.
  - Sidebar menus close when you click back into a terminal or browser.
  - Forked Codex sessions keep their inherited history when the chat opens.
  - After switching accounts, a usage limit hit by the new login shows again instead of staying hidden.
  - Account recovery no longer stops for every session when one of them fails, and failures are logged.
  - A third-party PostCompact hook no longer turns the compaction row into raw command output.
  - Account panels opened at the same time share one request instead of each asking the server.
  - Azure DevOps clone links keep their `_git` paths.
  - Remote machines' last-seen project lists are stored per machine, so one update cannot overwrite another's.

## 9.2.0 - 2026-09-10

- New Features

  - Slash commands you send from chat now stay in the conversation after a reload, together with their output. Long output expands when clicked, and model, effort, Fast mode and compaction results keep their status rows, thanks to @banozz0.
  - Bundled skills (Ghostex CLI, Ghostex Help, Browser Use and the rest) update themselves from GitHub, so a skill fix reaches you without waiting for a release. Installed skills refresh each time Ghostex starts; offline installs use the copy inside the app.
  - Park selected: select several sessions and park or unpark them together. Parking is on by default now, and its settings no longer hide behind Show Advanced.
  - The Extensions page has a Titlebar account usage section: star the Claude and Codex accounts whose usage you want to see in the titlebar.

- Major Improvements

  - The Subagents card knows what each subagent is really doing. Claude and Codex children are read from their own transcripts, so idle ones stay listed with paused clocks, each shows its model and effort (Opus 5 High), hovering shows the agent type, and clicking opens that exact transcript. When Ghostex cannot verify the roster it says so instead of animating stale work.
  - The chat box stays put while a conversation loads or resyncs. Loading shows in the message area, so a draft save or a status change can no longer take the box away while you type.
  - Hide emails now hides them everywhere: the status line under the chat box, context details, account dropdowns, sign-in fields, error messages, the New Thread picker and the titlebar usage popup.
  - Session titles no longer get stuck on generating. Interrupted title jobs are retired when Ghostex restarts, and Codex's own thread name finishes the wait.

- Minor Improvements

  - The Agents Hub MDs tab groups your shared agent markdown, and every group expands to its files.
  - Docs scans up to 20,000 files and folders per project, up from 1,200, and tells you when a project exceeds that instead of quietly showing an incomplete tree.
  - The titlebar's project name no longer collapses when there is room for it.
  - Disabled effort cards in the model picker stay opaque, in muted colors.

- Stabilization

  - The Help button and skill installs work again: 9.1.0 did not bundle the Ghostex Help skill, so Install and Help failed with "Ghostex Help install failed".
  - Sidebar menus close on the click that lands outside them, even when the pointer is already back over the sidebar, and the search button closes an open menu.
  - Codex 0.154's animated composer particles no longer make an empty chat box look like it has text.
  - Switching a draft's agent reloads its accounts, so the account menu cannot keep showing the previous provider's list.
  - The compaction hint reads "Send a message to queue it after compaction".

## 9.1.0 - 2026-09-10

- New Features

  - Ask Ghostex how it works. A new Help button in the titlebar starts a chat with an agent that knows the whole app: it explains a feature, walks you through a setup, or changes the setting for you when you ask.
  - Your browsing history is searchable, per project. Hold Back or Forward for the pages you visited, or press Cmd+Y (Ctrl+H elsewhere) to search everything.
  - Cmd+Shift+T opens a New Thread picker for the active project: type to filter your agents, press Tab on Claude or Codex to pick the account, or choose the Browser or Terminal row at the end.
  - Give a project its own views. Each one runs a command you choose and shows its output beside your work, alongside Agents, Code and Browser.
  - Drafts you type are now saved to disk as you go, so a restart, a crash, or a lost connection cannot take them. If a newer draft turns up from another device, Ghostex shows it with a preview and lets you keep either one.
  - File edits in a conversation can show the first lines of the change instead of only the path.
  - The quick model picker now works for Cursor, Grok Build and Antigravity too, not just Claude and Codex.
  - Every Space shows how many of its sessions want your attention and how many are working.
  - Reorder the titlebar's view buttons, and choose which ones appear at all.

- Major Improvements

  - Accounts has its own page in Settings, so every way in leads to the same place, and quick launch starts from your default account.
  - Every Settings page now shares one look: a heading, then a card of plain rows, one setting per row.
  - Activating a session reveals it wherever it appears: the sidebar switches Space, opens the project and group, and scrolls the session into view with room around it.
  - The sessions sidebar, the companion pane and the Commands pane remember how you left them for each view, so switching between Agents, Code and Browser restores that layout.
  - Starting a chat agent opens its chat right away and connects in the background, instead of waiting on a blank pane.
  - Opening Previous Sessions and the project lists is much faster: Ghostex no longer rescans every Claude and Codex file on disk each time.
  - Option+1, 2, 3 and so on now follow the titlebar order you see, so they keep working after you rearrange your views.

- Minor Improvements

  - Shortcuts now follow the key you pressed rather than the character it typed, so Option chords on macOS and non-US layouts such as AZERTY work as recorded.
  - Subagent transcripts open in the normal chat display, with verbose and summary as choices.
  - Paste or drop a link into the chat box and it becomes a reference pill like a file or a skill.
  - A Claude Code compaction shows its progress instead of looking frozen.
  - The usage rows in context details read your linked account, so they stay right even when the session has not reported yet.
  - Double-click either divider between three panes to make them all equal.
  - The compact view dropdown moves to the left of the titlebar, while the full view buttons stay centered.
  - The companion pane loses its own hide button; the titlebar owns it, and floating it out now fills the window.
  - Ghostex can trust an agent's project folder for you, so a session no longer stops on that prompt.
  - The Commands hint stays visible beside the pane buttons.
  - Extensions can see a project's git remote and which worktree it came from.
  - Sidebar project names all use one color, and a selected session's chrome matches the chat box.
  - Chat cards use the same font and line spacing as the rest of the conversation, and the minimap floats over the transcript instead of pushing it aside.
  - Two-finger zoom is off in Ghostex's own views and modals, and still on for web pages and extensions.
  - `ghostex guide` prints the app's reference docs, and `ghostex settings` reads and changes settings from the terminal.

- Stabilization

  - Pages no longer flash white while they load.
  - A link inside a conversation opens in the browser instead of replacing the chat with that page.
  - Sessions already running when Ghostex starts no longer arrive as attention notifications.
  - Cursor's newer input box is recognised again, Codex's thinking notice reads as a dialog, and Claude's streaming replies are no longer treated as finished turns.
  - A new session's title follows the project's own agent rather than a placeholder.
  - Drafts sort inside the project's Sessions list, below the pinned rows, on desktop and on your phone.
  - A prompt sent before the agent is ready is held in a queue instead of being lost.
  - A delayed send no longer closes the window it was typed in.
  - Closed sessions leave the Resources dropdown right away.

## 9.0.0 - 2026-09-08

- New Features

  - Sign in to your Claude and Codex accounts from inside Ghostex: start a login, confirm the email, reconnect one that stopped working, and swap which slot an account sits in, without opening a terminal.
  - Star an account to keep it in the titlebar, where a small badge opens a panel showing how much of each limit you have used, how quickly you are using it, and when it resets.
  - Ghostex now finds Claude and Codex conversations you started outside the app and offers to import them, so your existing work sits alongside your Ghostex sessions.
  - Open a subagent's own transcript from the task card that started it, and page back through its history without leaving the chat.
  - Recall prompts you recently sent straight from the chat box, including ones a delayed send delivered while you were away.
  - Grok joins the agents you can chat with.
  - Drag a project or a group onto a Space to move it there, and drag the Spaces themselves to reorder them.
  - Codex sessions now report their token use, limit windows, credits, and request timing, and you choose which of those rows to show and in what order.
  - Choose whether web pages in Ghostex's browser follow your system appearance or stay light or dark.
  - Swipe with two fingers on the trackpad to move Back and Forward through the sessions and views you visited.
  - Open a table from a chat message full screen.

- Major Improvements

  - The chat box is a new, much lighter editor. Reference pills, find and replace, go to line, soft wrap, and full keyboard editing all stay, and chats no longer download a large code editor to show you a text box.
  - The model and effort picker is now its own window, so you can call it up from terminal view as well as chat, and your choice is retried automatically when the agent is busy.
  - The Resources panel lists every project on its own instead of bundling them into "Other projects", and you can close a session straight from the panel.
  - However you activate a session, Ghostex now reveals it in the sidebar: it switches Space, expands the project and group, and scrolls the session into view.

- Minor Improvements

  - The app's default chrome color is a neutral dark instead of the icy tint; colors you picked yourself are untouched.
  - Draft sessions sit at the top of the list, newest first, marked with a pencil.
  - Hand the draft you typed in chat over to the terminal view.
  - Long dropdown lists can be searched instead of scrolled.
  - The chat box now glides out of the way as you scroll the conversation and back when you stop.
  - More keyboard editing in the chat box, including deleting to the start or end of a line.
  - Queued prompts show the shortcut that edits them.
  - Agent dialogs such as /usage always offer their exit button, so leaving one never means switching to the terminal.
  - The message history marks beside a conversation are larger and easier to hit.
  - The browser pane menu now opens as a titlebar popup like the other menus.
  - Turn off the terminal model picker shortcut if you would rather the terminal keep that key.
  - Chat images no longer show a zoom cursor.
  - Session menu labels are capitalized the same way everywhere.
  - The web app gets the same new chat box and reveals activated sessions too.

- Stabilization

  - Agent finished and stopped notifications no longer show up twice in a row.
  - Codex's local commands now show their full output, thanks to banozz0.
  - A rewind that works but cannot tidy the terminal keeps your draft and tells you, instead of dropping it.
  - Closing the last browser tab takes you back to Agents rather than leaving an empty browser.
  - The menu bar dropdown stays open when you switch to another app.
  - The active model is detected more reliably when the terminal and the picker disagree.
  - Double-clicking a prompt in Find resumes it even when the list scrolls under your cursor.
  - A project or group belongs to only one Space now, so it no longer appears twice in the sidebar or on your phone.

## 8.10.0 - 2026-09-07

- New Features
  - Sign in to several Claude or Codex accounts, switch a session between them, and let Ghostex move to the next account when one hits its limit. A setup guide walks you through each provider, and you can blur account emails when sharing your screen.
  - Diagrams written in chat messages or Docs now draw as pictures you can open full screen.
  - Dev Servers lists the local web servers your projects are running, with a titlebar button you can hide if you do not use it.
  - A minimap beside long conversations lets you jump back to any earlier turn.
  - Save a prompt you wrote straight from the chat into your Saved Prompts library.
  - Pick the model and the reasoning effort for Claude and Codex from the chat bar.
  - Browse the web servers running on a remote computer over SSH, inside Ghostex's browser.
  - Split the session you are on into a second pane on the right.
  - Mastra Code joins the agents you can launch.
  - Restart Ghostex from the app menu.
- Major Improvements
  - The terminal engine moves to zmx 0.8.1. Installing this version restarts your running sessions once. Agents come back automatically, but anything they had in flight at that moment is lost, so update at a quiet time.
  - Questions that used to appear only in the terminal now show up as cards in chat with buttons you can click: trust this folder, Codex dialogs, and the Grok, Hermes, and Pi permission prompts.
  - Long Codex transcripts open in a pager you can restore back to the chat view, and rewinding a conversation now works for Codex as well as Claude.
  - Chats you are not looking at release their memory against a budget, so keeping many sessions open costs noticeably less RAM.
  - Codex shows its local command output and its sub-agent activity in chat as they happen.
- Minor Improvements
  - Edit in the chat box with the keyboard: move the caret by word or line, reach the last queued message with Alt+Up, and use the usual text shortcuts.
  - Scroll continuously through models and effort levels in the picker instead of one step per gesture.
  - The chat box gets out of the way while you scroll the conversation and returns when you stop.
  - Context usage is shown as a percentage.
  - Notices, questions, tool runs, and activity rows share one card look across the chat.
  - A transcript you hand to another agent arrives as a named link instead of a bare file path.
  - The Resources panel lists sessions from every project, not only the one you are in.
  - The sidebar collapse, update, Back, and Forward buttons are one square family in the titlebar.
  - Terminal, browser, and editor panes follow the window's rounded corners.
  - Docs can reveal the file you have open, peek at the files list when you hover the expand button, and floats that list over a narrow pane instead of squeezing the page.
  - Docs uses the same palette as the Kanban board.
  - Settings previews the pairing QR code, and its search finds more of what is there.
  - The web app shows the same toasts as the desktop app and is served by its own `ghostex web` command.
  - Projects remember the session you were last on, and Quick Access reopens it.
  - The session list keeps the sections you collapsed.
- Stabilization
  - Restored terminals keep their soft wraps, so resizing the window reflows the text instead of leaving broken lines.
  - Sessions parked in the background stay attached and resize correctly when you come back to them.
  - Composer text is versioned, so the newest edit wins when the desktop, web, and phone apps are typing into the same session.
  - Arrow keys reach the agent list prompts that expect them.
  - A forked session no longer inherits an unbounded Codex history.
  - Codex sub-agent transcripts no longer set off agent hooks.
  - Command terminals for a remote project are created on that machine, and reconnecting recovers them.
  - Reading an agent's screen now takes a bounded slice of the terminal instead of the whole scrollback.

## 8.9.0 - 2026-09-05

- Major
  - Agent approvals start at Keep default. Claude and Codex no longer launch with permissions skipped; turn that on per app or per agent when you want it. Your existing choices are unchanged.
  - Quota, sign-in, and agent errors no longer lock the chat box. You see the error and can retry or switch models. Only screens that genuinely need an answer, like a sign-in dialog, still hold input.
  - Easy Connect installs itself with one click in Settings → Remote, instead of a `go install` command in a terminal. Pairing is now split into Connect a Phone, with the QR code, and Connect a Remote machine, with the code and SSH login the other computer needs.
  - First-run setup focuses on Codex, Claude, and Cursor, confirms each agent is connected before moving on, and offers skills as capabilities: Control Ghostex, Use your browser, Use your computer.
- Minor
  - Switching terminal tabs no longer reflows the agent's TUI. Only an on-screen chat view widens a session nobody is watching. This update does not restart your sessions.
  - Codex GPT 6 Astra joins the model lists, model rows are grouped under headings, and Claude's two Opus rows are listed separately.
  - A successful `/effort` or `/fast` change gets its own pill in the transcript, including Codex's Fast mode ON and OFF.
  - The model and options pills stay skeletons until a value is known, instead of showing the words Model and Options.
  - Codex's model picker works on a machine you are connected to remotely, so the phone and the web app can change it too.
  - A gxserver that fails to start offers Copy diagnostics on its toast, with the health probes, launcher log, and launchd state, minus tokens and paths.
  - Interrupting an agent twice no longer leaves you at a half-loaded shell.
  - Claude's "N skills available" notice is no longer drawn as a tool call.
  - The chat transcript's body text is easier on the eyes.
  - Tooltips wrap at a sensible width instead of stretching across the window, and the Terminal View preview shortens the agent's full-width rules.
  - Ghostex no longer quits a second after the Preparing Ghostex window on a first launch or after a Chromium runtime update.
  - A locally built development bundle no longer offers an update that would replace it with the released app.

## 8.8.0 - 2026-09-04

- Major
  - Pair a phone with Easy Connect: the new Mobile & Remote setup shows a code the Android app scans, Settings → Remote gains SSH access, Tailscale and paired-device cards in place of the tailcat panel, and saved machines are reached over an Easy Connect forwarder instead of a raw SSH host.
  - The Android app gets the matching setup flow (Welcome, connect, scan code, connected), a Can't reach checklist that names the cause of a failed connection, Tailscale SSH without a password, machine tabs with a cloud connect glyph and counts, and Delayed Send countdowns that tick from the phone clock.
  - Switch Account: a Claude or Codex session can be resumed under another account of the same agent family from the terminal action bar, the chat composer menu, or the sidebar card.
  - Antigravity CLI is a Chat View agent: its transcripts are followed, model and effort are read from the terminal footer, thinking stays visible as narration, its hooks are installed, and its own conversation names become session titles.
  - Only a terminal you are looking at sizes a session: hidden chat views and background clients rest at 200 columns, so agent CLIs stop truncating lines in Chat View and a hidden pane no longer pins the daemon narrow. Installing this release restarts every live zmx session once; agents resume from their saved commands.
  - Custom Views: add any number of named HTTP or HTTPS pages in Settings → Extensions, drag them into the order you want, and turn one off without losing its name and URL. Each enabled view becomes its own titlebar mode tab and opens in its own isolated page.
  - Codex model and effort are set from the chat pills: the pills list Codex's models, efforts and Fast mode the way Claude's do, and Ghostex drives Codex's own picker in the session terminal because Codex has no command form for it. Installing the hooks also adds a status line that names the model, so the pills are filled in before the first turn.
  - Chat View follows Claude Code's live work: the tool Claude is running right now shows as a card pinned to the bottom of the transcript, carrying the tool's own painted text and opening to the full call, and Claude's permission dialog is answered from chat with Yes or No. Claude's side panes no longer leak into the transcript, and its diff panel is closed again when nobody is watching the pane.
  - The context meter opens onto Claude's status line: a More details section groups Usage & cost, Context & cache and Session rows, including spend, rate limits, lines changed, prompt cache state and token counts. Star the rows you care about to keep them under the chat box, and drag them into the order you want.
  - A prompt Claude hands back comes back to the chat box. Pressing Escape just after a send returns the text to the composer, whether you pressed it in chat, in the terminal pane or on a phone, instead of leaving the message lost or reported as undelivered.
- Minor
  - The chat box is never locked. When a send is refused because input is held by another device, a question is waiting, or a mode switch is in flight, the draft stays editable and a red toast says why instead of the text box going read-only.
  - Settings → Remote stacks Easy Connect and Tailscale as expandable cards, so only one QR code is ever in front of you, and each path has its own on/off switch. Turning Tailscale off also hides it from the Mobile & Remote setup. Saved machines move to a compact grid and are edited in their own dialog.
  - A Codex session keeps the title Codex generates for it: Ghostex waits for Codex's own name and only steps in when that never lands, so a session no longer settles on the first few words of your prompt. A custom agent's default session name no longer blocks auto-naming either.
  - Dismissing a terminal notice in Chat View makes it stay dismissed; a banner that missed one screen probe used to reappear every few seconds.
  - Clicking a fork's earlier thread now opens it: a stopped ancestor is woken in place first, which keeps the fork family intact.
  - Sleep on a session card sends the running session to sleep instead of waking it.
  - nix-darwin computers find their own tools again: sessions keep the PATH their login shell builds instead of the app's, and Ghostex also looks in the Nix profile directories for `claude`, `bd` and `gx` (GitHub issue #118).
  - Queued prompts that Claude Code submits together no longer stay on screen labelled Queued, and the pane stops showing a stale working spinner after the reply lands.
  - Minimizing an Agents panel no longer nudges the first tab sideways or drops the panel's left edge line.
  - Usage analytics record which OS a phone or browser connects from, and which agent CLI a custom agent actually runs, instead of collapsing every custom agent into one bucket.
  - Product copy across the desktop, web and Android apps drops em dashes for punctuation that reads naturally.
  - Claude Code questions now stay in Chat View while background subagents work in the same session; previously a subagent's tool traffic retired the card within a second.
  - Claude's task list shows above the composer, context window usage appears in the composer, and a Claude statusLine command keeps the model and effort pills live.
  - Chat View retires terminal status rows by their first paragraph and transcript recency, approval cards no longer repeat the tool name, the context meter Compact button is simpler, option pill menus stay anchored while a choice loads, and the transcript selection toolbar stays one pill on mobile.
  - Cursor's Ready title ends the working spinner, custom agents built on Claude keep a stable identity, and disabled Antigravity or Claude hooks count as not installed.
  - Ghostex no longer bundles, downloads or symlinks the Beads `bd` CLI; install Beads on the machine and the Kanban board uses that copy.
  - Machine tabs count every session on the machine regardless of filters and hide idle awake counts, and Space swipe works across the whole sidebar page.
  - Tips & Tricks is refreshed for the current product surfaces, first-launch setup can finish when a project already exists and hands phone setup to Mobile & Remote, and product copy says computer instead of Mac.
  - The model catalog adds Cursor's Gemini 3.8 Flash and the Antigravity CLI lineup, the `ghostex sessions` mobile summary carries delayed-send deadlines and automation flags, and the Linux app syncs the X server before CEF parents into the embed host.
  - A Claude turn that finishes now shows the blue dot and plays the attention sound the way Codex does.
  - Split Right opens a session in a pane beside the focused agents pane from Advanced in its sidebar menu, on local and remote machines, the tabs bar above the pane can be shown even when the screen is not split, and a restart comes back on the pane and session you left.
  - Codex's options pill gains Plan mode, checked while Codex is actually in Plan mode, and Fast mode and Plan mode now share one Modes section.
  - Picker cards in chat start collapsed with their first two options side by side and expand when you click the title, answering one takes effect immediately and only returns with a reason if the answer did not land, and the Selected in terminal badge is gone.
  - Claude's "Resume full session as-is" row is answered with a single Escape.
  - Reasoning rows show their whole heading wrapped instead of one clipped line, the transcript stays pinned to the bottom as it grows, and the chat box's scrollbar appears only when the pointer is over it.
  - A session card shows a white dot on its agent icon while the chat box holds unsent text, and Switch Agent lists agents in the same order as the sidebar.
  - Session rows on a remote machine use the same spacing as the same project opened locally.
  - The new `ghostex resources` command prints the rows the desktop Resources panel shows together with the per-process sample behind them, and Clean RAM copies a diagnosis prompt built from that snapshot.
  - Sleep Inactive counts idle session daemons across every project rather than mounted panes, and a daemon belonging to another project's session is listed under that session instead of as an orphaned process.
  - The Tips warning about missing agent hooks opens Settings → Agents at the roster instead of an empty search, and only names agents you actually keep in the sidebar.

## 8.6.0 - 2026-09-02

- Major
  - The sidebar shows one machine at a time: machine tabs sit under Search, a remote tab's cloud icon connects or retries and explains a failure in its tooltip, right-clicking a tab hides the machine or opens its settings, and Add Project, Sort & Filter, Collapse All and Edit Machine now live in the More menu.
  - Spaces end with a built-in Other space that holds every project no Space claims, replacing the All Projects view on desktop, web and mobile.
  - Claude conversations can be rewound from Session Chat: every prompt has a Rewind to here button that drives Claude Code's own restore flow, the transcript then shows only the active branch, and the rewound prompt lands back in the composer for editing.
  - The Android app gains machine tabs and Spaces, a Web Preview that opens any port listening on the computer through the SSH connection with a picker of live ports, QR scanning to pair a tailcat machine from the desktop's Remote settings, and Hide from Sessions for machines you do not want polled.
  - Model, reasoning effort and Fast mode choices in the chat composer come from a published catalog that updates without an app release, and Codex sessions gain a Fast mode pill.
  - Sessions whose agent process dies outside Ghostex, including Quit Ghostex & BG Service, a crash or a reboot, now go to sleep instead of disappearing into history, keep their chat, and wake on the next launch.
- Minor
  - Native alert dialogs such as the paste-protection and close-terminal confirmations appear again instead of invisibly holding keyboard focus, thanks to banozz0.
  - Codex hooks are now marked trusted when installed, so Codex actually runs them instead of showing Installed while silently skipping them, and Settings says when a hook update is required.
  - A session started from Handoff or Export is created as a draft, so Chat View is available immediately and, when your default agent view is Chat, it opens straight into chat with the handed-off prompt in the composer.
  - Forked Claude and Codex sessions no longer share the parent's identity, so the fork and its parent stop showing the same chat and title.
  - The Session Chat context menu adds Locate File on paths and Copy URL, Open in Embedded Browser and Open in External Browser on links, and Save Image now works for large images.
  - Sleep and Wake are only offered for sessions that are actually running or sleeping, and Sleep All, Wake All and Pop Out Pane skip stopped history rows.
  - Waking a session or opening a new terminal takes one probe instead of three, session listing no longer reloads the whole registry on every request, and long zmx operations no longer stall unrelated actions.
  - The sidebar reconnects on its own with backoff after a gxserver restart, and remote machines stop refetching a full snapshot for every stale update over SSH.
  - Tailcat connections from Android 11 and newer work again, connection failures show the real cause, and newly paired machines no longer need to reach the relay map before the first tunnel byte.
  - Bundled skills install without Node or npm on the PATH, the Ghostex CLI link refuses to overwrite an unrelated ghostex or gx command, and first-launch setup waits for the daemon before reporting hook status.
  - Claude status rows are retired against the transcript so near-duplicate lines no longer linger, a /compact typed in the terminal shows the compacting card, and a sent message no longer reappears as an unsent draft on the next start.
  - The new `ghostex ports` command lists every listening TCP port on the machine, and `ghostex rewind-session-chat` drives a Claude rewind from the CLI.

## 8.5.0 - 2026-09-01

- Major
  - Cursor is now a full Chat View agent: sessions read their model, context window, reasoning effort and Fast mode live off the terminal, thinking shows as an activity card, Cursor's own questions are answered inline with Space and Enter, and transcripts export like any other agent.
  - Remote machines can be reached without a Tailscale account: gxserver runs its own tailcat tunnel, owns its key and address, comes back automatically after a restart, and is set up from the Remote tab in Settings.
- Minor
  - Cursor, Grok, Hermes, OMP and Pi sessions now name the screen that is blocking the composer — a picker, approval, sign-in or editor — instead of silently swallowing the message.
  - Codex and Claude always launch with their unattended permission flag, so turning Accept All off no longer leaves them stuck on an approval prompt nobody can answer.
  - Resuming or forking a Codex or Claude session no longer replays a stale session id from an earlier one-time command.
  - Session notes are edited in the same Monaco editor as the composer.
  - File and skill references appear as pills in the mobile and embedded chat composers instead of raw Markdown, and copy back out as plain text.
  - Opening a file at a specific line in an already-running Code tab works again, and falls back to copying the path with a notice when it cannot.
  - A single click on the collapsed command pane expands it, and the double-click-for-a-new-terminal gesture now applies only once it is open.
  - Close After Done and Sleep Inactive only sleep sessions that are actually running, so pinned, tagged and favorited sessions are no longer swept away.
  - The sidebar has symmetric gutters, separators that run its full width, and a plain gap against the workspace in place of the stacked separator strip.
  - The mobile chat view can switch an unprompted draft session's agent and send Enter or a shifted option key without typing into the terminal.
  - The transcript no longer shows a second typing indicator while the agent works, since the pinned working strip already reports it.
