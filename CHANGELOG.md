# Changelog

## Unreleased

- **The phone's scan screen says where the code is:** two steps with icons (click Connect a phone on the computer, then point the camera at its code), with the camera inside the second step, Paste a code instead as a full button under it, and "It won't scan" for tips (hold the phone a hand away, turn up the brightness, or copy the code on the computer and paste it). Android keeps its live camera there and gains a light button; an iPhone still opens Apple's scanner from that step. Comes with the next phone app.
- **The phone's Machines screen is tidier:** a computer's state is coloured once (its badge, "Can't reach" instead of "Failed"), the line explaining the swipe is gone (a tap opens the computer's page; the swipe keeps Remove), and the ways to add a computer are one compact row each in the same words as the first screen. Comes with the next phone app.
- **A computer's page on the phone puts what matters first:** Machines > a computer now opens a page that says whether it is connected and how, shows its Approvals with what is waiting and the devices that can reach it, and keeps the rest (name and Sessions tab, Ghostex updates, agents and tools, Advanced) behind one row each, instead of one long form with a Save button. Comes with the next phone app.
- **A coordinator's threads sit under its card in the Unified sidebar:** folded by default, with the crew count on the card to open and close them; an open card lists the busy and recent threads first and the rest behind one "N older threads" row, and a search or Reveal opens the way to a matching thread. On computer, web and phone, thanks to @sebastianarena.
- **Pairing a phone says what is happening, and why it failed:** after the scan the phone shows "Check your computer" with the three words in the middle, how long it still waits, and help when the computer shows nothing. A failed pairing stays on screen with the real reason ("Nobody approved this phone in time" is no longer "This code has expired") and Scan again as its button, and a pairing that works opens your sessions with "Connected to <computer>" on top instead of a separate Connected screen. Comes with the next phone app.
- **The phone's first screen says what to do on each device:** three steps with icons (get Ghostex on the computer, click Connect a phone there, scan the code here) and one Scan the code button. Paste a code and Use Tailscale moved into "Other ways to connect" (the old "How does your phone reach your computer?" screen is gone), and Try a demo computer is a small link under the button. Add a computer on the phone shows the same steps. Comes with the next phone app.
- **The Tasks pill drops tasks the agent no longer has:** a Claude chat's pill used to keep showing the tasks of a finished piece of work after the session restarted (Claude starts a restarted session with an empty task list but leaves its old task files behind). It now follows what Claude answered in the chat: a restart, a TaskList with no tasks, or a task Claude says it does not have removes the old ones, and a TaskList that names tasks shows exactly those. On computer, web and phone, once Ghostex is updated on the computer running the session.
- **Arrow keys move the caret in a Ghostex page's note box again:** Left and the other arrows, Home/End, Page keys and Delete, with Option or Shift too, no longer type a blank into a page's text field, the Code view or another web view when a key was pressed before the web runtime finished loading (for example while it updated in the background after an app update).
- **Claude and Grok Bot buttons moved to the header:** they sit directly left of the "Talk to the voice agent" button (each only while its extension is on), and are gone from the sidebar's bottom bar and from the sidebar row the header shows while Claude is on.
- **Connect a phone in one window:** the sidebar menu's Connect a phone (was Mobile & Remote) opens a window that turns Easy Connect on by itself and shows the code at once, with two plain steps, a line that shows it's waiting for your phone and Copy the code. It replaces the Remote Setup dialog, the stop at Settings > Remote and the separate Show QR popup, and also says you can use Tailscale or connect another computer from the same window.
- **One question for a phone that asks to pair:** the Connect a phone window, the notice that shows when that window is closed and the row in Settings > Remote all ask "Is this your phone?" (or computer) with its name, the three words side by side in large letters, one sentence and Not my phone beside Approve; after Approve the window says "<phone> is connected" with Done. While the window is open the request shows only there. The window's "Need the app?" iPhone link now opens the App Store.
- **Settings > Remote fits on one screen:** no more tabs. On top, "Phones and computers that can reach this machine" with Easy Connect's switch, Connect a phone, Connect a computer and every paired or waiting device (a waiting one first, with its three words and Approve, whether Easy Connect is on or off); then "Computers this machine connects to" with Add a computer; then three rows that open in place: This computer (name, short name, SSH access), Tailscale and Advanced (now also holding the Relay choice). Onboarding's "Configure my phone after this flow" opens the Connect a phone window.
- **The Grok extension is called Grok Bot:** its sidebar button tooltip, its Settings rows (Grok Bot start page, Grok Bot logins) and its Built-in switch now say Grok Bot.
- **No more stray "23;24;28;32;42c" in an agent's terminal:** when a session was also open on a second computer (a Windows one in particular), switching to its terminal could type that text into the agent's prompt and shrink the terminal to the other computer's size, because the other computer's terminal answering a question from the agent was taken for typing. Fixed for sessions started after Ghostex is updated on the computer that runs them.
- **OpenCode threads open on their conversation, also on another computer:** a thread a coordinator starts (here or with `--computer`) now shows the conversation in its terminal instead of OpenCode's empty start page, and a thread on another computer is no longer listed as Finished while it's still working, even when that computer still runs an older Ghostex. The terminal half needs Ghostex updated on the computer running OpenCode.
- **Cmd+= and Cmd+- zoom in half the steps:** a press changes the interface scale (sidebar, chats, Files and terminal text) and a chat's or Files' zoom by 5% around 100% instead of 10%, with the larger steps above and below halved too, and the focused terminal's own text zoom (Scale areas separately) steps half a point instead of one. 70%, 200% and Cmd+0 are unchanged, and the scale you saved before loads as it was. On computer and web.
- **Each project has a main coordinator on each computer:** the first coordinator you create in a project gets a quiet "Main" tag, and right-click (or long-press on the phone) > Make main coordinator gives it to another one there. It is the one agents and Chief of Staff contact when they want something from that project: `ghostex agents send --main-coordinator --project-id <id> "…"` (add `--computer <name>` for another computer). When the project has none, the agent chooses: `--if-none create` starts the project's coordinator, `--if-none session` starts a session that does the work and reports back. Closing the main one passes the tag to the oldest coordinator left.
- **Coordinators wear a captain's hat:** a coordinator's row, its chat pills, its message cards, the `+` picker and the phone show a captain's hat instead of the crown, in the same white (black on light themes). Chief of Staff keeps its crown.
- **Busy chats send the phone much less while you watch:** a chat's live updates no longer repeat its unchanged draft and lists every few seconds, so a busy coordinator chat stays lighter on the phone and on mobile data.
- **One question to Chief of Staff, a GBot or Ask Ghostex no longer starts a swarm of Help chats:** a Ghostex Help chat that couldn't find an answer used to ask Ghostex Help again, which started another chat that did the same, so one question could fill the "Ghostex" project with a dozen chats and use up your plan. Help now answers from its guide or says the guide doesn't cover it, asking again waits for the answer already on its way, and Chief of Staff, a GBot or Ghostex Help can start at most 6 sessions for one request.
- **OpenCode keeps showing that it's working for the whole turn:** the chat box shows Stop and the working word until OpenCode finishes, also when OpenCode's plugin stops reporting partway through a long turn (it used to show the Send arrow while OpenCode was still working, #529). On computer, web and phone, once Ghostex is updated on the computer running OpenCode.
- **Cmd+D from a session in a sidebar folder keeps the split:** the new session joins that folder and opens beside it, instead of landing in the project's own list and leaving the folder; Cmd+Shift+D and the other quick creates there do the same, thanks to @roypadina.
- **A project's buttons work on an entry that merges two copies:** Add Worktree, History, Create Agent and the launcher's agents open for the copy the entry acts on (another computer's, or a second clone), and the Unified sidebar's project menu works for a project the Project sidebar hides, thanks to @sebastianarena.
- **The VS Code view and the web runtime stay installed across updates:** when an update brings a new version of either, it updates by itself (the view shows "Updating" with its progress) instead of asking again as if it were a first install, and a new version whose download is identical to the one you have is kept without downloading it again (GitHub #530).
- **A thread you message after marking it done reports back again:** a message (or `ghostex coordinator reopen`) to a finished thread puts it back under its coordinator and its reply always arrives, even when its session takes a while to start; a thread counts as closed only once its session has stayed closed for a minute, and `reopen` answers within seconds instead of timing out.
- **A message to a sleeping or closed agent always says what happened:** `ghostex agents send` answers within seconds (sent, or waiting for the agent to start) instead of timing out while the agent wakes, and sending the same message again while the first copy still waits no longer delivers it twice.
- **`.tsx` and `.jsx` files are coloured in Files:** imports, strings, comments, `const`, JSX tags and attributes now get colours like `.ts` files do (a `.tsx` file used to colour only types, capitalised names and a few keywords).
- **Chat look follow-ups (Settings > Chat > Advanced):** the `!` command card, picture-only messages and the loading placeholder follow Left aligned, Band and Compact, and Compact also tightens the gaps inside a reply. Agent work Reduced and Quiet fold a subagent's report into the agent's work line, keep questions and pictures in view, and the status line names the running call instead of a working word. A custom Transcript width is now the widest the transcript gets, so a side panel you open takes the empty space beside it first. On computer, web and phone, thanks to @sebastianarena.
- **Coordinator in the New thread picker works in the Unified sidebar:** picking it opens the New Coordinator dialog, also for a project merged into another entry, thanks to @sebastianarena.
- **Quieter Autopilot and Resources pills:** the Autopilot pill's tooltip says just "Autopilot is on" or "Autopilot is off"; the Resources pill says just "Resources" with a number only while a question is unanswered (it used to count every form, page and visual ever made, 867 in a long chat), and its list shows the newest 30 per group with one "Show all N" row for the rest. On computer, web and phone.
- **Ghostex asks only at the door, with one click:** a plain Approve card shows when a new phone or computer asks to pair (compare the three words), when a Collab guest sends a message in Approve mode, and when an agent turns on Easy Connect, lets a device into it or shares a chat. Approve no longer asks for Touch ID, Windows Hello or a password unless you turn that on.
- **The security log says how each approval request ended:** next to its request, the log now records when it was approved (and on what: the Ghostex app, a paired phone or the CLI), expired (unanswered, or approved and not used), was replaced by a newer request (named), or was withdrawn because the session that asked was closed. gxserver's own log keeps a short line for each request made, approved, denied, ended and used, with its id, so a request that keeps failing can be followed. Neither log holds a secret or a key, and older log files read as before.
- **A phone waiting to pair shows under Paired devices with Easy Connect off:** a phone that signs in over SSH or Tailscale and asks to pair now shows its three words and Approve under Settings > Remote > Paired devices whether Easy Connect is on or off (the list used to show only while Easy Connect was on, so there was nowhere on the computer to see the phone).
- **An approval card survives a restart and never dead-ends:** a request still waiting when Ghostex or a computer's gxserver restarts comes back with the same words and time left, so Approve on its card still works (it used to say the request was already answered or ran out of time, while a fresh one sat unseen). The same phone or agent asking again gets that request back; when it asks for something different, the newer request replaces the waiting one and its card shows instead; and a request that ran out of time says so ("This request expired. Ask again from the phone.").
- **Everything else just works, for you and your agents:** connecting your computers, every setting (including the ones agents were refused), GBots, Autopilot, Cloud Boxes, teams, extensions, secret providers and starting an agent with full access no longer ask or say "only you can do this". Agents still can't approve a card, answer a secret request or read your secrets.
- **Approve any connected computer's cards from the one you're at:** a card from a computer you added under Settings > Remote can be approved right there; the "act as the owner" step on the other computer is gone, agents reach your connected computers as soon as you connect them, and a Mac restored from another Mac's backup no longer hides that Mac's cards.
- **New switch: require Touch ID or your password to approve** (Settings > Security; Windows Hello on Windows), off by default; on the phone it's "Require Face ID to approve" on the computer's page under Machines. It applies to every device that approves for that computer.
- **Settings > Security shows what still asks, and holds the security log's switch:** a short list says exactly what asks for an Approve (a phone or computer pairing, a Collab guest's message in Approve mode, and a few things only when an agent asks), the Touch ID switch explains itself right in its card, and "Show the security log" (moved here from Settings > Extensions > Experimental, same setting) sits beside Open log.
- **Phones on Tailscale, your network or SSH need no pairing:** with the phone app update they work, and can approve, from the first sign-in, with no "Approve this phone?" on the computer. Older phone and Ghostex apps keep working: their Face ID or Touch ID approvals count as a plain Approve.
- **Computers you control over Easy Connect stay connected to each other:** a computer you already paired now gets connection keys of its own for each computer it reaches, by itself, with nothing to approve and no need to pair again, so the app's connection, Ghostex's own connection and a connection to a second computer no longer get in each other's way. Computers and phones on older Ghostex versions keep connecting as before.
- **Coordinators keep work items:** one item per line of work, with where it stands, who has the ball (you, the coordinator or a thread) and one of six statuses: Needs you, Blocked, In progress, Waiting, Later and Finished. Threads, steps, questions for you and what it waits on belong to the item, and a wait on another item or question clears itself once that's done. The coordinator updates them with `ghostex coordinator item` (add, now, step, ask, wait, done, drop, list). Your board's open items move over once, keeping their numbers, and the Board pill keeps showing them, now drawn from the items; older coordinators' `ghostex coordinator board` commands keep working.
- **Dictate types exactly what the engine heard:** Ghostex no longer polishes dictated text with AI, removes filler words or keeps its own dictionary; set those up in Hex if you want them (OpenAI and xAI will mishear names more often). The speech models Ghostex's old built-in engine downloaded are deleted by themselves.
- **A computer you first set up over SSH runs its newest Ghostex:** when that computer also has Ghostex installed (from a package, the AppImage or the Mac app), connecting to it starts whichever Ghostex there is newest instead of always the copy Ghostex installed over SSH, and an older one still running is replaced without closing its sessions. A computer that only has its own Ghostex now connects to it instead of offering to install another copy, and Settings > Remote shows the version that runs there.
- **The Machine picker lists only connected computers:** on Settings > Agents, Integrations, Dictation and Updates it shows this computer and the ones connected right now ("Only connected machines shown"), and goes back to this computer when the one you picked disconnects.
- **Work item cards under a coordinator's replies:** under each reply, a card for every item the coordinator changed in that turn says its status, where it stands, who has the ball and how long ago, and anything it asks of you is a card of its own with its answer buttons (or Done). When the same item changes again later, its earlier card folds to one quiet line, and its questions count in the chat's ? pill. On the computer, the web and the phone.
- **The Board pill is now Work:** one row per line of work, grouped by who has the ball (Needs you, Blocked, In progress, Waiting, Later and Finished today), with what the coordinator asks of you listed first, each naming the work it belongs to and whether it holds that work up, with its answer buttons or Done. Rows in In progress say who is on it and how far its steps are; Waiting, Later and Finished stay folded. On the computer, the web and the phone. Phones on 11.2.1 keep their Board as it was.
- **The chat box opens the moment you scroll back to the end:** after scrolling up, which folds the chat box to one line, scrolling back down to the last message opens it at once, and so does a click or a keystroke in it. On a very long chat on a busy computer it could stay one line high for seconds, with an empty gap above it. Reaching the end with the scrollbar opens it too. On the computer and web.
- **A coordinator's reply is news on top, then its cards:** cards come what-needs-you first; a finished item with nothing more to read is one line; everything still running is one "In progress · N" row that opens to a list, each row going to that item's newest card. The chips under a card are quieter (an icon and muted words, no outline), "You" and "Coordinator" are gone from them, and a thread, link or file chip opens it. On the computer, the web and the phone.
- **A question is asked once:** when a coordinator's reply asks a question that a work item also asks, only the ask card shows, and answering either settles both. A task the coordinator gave you stays under Needs you after its work is finished, until you mark it done. `ghostex coordinator item ask` answers can hold a comma (`--answer "Too small to read, simplify it" --answer "Looks fine"`, or `\,` in `--answers`). The Work panel's last group line is no longer cut off.
- **Coordinators write news, not status sections:** a coordinator's reply is now a couple of sentences of news and whatever a card can't hold (a cause, a caveat, a draft, the reasoning behind a recommendation); its work item cards replace the Done, In progress and Needs you sections. It asks a question in one place, as a work item's ask when the question belongs to a line of work. Running coordinators are told to re-read their playbook at their next message. Marked handovers keeps working for every other session and for a coordinator's threads.
- **Coordinators can save a rule for every coordinator:** when you give a lasting rule for all coordinators, the coordinator proposes the wording and, once you confirm, saves it as a playbook override (saving overrides from a coordinator used to be refused). `ghostex coordinator playbook --list-overrides` numbers them, and `--add-override "<rule>"`, `--replace-override <n> "<rule>"` and `--remove-override <n>` change one without touching the others; every coordinator is told its playbook changed with its next message.
- **Work item cards read like the chat:** the cards under a coordinator's reply, their one-line folds and the In progress row with its list use the chat's text size and your Text strength (Settings > Chat), and follow them as you change them. The In progress row counts only work that is really moving, without a "waiting" count; Waiting and Later items show under a reply only when that turn changed them, and the Work panel keeps listing them. On the computer, the web and the phone.
- **The Work panel's columns fold, and Finished today is a column like the others:** a click on a column's header folds it to a narrow strip with its count (a click on the strip opens it again), so the open columns get the room; Finished today opens in its place in the row instead of floating over the others. Which columns are folded is remembered. When the open columns don't fit, the row scrolls sideways with the trackpad, with Shift and the mouse wheel, or by dragging its background. On the computer and the web.
- **A coordinator's question outlives its finished work:** when the coordinator marks a line of work done, a question it still asks you stays under Needs you, as a task already did, until you answer it or the coordinator withdraws it; it no longer shows as Withdrawn.
- **Pictures in terminals:** programs that show pictures inline with the Kitty graphics protocol (agent CLIs such as omp that draw diagrams and charts, and TUIs that use Unicode placeholders) now draw them in Ghostex's terminals on the computer, as they do in Ghostty, instead of leaving a blank gap or stray symbols (#536, thanks to @adriancendon). Pictures scroll with the text and are removed when the program deletes them or clears the screen. The web terminal doesn't draw pictures yet and doesn't answer programs that ask, so those show text there; a picture already on screen is gone after Ghostex reattaches to the session.
- **Work item cards: one look for GitHub links, everything at the chat's size:** a GitHub PR or issue on a card or in the Work panel shows GitHub's mark and its real state in that state's colour (open, merged, closed) whether the coordinator linked it to the work or a thread added it; it stays neutral only until Ghostex has read it (`item add --link` links used to stay grey, or show green whatever their state). Under a card, the thread shows its agent's logo, and each part (thread, PR or issue, file, steps) is its own quiet link at the chat's text size, wrapping to a second line instead of being cut; the status and age labels are the chat's size too. A withdrawn question no longer leaves a line under the reply, and a one-line fold always shows its answer or status and its time in full. The In progress row has the cards' background. On the computer, the web and the phone.
- **"In progress" instead of "Moving", and the In progress list grouped by who has it:** wherever you read a work item's status (cards, the row under replies, the Work panel and its Columns, the phone, `ghostex coordinator item list`) it says In progress. Opened, the In progress row groups its work under quiet labels: With you first, then what waits on something outside such as GitHub, then threads working, then the coordinator's own; closed, it says who has what ("1 with you · 1 thread · 2 with the coordinator"). On a wide chat the list is a compact table (Item, Where it stands, Who, Age); on a narrow chat and the phone each row shows who has it (the thread's agent logo, GitHub's mark, you) with its full text. On the computer, the web and the phone.

## 11.2.2 - 2026-11-10

**Ghostex 11.2.2 is out.** Phones and Easy Connect computers stay connected through restarts and slow links, iPhone voice calls work even when the call microphone keeps stopping, Dictate never loses a recording, messages between agents fold into "Worked for", approvals work across your connected computers, and Android gets push notifications.

### 📱 On the phone

- **iPhones on 11.2.1 already have this release's phone fixes:** they arrived over the air, and the last ones come with this release's update. Per-computer Easy Connect keys and the full fix for the iPhone call microphone come with a later iPhone app; Android gets everything in this release's app.
- **The phone is back right away after Ghostex restarts or updates on your computer:** over Easy Connect, the phone reconnects within a few seconds of Ghostex coming back, instead of showing "Reconnecting to …" for half a minute or more.
- **The phone stays connected on a slow connection:** on weak cellular, or when Easy Connect or Tailscale goes the long way round, the phone no longer starts over while a chat, a long reply or a picture you're sending is still coming through, so nothing is lost, and a connection that really drops is still noticed and restored within seconds.
- **A phone with two or more Easy Connect computers keeps them all connected:** once your phone app is updated, it connects to each computer with its own key, so connecting to one no longer stalls another for half a minute. Ghostex on your computers is ready for it now, and phones on the current app keep working as before.
- **A phone on Tailscale or SSH asks each computer to pair once:** reopening the app or reconnecting waits on the same request instead of asking again, and one plain card says where to approve ("Approve this phone?" in Ghostex on that computer) and what waits meanwhile; it stays away for a computer that's asleep or that you hid.
- **Android: push notifications reach the phone:** with this release's Android app, finished, waiting and error notifications arrive as push notifications. iPhones get them with a later iPhone app.

### 🎙️ Voice and Dictate

- **iPhone voice calls work even when the call microphone keeps stopping:** on iPhones where the call's microphone stops right after it starts (iOS 27 with the 11.2.1 app), the call switches to recorded turns: hold Talk, speak and let go to send, and the agent's replies show on screen and are read aloud. Needs Ghostex updated on the computer.
- **A voice call survives Ghostex restarting on your computer:** a phone that reconnects to a call the restart cut off gets a new call with the same settings straight away (the screen says it is a new call), instead of "That conversation already ended", and Ghostex's log says how each call ended and when a microphone stopped sending sound.
- **The phone's voice agent says what's missing and where:** when a computer has no voice key (or can't read it, or has the voice agent off), the phone names that computer and says to add the key in Settings > Voice Agent there; a call about all your sessions uses another computer that has a key, and "The microphone stopped." shows once instead of over and over.
- **Dictate on the phone never loses a recording:** the phone keeps your audio until its words are in the chat box. If your computer can't be reached, it says "Waiting for your computer…" and sends it again on its own once the connection is back; leaving the chat keeps the recording, which waits above the chat box as "Recording not transcribed" with Transcribe, Share (iPhone) and ✕.

### 💬 Chat

- **Messages between agents and your answers fold into "Worked for":** with Tool call cards on (or Agent work set to Reduced), "Message to …", "Message from …" and "You answered …" cards no longer stand on their own; the turn's "Worked for" line counts them ("2 messages", "1 answer") and opening it shows them first, on the computer and the phone.
- **The pills above the chat box no longer scroll or fade away:** when the row does not fit, pills drop their label from the right (icon and count stay, hover shows the name), and only then do the last ones go into one "+N" pill that lists them. On the computer, the web and the phone.
- **Bigger text in the chat box's popovers and pills:** the Context window popover, the pills above the chat box and the lists they open use the same size as the model name (they were 9.5 to 12.5 px), with taller rows and wider menus so nothing clips. On the computer, the web and the phone.
- **Questions with previews show each option's preview in full:** in the "Needs your answer" card a preview shows up to 10 lines, then scrolls with a visible scrollbar, and long lines scroll sideways instead of wrapping, so mockups stay lined up (#524).
- **Press Enter while a pasted picture is still saving and the message goes at once,** with a loading tile where the picture will be; after a moment the Back to composer arrow lets you take it back, and a picture that can't be saved brings the message back with its error.
- **Dialogs over the chat no longer black it out:** Save to Markdown and Rewind keep the conversation in view behind a light dim, and so does the Agents Hub's sync sheet; in Ghostex Web, every dialog laid over the page.
- **Your messages to a coordinator show only what you typed,** with no "your playbook changed" line under them, and View playbook is on a coordinator's right-click menu, its chat's More actions menu, the phone's long-press menu and Settings > Chat > Coordinator playbook.
- **New installs show no status line under the chat box** until you star items in Context details; everyone who already used Ghostex keeps the line they had, and Reset to recommended still pins the suggested setup.
- **Claude Code's Fable 5.2 is in the model menu:** pick it and the pill shows Fable 5.2 instead of falling back to Fable 5.1, which still works too.

### 🌐 Remote computers

- **Remote machines over Easy Connect stay connected** (#526): a computer you control through Easy Connect no longer keeps dropping and reattaching its terminals, because all of its connections now share one link instead of competing with each other.
- **Approve your connected computers' requests from the computer that controls them:** a request shows Approve only where approving it works. A computer with its own Ghostex app asks you once, there, to let this computer answer its requests; after that its cards work from here. Until then a notice says where to approve.
- **Agents see the computers you connected, and say what's missing:** `ghostex computers` and every `--computer` command list a computer that still waits for your "Let the agents on … reach each other" approval and say which computer shows that card, instead of saying none is connected.
- **Settings > Remote shows the Ghostex version a computer is really running,** the same as its About screen; when its background service still runs an older build, the tile says so ("background service 10.45.0") and what to do, and a failed version read is tried again within a minute.
- **A remote computer's session drags onto the work area** to split a pane or show it in the middle, like a local one, pinned or not, in the Separate-by-machine tabs or the Unified sidebar.
- **No more "Git unavailable on Windows" notice at every start:** Git stays quiet for a computer that isn't connected yet (or is asleep) and fills in once it connects; clicking Commit or the Git menu says plainly that it isn't connected, or is connecting.

### 🔒 Pairing and security

- **A phone pairing over Tailscale or SSH no longer opens Easy Connect to any device while it waits.**
- **The "still use the old pairing" count in Settings > Remote counts only devices that really need pairing again:** it skips phones you removed, older or unused pairings, phones that now sign in over Tailscale or SSH and phones waiting for your Approve, and each old pairing that needs nothing has Forget.
- **Settings > Remote's Relay picker fits its labels, and each option has an (i)** that explains Ghostex relay, Public relays only and Custom on hover.
- **Settings > Remote's tab strips and pickers keep their labels off the edges at narrow widths,** shortening them with an ellipsis instead.

### 🧩 Settings, Files and accounts

- **Settings > Accounts: adding a Claude or Codex account you removed earlier no longer gets stuck** ("already saved, use its Reconnect action" or "Login did not complete"): Ghostex repairs the leftover saved entry and saves your sign-in without asking you to sign in again, and a sign-in that still fails says why, with Copy details.
- **Settings > Extensions: click an installed or Community extension's tile to open its page** (details, settings, permissions, version, Update or Uninstall); Back returns to the same tab.
- **Files moves lines with Option+Up and Option+Down** (Alt+Up / Alt+Down on Windows and Linux): the current line, or every line the selection touches, moves by one line, like VS Code, one undo step each.

### 🤖 GBots (beta)

- **Calls with a GBot:** call a GBot from its row or one of its chats through the Voice agent; it answers in its own voice (chosen in GBot Settings), says what it is doing while it works, asks permission prompts out loud (Touch ID approvals stay on the computer or phone), and the call's turns land in its chat.

## 11.2.1 - 2026-11-09

**Ghostex 11.2.1 is out.** GBots come back as Ghostex's own bots (beta, off by default), chats get opt-in looks and one / key for skills, cloning runs in the background, Settings and Extensions get tidier, iPhone voice calls keep the microphone, and Windows and macOS get a round of fixes.

### 🤖 GBots (beta)

- **GBots are now Ghostex's own bots (beta, off by default):** turn on Main Hub, then GBots in Settings > Extensions > Experimental. The previous GBots and its separate download are gone; their bots and chats were not carried over, and their old files are in your Trash as "GBots (previous)". The Bots space shows Hermes bots again.
- **GBots sit in Main Hub under Chief of Staff** on your computer and phone, each with its icon and colour and its chats folding under it; New Chat, GBot Settings and Remove GBot are a right-click (or long press) away, and GBot folders never show as projects.
- **Chief of Staff hands work to your GBots:** it gives a job to the right GBot or asks one a question and gets the answer back in its chat; when no GBot fits it adds one and tells you in a "GBot added" card you can undo (Settings > Agents makes it ask first). `ghostex gbots ask`, `assignments`, `changes` and `undo` are new.
- **GBots remember:** each GBot keeps short dated notes that load into every chat, longer topic notes and a daily log, and the oldest notes move to an archive when memory fills up. The Memory tab shows what loads and lets you edit or undo any change, and `ghostex gbots prefs set` sets preferences every GBot follows.
- **GBot routines:** ask a GBot for something on a schedule and it sets a routine; a free check (new mail, a calendar event, a web page, a folder or a command) runs first so a quiet day costs nothing, each run is a chat of that GBot, and a "Routine set" card has Pause, Edit and Undo.
- **GBot departments** group GBots with shared instructions and a lead who hands the others work; they show as small headings under GBots.
- **GBots tell Chief of Staff when they're stuck:** a GBot chat that can't start, stops, goes quiet for 20 minutes or whose routine fails is reported to Chief of Staff, which restarts it where it stopped or tells you in two lines what only you can fix; it answers approvals only for GBots it added.
- **Keep an eye on my day:** with Gmail or Google Calendar connected, Chief of Staff can set up Day Watcher, which briefs you every morning and warns you about a meeting about to start or important new mail, with a notification on your computer and phone. It only reads; Ghostex refuses anything it tries to send or change.
- **Team files and presets:** export your GBots (or a department, or one GBot as a preset) as a JSON file with secrets removed, and import one with a preview first; routines arrive paused and Undo removes the whole import. New GBot can start from built-in or kept presets.
- **GBot API:** an app or script on your computer can message a GBot and get its reply, or start one of its routines, with a key from GBot Settings > Bot API. It's off until you turn it on with your approval; messages from apps run in their own folder, can't read your secret folders, don't see the GBot's memory unless you allow it, and only you answer what they wait on.
- **A computer per GBot:** in GBot Settings > Computer, a GBot's chats can run on one of your connected computers or a Cloud Box, still reading its instructions and keeping its memory here, optionally using that computer's screen and apps (Touch ID to give it more).
- **GBots and Chief of Staff on Codex can run `ghostex` commands:** a new Codex GBot starts at Full access and Chief of Staff on Codex runs without asking; on macOS and Linux Codex still keeps them out of your secret folders.
- **A GBot asked to start work in a folder adds it as a project** and starts the coordinator, thread or session there (`--path <folder>`), and nothing but a GBot's own chats can land in its hidden folder (#519).
- **GBots hide completely while they are off,** including their chat cards, Quick Access folders and Automate routines, and GBot chats install the GBot playbook skill they are told to read, here and on a connected computer.

### 💬 Chat

- **Chat look settings (Settings > Chat > Advanced, all off by default):** messages that start at the left, your messages on a full-width band, the agent's work as one small line per message (Reduced) or behind a button on each reply (Quiet), the status line as the chat's last line, a chat box as wide as the transcript, and compact spacing; the phone follows the computer, thanks to @sebastianarena.
- **Tool call lines (Settings > Extensions > Experimental, off by default):** each run of an agent's tool calls is one quiet line you open to see its calls, and running calls now shimmer in every look. On the phone the switch is in Settings > Chat.
- **/ is the one key for skills and commands with every agent:** the list shows the agent's skills, then its custom and plugin commands, then its built-in commands, each under a heading, and each agent receives a picked skill in its own spelling. $ still opens the same skills, and a Codex message with two skills no longer shows the text between them as a formula.
- **The skill picker lists a Claude Code plugin's skills and commands as Claude Code loads them** (#513): skills named in its plugin.json, commands in nested folders or named by the manifest, and a picked command reaches Claude as /plugin:name. The prompt editor draws command pills like the chat does.
- **A Claude chat on a Claude Swap account reopens again** (#521): Ghostex finds the conversation in that account's own history, and a chat whose conversation really is gone says so and offers Start fresh, which starts a new conversation in the same session instead of leaving the input box stuck.
- **Right-click an HTML file an agent linked to Open in Browser** in a new Browser tab of the session's project (#515).
- **File and folder paths in Ghostex's own screens open in Files when clicked** (Settings pages and their descriptions, dialogs, chat folders): `~` expands to your home folder, a right-click offers Copy path and Reveal in Finder, and a folder outside the project opens in your file manager.
- **A session's 5h and 7d limits read as reset once the reset time has passed** and take the newest reading from its login, so an idle coordinator no longer keeps its old percentages (#514).
- **Text strength raises an agent's reply from its standard colour,** so the slider works with Show thinking in grey on.
- **The working line above the chat box no longer repeats the running tool call's words** when the transcript already shows them; it shows the agent's status or its working word and time instead.
- **The pills strip fades each pill out at a scrolled edge** instead of painting a dark band over them, so no half-pill stub is left, on the computer and the phone.
- **A click on the coordinator board that jumps into a chat closes the board first,** so the chat is visible.
- **Session Links: an × on the Suggested header removes every suggestion** in two clicks (the first arms it red); they go to Removed and can be restored, on the computer, the web and the phone.

### 🧩 Settings and Extensions

- **The Extensions page is in tabs:** Built-in, Views, Panels & menus, Installed, Community and Experimental, with one search over every tab, a short title on each tile with its description behind an (i), and the page reopens on the tab you had open.
- **Each experimental feature has its own switch in Extensions > Experimental,** replacing Enable Experimental Features; Faster rendering moved there from About.
- **Grok, a new built-in extension (off by default):** a Grok button beside Claude's opens grok.com inside Ghostex, with several Grok logins kept apart (each signed in on its own) and a start page you can set in Settings > Files & Links.
- **Settings > Updates has the machine picker and Update all machines:** pick a machine at the top to see and update only its agents and tools, or update every connected machine at once with a line per machine saying how it went. Not installed pills install on a click and spin while they do.
- **Settings never shows a long build id, hash or path whole:** it shows it short, and pointing at it shows it in full.
- **Settings > Security shows only while Extensions > Experimental > Security logs is on (off by default):** the log is still written, its alerts and `ghostex security log` work as before, and a security alert's link still opens the page.
- **Problems in Settings read plainly:** each one leads with a sentence, raw installer and command output moves under Details with Copy details, pages that can't reach Ghostex's service show one empty state with Try again, and Accounts says "Sign in needed" quietly.
- **The Home folder card is the last on Settings > Agents,** and its folders row starts folded to a count ("25 folders") that opens the editable list.
- **Merge copies of the same repository, a switch in Settings > Sidebar & Sessions:** turn it off to keep two copies of one repository on a computer as two projects; the phone follows it (#518).
- **Settings > Autopilot says when the supervisor wakes,** and a supervisor that can't start or wake is logged and retried less and less often instead of every 10 seconds.
- **When Connectors, Workspaces, Main Hub or Bot automations are off, the terminal hint now names a command that works.**
- **Long text in the Updates popover is cut with an ellipsis inside the popup,** and hovering shows it in full.

### 🌐 Remote computers and projects

- **Cloning a project no longer holds the Add Project dialog open:** it runs in the background behind a progress button in the header, a project on another computer can be cloned onto any connected computer from its right-click menu (Clone on…), and the folder browser lists your home folder's folders again.
- **The machine dialog (Settings > Remote) shows less at first:** each field's explanation is an (i) that appears next to its label when you point at the field, errors still show under the field, and Agent access moved into Advanced as its first row.
- **A failing machine's tile shows two words and a red dot,** and its sheet leads with the problem, Reconnect, Reinstall and Copy details.
- **Threads started with `--computer` get `GHOSTEX_COORDINATOR_ROLE=thread` and `GHOSTEX_COORDINATOR_SESSION` too** (#523), so launch wrappers on that computer can tell them from your own sessions.
- **An approval a connected computer waits on shows there even when its app missed the moment it was asked,** and the asking app no longer shows one only that computer's owner can answer.
- **A phone on Tailscale or SSH no longer brings up the pair-again notice** or its "can't connect" line in Settings > Remote.
- **A command to a connected computer no longer gets lost right after that computer's Ghostex restarted** (#520), and Ghostex's log says when and why an Easy Connect connection ended.

### 📱 On the phone

- **iPhone voice calls keep the microphone** (#500): a call no longer loses it in a loop, "The microphone stopped." shows once, and Unmute or Talk brings it back. This arrives with the new iPhone app.
- **A dropped connection says so on the phone** ("The connection to the computer dropped. Ghostex reconnects by itself…") instead of blaming Ghostex on the computer.
- **The phone gets the chat look settings, Tool call lines, the / list's headings, GBots in Main Hub and the Suggested links ×** with this release.

### 🍎 macOS

- **`ghostex agents reload` works again on macOS** (#522): a reload, or any wake right after a sleep, no longer fails with "the session could not be started again" or "macOS refused to start it from this login", and a reload that still fails says why, as does a thread's report.
- **A Mac's Ghostex service started from its app bundle finds its terminal program,** so a computer connecting to it no longer says its zmx program is missing, and start at login starts it the right way.

### 🪟 Windows

- **On Windows without Windows Hello, approvals take your Windows password** (#520) instead of needing a PIN: pairing a computer and letting it act as you.
- **Glass set to "Wallpaper only" no longer crashes Ghostex at start** (#516), an open window picks up a new desktop wallpaper, and a solid-colour desktop no longer shows the previous picture blurred.
- **A Windows computer no longer keeps trying to connect to itself every minute.**
- **An administrator prompt nobody answers no longer holds pairing or SSH setup forever;** after 5 minutes Ghostex says so.

### 🩹 Fixes and polish

- **Daily, weekly and cron automations run at their time in your timezone** (they ran on UTC time before) and handle daylight-saving changes.
- **Long chats are faster:** loading older rows ends in half the time, a new row no longer rebuilds the whole transcript, and questions, the coordinator board and Resources are no longer rebuilt on every event.
- **Ghostex uses less of your computer while idle:** the menu bar, the settings poll, the sidebar's flash and spinners, account usage and coordinator panels do less work.

## 11.2.0 - 2026-11-08

**Ghostex 11.2.0 is out.** Bots get your connected apps and sign-ins, pasted images send with your message, agents work across your projects without asking again, approvals open as centred dialogs, remote computers connect more smoothly, and Settings gets tidier.

### 🤖 Bots

- **GBots bots get every app you connect in Settings > Connectors** and your gh and Google sign-ins, with the same Ask, Allow and Never rules, and they answer setup questions with Ghostex's own screens.
- **Bot chats stay calm:** no Autopilot nudges, and a Ghostex message never becomes a bot chat's title.
- **One sidebar session per bot chat**, and a renamed bot chat keeps its name without "/rename" being typed to the bot.

### 💬 Chat

- **Enter waits for a pasted image and sends your text and the image together**, on the desktop, the web and the phone, and pasting is faster.
- **Tool cards stay one line** with the running call in the header ("+2 more"), and open when a call waits for your approval.
- **Red error lines are one-line cards**, and several pictures in one message show without "[Image #2]".
- **A queued Codex message never leaves a stuck "left the queue" card** (#509).
- **The Summary and Detailed view chips show their icon**, turning into the × to close, and handover headings below a one-click answer are styled again.

### 🔐 Approvals and agents

- **Agents work across your projects again without asking:** they message, start, type into, interrupt and answer sessions in any project, while pairing, Collab, security settings and starting an agent with approvals off still ask you.
- **Every notice with a button is a centred dialog**, one at a time ("1 of 3"), and an "Approvals waiting" pill brings back the ones you put aside. Copy details sits with the buttons, and light dialogs' main buttons are frosted grey.
- **A Claude Code prompt with the auto-block countdown is answered on the first click**, and an answered card dims with "Sending…".

### 🌐 Remote computers

- **A computer added with its password connects right away** (#508), and an Easy Connect computer that must pair again says so instead of retrying.
- **A connected computer asks once to act as you**, not on every connect; a computer that is really this one never asks to let itself in, and an approved request takes effect at once.
- **Settings > Remote has a Current machine card** with this computer's name, short name and SSH access, and it no longer checks SSH every 10 seconds.
- **Windows can message a Mac when signed in as an administrator.**

### ⚙️ Settings and the window

- **Theme presets sit in one compact sideways row**, colour rows stack in a narrow window, and the Settings rail's groups read in sentence case with more room.
- **Settings > Dictation explains Hex clearly**, with Install Hex (the official installer, after you confirm with Touch ID) and a link to anoma.ly/hex.
- **View playbook shows the playbook your coordinators follow**, from the New Coordinator dialog, Settings and the right-click editor.
- **macOS: full screen with transparency shows your blurred desktop picture** instead of black.
- **macOS: Login Items lists Ghostex's helpers as Ghostex**, and Ghostex stops rewriting them on every start (#510).
- **Main Hub (beta) in Settings > Extensions**, off by default.

### 🩹 Fixes and polish

- **A session whose terminal ended outside Ghostex shows as sleeping and reopens** (#512), thanks to @bet4it.
- **Chief is now Chief of Staff.**
- **macOS: programs in your sessions reach the local network again** (#504).
- **Coordinators: a message and a thread report sent at the same moment both arrive** (#505, #506), and a finished report no longer raises a false "not moving".
- **Pins are one filled and outline pair everywhere**, and right-clicking a pinned extension's header icon offers Unpin and Disable.
- **The extensions menu's Terminal chip reads in the light theme**, the × on toasts and board cards is easier to see, and Linear tickets linked by their ID open in the right workspace.

## 11.1.1 - 2026-11-08

**Ghostex 11.1.1 is out.** Connected computers ask for your approval once instead of over and over, the pair-again notice walks you through each device, the settings that open your computer up are yours alone, agent skills arrive signed, Linux follows COSMIC's display scale, and many fixes.

### 🔐 Security and pairing

- **A connected computer asks once:** one approval card per pair of computers, never a PIN prompt every minute, and with Agent access off it never asks at all.
- **The pair-again notice lists each device on its own row** with one status and a page per device, and tells you the good news: the security review is done and pairing again takes a moment. A phone's pairing code is made only while its page is open.
- **Phones that sign in over SSH or Tailscale pair with matching words**, a re-paired computer counts as paired, and a phone you deny is told so. Every Easy Connect copy gives a code that can pair.
- **Some things are yours alone, in Settings:** installing or turning on an extension and its permissions, secret providers, joining or setting up a team, and the settings that loosen security. Agents can't ask for these; they point you to the page.
- **Install from folder… in Settings > Extensions** installs an extension from a folder, with the permissions you tick.
- **Agents ask once per project:** the first time an agent reaches another project's sessions it asks you, and then it can go on without asking again.
- **Secret requests are answered only by you**, and each one is tied to the session that asked.
- **Agent skills that update between releases install only when Ghostex signed them.**
- **Cloud Boxes ask you before an agent puts your logins or a project's secrets on a box.**

### 💬 Chat and sessions

- **The handover headings are coloured:** Needs you pink like the sidebar's question dot, Blocked red like its red dot, and Kicked off sky blue.
- **A message held because the agent's input box is off screen heals itself**, reloading the agent after a few minutes when nothing else brings the box back, and Claude Code's rating survey no longer holds a message for hours.
- **An automatic reload or restart waits while the agent still has background jobs running.**
- **Chief's row is drawn like a project header**, its name and flower lined up with the projects, with a fold chevron to its left when other computers have a Chief, on the computer and the phone.
- **Clicking a Global Action in the Start menu runs it** (#501).
- **A coordinator's thread on a connected computer starts in a worktree without stopping at the folder-trust question** (#502), and a stale "not moving" report is dropped instead of arriving late.

### 📲 On the phone

- **Calmer notifications:** the bell's rows read like the desktop's cards with the session's name, and a blocked session shows its red dot with the reason in its menu.
- **The phone calls it the Easy Connect code** everywhere you paste or scan it.

### 🩹 Fixes and polish

- **Linux: Ghostex follows COSMIC's display scale**, Settings > Interface has a Linux-only Display scale row, and a raised Interface scale is offered a one-time reset (#503).
- **Switches are clearer:** an on switch is a mid-tone track under a white handle.
- **Spaces and playbook sync fixes:** each folder of a repository keeps its own playbook, a newcomer takes the shared Spaces, Settings > Sync lists projects that sync moved, and sync waits for git remotes after a start (#488).
- **Alert me for says what each choice changes.**
- **A session of a removed custom agent is named by its base agent**, not its raw id.

## 11.1.0 - 2026-11-08

**Ghostex 11.1.0 is out.** A security update that puts you in charge of everything sensitive, with safer pairing for phones and computers (including over SSH and Tailscale); Collab becomes an extension you install, with a new Share popover; Chief, one agent per computer you can ask about anything; calmer notifications; a calm card when a computer is offline, with messages held until it is back; Settings in six groups; and many fixes.

### 🔐 Security update

- **You approve what matters:** turning on Easy Connect, letting a computer connect, Autopilot's approvals and limits, GBots, settings that loosen security, and an agent answering, stopping or typing into another session all wait for your approval with Touch ID, Windows Hello or your system password, or Face ID or the fingerprint on your phone.
- **Agents and scripts can ask but never approve:** every session gets its own key when it starts, the `ghostex` CLI waits for your approval and then repeats the call, and settings only you may change can't be written through the API.
- **Settings > Security keeps a security log** of every sensitive action, chained so an edited or removed entry is flagged; `ghostex security log` shows it in a terminal, and only the important entries reach the bell and the phone.
- **Easy Connect serves only SSH and Ghostex itself**, an empty allow-list lets nobody in, and file links never open keys, Ghostex's own data or a project's hidden files like `.env`.
- **Every downloadable component (Collab, GBots, the browser engine) carries Ghostex's signature**, checked before it is unpacked.
- **After updating, a one-time notice lists the phones and computers that connect through Easy Connect and must pair again**, with the pairing QR right there, ticking each one off as it pairs; phones and computers on Tailscale, your network or SSH keep working.

### 📲 Pairing phones and computers

- **Each phone pairs with its own device key** kept in the Secure Enclave or Android Keystore: it waits as Pending with three words, and you approve it on the computer, or on a phone already paired, only when the words match. Removing a phone cuts it off at once.
- **Computers pair the same way:** paste a Show QR code into Add a machine, check the three words on both screens and approve.
- **Phones that sign in over Tailscale, your network or SSH pair once too**, with the same three matching words; until then answers, Stop and typing from that phone wait for your approval. Run in terminal on the phone uses the phone's own key.
- **Pairing again needs the new phone app** (Android in this release, iPhone on TestFlight).

### 🤝 Ghostex Collab

- **Collab is now an extension:** the Ghostex you download doesn't contain it until you install it from Settings > Extensions (or right in the Share popover), with Touch ID; Settings > Collab and Share appear once it is installed, and Uninstall ends every share.
- **The new Share popover is the live panel:** the link and code, who is watching and who is waiting, a Link lasts chip with 1 hour, 4 hours, 24 hours or Until I stop it, Review for a guest's waiting message, and Stop sharing that says who gets disconnected. The phone's Share sheet matches.
- **Every share, Send mode and guest message you send on asks for your approval**, links need an eight-digit code, and the guest page is pinned to the exact version your Ghostex trusts.
- **Guests can answer the agent's questions and open the chat's Ghostex pages and pictures**, and the guest page works on phones; their answers wait for you like their messages.
- **Collab and GBots show in Settings > Updates and the Updates menu** with Update when a new version is out.

### 🌸 Chief

- **Chief is one agent per computer you can ask about anything:** any session, agent, project or computer, and Ghostex itself. Its row with the Ghostex flower sits under the Spaces bar, on the computer and the phone.
- **Chief reads every session and steers them for you** (messages, interrupts, answers prompts) without asking each time, hands project work to that project's coordinator, and leaves security, sharing and pairing to you. "Ask anything about Ghostex" in the Help menu opens its chat.
- **Each computer has its own Chief**, and you can turn it off in Settings > Extensions > Chief.

### 🔔 Notifications

- **Alerts come only when a session needs you:** a thread finishing stays quiet, a coordinator alerts only when its handover needs you, and every red dot alerts unless Autopilot heals it.
- **The phone's bell is an inbox of sessions** under Needs you, Done and Failed, one row each, and Android's persistent notification follows it, thanks to @cagatay-io.
- **The bell's dot counts only the rows its panel lists.**

### 🌐 Remote computers

- **A calm "Windows PC is offline · last seen 2h ago" card** sits above the chat box when a computer is off or asleep, with Retry; a message you send then waits and goes out by itself when the computer is back, with Cancel.
- **Machines that ask for a verification code (2FA) connect:** turn on Reuse an existing SSH connection in the machine's Advanced settings and sign in once in a terminal, thanks to @chen1plus.
- **Updates covers every connected computer**, with Install for what one of them is missing.
- **Autopilot and Not moving wait while the computer is offline** instead of nudging agents that can't hear them.
- **Easy Connect recovers faster** after a stall or a network change on the phone.

### 🧭 Coordinators

- **Playbook overrides replace editing the playbook**, and the coordinator picks which memory notes go into a thread's brief.
- **A coordinator links its own threads on another computer**, and a thread's brief there waits out a folder-trust prompt (#497, #502).
- **The Start menu lists Global Actions under a Global heading** (#501).

### 💬 Chat

- **The agent's reply fades in, and a long one stops the chat at its top** so you read from the start.
- **Opening a tool group or any fold at the bottom of the chat scrolls to show its end.**
- **A board card mention is its own teal chip with the card's title**, dimmed once the card leaves the board.
- **The model menu shows tabs only for agents you turned on**, its long tooltips stay inside the menu, and the "Not applied" reason shows in full, thanks to @cagatay-io.

### ⚙️ Settings and the window

- **Settings lists its pages in six labelled groups**: Working, Appearance, Voice, Agents & Accounts, Connections, and Privacy & System; every page opens with a one-line intro.
- **Frame the work area is on for new installs**, and macOS keeps the window buttons when the sidebar is collapsed (Settings > Interface > Advanced).
- **The floating sidebar runs the full height, floats as a card inside the frame and keeps its traffic lights crisp**, thanks to @zxcvbinz.
- **Dictation uses Hex**, Ghostex's own speech engine is gone, and Dictation and Voice Agent are separate pages with each key under its picker.
- **The voice panel resizes from its edges** and follows new lines until you scroll up.
- **Files groups filtered and searched files by folder**, and the Changes view can hide test files, on the computer and the phone.

### 🩹 Fixes and polish

- **A Mac that runs gxserver without the Ghostex app can install skills**, thanks to @sebastianarena.
- **Automatic account switches no longer hold back an account with room**, and a switch whose agent won't return to the shell reloads the session on the new account.
- **The Gemini account shows Antigravity's Claude and GPT limits**, matching agy's /usage.
- **Spaces sync keeps each folder of a repository in its own Space** and puts back Spaces that 11.0.0 folded together (#488).
- **macOS: arrow keys no longer type stray characters into a Ghostex page's text fields.**
- **A GBots conversation opens in the chat**, Cloud Boxes run the model and effort you picked, and Cancel stops a waiting setup at once.
- **OpenCode names the provider when two providers share a model name** (#482), and push and event-stream fixes, thanks to @cagatay-io.
- **The Install button for Ghostex Agent Guide works**, and the Secrets row's trash button and Uninstall all remove its skill.
- **Long chats load older pages faster** and gxserver starts faster with many sessions.

## 11.0.0 - 2026-11-07

**Ghostex 11.0.0 is out.** Share a chat with anyone through a link with Ghostex Collab, let Autopilot keep your sessions moving while you are away, build your own bots on Claude Code and Codex with GBots, dictate with Hex, sync accounts, agents and skills between your computers, and annotate pages with screenshots; the iPhone gets a Live Activity, Home Screen widgets, the iPhone Duo two-pane layout, terminal themes and a Files browser.

### 🤝 Ghostex Collab

- **Share one chat through a link:** turn it on in Settings > Collab, press Share in a chat's header and send the link and its six-digit code; the other person follows the chat live in their browser, with no app and no account, and sees who is watching and typing.
- **You decide what a guest's message does,** per chat: Ask me (the default) holds it as a "Message from <name>" card with Decline, Edit and Send, Send passes it straight to the agent, and View only turns their chat box off.
- **A guest only ever reaches that one chat:** they can't answer the agent's questions or permission prompts, stop it, change its model, or see any other chat, file, terminal or setting.
- **Links end after 24 hours by default** (1 hour, 7 days or never in Settings > Collab), Stop sharing ends one at once, Remove keeps a guest out, and the Guest messages log lists every message and who joined or left.
- **The link's traffic is locked end to end** with the key in the link, so Ghostex's share relay can't read the chat; the chat itself runs on your computer.
- **The phone has the same Share sheet,** the guest message cards and Settings › Collab for each computer.

### 🛟 Autopilot

- **Autopilot keeps your sessions moving while you are away:** turn it on in Settings > Autopilot, from the Autopilot pill in a coordinator's chat, on the phone, or with `ghostex autopilot on`.
- **It carries on sessions that stopped too early,** interrupts a hung command after 8 minutes, reloads a session that shows nothing, restarts a terminal that ended, and moves a session at a usage limit to the account with the most room.
- **It can answer questions and approval cards for you** (both off by default): unanswered questions get their recommended choice after 5, 10 or 15 minutes, and approvals follow the Heal, Heal and decide or Everything tier you pick.
- **Hard limits stay yours:** pushing, merging, releasing, deleting outside the project and posting publicly are each a switch that is off until you turn it on, and sign-ins, 2FA and payments are always declined.
- **A supervisor agent wakes only for judgment calls,** in an Autopilot project in your sidebar where you can read why it did each thing, and every decision is in the Log at the bottom of Settings > Autopilot.
- **Pick which sessions it covers:** every session, the projects you pick, or the sessions you pick; each chat's Autopilot pill takes that session out or puts it back.

### 🤖 GBots

- **GBots are long-lived bots on your own Claude Code or Codex** that can start, message and steer agent sessions on any of your computers. Turn them on in Settings > Extensions, under Planning and automation (off by default; macOS and Linux).
- **Make a bot from the Bots space** with a name, Claude Code or Codex, and its instructions; each conversation opens in the chat like any session, on the phone too.
- **Bots run on the Claude and Codex accounts in Settings > Accounts,** so a bot moves to another account when one reaches its limit, and the sessions a bot starts show in your sidebar with a card naming the bot.

### 🎙️ Dictation

- **Hex is now a Dictate engine:** pick Hex in Settings > Voice to transcribe with Hex 3, the free local dictation app (macOS on Apple silicon, Windows and Linux), and your recording never leaves the computer. Install Hex installs it for you, and the phone can pick it per computer.

### 🧭 Coordinators

- **Make a connected computer's session a coordinator** from the sidebar's Advanced > Make Coordinator, or with `ghostex coordinator promote --computer <name>`.
- **A thread on another computer is watched like one here:** it is nudged after 8 minutes with no new output and reported finished from its transcript, so it never sits at "Working" with no report.
- **DeepSeek Harness can run a coordinator,** on the desktop and the phone (#480).
- **A thread's and a coordinator's agent start with `GHOSTEX_COORDINATOR_ROLE`** in their environment, so a launch wrapper can tell them apart, for example to start threads with fewer MCP servers (#463).
- **The board's Moving group is now called In progress.**

### 🔄 Sync

- **Accounts, Agents and Skills can sync between your paired computers** (Settings > Sync, each off by default): the Claude and Codex account list without the logins, which agents are on with their commands and order, and your own skills in `~/.agents/skills`.

### 🖍️ Browser and Files

- **Annotate captures a picture of each spot you pick,** and Send puts your notes and their pictures into the agent session you last clicked: its chat box, its terminal, or the clipboard. HTML files in the Files view get the same.
- **A file link in a remote chat opens in Files** for any file in the project, and a text file outside it (an agent's log, say) opens read-only (#462).
- **The Files page's fonts load again,** and a drawing opened during a project switch no longer shows "Files request timed out."

### 📱 On the phone

- **iPhone: a Live Activity lists your working and waiting agents** on the Lock Screen and in the Dynamic Island, and tapping a question row opens its question card, thanks to @cagatay-io.
- **iPhone: small, medium and large Home Screen widgets** show the agents that are working or waiting, thanks to @cagatay-io.
- **iPhone Duo: two panes side by side** (sessions, chat, Web Preview, Files, Git Changes, Agents, Usage and Attention), a wall of session tiles, the sessions waiting for you folded above the list, and a laptop mode that keeps everything off the fold, thanks to @cagatay-io.
- **The phone gets the desktop's theme presets and colour controls,** terminal palettes for every terminal, and Ghostty theme file import, thanks to @cagatay-io.
- **Files is a browser of the whole project** with search, a type filter, previews, Copy full path, Share and Add to session context, thanks to @cagatay-io.
- **iPhone: a movable command pad** with nine keys, an Upload key and its own text editor, thanks to @cagatay-io.
- **An attention list and agent notifications on the phone,** including a red "not sent" row for a message the computer couldn't send, thanks to @cagatay-io.
- **Actions… lists the computer's Global Actions** after the project's, also from a Unified session's long-press menu, thanks to @cagatay-io.
- **The chat shows held messages with a Pick model button,** OMP's advisor notes, and a bottom sheet closes when you drag its grabber down, thanks to @cagatay-io.
- **Edit machine shows whether an Easy Connect link is direct or relayed** beside "connected now".
- **A pasted image is one chip in the phone's chat box,** as on the computer, and tapping anywhere on a switch row toggles it.
- **Settings › Licences lists the third-party notices,** and text keeps its contrast across the phone's screens, thanks to @cagatay-io.
- **Android: a terminal fling no longer crashes the app** when the session reconnects mid-scroll (#477).

### 🔔 Notifications

- **Alert me for: Needs you only** (Settings > Notifications) plays the sound and shows the notification only when an agent asks you something, waits for approval, or ends with a Blocked or Needs you section (on the phone too), thanks to @gittyforthehub.
- **A failed agent run is its own red row** in the Notifications bell, thanks to @cagatay-io.
- **An open question card counts as a question** in the sidebar, thanks to @cagatay-io.

### 🎨 Look and feel

- **Twenty theme presets** in Settings > Interface > Presets (Catppuccin, Dracula, Solarized, Tokyo Night, Nord, Kanagawa, Everforest and Gruvbox Material) set the whole app, chat and terminals to a theme in one click, with light-mode Full colour, thanks to @cagatay-io.
- **macOS: the floating sidebar shows the window's traffic lights** while the sidebar is hidden, thanks to @zxcvbinz.
- **A switch row toggles from a click anywhere on it,** not only on the switch.
- **Pinned order (Manual or Most recently updated) is also in both sidebars' Sort & Filter menu,** and pinned sessions can be dragged again when Pinned mixes several projects.
- **The tool card's stacked icons follow the theme** on window glass instead of drawing solid black or white circles.

### 🌐 Remote computers and Cloud Boxes

- **A computer without the agents' hooks still shows sessions going from working to finished:** Ghostex follows Claude Code, Codex and OpenCode from their own files, and `ghostex settings set` works on a computer that runs only Ghostex's server (#475).
- **Session resume hooks reach a computer that runs only Ghostex's server,** from Settings > Agents with that computer picked or `ghostex agent-hooks install --all --computer <name>` (#431).
- **Setting up a Mac that runs the Ghostex app as a remote computer keeps the app's own server running** (#476).
- **Cloud Boxes on Instabox's free plan start,** a failed box never blocks the next one, and Claude and Codex logins reach boxes again.
- **Instabox boxes run as their own user,** with your logins and secrets in a private folder only that user can read (#433).

### 🪟 Windows

- **Windows: Ghostex uses Codex's and Claude Code's own folders** when a global HOME is set (Git Bash, MSYS), so hooks, folder trust, logins and transcripts land where the agents read them. If you have a global HOME, press Install hooks once in Settings > Agents after updating (#458).
- **Windows: starting from Git Bash no longer fails** while unpacking the VS Code view.

### 🩹 Fixes and polish

- **Fast Computer & Browser Use installs from GitHub** when cua.ai does not answer, on Mac, Linux and Windows.
- **Ask Ghostex opens its chat within about a second** instead of waiting on the skill install.
- **A Claude question with previews is answered from the chat,** and a message you send while a question is open waits for your answer (#479).
- **A dialog card whose answer was refused can be dismissed,** and a Codex dialog card's pick is no longer refused when Codex only redrew the dialog (#458).
- **Codex's hook trust matches what Codex 0.167 reads** (#458).
- **A resume the agent can't find is never retried forever:** OMP starts fresh with your held message, and Claude and Codex mark it Not sent with Retry (#456).
- **The not-moving watch leaves a session with an open question or approval alone** (#459).
- **Projects that share a git remote on one computer stay separate** in the merged sidebar, while two clones of one repository still merge (#461).
- **A custom agent that runs `wally codex` or `wally claude-code` keeps its own login,** thanks to @sanchitmonga22 (#478).
- **Pi on Linux gets its chat** when only your shell's startup puts it on PATH (nvm, fnm, bun, pnpm), and a Pi or OMP chat that can't see its conversation says why and offers Install and restart.
- **OMP: inline approvals answer through the permission picker,** Allow answers only its own card, subagent rows open their transcript, and approvals and questions notify, thanks to @cagatay-io.
- **macOS: a dead key followed by a consonant no longer types a second apostrophe** (U.S. International), thanks to @sebastianarena.
- **A pill strip menu opens under its pill** wherever the chat window stands, thanks to @cagatay-io.
- **OpenCode no longer sits at "Waiting…" in Updates,** or offers an update mise won't install.
- **`zmx` in a Ghostex session runs Ghostex's own zmx first.**
- **The Gemini usage meter stacks its 5h and 7d on two lines** like Claude and Codex.
- **A question tag answered on any copy is answered on every copy.**
- **A new session in another project opens in Chat right away,** and a Codex boot no longer swallows a new chat's draft.

## 10.49.0 - 2026-11-06

**Ghostex 10.49.0 is out.** A long chat no longer slows the app down, worktree sessions show under their project, coordinators get a How to use pill and a board you can click, tag and drag, Ghostex pages, forms and visuals get their names, Windows installs from PowerShell, and Ghostty 1.4, VS Code 1.145 and a newer GPUI arrive under the hood.

### ⚡ Faster and lighter

- **A long chat no longer keeps the app busy:** a chat rebuilt its whole transcript whenever the sidebar's order changed, which used close to a full CPU core on a long chat and made a pasted image take seconds to appear; it now rebuilds only what changed.
- **GPUI, GPUI Kit and Zed's renderer are updated,** with fewer redundant redraws, a fix for a Windows startup stall, Zed's drag cursor fix and colour emoji on Windows.

### 🧭 Coordinators

- **A How to use pill in every coordinator chat** opens the tips for working with a coordinator, each in its own card with a line you can click to put in the chat box; tick Don't show again to hide it everywhere.
- **Clicking a board card opens it in place** with its whole title, latest line and links; ⌥-click or its Mention button puts it in the chat box.
- **Right-click a board card (or its ⋯) for Open, Mention in chat, Tags, Mark done and Remove,** in the same menu style as the sessions sidebar.
- **Board cards take the same tags as sidebar sessions,** and move between groups by drag and drop.
- **A new On hold group** holds work waiting on something outside, while Blocked now means it needs you right away; a thread still not moving after Ghostex's nudge lands there by itself.
- **Blocked has its own pill** next to the ? pill, and the Board pill shows only Needs you and Moving.
- **Approving a thread's prompt from its card in the coordinator's chat works again;** it had been refused with "That session is not one of this coordinator's threads."
- **The In progress heading in a handover is amber,** matching the board's Moving group.

### 💬 Chat

- **The things agents show in the chat have names:** a Ghostex page (gx page) is an HTML page an agent publishes, a Ghostex form (gx form) is a question form or one-click answers, and Ghostex visuals (gx visuals) are charts and tables. Ask for them by name; the old words still work.
- **A Ghostex page opened from its HTML file shows styled,** from Files, a file link in the chat, Open in browser or the phone, instead of as plain HTML.
- **A message you send while the agent works shows once,** not twice, when Ghostex adds a note to it.
- **A picture you pasted yourself stays out of a closed fold's thumbnails;** the row that read it still shows it when the fold is open.

### 🗂️ Sidebar and projects

- **Worktree sessions show in their project:** a session in a worktree is listed under its repository's project with its branch, instead of as a separate project (Settings > Sidebar & Sessions > Advanced, on by default).
- **The Unified sidebar's project header lists its computers in its tooltip** instead of showing computer pills.
- **The Working shelf is now called Moving,** and the Links panel uses the same light menu surface as the rest of the sidebar.

### ⬆️ Updates and installing

- **The top bar's Updates menu shows progress:** an item you click updates in place with a spinner, the button spins while any update runs, and a failed update turns it red with a reason on hover until you dismiss it.
- **Windows: install from PowerShell** with `irm https://ghostex.dev/install.ps1 | iex`, which checks the installer against GitHub's checksum and skips the browser's SmartScreen warning.

### 🌐 Remote computers

- **Easy Connect can use Ghostex's own relay** (Settings > Remote > Easy Connect > Relay), with Tailscale's public relays as the fallback; computers keep their current address, so no phone has to pair again.

### 🖥️ Terminal and Code view

- **Ghostty 1.4** under the terminals, built with Zig 0.17.1, with the session daemon on the same Ghostty.
- **The Code view runs VS Code 1.145.0** (code-server 4.145.0).
- **Windows: Claude Swap updates to 0.29.0.**

## 10.48.0 - 2026-11-06

**Ghostex 10.48.0 is out.** One Projects list across all your computers with shared Spaces and settings sync, a Settings > Updates page for the app and every agent, a simpler Accounts page with Cursor, Gemini and Copilot usage, a local Dictate engine, chats for Pi, OMP and OpenCode that no longer drive the terminal, coordinators that heal stuck threads, and one Interface scale.

### 🗂️ One sidebar across your computers

- **One Projects list across all connected computers:** a project on several computers (same Git remote, else the same folder name) is one entry with every copy's sessions mixed, other computers' cards named, and actions run on the computer you used last. Turn on Separate by machine in Settings > Sidebar & Sessions to bring back the computer tabs.
- **Spaces and project groups are shared by every connected computer** in the one list, and the Unified sidebar shows the active Space's sessions from all of them.
- **Each computer has a two-letter short name** (set in Settings > Remote), shown in the sidebar's computer pills and after the branch on a Unified card.
- **Every Unified card is two rows,** with the computer after the branch; drag pinned cards to put them in your own order, and project groups fold with an animation.
- **Delete Worktree in the Unified sidebar:** a project header's right-click lists its worktrees, and a worktree's card has its own Delete Worktree.
- **The Unified sidebar's hover buttons follow the pointer from card to card,** and a closed pinned, favorite or tagged session leaves its group instead of staying as a row whose × does nothing (#435), thanks to @roypadina.

### 🔄 Settings sync

- **Paired computers can sync Spaces, project groups, Appearance, Agent defaults, Sounds and notifications, Keybindings and Coordinator settings,** each behind its own switch in Settings > Sync (off by default, except Spaces). The newest edit wins, deletions carry everywhere, and it goes over your existing Easy Connect or Tailscale links, never through a server. Secrets never sync.
- **A phone can join settings sync** and shows as Phone in Settings > Sync.

### ⬆️ Updates

- **Settings > Updates lists everything this computer uses that has an update:** the app, agent CLIs, Claude and Codex Swap, cua-driver, SpaceO, extensions, remote computers, the VS Code view and CEF, with Update all (the app last), a per-card Auto-update switch and a counted badge in the top bar. Updated agents restart into the same chat when idle.
- **Gemini installed with npm from mise's Node updates through mise** instead of being refused as a manual install.

### 👤 Accounts

- **Settings > Accounts has one Add account flow:** pick a provider, the browser opens, and you land on the new account, with its usage pinned in the sidebar. A Use this account menu picks Automatically, Only when I pick it, or Paused.
- **Cursor, Gemini and Copilot usage** show in Settings, the sidebar meters and the phone.
- **A chat's account switcher lists only that agent's accounts,** and notices name an account by its nickname or email, never "Account 5".

### 🤖 Agents

- **Pi and OMP chats send, switch model and thinking, stop and rename through Ghostex's extension inside the agent** instead of typing into the terminal, and a model pick confirms even when an extension like pi-footer replaces the footer (#423).
- **An OpenCode chat works without opening its terminal** (#418), and OpenCode conversations started outside Ghostex show under Quick Access > Sessions (#426).
- **OMP chats show background jobs, agent messages and advisor notes as rows and cards** (#444), get the Subagents pill, and read OMP's symbol presets in model picks and questions (#442), thanks to @cagatay-io.
- **Hermes chats stop, pick model and effort, rename and answer questions through Hermes's gateway.**
- **DeepSeek Harness gets a Ghostex plugin** for its chat.
- **Cursor sessions start reliably** with zen mode and oh-my-zsh prompts, and pin only real model ids (#422).
- **Claude Code's Context details can show an Agent row** with the session's --agent name.

### 🧭 Coordinators

- **A thread that shows no new output for 8 minutes is nudged and its coordinator told,** every 8 minutes until it moves; if it still doesn't, it gets a red dot, a Not moving board chip, a notification and its own sound (#427).
- **A session whose terminal process dies under a running agent restarts on its own** on the same conversation and tells its coordinator once.
- **The board shows each thread's live state** (working, waiting on a job, stopped mid-task, idle), and a parked thread whose background job finished is nudged after 10 minutes.
- **A thread's approval prompts show live in the coordinator's chat,** in an Approvals pill and on the board, answered only by your click.
- **The Board pill sits before Resources,** its Needs you count leaves out questions the ? pill already counts, and a card about a question moves to Done once it's answered.
- **Typing "Q12: yes" in the chat box answers that question everywhere,** the same as clicking its answer.

### 💬 Chat

- **A tool row that reads a picture shows it as a small thumbnail** on the row's right; click it to open the viewer.
- **Messages held by a failed model change explain why** and offer Pick model (#445), and "finished" attention is told apart from "needs input" (#446), thanks to @cagatay-io.
- **A send that finds the terminal service missing waits and goes out by itself** once it's back, and a failed send's note wraps under its own message.
- **Notices and errors are in plain words,** with the raw text behind Copy details.
- **The Tasks pill opens Session details.**

### 🎙️ Voice

- **Dictate has a local speech engine** (Parakeet v3, Whisper turbo and base) that runs on your computer's CPU, with a per-computer engine choice (Local, OpenAI, xAI), a dictionary, filler removal and AI polish.
- **Settings > Voice on the desktop** sets Dictate's engine, model downloads, language, hotkeys and microphone, and the phone sets each paired computer's in its own Settings > Voice.

### ⚙️ Settings and projects

- **One Interface scale for the sidebar, chat, Files and terminal text,** with Scale areas separately for per-area sliders (#425).
- **A Default projects folder,** and Add Project's New project folder opens a small form (name, folder, Space, group, what starts) filled in from where you are (#419).
- **When your computer starts** (Settings > Power & Sleep) can start Ghostex's server at login, also open the app, and resume agent sessions that were running when the computer shut down.
- **A handoff carries the files its transcript points to** (pasted images, files the agent saved elsewhere), even to another computer.
- **Files shows open files as tabs by default,** and onboarding's Workspace panel has a Spaces switch.
- **macOS: an optional Arc-style frame around the work area** (#430), thanks to @zxcvbinz.
- **Searching Settings for "code editor" finds the VS Code settings,** and the Extensions page has one search.

### 🖥️ Terminal

- **Switching a busy chat to its terminal no longer replays its whole history:** a viewer that falls far behind skips to one fresh screen, so remote sessions catch up in seconds (#429).
- **Wide characters such as CJK select and copy whole** (#453), thanks to @bet4it.

### 🐧 Linux and Windows

- **Linux: X11 follows Xfce's Window Scaling,** so Ghostex draws at the desktop's scale (#413).
- **Linux: Copy Image puts the picture on the clipboard** on X11 and Wayland (#420).
- **Linux: app dialogs, menus and toasts move with their window on X11,** and the narrow Files pane slides in properly.
- **Windows: a session whose host died restarts when you open it** instead of staying stuck.

### 📱 On the phone

- **The phone answers a thread's approval prompts relayed through its coordinator.**
- **The Tailscale notice shows only when the computer can't be reached,** has a close button that remembers, and a Tailscale reminder switch in Settings > Connection (#439).
- **Account usage columns, global quick actions and plain-word errors on the phone** (#441, #449), thanks to @cagatay-io.

### 🎫 Work mode

- **Create a ticket with a workspace's ticket command** from `ghostex issues create-ticket`, once and never twice (#434), **start work on a ticket command's ticket** from the Work page, `ghostex issues start` or the cloud (#428), and clear a stale ticket command's links with `--ticket <id>:none` (#421), thanks to @lardissone.
- **A Terminal Action's command runs when its new session starts** (#440), thanks to @cagatay-io.

### 🔒 Security

- **Each paired phone gets its own revocable token,** and Easy Connect can admit only paired devices.
- **Links that open another app, runnable files and ghostex://terminal commands ask first.**
- **Ghostex's data folders are private to your account,** and the server's token is sent only to your own account's listener.
- **Extensions get only what you approved,** CEF's DevTools port stays closed outside debugging mode, and background jobs (commit messages, titles, Catch me up) run read-only.

### 🩹 Fixes and polish

- **Settings repaints when the theme changes** (#451), **a remote terminal image paste asks the right computer** (#450), and **an answered question card no longer flashes on reopen** (#448), thanks to @cagatay-io.
- **Pairing a phone again replaces its earlier entry** instead of listing it twice.
- **The Install guide finds an agent installed in its installer's own folder** before it's on PATH.

## 10.47.0 - 2026-11-05

**Ghostex 10.47.0 is out.** Ghostex runs much lighter in the background on Windows and macOS, coordinators get a work board, agents' questions take one-click answers, the iPhone gets Voice mode, and the Unified sidebar, the new session picker and the new chat's tray are easier to use.

### ⚡ Faster and lighter

- **Windows: the Ghostex server uses far less CPU in the background,** from 2 to 3.6 cores down to about 0.13 on a busy computer with several sessions.
- **Windows: reading a terminal no longer starts a helper program each time,** and background helpers no longer open a hidden console window of their own.
- **Windows: Git is asked about a project again only after its files change,** for session status and diff counts, and the status line runs your command without extra launchers.
- **macOS: the Ghostex server reads running sessions and saved logins itself** instead of starting a helper program several times a second, and finds agent CLIs without starting a login shell.
- **Git answers are kept until the folder's files change,** so idle projects no longer run Git every few seconds.
- **Ghostex starts far fewer GitHub CLI calls** for Claude cloud sessions, session links and branch pull request badges.
- **A damaged line in an agent's transcript no longer makes Ghostex read the whole transcript again,** and scrolling back through a long chat's history reads only what it hasn't read yet.
- **The Unified sidebar redraws once a second instead of 30 times while agents work,** with the classic sidebar's steady status dots.
- **macOS: the Ghostex server raises its open-file limit,** so busy computers no longer fail sends and reads with "Too many open files".

### 🧭 Coordinators

- **A coordinator chat has a work board:** its lines of work grouped To do, Needs you, Blocked, Moving, Done and Backlog, placed from its threads, questions and pull request checks; open it from the Board pill above the chat box as a side list or a full overlay, and on the phone as a sheet.
- **Board cards say what to do:** a Needs you card offers one-click answers or Answer and Approve in the thread, Done folds into a slim rail, and × twice moves a card to Removed.
- **Clicking a board card puts it in the chat box as a pill,** and clicking that pill in a sent message opens the board at the card.
- **A coordinator's threads on other computers show in its Workers list and under it in the sidebar** with the computer's name, and open on their own computer.
- **A worker waiting for approval shows a red tag and a red dot in the Workers list.**

### 💬 Chat

- **Answer an agent's question with one click:** answer choices in its reply show as buttons under the question, and picking another one corrects the answer you sent.
- **The new chat's tray is one breadcrumb** (project › computer › where › branch) that centres or wraps onto two aligned rows when narrow, with ⌥O, ⌥M, ⌥W and ⌥B to change each part.
- **A Saved draft pill replaces the "Another saved draft is available" row:** it sits at the start of the pill strip, and clicking it previews the draft with Dismiss and Use draft.
- **The + mention menu lists sessions on your connected computers** with their computer and project, and a mention pill for one opens it on its own computer.
- **When the Ghostex server restarts, the chat waits quietly and carries on;** only a real outage shows "Couldn't reach Ghostex. It may be restarting." with Retry.
- **Links in questions show their words with a dotted underline** in the text colour, also in the answered card, the ? list and Resources.
- **Every pill shows its status the same way,** as small coloured dots with counts after its label, with no corner dots.
- **The working bar and tool rows never show a tool's raw JSON;** an unknown tool shows its name in words.
- **Marked handovers gain Comms, Held for release and Suggestions sections,** and what needs you (Read this, Blocked, Suggestions, Needs you) always comes last.
- **The floating Ghostex button's panel, count tray and docked tab are frosted glass** when Glass is on.

### 🗂️ Sidebar and new sessions

- **Unified cards say Untracked for folders without Git,** always keep three rows, and show the links card when you hover.
- **The Unified sidebar hides a disconnected computer's cards until it reconnects,** and leaves a coordinator's workers out, with a crew count on the coordinator's card instead.
- **Right-click a Unified project header for the project menu, and drag a card into the main area** to split or open it, like a Project sidebar row.
- **Reveal Active Session opens the Working shelf, project group or filter that hides the card** and flashes it, and a thin divider sets Drafts apart from the sessions below.
- **The new session picker shows computers under the project name,** search starts a session in a found project, projects with the same name merge one way, and Cloud Box shows only once it is set up.

### 📱 On the phone

- **iPhone: Voice mode is here** (#412): talk with an agent in a call that the lock screen, the Dynamic Island and the green call pill show with Mute and End, and in a push-to-talk call a Live Activity on the lock screen gives you Talk and Send with the phone locked; it uses Bluetooth or CarPlay when connected and offers to hold the conversation when a phone call comes in.
- **iPhone: screens with a header no longer crash on iOS 27.0.**
- **The phone gets the coordinator board, one-click answers, the Saved draft pill, mentions of remote sessions and the quiet reconnect line.**

### 📄 Pages

- **Text you type in a page is never lost,** even when you quit Ghostex, reload the page or the Ghostex server restarts.
- **Tooltips inside pages are frosted glass** instead of solid black or white.
- **The page window stops at its ends instead of showing grey when you scroll past them,** and blurs the wallpaper by the same amount every time.

### 🎫 Work mode

- **A workspace's ticket command can be its primary tracker,** with its open tickets on the Work list, picked with `ghostex issues tracker <id>`, thanks to @lardissone (#411).
- **Link a session to a ticket command's tickets by hand** with `ghostex link-session --ticket`, or Link to on the Work page, thanks to @lardissone (#414).

### 🩹 Fixes and polish

- **macOS: dead keys compose in terminal panes** with layouts such as U.S. International, thanks to @sebastianarena (#417).
- **Windows: switching accounts works in a forked Claude session,** thanks to @DaveW001 (#415).
- **Create Linear Ticket… shows only where Linear applies,** when the workspace picked Linear or a project or workspace Linear key is set.
- **Opening a worker on another computer connects that computer first** if only its saved copy is known.
- **A connected workspace's GitHub sign-in counts only when it holds a token,** and Settings says when its sign-in needs redoing.
- **The Ghostex server ends session keepers left behind by a deleted Ghostex server** when it starts.
- **Resources' row buttons line up,** so orphaned rows' × sits under the session rows' ×.

## 10.46.0 - 2026-11-04

**Ghostex 10.46.0 is out.** macOS keeps Ghostex's permissions across updates, pages open in a cleaner window that remembers where you left it, and fixes for Claude account sign-in, OpenCode and chat sends.

### 🍎 macOS

- **macOS keeps Ghostex's permissions across updates.** The background server and the session keeper now run as the signed "Ghostex Server" and "Ghostex Sessions" apps from one fixed folder, so Local Network, Files and Folders and other grants stay after each update, and old release folders are cleaned up. macOS asks for these permissions once more after this update; after that you can remove the old gxserver and zmx rows in Privacy & Security.
- **A Ghostex server installed on another Mac keeps the same place across upgrades,** so that Mac stops asking for its permissions again after each update.

### 📄 Pages

- **The page window shows your blurred wallpaper and has no title bar.** Round Close and Open in browser buttons float in its top-right corner, and you can move and resize the window; the next page opens where you left the last one.
- **Pages use a neutral glass tint and neutral picks.**
- **Every question page ends with an "Anything else?" box** that takes text and pasted images.
- **Links in question forms and question pages open on click,** and agents link what they ask about.

### 💬 Chat

- **The Subagents menu shows Done for finished subagents** instead of Idle, and counts them.
- **The Stop button in dark mode is see-through with a thin border,** like the buttons beside it, on the computer and the phone.

### 🧭 Sidebar and coordinators

- **The Claude button moved to the bottom bar, right of Commands,** and shows in the header's top row while Claude is open so you close it from the same spot. The view picker no longer lists Claude.
- **Coordinators can start a Claude or Codex thread on a saved account** with `ghostex coordinator start-thread --account <number|email>`; an account that needs sign-in is refused before anything starts.

### 🎫 Work mode

- **Ticket commands are checked again every hour and run less often:** a ticket is read again after two minutes, sooner while it reports pull requests, thanks to @lardissone (#409).
- **A project row follows its ticket command's name,** thanks to @lardissone (#404).

### 🩹 Fixes and polish

- **Signing in to a Claude account no longer breaks account actions and chat sends until the Ghostex server restarts,** thanks to @zxcvbinz (#410).
- **OpenCode no longer stays on "Cogitating" after its turn ends,** thanks to @DaveW001 (#403).
- **Chat sends work while Claude Code's /ide is connected** (#406).
- **When Git can't be read on a connected computer, Ghostex retries quietly first,** and the warning names the computer and the reason, thanks to @greggh (#405).
- **Windows: threads on the default Claude account start signed in** even when the computer's default Claude login is signed out, and a thread whose chat a crash emptied shows as waiting with a clear message.
- **A build-only release run no longer publishes the code-server component,** for people who build Ghostex themselves, thanks to @soarkit (#408).

## 10.45.0 - 2026-11-03

**Ghostex 10.45.0 is out.** Start a new session in any project on any computer from one picker, connect your own ticket tracker to Work mode, use Voice mode with sessions on your other computers, and hand over only the last few turns of a conversation.

### ✨ New sessions

- **Start a new session anywhere.** The New Thread picker (Cmd+N) names the project it starts in on its top row: click it or press Cmd+N again to pick any project on any computer, with your recent projects first. A project on several computers shows them as chips on its row, starting on the one you used last for it, thanks to @sebastianarena (#359).
- **The "+" on a project in the Unified filter and on a project group header opens the New Thread picker on that project,** and a project the filter hides joins the filter when you start a session there.
- **On the phone, the agent menu that starts a session shows its project on top** (tap it to pick any project) and a row of computer chips.

### 🎫 Work mode

- **Connect your own ticket tracker with a ticket command.** Set it with `ghostex issues source set <path>`, store its credentials with `ghostex issues source credential <name>` from stdin (secrets need at least 3 characters), and a session whose branch names one of its tickets shows it on its card and in its links with its title and state. Workspaces connected to a team can't use one, thanks to @lardissone (#396, #402).

### 🎙️ Voice mode

- **Voice mode works with sessions on your connected computers.** It can read them, send them prompts, answer their questions and approvals, stop them, show them and start agents in that computer's projects. A computer whose Agent access is off stays out of reach, and Voice mode tells you why.

### 🤝 Handoff

- **Hand over only the last turns of a conversation.** The History slider in Handoff picks how many recent turns go over (20 by default, remembered), with your earlier prompts listed, or turn on Whole conversation to send everything. On the computer, the web and the phone.

### 🩹 Fixes and polish

- **Clicking a notification switches the sidebar to the session's computer,** and so do the menu bar, Quick Access, the status item, `ghostex focus` and Voice mode when they open a session on another computer, thanks to @lardissone (#401).
- **Install another agent shows Installing, then Installed,** or the error with Retry.
- **Line numbers in Files are no longer cut off** on documents with 1,000 lines or more.
- **The built-in browser updates to Chromium 156.**
- **Linux: Ctrl+G opens Ghostex's prompt editor instead of vi, so the chat no longer gets stuck behind an editor,** and the false "zmx daemon is not running" alarm is gone, thanks to @isfix (#400).
- **Linux: Ghostex ships one copy of its keyring library,** thanks to @bet4it (#399).

## 10.44.0 - 2026-11-03

**Ghostex 10.44.0 is out.** Agents on two connected computers can run sessions on each other, closing a coordinator or a busy agent asks first, Cloud Boxes is on for everyone with Instabox and Boat, agents can ask you privately for a password or key, Ghostex cleans up its old files by itself, and chat cards say what happened in plain words.

### 🌐 Remote computers

- **Agents control sessions on a connected computer, both ways.** A coordinator can start threads on another computer and get their reports, and that computer's agents can start, message, read, sleep, wake and close sessions on this one while the connection is up. Turn Agent access off in the machine's Edit dialog to stop it. Both computers need Ghostex 10.44 or later.
- **The floating button's panel lists each remote machine,** with its agents when it's connected and a Connect row when it isn't, and the floating button and menu bar count only this computer and computers connected right now.
- **The pairing QR stays hidden until you click it,** and Copy as text sits right under it.
- **Easy Connect keeps working on the computer that hosts it,** so paired phones keep its address.
- **Linux: every package includes tailcat,** so Easy Connect and Cloud Boxes work without installing it yourself.

### 🧭 Coordinators

- **Closing a coordinator or a busy agent asks first,** naming what closes, from every way to close a session. A coordinator closes its open threads with it, and restoring it from Previous Sessions brings them back under it.
- **OpenCode coordinators start on the model and effort you choose,** with a search field for long model lists.
- **The New Coordinator dialog's agent list closes when you pick one,** and Model and Effort appear for every agent that takes them, thanks to @OliverJEvans (#395).

### ☁️ Cloud Boxes

- **Cloud Boxes is on by default on every computer,** Windows included. Run a new chat on an Instabox or Boat machine: pick Cloud Box beside This folder in the new chat, choose a snapshot, provider and size with its price, and the chat opens on the box once it boots.
- **Each box session gets its own branch,** Pull branch brings it home as a worktree project, and pictures in the first message go with it.
- **Box sessions show in their project in the sidebar** with a cloud badge, a stopped box's chat still opens, and sending a message wakes it.
- **The box chip in the header** shows the box's state, time and cost, with its terminal, snapshots, Open PR and Stop. Settings > Cloud Boxes sets providers, spending, timers and agent logins.

### 🤖 Agents

- **Agents learn what Ghostex can do from a short note** (pages, question forms, charts, its own browser, other agents), read the details from `ghostex agent-guide` and can ask Ghostex Help with `ghostex help ask`. Turn it off in Settings > Chat > Tell agents about Ghostex.
- **Agents ask which browser you prefer for previews** once, Ghostex's own browser or your usual one, and remember the answer.
- **Agents can ask you for a password, key or token privately.** A private card on the computer and the phone takes it, and it never goes into the chat. Settings > Secrets adds 1Password accounts to read keys from.
- **New accounts show in the sidebar,** and the accounts area starts pinned until you unpin it.

### 💬 Chat

- **Chat cards say what happened in plain words,** with the agent's raw lines behind Details, retry buttons on their own row, and accounts named by email unless you hide it.
- **The API error card says Ghostex keeps retrying** only while another try is coming.
- **Question cards and pages use a quiet Recommended mark,** and chat pages follow the window's wallpaper tint live with a darker dark mode.
- **A mentioned session shows its full title** in the chat box.
- **Search by Prompt's toolbar fits its window,** switching to icons or a … menu so the search field is never cut off.

### 📱 On the phone

- **Reply quotes the words you selected** when you select part of a message and pick Reply.
- **Android: Scan code reads the computer's pairing QR again,** with a coarser QR phones read easily from a screen, thanks to @knigaperevodchik (#394).
- **Android: the terminal no longer crashes** when its history shrinks while you're scrolled up (#398).
- **The Saved prompts sheet loads once** instead of flashing while the agent works, and the paste sheet points at Copy as text under the QR.

### 🩹 Fixes and polish

- **Ghostex deletes its own old files once a day:** old screenshots, logs, crash reports, backups, published pages, unread Codex copies and long-closed sessions, keeping pinned and favorite sessions and anything you pasted. Diagnostic log areas stop after 3 days at most.
- **The Environment card in Session details shows the session's RAM.**
- **Dialogs inside Settings use your theme's raised surface** instead of black.
- **macOS: Caps Lock types capitals in terminals.**
- **Windows: Antigravity's background checks no longer pop up a terminal window,** thanks to @DaveW001 (#397).
- **A forked session stays in Previous Sessions** when it kept working after the fork.

## 10.43.0 - 2026-11-02

**Ghostex 10.43.0 is out.** Side chats, thumbnails of the pictures an agent opens, one view choice of Summary, Normal or Detailed, an OpenCode tab in every model picker with a see-through OpenCode theme, zoom for files, and a calmer chat while agents stream.

### 💬 Chat

- **Side chats fork the session and open beside it.** Side chat in the chat box (or `/btw <question>`) starts a side conversation that knows the chat so far, runs in the same folder in ask-before-edits, and opens as a side panel tab; the main chat's Side chats pill lists them, with Bring back and Delete.
- **Pictures an agent opens show as thumbnails.** When Claude, Codex, OpenCode or Gemini reads an image from disk, its row shows the picture, and a folded turn keeps its pictures inside its card in one row that scrolls sideways, with an "N images" count. Click one to open it large. A picture you already pasted isn't shown twice.
- **The chat's view is one of Summary, Normal or Detailed.** Simple is now called Normal, Verbose is gone (chats on it open Detailed), turning Summary off always goes back to Normal, and a "Summary view" or "Detailed view" chip at the top of the chat box has an X back to Normal. Default to Detailed view is in Settings > Advanced.
- **While an agent works, its steps fold like Claude's.** A running thought folds the calls it made behind "N tool calls", for every agent, and OpenCode's and Codex's many short thoughts join into one Thought row.
- **The chat holds still while an agent streams.** A form field you're typing in no longer slides under the caret, and a chat you scrolled up stays where you left it, thanks to @suatkocar.
- **Answered forms stay answered,** thanks to @suatkocar: an old answer no longer comes back as "Sending to the agent…", the ? pill counts only questions really waiting, Go to chat reaches an early form in a long turn, and a queued form reads Queued.
- **A Stashed pill above the chat box** shows how many prompts this chat has stashed; click it to open just those.
- **The scroll marks on the chat's right edge** show only messages you typed, not form answers or messages from other agents.
- **The new chat's project and worktree bar** hides as soon as you send the first message.
- **Message from and Message to cards** lose their Go to session button: click the session's name instead, which now works for every card.
- **The question popup** takes the chat's colours and fits the whole form.
- **The menus above the chat box** (Resources, Workers, Links and the others) use the same lighter frosted glass as the sidebar's menus.
- **The voice agent panel is frosted glass** like the Notifications menu, with its controls in the header and your conversation on the same glass.
- **Slash commands look like the rest of the chat:** a command such as `/rename` shows as your own message, and its output as a quiet note under it, for every agent.
- **Paste pictures into question notes,** in chat forms and question pages; they show as Image pills and reach the agent with your answers.
- **Question pages get a softer look:** frosted glass cards over your own wallpaper or custom picture on macOS (a soft dusk or dawn elsewhere), and options side by side with a Pick button.
- **Page cards show a preview before you open them,** and the preview shows the page's real top.

### 🤖 Agents

- **Every model picker has an OpenCode tab** with the models your OpenCode can use; picking one in a new chat hands it to OpenCode, thanks to @DaveW001 (#390).
- **OpenCode gets a see-through Ghostex theme,** so the window shows behind it like Claude Code and Codex. It's used only when you haven't picked an OpenCode theme yourself.
- **OpenCode's own session name shows in the sidebar,** and a name you gave it in Ghostex wins, thanks to @DaveW001 (#392).
- **The model picker gets a search field** when an agent has more than 6 models, and starred models sit on top in alphabetical order.
- **Install hooks repairs a damaged OpenCode settings file** and says why an install failed: the other computer's error, a timeout, an older Ghostex there, or the agent that didn't install.
- **Coordinator threads can start on OMP, Copilot, Grok Build, Hermes, Cursor and Gemini** with a chosen model, and effort where the agent has it.
- **Codex's footer is read when it says "Fast"** with a capital F, so its model and effort no longer go missing, thanks to @suatkocar.

### 🧭 Coordinators

- **Handing off a coordinator keeps it a coordinator,** with its Goal and Standing instructions, locally and to another computer, thanks to @diocata (#383).
- **A coordinator hears about a thread stuck on a permission prompt** without anyone reading its screen.
- **The New Thread picker has a Coordinator row.**
- **A handed-off coordinator keeps the title Coordinator** instead of being named after its setup line.

### 📂 Sidebar and files

- **New and never-sent sessions go to Drafts,** which opens and selects the new row when one arrives.
- **Notifications keep a waiting permission request unread and red** until it's answered, and the bell turns red while one waits.
- **The text of an open file zooms** with Cmd/Ctrl+=, - and 0 like the chat, with a Default files zoom setting, thanks to @DaveW001 (#388).
- **The Add a comment box in Files opens beside the text** instead of covering it.
- **Filter dropdown rows have a ⋯ button and a right-click menu,** and each row is only as tall as its content.
- **The Rename dialog shows Generate** only for long or empty names.
- **Settings > Notifications has a Finished-work border switch and colour,** off by default.

### 🪟 Windows

- **Windows: transparency changes apply right away** to every open window, without restarting Ghostex.
- **Windows: `file:///C:/…` and `/C:/…` chat links open the file** instead of copying its path, and documents and other files can open in the system's default app, thanks to @DaveW001 (#391).

### 📱 On the phone

- **The phone picks its own sessions list layout** in Settings > Sessions list; Unified lists every computer's sessions together without the computer tabs.
- **The Android notification** leaves out coordinators' threads and side chats, and its status bar icon is the seven-petal flower.
- **Reconnecting shows a plain spinner** and keeps its gap above the working bar.

### 🩹 Fixes and polish

- **Switching accounts brings the Claude conversation along,** completes on an account at its limit, and no longer shows the previous login's sign-in error.
- **A signed-out Claude account is restored** from this computer's newer copy of its login.
- **Sessions start while the Mac is logged out,** and Ghostex recovers by itself after you log back in.
- **A command's auto-detected title goes back** when the terminal returns to its prompt, thanks to @Bet4 (#379).
- **Work mode keeps a workspace's ticket command and its masked credentials** (not switched on yet), thanks to @lardissone (#378, #384).
- **The onboarding screens show the seven-petal logo,** and the Claude extension's description names the sidebar's top bar.

## 10.42.1 - 2026-11-01

**Ghostex 10.42.1 is out.** Larger pasted images work in chat, and answers from forms and pages reach a busy agent sooner.

### 💬 Chat

- **Paste images up to 32 MiB on the computer and web.** BMP and TIFF images are sent as PNG, very large pictures are reduced when needed, and the chat box shows the image size and limit if one still cannot fit. OpenCode reduces large images further when sending them.
- **Answers from question forms and chat pages send like normal messages,** reaching a busy agent after its current tool call when supported, on the computer and phone. Turn on the Advanced setting **Wait for turn end before sending form answers** to keep the previous wait.

## 10.42.0 - 2026-11-01

**Ghostex 10.42.0 is out.** The Unified sidebar is easier to filter and organize, chats show failed sends where they happened, sessions have shareable links, and the new Interface page lets you preview and apply your fonts. Update Ghostex on remote computers too so session links and handoff work across them.

### 🧭 Sidebar and sessions

- **The Unified sidebar has a filter bar with chips** for projects, machines, Spaces and session search. Search in the dropdown for a project or session, remove one chip at a time, or open the count for chips that do not fit.
- **Remove a project from the Unified list with its row's X,** then reopen it from Recent Projects when you need it. Its sessions keep running while the project is out of sight.
- **The Unified sidebar scales to more sessions:** group sessions by project, arrange project groups, remember filters and folded shelves, and use its own hover actions. Machine tabs scroll when many computers are connected.
- **Copy Link gives a session its own shareable link.** Opening one focuses the session on this computer or a connected machine, asks before starting its agent, and can offer to restore a closed session. Windows and Linux open `ghostex://` links too.
- **`ghostex://terminal` links are removed** because they ran commands with no prompt.
- **Handoff / Export has a simpler dialog** for sending a conversation to an agent in this project, another project or another computer, or saving it as Markdown. Drag a session to a Project sidebar project to hand it off there.

### 💬 Chat and worktrees

- **Failed sends stay in the thread with the real reason, Retry and Fix it.** The session gets a red dot and a Not sent notification until you deal with it; a mode that the session cannot use is greyed out with instructions.
- **A queued message or answer that stops progressing says what is known and can be removed,** instead of spinning or claiming it is still waiting.
- **The new chat's worktree tray can use a new or existing worktree,** creating a new branch when you send the first message and remembering your choice per project.
- **The `$` picker includes enabled Claude Code plugin skills and commands** under their `plugin:name` names, thanks to @roypadina.
- **Chat Resources covers the whole conversation,** and forms and charts open in a floating view where you can read or answer them. The Resources list calls coordinator pages Worker pages.
- **Accounts & limits marks the active account and shows a problem on its row,** with Reconnect when it helps. The Usage panel has a clearer layout and shows when its limits were updated.

### 🎨 Interface and integrations

- **Settings > Interface puts App font, Monospace font, Text strength and Default chat zoom together.** Pick a font, see it in the live preview, then press Apply. The monospace choice reaches terminals, code, diffs and chat code blocks; sidebar icons follow Sidebar zoom.
- **Colourfulness reaches Full,** so a dark sidebar and work area can take the theme colour you picked.
- **Settings > Integrations groups each computer use, hidden screen and embedded browser tool with its skill and permissions,** so their install and setup controls stay together.
- **A coordinator can use Low effort** when its agent offers it. The start level stays High.
- **Kanban cards show Blocked by and Blocks,** including links added by an agent, and refuse a dependency loop.
- **Linear pull request links are accepted only when they are GitHub pull request links.**
- **Work tickets explain invalid icons and unfinished answers more clearly,** thanks to @lardissone.

### 📱 On the phone

- **Chat shows failed sends and unavailable modes like the computer,** and its Theme page is now Interface.
- **Android terminals allocate scrollback as it is used,** reducing memory pressure when several terminals stay warm.
- **iOS scrollback uses the selected row limit correctly,** so a terminal can retain the history you chose.

### 🩹 Fixes and polish

- **Linux AppImages start on a clean offline computer,** and the packages support Ubuntu 22.04. When the embedded browser cannot download while offline, its window explains that and retries.
- **Windows explains when its session host is missing or blocked,** thanks to @ilmimris.
- **Remote connections wait through slow responses and repair a missing or broken session runtime** when they reconnect.
- **Ghostex keeps settings and saved remote machines after a crash or power loss.**
- **A Claude session in Bypass Permissions mode continues past its startup warning,** and the chat's Alt+S shortcut stashes a prompt again.
- **Browser tools stay on pages in their agent's project,** and Kanban blockers and browser tabs receive clearer help.

## 10.41.0 - 2026-10-30

**Ghostex 10.41.0 is out.** A Unified sidebar lists every session from every computer, Space and project in one list, the agent button remembers its pick per project, the voice agent can stop sessions and start coordinators, Dictate says why it failed and can use an OpenAI key, and queued messages that got stuck now unstick themselves. Update Ghostex on your remote computers too: several fixes live in the background service.

### 🧭 Unified sidebar

- **A new Unified sidebar (Settings > Sidebar & Sessions > Sidebar) lists every session from every computer, Space and project in one list.** Today's sidebar stays the default, now called Project sidebar.
- **Each session is a card:** the project's icon and name (with the computer's name when it runs elsewhere), its status with a live timer (Working, Waiting, Question, Approval, Failed, Done), its title, and its branch, pull request and agent.
- **Sessions are ordered by when they last needed you,** never by output while an agent works; Drafts (sessions you haven't sent anything to) sit at the top, then pinned ones.
- **Working, Snoozed and Parked fold into shelves at the bottom,** with a Working section switch of its own, and hovering a card offers Snooze and Park.
- **The filter narrows the list to computers, Spaces or projects,** and the same repository on two computers counts as one project, named after the repository. Picking a project in Quick Access > Projects filters to it.
- **The header's New session button opens the agent picker,** and right-clicking any card opens its session menu.

### 💬 Chat

- **Reply:** the transcript's selection menu has Reply, which opens a small box to type a reply; Add to prompt puts the quote and your reply at the end of the chat box, and Send reply (Cmd+Enter) also sends it. On the computer and the phone.
- **Chat Resources lists unanswered questions oldest first,** so they read in the order they were asked.
- **Hovering a For later question offers Back to open and Remove without answering;** a removed question reads Canceled, opens to read and takes no answer.
- **A queued message stuck behind an input box Ghostex can't read now unsticks itself:** fresh reads, a redraw, a small resize, then the normal send path or Not sent with Retry, and every waiting row says why after a minute.
- **Answers, queued messages, Send now and Retry wake a sleeping session** and arrive once it is up.
- **Pages no longer show a note box under each option;** Pick and each question's own note stay.
- **The View menu is a Simple, Detailed or Verbose choice plus Summary,** kept per chat.

### 🎤 Voice and Dictate

- **Tell the voice agent to stop a session and it stops that session's turn right away,** like the Stop button; it asks first only when it isn't sure which session you mean.
- **"Start a coordinator in Ghostex to …" starts one right away,** with what you said as its first message.
- **When Dictate fails, a two-line card says why** (no key, key refused, offline, rate limit, out of credit, bad audio) with Add key or Check key opening the right Settings field.
- **Dictate uses your OpenAI key when no xAI key is saved,** and Settings > Extensions > Voice agent shows both keys even while the voice agent is off.

### 🤖 Agents and coordinators

- **The agent button remembers its pick per project:** Settings > Agents > Remember the agent you pick switches between Per project, Per Space and Everywhere, and the header button, Cmd+Shift+O, the command palette and the New Thread picker follow it.
- **The coordinator effort picker offers Medium, High and XHigh,** and a new coordinator starts on High.
- **The New Thread picker is wider so its key hints fit on one line,** its scrollbar shows only while scrolling, and Claude (cloud) gets an orange cloud icon.

### ⚙️ Settings

- **Session notes is a built-in extension (Settings > Extensions), off by default.** Turn it on to get notes back in menus, the chat box and the hotkey; notes you already wrote are kept.
- **Every Settings page ends with a folded Advanced section** instead of a Show Advanced switch.
- **Menus, popovers, small dialogs and tooltips take a darker frosted glass on macOS,** and the Changes tab's Commit and agent menus are solid so the diff never shows through.

### 📱 On the phone

- **The phone's layout gets iPhone Duo support work:** the terminal slides in, the tab strip peeks, and the keyboard moves smoothly on every phone, thanks to @cagatay-io.
- **The phone remembers its agent pick per project,** following the computer's setting, and hides Session notes while that extension is off.

### 🩹 Fixes and polish

- **Quick Access > Sessions shows your closed sessions again on macOS** (it was empty after ZCode quit), and restoring one keeps it open so you can bring back several in a row.
- **History from Cursor, ZCode, Hermes, OpenCode and other agents reads again on macOS** after that agent quits, without ever writing to it.
- **A `claude -p` or Agent SDK run started by another program from a Ghostex terminal no longer takes over that terminal's session.**
- **Answering a Claude question no longer fails with "where its highlight sits could not be read"** (#350); if it still can't place the answer, it sends it as a message, and a real refusal offers Open terminal.
- **Windows: a timed-out git or gh command ends everything it started,** not only the command (#351), thanks to @lardissone.
- **A successful git fetch for worktrees and pull requests no longer reads as failed** when a leftover process holds its output (#352), thanks to @lardissone.

## 10.40.0 - 2026-10-29

**Ghostex 10.40.0 is out.** Settings split General into six pages under App, Setup and More, the chat's pills move into a strip along the chat box, full-window Claude takes over the whole window, dialogs and menus turn to frosted glass on macOS, and tool call cards arrive as a preview. Update Ghostex on your remote computers too: the Work page's tickets changed in the background service.

### ⚙️ Settings

- **General is split into six pages:** Chat, Sidebar & Sessions, Terminal, Notifications, Files & Links, and Power & Sleep, and the Settings sidebar lists every page under App, Setup and More. Sleeping Sessions moved to Sidebar & Sessions, and Experimental and Reset to defaults moved to About.
- **Show Advanced never hides a page:** Workspaces, Cloud Boxes, Open In, OS Integration and Debugging always stay in the Settings sidebar, and Show Advanced only shows or hides rows inside pages.
- **Settings > Integrations has a Web runtime row** for the Chromium runtime with its status, Install, Reinstall and Uninstall; the Extensions store no longer lists it.
- **Session status is always Combined:** the Session status from setting is gone, and every session uses Ghostex's own detection plus what the program reports about itself.
- **A Session tooltips switch** (Sidebar & Sessions > Session Cards, on by default) turns off the hover tooltip on sidebar session rows.

### 💬 Chat

- **The pills move into a soft strip along the top of the chat box,** the bar above it keeps only the working line, and each pill's menu opens right above the pill you clicked.
- **Tool call cards (preview, Settings > Chat, off by default):** one card per burst of tool calls with stacked icons, results and times, failures that say why and app logos for MCP tools; they fold once done and show on the phone too. A Codex command that failed inside a script shows its exit code.
- **Chat Resources has two groups,** Unanswered (For later included) and Earlier (answered questions, pages, charts and withdrawn ones, told apart by icon and colour), and long chats load it a page at a time.
- **The ? pill steps only through questions still waiting** that aren't For later; with none waiting, a click opens Chat Resources.
- **The Tasks pill and the Session details Tasks card show every open task,** the 4 newest done ones, and Show more for the rest.
- **Links read #270+** in the chat's Links pill and the sidebar badge (no repo name, no count), in the header's muted colours, and the Links pill comes last.
- **Session details' folds ease open and shut,** and its account row shows the email alone.
- **Message from and to cards** read the session name as part of the title and show the agent's icon once.
- **The chat's More menu has Session note,** and the terminal view's buttons follow the chat's order, with Session note, Stash prompt and Saved prompts in More.
- **A folded approval card shows the row Cmd+Enter answers and the No that Escape answers.**
- **The working bar drops the elapsed time,** and Claude Code's own update notices no longer reach the chat.

### 🧭 Coordinators

- **Thread reports arrive on their own:** a report held for an idle Claude coordinator whose input box can't be read gets its screen redrawn, so it no longer waits until someone opens the coordinator. Copy Report masks session refs, names, paths, emails and ids.
- **The sidebar row reads at a glance:** the links badge is a plain #348 before the workers count, and the crew icon shows only while a thread works, waits on you or waits for approval.
- **The Workers menu header reads "2 active · 429 total".**

### 🖥️ Full-window Claude

- **Turning Claude on fills the window:** it hides the sidebar, side panel and Commands pane, the header keeps the sidebar's top-row buttons, and turning it off restores the layout.
- **Opening any session turns Claude off,** and showing the sidebar returns to the Space, project, session and layout you came from.
- **The Claude button moves to the sidebar's top row** in its grey; the Issues briefcase leaves that row (Issues stays in Open a view, its hotkey and the session chips).

### 🪟 Frosted glass and tooltips

- **macOS, with transparency on:** Settings, Search by Prompt, Agents Hub, Quick Access, Browser History and the New Thread picker get a darker frosted fill, and Send Bug / Feedback and 20 other dialogs, the Notifications, Tips & Tricks, Resources and Dev servers dropdowns, and the terminal bar's ⋯ menu and flyouts are frosted too, with a quarter more blur and a 45% dark tint.
- **Every plain tooltip uses the same roomy look,** centred with a gap, with shortcuts as key caps.
- **The sidebar's icons and machine tab names take the header's colours,** and a machine tab shows a tooltip only when it has an error or a waiting update.
- **The Files sidebar drops the Project Docs label** above the tree.

### 📱 On the phone

- **Chat improvements reach phones over the air** without a new app install, starting with this release.
- **Tool call cards, the plain links badge and the quieter crew icon** match the computer.

### 🩹 Fixes and polish

- **Antigravity:** messages no longer fail with "The terminal draft could not be cleared" while its Accept edits, Auto, Plan or Best-of-N hint shows in the input box (#349).
- **Adding a second Claude account no longer says "Login did not complete"** after it was saved, and a real failure shows its reason (#347).
- **Windows: Ghostex no longer closes when a monitor is plugged in or unplugged** (#346), thanks to @Dev-Mriganka.
- **Agents installed through mise start without any shell setup;** Install fetches mise itself and Node.js when npm is missing, and a missing agent command offers Install on its card.
- **Work:** Linear and GitHub tickets share one ticket shape, the groundwork for more ticket sources, and GitHub issue details keep their assignee and labels, thanks to @lardissone.
- **Remote:** clicking into a remote session clears its blue dot on its own machine too.
- **Linux and Windows:** a click into an embedded web page closes an open sidebar menu or titlebar dropdown.
- **Page text boxes wrap long lines** instead of widening the page.

## 10.39.0 - 2026-10-28

**Ghostex 10.39.0 is out.** Linux works like Windows from the first run on Omarchy and other Hyprland desktops, and the AppImage updates itself. Agents show as pills you can click and mention with +, DeepSeek Harness joins the agents, Orchestrators are Coordinators again, the bar above the composer gets two rows, a Resources pill and a Links pill, and GPT 6.1 Terra joins Codex.

### 🐧 Linux

- **The AppImage updates itself like Windows:** Download update fetches the new version, checks its signature against the Ghostex key built into the app, and Restart and update swaps it in and relaunches, going back to the old version if anything fails. deb, rpm, tarball and AUR installs keep the update notice.
- **Omarchy and Hyprland treat Ghostex's small windows as floating:** the setup window, popups, menus, tooltips, the Running Agents panel and Docs note windows no longer tile, and a dialog's close button, toast buttons and chat suggestions can be clicked.
- **Omarchy's Super+C and Super+V copy and paste in Ghostex** without stopping a running command; Ctrl+Insert and Shift+Insert also copy and paste in text fields on Windows and Linux.
- **No light flash at launch on a dark desktop,** and the setup's System appearance follows the desktop.
- **Links open in Wayland-only browsers:** the intro video, Settings links, Open In, Reveal, the update dialog and sign-ins inside agent sessions no longer do nothing on Omarchy.
- **Dropping a file onto a chat or the terminal works on Hyprland,** from X11 and Wayland file managers.
- **Clicking a notification opens its session or browser tab,** and browser page notifications show once.
- **Middle-click pastes the selection in the terminal,** as in other Linux terminals.
- **Computer Use can see the screen on Hyprland and Sway.**
- **When the background service can't start, the sidebar says why within seconds** (a taken port names the program holding it) with Try again, on Linux and macOS.
- **First run on Linux:** a folder chooser when the system has none, the AppImage adds Ghostex to the app launcher, and the ghostex command goes to ~/.local/bin; Settings > Integrations says why the command isn't usable and gives the PATH line to copy.
- **Looking for agents never installs them:** Omarchy's install-on-first-run commands count as not installed until you start that agent.

### 🤖 Agents

- **Agents show as pills:** a mention of another session, and every message between agents, shows a +Session pill with the agent's logo, and a click switches to that session.
- **Type + in the chat box to mention another agent in the project;** Copy ID copies a session's mention to paste into another chat, and Copy Details tells the agent to run `ghostex agents`.
- **DeepSeek Harness is a supported agent** (under More agents): Chat View, install and update from Settings > Agents through mise, and its models and reasoning levels in the model menu and the chat's model pill.
- **Orchestrators are called Coordinators again,** and `ghostex coordinator` is the command (`ghostex orchestrator` keeps working). The chat's Threads pill is now Workers.
- **GPT 6.1 Terra joins the Codex models,** and Cursor adds DeepSeek V4.1 Flash 1017 with its current ids and effort levels. A small ⓘ beside Codex's Fast notes that Codex keeps Fast for new sessions too.

### 💬 Chat

- **The bar above the composer takes two rows when the working text doesn't fit:** the current action on top, the pills below, with a short animation, and it doesn't jump back and forth during a turn. Pills that don't fit scroll with a soft fade at the edge.
- **A Links pill in that bar replaces the links in the chat's top bar,** with one link's ID and +N (`SPX-123 +2`); the sidebar's links badge shows the same, and a pinned link becomes the badge's ID.
- **Suggested links open unfolded when a session has 2 or fewer links.**
- **A Resources pill opens Chat Resources,** and the ? pill jumps from question to question. Rows show how long ago they were asked, without question numbers, and a button puts a question off For later or brings it back.
- **The Accounts & limits panel is about half as tall:** each account's 5h, 7d and model limits sit in one row, with reset times in their tooltips.
- **@ shows a file pill only for a file or folder that exists in the project,** so npm packages and handles stay plain text.
- **Pages agents show in the chat fit a phone:** columns stack, small pills stay on one line, and wide mockups scroll inside their own box.
- **The Running Agents panel opens with its top 3 projects expanded,** and with every session-card hover button turned off, hovering a sidebar row no longer changes it.

### 🔌 Remote computers

- **A machine that points back at this computer says so** and never installs or updates a background service there.
- **Editing a machine reconnects at once,** the Settings tile shows why a machine can't connect, ~/ paths work when adding a remote project, and removing a machine closes its connection.

### 🩹 Fixes and polish

- **Answering Claude's question and approval cards works with Claude Code 2.1.311's files panel open,** No clears the approval, and a refused tool call ends the turn instead of leaving the session working.
- **Picking GPT 6.1 Terra works,** new Codex sessions show their model before the first message, and a stale red Copy Report clears once an approval closes.
- **On the phone, Send stays above the keyboard** however long the message is.
- **Files no longer gets stuck on a file that was deleted.**

## 10.38.1 - 2026-10-27

**Ghostex 10.38.1 is out.** On iPhone, Scan code reads a computer's pairing QR up close, and removing a computer on the phone takes one tap.

### 📱 On the phone

- **Scan code on iPhone opens Apple's own scanner,** which switches to the macro lens for a QR held close, so a computer's pairing code scans as easily as with the Camera app.
- **Removing a computer takes one tap to confirm** instead of typing its name.

## 10.38.0 - 2026-10-27

**Ghostex 10.38.0 is out.** Codex sessions get a Mode button, the chat's ? pill becomes Chat Resources with every page, form and chart, session links stop picking up unrelated tickets and take links you add by hand, approval cards show one hotkey per row, Claude cloud sessions open as a chat, and Linux gets signed, self-updating AppImages and a working mouse wheel in the Browser.

### 💬 Chat

- **Codex sessions get a Mode button like Claude's** that switches the running session between Ask for approval, Approve for me, Full Access and Read Only, on the computer and the phone.
- **The ? pill opens Chat Resources:** every page, question form and chart in the chat, with a dropdown for All, Html, Q&A or Charts, and a row jumps to its card. A form or page can be put off For later, so it stops counting as unanswered.
- **Summary mode keeps messages with pages, forms and charts open,** and folds other agents' messages and thread reports into Agent reply.
- **Approval and dialog cards show each row's number as the only hotkey hint,** without the extra buttons under the rows, and Show terminal output opens the lines above the prompt on the card itself. On the phone, a dialog with no No row keeps Cancel.
- **A tool call reads as done only once it ran:** one waiting for approval, declined or failed reads "Create", "Run" or "Edit".
- **A code block's Wrap, Run and Copy buttons are icon-only, with tooltips.**
- **Copy Report states the same input-box verdict as the Terminal View light above it.**

### 🔗 Session links

- **Only your own words link a Linear ticket:** IDs in thread reports, other agents' messages, tool output or the agent's replies no longer link unrelated tickets. Ones the session's agent mentions show under Suggested, and an orchestrator's own replies suggest nothing.
- **A suggestion's × dismisses it for good, and + adds a PR, issue or Linear ticket by hand:** type `SPX-123`, `#123` or paste a link.
- **Suggested shows the 5 newest, with Show all for the rest.**
- **Remote sessions show their links in the sidebar card and the top bar.** The sidebar shows a badge by default, on the title's line and gray once every link is merged, and Settings > General > Chat > Show session links in the chat's top bar can hide them.

### ☁️ Claude cloud

- **A Claude cloud session opens as an empty chat:** pick its account, send the first message, and its claude.ai page appears in the chat pane. A Cloud button in the header switches to the terminal, opens or copies the claude.ai link, and starts a copy on another account.

### 🧭 Views and pages

- **Changes: one button on the right of the header picks Whole branch or Uncommitted only,** and picking a file unfolds it and centres it.
- **Settings > Extensions opens the extension catalog from the sidebar,** its pages fold under it, and About moves up with the top rows.
- **Terminal extensions like Lazygit are easy to open:** an Open row in Quick Access, pinned on install, and Install with Homebrew when the command is missing.
- **Inline HTML pages get a centred design that works in dark and light,** with a Dark / Light switch, and their tooltips look like the app's, with your platform's shortcut keys.

### 🐧 Linux

- **The mouse wheel scrolls Browser pages under xfwm4 and other window managers that grab mouse buttons,** and touchpads stay pixel-precise (#338).
- **A leftover libcef.so beside the app no longer crashes Ghostex at launch:** it's skipped for the downloaded CEF, and a mismatched CEF shows an error instead (#339).
- **AppImages are signed and carry update information,** so AppImage managers such as AppManager show the update link and web page and can update Ghostex. ghostex.dev's Linux downloads add Install with AppManager.

### 🌐 Remote computers

- **Open Folder on a remote project opens it in Files,** and Open In opens VS Code-family editors and Zed over SSH.

### 📱 On the phone

- **The Sessions screen is drawn like the desktop sidebar,** with Voice, Search, Web and Machines in a bottom bar.

### 🩹 Fixes and polish

- **A tool whose version check never answers no longer stalls Settings > Integrations:** it's stopped after 5 seconds and the rest of the list shows (#336).
- **Restoring a Space or workspace keeps sleeping sessions asleep,** thanks to @suatkocar.
- **Anonymous usage stats (Settings' existing opt-out) now include which built-in and official extensions are on and which views are opened.**

## 10.37.0 - 2026-10-27

**Ghostex 10.37.0 is out.** Sessions stuck on a permission prompt now show red everywhere, Settings > Hotkeys gets a Terminal focused section, Linux gets an AppImage, remote computers get a Machine picker for agent CLIs and integrations, Claude and Codex dialogs can be answered from the chat again, and phone pairing works for phones that couldn't scan.

### 🔴 Waiting on you

- **A session stuck on a permission prompt shows a red dot,** on its row, its coordinator's crew badge, the Threads pill and the project, section, Space and computer headers, on the computer and the phone. Notifications say "Waiting for your approval", and `ghostex sessions` prints "waiting for approval".
- **A coordinator's crew badge reads red, then light blue, then orange,** so a thread waiting on you shows ahead of working ones.
- **The Threads pill shows one corner dot per state:** amber while threads run, red or pink when one waits on you, overlapping when both.

### ⌨️ Hotkeys

- **Settings > Hotkeys has a Terminal focused section,** so a key can do one thing in a terminal and another everywhere else, and the same key can be set in both. Keys you changed keep working in a terminal until you set its terminal row.
- **Windows and Linux: plain Ctrl keys reach the program in a terminal.** In a terminal Ghostex uses Ghostty's keys (Ctrl+Shift+V pastes, Ctrl+Shift+C copies); outside a terminal your keys work as before.
- **Unfocus Terminal and Focus Last Terminal** (Cmd+Shift+M, Ctrl+Shift+M on Windows and Linux) take the keyboard out of a terminal and back again.
- **Terminal Copy and Select All, Browser History, Reload Page and Sleep's second key can be changed in Settings,** and `ghostex settings hotkeys` takes `--terminal`.
- **Export Transcript has no default key,** which ends its clash with Expand View Panel on Windows and Linux. Set one in Settings > Hotkeys (Handoff / Export).

### 💬 Chat

- **Codex's questions can be answered from the chat again** with Codex 0.161 and later, Skip included, and the chat no longer locks while Codex's question editor is open. Codex's "conversation open elsewhere" screen offers Continue here again.
- **Claude's Ready to code card shows the whole plan and every choice,** and the "We recommend compacting earlier" dialog can be answered from the chat, with Smaller and Larger for its size.
- **The sign-in error card gets Reconnect,** and an account whose login expired shows Needs sign-in.
- **Two one-line cards in a row are always the same distance apart,** on the computer and the phone.

### 🤖 Claude

- **Claude cloud sessions open in the chat view:** type the first message, then its claude.ai page appears in the chat pane. Their state and replies come from claude.ai. On the phone, a tap opens claude.ai and a long press offers Open Terminal.
- **Claude is a built-in extension, off by default,** that opens claude.ai over the work area from a sidebar button, with a bar to switch between your saved Claude accounts' sign-ins. Turn it on in Settings > Extensions > Store > Features.

### 🌐 Remote computers

- **Settings > Agents and Integrations get a Machine picker** that installs and updates agent CLIs, skills and tools on another computer, including Fast Computer & Browser Use and SpaceO on a remote Mac. The phone shows each computer's agent CLIs and skills on its Edit screen.

### 🪟 Windows

- **gxserver no longer quits every few seconds** after updating to 10.36.0 (#312), thanks to @DaveW001 for finding the cause.
- **Hooks and skills link without Developer Mode:** a refused link becomes a junction or a copy, and a failed link puts the agent's own folder back. Settings > Agents and Integrations recommend Developer Mode for a Windows machine.
- **A new browser tab with glass no longer shows a stale frame** while its page loads.

### 🐧 Linux

- **Linux releases include an AppImage** for x64 and ARM64: download it, make it executable and run it.

### 📱 On the phone

- **Pairing works when the phone can't scan:** Copy as text sits under Show QR on the computer, and the QR is larger, black on white and easier to scan. The phone reads every code a recent computer shows, and says when a code needs a newer app.
- **Settings no longer crashes on iOS** when no update is pending, and a broken Settings section shows a Try Again card instead of closing the app.

### 🧭 Views and pages

- **Keep views loaded** (Settings > General, off by default) keeps side panel views loaded across project and Space switches until their Auto Sleep time, thanks to @suatkocar.
- **Closing or hiding a side panel tab selects the tab next to it,** thanks to @suatkocar.
- **A page's notification brings back the tab that posted it,** in its own project, thanks to @suatkocar.
- **Code view: arrow, Home/End and Page keys no longer type control characters into files** on macOS (#326).
- **Files: Annotate's copied feedback names the file's project path** instead of "srcdoc" (#329).

### 🩹 Fixes and polish

- **Ghostex no longer leaves the programs it starts behind as zombie processes** (the completion sound, Open In, editors), which piled up over a long day (#334), thanks to @suatkocar.
- **Codex sessions start when Superset is installed;** the two tools' codex wrappers no longer call each other (#325).
- **The OpenAI voice agent starts with many projects,** and voice session errors reach `ghostex logs` (#331).
- **`ghostex browser mcp` answers Claude Code and Codex.**
- **Feedback's Start Agent says plainly why an agent couldn't start.**
- **Terminals get Ghostty's latest fixes,** including a fix to its zsh integration, and Ghostex is drawn through Zed's newest Metal renderer.
- **Background pages get the Browser panel's size** and close with their project.

## 10.36.0 - 2026-10-26

**Ghostex 10.36.0 is out.** Ghostex moves to the newest GPUI, Ghostty, Chromium and VS Code, uses far less CPU while accessibility tools run, gets open files, VS Code windows and Browser pages as tabs, background pages for agents in other projects, a Session details panel for every chat agent, separate accounts for people who share a Codex Business workspace, and a round of fixes for remote computers, Linux installs and the phone.

### ⚡ Faster and newer

- **Ghostex is drawn by the newest GPUI (Zed's latest, GPUI Fast and GPUI Kit 0.8),** so idle CPU drops from about 26% to about 4% while accessibility tools are running.
- **Lists and menus scroll with macOS-style bounce.**
- **Terminals run on Ghostty's late-October code and the newest zmx,** and sessions that are already running keep going through the update.
- **The built-in browser runs Chromium 156,** October's security update. After updating, Ghostex downloads its Chromium runtime once more the first time a browser view opens.
- **The VS Code view moves to VS Code 1.143.1.**
- **Rendering and window errors are written to Ghostex's log,** so a problem like a window that stops updating can be diagnosed.

### 💬 Chat

- **Every chat agent gets a Session details panel** with Environment, Context, Skills and Changes, Codex and OpenCode show every card their data supports, and empty cards are hidden.
- **Each chat remembers whether its Session details panel is open,** and switching chats no longer turns the floating card off for every chat.
- **The Prompt Editor shortcut (Ctrl+G) in a chat box opens the chat box's maximized editor** and restores it on the next press; the terminal view keeps opening the Prompt Editor.
- **Comments and notes sent to a chat box show at once and add to what's there** instead of replacing earlier pills or typed text.
- **Add to Session Context adds the file to the chat box** when the session is shown as a chat.
- **A failed command row no longer shows "Exit code 1" after its title;** the red title and cross already say it failed, and the full output stays in the expanded row.
- **Ghostex dismisses Claude's self-drafted feedback card, the chat gets Copy Report,** and a Claude panel's selected tab is readable.
- **Claude's context breakdown waits for the conversation's first message** instead of failing, and a real failure shows Claude's own message.

### 📂 Files and VS Code

- **Files can show open files as tabs above the document** (Settings, off by default), with keyboard switching on the tab row, thanks to @suatkocar.
- **VS Code windows can show as tabs above the VS Code page** (advanced setting, off by default), thanks to @suatkocar.
- **Web links from the VS Code view open where Open links in says,** and on the project's remote computer for remote projects.
- **Cmd+Shift+F opens Search in Files anywhere in the Files view,** including the file list and its filter field, and stays Find Prompts everywhere else.
- **Tab tooltips show below their tab** instead of covering the tab beside it.
- **The VS Code view's terminals get their fonts without a permission bar;** outside sites still ask.
- **`ghostex open` follows your File opening setting for every file type,** and `ghostex edit` on a file outside your projects opens it in the project on screen instead of adding a project.

### 🧭 Sessions and views

- **Session groups from the CLI,** plus `create-session --group-id` and `--cwd`, thanks to @suatkocar.
- **A deleted Linear ticket no longer leaves every session's Linear links on "Loading…",** thanks to @suatkocar.
- **With Dim sleeping sessions on, a session that sleeps right after finishing a turn dims like the others,** and a view whose Auto Sleep is Off is no longer slept by the awake cap, so VS Code keeps its terminals and place, thanks to @suatkocar.
- **A view hidden in one project comes back to its own place in the tab bar,** not at the end, thanks to @suatkocar.
- **Hiding or turning off the view on screen shows the tab beside it** instead of closing the side panel, thanks to @suatkocar.
- **An orchestrator's tree line in the sidebar runs on past a nested orchestrator's threads** instead of breaking beside them, on the computer and the phone, and a selected card no longer shifts by a pixel, thanks to @suatkocar.
- **Next and Previous Session work while the VS Code view, a Files preview, Work or an extension view has the keyboard.**

### 🌍 Browser

- **The Browser can show its pages in their own row under one Browser tab** (Settings > General > Tools > Browser > Show pages in their own row, off by default), with arrow keys and Delete on the row, thanks to @suatkocar.
- **A page an agent opens for a project that isn't on screen goes to that project's Browser in the background:** the project you're looking at stays as it is, a notice says where the page went, and the agent can use the page right away, thanks to @suatkocar.
- **Clicking a PR, issue, Linear or Work chip on another project's session takes you to that project** and opens the page in its Browser with that workspace's sign-ins, thanks to @suatkocar.

### 🌐 Remote computers

- **Connecting to a remote computer logs in once,** so each start or reconnect costs two SSH logins instead of seven or eight.
- **Remote tunnels stop when Ghostex quits,** and a tunnel left behind by a crash is stopped at the next launch.
- **Interrupting OpenCode ends its running shells,** and deleting a session, project or worktree drops its queued prompts.

### 👥 Accounts

- **Codex logins from different people in one ChatGPT Business workspace are separate accounts,** each with its own name and settings, and existing accounts keep theirs (needs codex-swap 0.3.2 or later).
- **Windows: Claude account switching uses Claude Swap 0.28,** still with shared history.

### 🐧 Linux

- **The setup wizard's Browser Use and Computer Use steps work on AUR, deb and rpm installs,** which ship `ghostex` as a small script Ghostex now recognises.

### 📄 Pages

- **A page's answers keep trying to send and save while Ghostex can't be reached,** on the computer and the phone.
- **A page published earlier gets today's built-in page script whenever it opens,** so it picks up later fixes.

### 📱 On the phone

- **The phone app moves to Expo SDK 58 and React Native 0.88.**
- **A page open on the phone keeps its address and its typed answers through a dropped link.**
- **On the Machines screen, a long machine name no longer covers Edit** when a row is swiped open.
- **The chip bar scrolls instead of overflowing,** its sheets use the chat's text size, and bottom sheets fade their backdrop in place while they slide up.

### 🩹 Fixes and polish

- **macOS: closing a focused chat menu or the maximized chat box no longer quits the app** through the Touch Bar, thanks to @WangAxerold.
- **A Chromium-style new tab (DevTools or the + button) no longer quits the app.**

## 10.35.0 - 2026-10-25

**Ghostex 10.35.0 is out.** A new Changes view reviews a project's diff with line comments for the agent, revert and a commit bar, the Code Editor view is now called VS Code, Handoff to Remote moves a session to another computer's project, and the Session details panel, Files compare and remote starts get a round of fixes.

### 🔍 Changes view

- **A Changes view shows a project's diff against main or its uncommitted changes,** in Unified or Split, with word-level highlights and Files-sized text, and it works for projects on remote computers too.
- **Comment on any line and the comment joins the chat box as a pill,** one pill per comment, and the agent gets the file, the lines and your comment when you send. The chat shows the same pills, and the lines stay marked Sent until they change.
- **Revert a whole file or a single hunk, and mark files viewed** as you go; a file's buttons show on hover and the file count opens a jump-to-file menu.
- **A commit bar with an agent picker commits what you picked:** Commit, Commit & Push, Commit, Push & Create PR, Multiple Commits or Merge into main, and leaving the message empty lets the agent write it. Only agents that run on your computer are offered.
- **It replaces the old commit review window,** so every place that opened that window now opens Changes on the matching scope.
- **It is on by default, right after Files** in the view picker, the + menu and the Store.
- **Settings > General > File opening > Open file changes in picks Changes view or VS Code** for Review all and the Session details Changes card.
- **The phone gets a read-only Changes screen** with the same diff, from a session's ⋯ menu or a long-press on its row.

### 💻 VS Code view

- **The Code Editor view is now called VS Code** everywhere, with the VS Code icon, and the outside editor in File opening menus is called VS Code app.

### 🤝 Handoff

- **Handoff to Remote hands a session to another computer's project:** pick the computer and project (the one with the same repository is preselected), clone the repository there if it is missing, and see a note when the branches differ. The original session keeps running.
- **Picking another agent's model hands off to exactly that agent and model,** and Handoff says why when no agent of that kind is set up.
- **The chat's … menu works on remote sessions:** Handoff/Export, Fork, Full Reload, Rename, Close After Done and Note.

### 🧩 Session details panel

- **Clicking a skill opens that skill,** in Agents Hub for your own skills and as its SKILL.md for a project or plugin skill.
- **The details button stays on the last chat you focused,** even while you work in the sidebar, Files, the Commands panel or another view.
- **The docked panel slides in and out** like the side panel, without the chat re-wrapping mid-slide.

### 📂 Files

- **When a file changes on disk while you edit it, Compare shows the difference right inside Files,** with Back to editing, Keep mine and Take theirs.

### 🤖 Agents

- **A custom agent whose command is a built-in agent, such as omp, starts on a remote computer as that agent,** and one that matches no built-in agent says how to fix it.
- **Antigravity chats no longer show agy's system messages,** each turn closes after agy's last reply, and agy 1.4.7's file edits, timers and task checks show as proper tool rows.
- **A finished background command shows as a tool call inside its turn** for every agent, instead of a separate card.

### 🌐 Startup and remote computers

- **Ghostex no longer shows a false startup report** when another start of gxserver got there first.
- **macOS: when a remote computer starts gxserver over SSH, it keeps Keychain access,** so connectors stay signed in.

### 🩹 Fixes and polish

- **Floating controls are solid, so text behind them no longer shows through:** the chat's command bar, the picture viewer's buttons in the chat and Files, and the composer's thumbnail remove button.
- **Tooltips across the app go away when their click opens a window** instead of staying up over it.
- **Windows: clicking a file link in a WSL session's chat opens the file** instead of saying the project folder is unavailable, thanks to @omegafrog.

## 10.34.0 - 2026-10-25

**Ghostex 10.34.0 is out.** Updating no longer stalls when every session has to restart, Files searches the whole project with Cmd+Shift+F and finds text in any file with Cmd+F, session links stay within the project's own repositories, OpenCode chats follow the terminal's model, and Claude sessions get a Session details panel in beta.

### 🔄 Updating

- **Updating no longer stalls when every session has to restart:** after the 10.33.0 update, a few dozen sessions could hold Ghostex at "starting" for minutes, and now the switch-over takes a couple of seconds, on macOS, Linux and Windows with WSL.

### 📂 Files

- **Cmd+Shift+F searches every file in the project in a Search view like VS Code's** (Ctrl+Shift+F on Windows and Linux), with match case, whole word and regex, files to include and exclude, results grouped by file, F4 to step through them and Esc back to the editor.
- **Project search skips what isn't your code:** it follows .gitignore, leaves out dependencies, build output, vendored code and large or binary files, and keeps its notes behind an info icon so the results get more room.
- **Cmd+F finds text in code and text files** with the same panel Markdown files have: whole word, match case, replace and a match count, with every match highlighted.
- **Long Markdown files scroll smoothly,** several times faster per frame than before.
- **A filter button in the file list's search box shows only HTML, Markdown, Excalidraw, image or video files,** one type or several at once, with the folders that hold them.

### 🔗 Session links

- **Ghostex only suggests and adds links from the project's own repositories** (its origin, plus upstream for a fork), and links from other repositories added earlier are dropped unless you pinned or added them yourself; Linear tickets are unaffected.
- **Hovering a link shows the sidebar's see-through tooltip right under it,** with the link's title and a pull request's branch, never cut off by the links card.

### 💡 Tooltips

- **Tooltips across the sidebar and Files use the same see-through bubble and stay inside the window,** from session card and project header buttons to the Search view's info note.

### 🧩 Session details panel

- **A Session details panel shows what a Claude session is working with,** beside the chat when there is room and as a floating card when there isn't: where it runs and on which Claude account (by email), how full its context is with Claude's own breakdown, its task list, connectors, skills, memory files, changes against main with Commit or Create PR, and background commands with a stop button.
- **It is a beta, on by default:** click the context ring at the top right of the window to show or hide it, and turn it off in Settings > Extensions > Store. Each part shows a pulsing placeholder while it loads, and clicking a skill opens it in Agents Hub. Claude sessions only for now.

### 🎙️ Voice

- **The voice agent is an extension you can turn off** in Settings > Extensions: its button leaves the top of the window, its hotkeys and call entries go away on the computer and the phone, and Dictate keeps working. Its settings now live under Settings > Extensions > Voice agent.

### 🤖 OpenCode

- **An OpenCode chat shows the same model as its terminal,** and messages sent from the chat use it.
- **Ghostex starts OpenCode 2 even when an older OpenCode comes first on your PATH,** and when OpenCode 2 is missing or can't start, the chat says why and offers Install OpenCode 2 or Restart on OpenCode 2.
- **OpenCode's long thoughts open in full** instead of being cut off on one line, on the computer, in the browser and on the phone.
- **Windows: OpenCode installs and updates from Settings,** using Git for Windows.

### 🌐 Remote machines

- **A remote machine's connection notice floats over the top of its list,** so the rows never move: a failed machine shows its reason with Reconnect and Remote Settings and turns only the tab's cloud icon red, and connecting shows a spinner and a one-line pill.
- **The notice's × shrinks it to a pill that keeps the red mark** and opens the card again on click, and the phone shows the same notice.

### 🩹 Fixes and polish

- **macOS: Option and Shift with the arrows, Delete and Forward Delete edit text in web page fields** instead of typing a blank.

## 10.33.0 - 2026-10-24

**Ghostex 10.33.0 is out.** Sessions keep their pull requests, issues and Linear tickets at hand as links, a voice call can focus on one session with a push-to-talk button, shell commands in chat run in a terminal with one click, and new sessions start reliably while crashes on Windows and macOS are fixed.

### 🔗 Session links

- **A session keeps links to any number of pull requests, GitHub issues and Linear tickets,** shown as chips beside its title, a line under its sidebar card and a list with review status and checks, on the computer and the phone.
- **Each link shows what it is by its icon** (pull request, draft or closed pull request, issue, Linear ticket or branch), and hovering one shows its full title and details.
- **The links list leads with open work and folds merged and closed links into their own section,** every section folds with a chevron, and the list grows up to half the window.
- **Pin a session's main links** so they lead every list and always show in the sidebar; a pinned link can't be removed until you unpin it.
- **A removed link can be undone or restored later,** links Ghostex suggests show their titles and states, and a session with only suggestions shows a quiet badge that opens them.
- **Session links show under every session card by default:** Settings > Sidebar > Session links in the sidebar starts at Always and also offers Selected session, Badge, Open work only and Off.
- **Hovering a session in the sidebar opens its links in a floating card,** and +N opens the full list.
- **Links stay fresh without slowing Ghostex down:** each repository is read from GitHub in one go, idle sessions refresh every 30 minutes and whenever you open them, and link titles survive restarts.
- **On the phone, a chat shows its session's links in a strip under the header,** session rows follow the sidebar setting, and a long press on a link opens its details with Pin and Remove.

### 💬 Chat

- **Shell commands in replies run with one click:** code blocks get Run in terminal and inline commands get Copy or Paste in terminal, using the project's Commands panel terminal, on the computer and the phone.
- **The picture viewer moves through every picture in the chat** with the arrow keys, edge buttons and a counter, and with swipes on the phone.
- **Every agent's tool calls read like Claude's:** a Codex command says what ran and a failed one leads with its exit code, and thinking is a slim row that opens on click.
- **Settings > Chat > Open chat pages in picks Popup, Files or System browser** for the pages agents show in chat.
- **Question forms show an info icon instead of the sentence about skipped questions,** on the computer, the phone and pages, and a page's choices follow the order of the variants shown above them.
- **The running dot sits in the pill's corner like the questions dot,** and the questions pill shows its count instead of a question mark.
- **Long chats use much less memory:** each chat now keeps its drawn messages once instead of twice, tens of megabytes less on a chat with a few thousand messages.
- **A session keeps the Terminal or Chat view you picked for it** when it wakes, resumes or comes back after a restart.

### 🎙️ Voice

- **A voice call can be about one session:** start it from a chat's menu, or a session's long press on the phone, and widen it to all sessions when you need to.
- **Push-to-talk gets a hold button beside Mute,** plus Space and a Hold to Talk hotkey and a Stop pill; on the phone, sliding off the button cancels, it buzzes as you press, and the call notification gets a Talk button.
- **Long voice replies read as short paragraphs,** a run of the same tool shows as one row, and rows get more room.
- **The computer asks Keep the call or Hang up and call when a call is already open on another computer,** like the phone does.

### 🗂️ Sidebar

- **A project's session groups sit indented under it with a tree line and a folder icon, and fold with it,** on the computer and the phone, thanks to @suatkocar (#286, ghostex-mobile#2).
- **A session can leave a session group again:** drag it onto another group or its project's list, or use Advanced > Move to Group and Remove from Group, thanks to @suatkocar (#284).
- **A folded orchestrator shows its threads' overlapping status dots:** blue means a thread finished and clears when you open the orchestrator, and pink means a thread is waiting on you.
- **The sidebar toggle keeps its place beside the window buttons** when the top row is wider than the sidebar (#288).
- **A computer tab's status circles sit below its label** instead of pushing the icon and name aside, on the computer and the phone.
- **An awake session's agent icon is brighter at rest,** and Collapse All skips hidden session groups.

### 🧭 Orchestrators

- **Every message an orchestrator sends a thread reminds it of its title,** so threads rename themselves to their latest task.
- **Messages between agents always say who sent them,** and a thread can't message its own orchestrator in the middle of a turn.

### 🧩 Issues Workflow

- **Work mode becomes Issues Workflow, on for every project once a Linear API key or `gh` is set up,** with Title sessions by their branch and Offer Clean up when a PR merges as their own settings, on the computer and the phone.

### 🌐 Remote access

- **The pairing QR is a ghostex.dev link,** so the phone's own camera opens Ghostex and pairs, on Android and iPhone.
- **Settings > Remote has Easy Connect, Tailscale, Remote machines and Advanced tabs,** each showing its status on the tab strip, and the pairing QR waits behind Show QR, which opens a larger popup.

### 📂 Files

- **Pictures zoom with pinch, keys and header buttons, tall ones open at the view's width, and a click opens a full-window viewer;** on the phone, a double tap switches between screen width and actual size.

### 📱 On the phone

- **This release's phone features come with the phone app update, not over the air,** so install the new app to get session links, voice calls about one session, push-to-talk, Run in terminal and the rest.
- **Android pastes and drops images into the chat box,** and the chat box's pills stop flickering while you type.

### 🪟 Windows

- **Creating a Codex session no longer crashes the background service (#289),** a session step that crashes is skipped instead of taking the service down, and leftover session terminals and helpers are ended.

### 🔄 Updating

- **Installing this version restarts your running sessions once on macOS, Linux and Windows with WSL.** Agents come back on their own, but anything they had in flight at that moment is lost, so update at a quiet time.

### 🩹 Fixes and polish

- **New sessions start reliably (#290):** Ghostex checks the launch command, types nothing before the agent's input box is ready, and sends a model only to its own agent, so Codex no longer opens blank and Claude no longer gets a Codex model.
- **macOS: picking a model or closing the Feedback window no longer quits the app (#291).**
- **Kanban card text fields take clicks again when the board has its own window (#287).**
- **Codex sessions idle for a week show their chat and resume again** after Codex compresses their history.
- **A session whose account was removed moves to a working account when it wakes or is restored,** and an account refusal shows one toast naming the session, agent and account, with Open Accounts.
- **Arrows, Home/End, Page Up/Down and Forward Delete edit text in web pages** instead of typing a blank.
- **Agents Hub: the Sync all agents panel is opaque and lists only the groups with changes,** and an empty plan just says everything is in sync.
- **The floating Capture button keeps up with a fast drag.**
- **Settings hides rows this computer can't use until you search for them,** so a Mac with Read aloud off no longer says it can't speak.

## 10.32.0 - 2026-10-24

**Ghostex 10.32.0 is out.** Claude cloud sessions get their own sidebar rows, floating pages open the moment you click them with a working picture viewer, the Browser handles sign-in popups and site permissions, and Ghostex uses far less memory and CPU.

### ☁️ Claude cloud sessions

- **Run a Claude Code session on Claude's own computers and keep it in the sidebar:** pick Claude (cloud) in New Thread, choose the Claude account it runs on, and type the task; it starts on claude.ai from the project's GitHub repository on a new `claude/…` branch.
- **Each Claude account has its own claude.ai sign-in inside Ghostex,** with Google sign-in, a spinner while it loads, and Back to Settings.
- **A cloud row keeps its cloud mark in every state, and Close puts it to sleep instead of removing it,** since the session goes on running on claude.ai; close it again to take it off the list.
- **The phone lists cloud rows too,** and tapping one opens the session on claude.ai.

### 💬 Chat

- **A floating page window opens the moment you click its card,** with a spinner while the page fills in, and Ghostex warms up its web engine a few seconds after launch so even the first page fills in at once.
- **Clicking a picture in a floating page opens a viewer inside the page with Zoom, Copy and arrows, and a floating page reopens where you left it scrolled;** both were announced in 10.31.0 but didn't work there.
- **A running Subagents or Threads pill shows a pulsing yellow dot,** and Subagents counts the running ones while any run.
- **While the agent thinks, its thinking is a closed card showing the newest finished line.**
- **A tall question card keeps its choices and buttons on screen and only the question text scrolls,** and a Claude question taller than its terminal can still be answered from the chat.
- **An answer sent while the background service restarts is retried until it's back** instead of failing after 5 seconds.
- **The cards stacked above the chat box are evenly spaced,** with the same gap under the last card as under the working bar.
- **The working bar above the chat box uses the chat box's own background, and file names in the @ popup use the normal weight.**
- **The docked question card's arrow points down while it's open and up while it's folded.**
- **A forked session counts only the questions it asked itself.**
- **A long single-line message, or one that starts with a path, is sent as one paste,** thanks to @suatkocar (#274).
- **A wide terminal keeps its width while it rests in Chat View,** thanks to @suatkocar (#276).

### 🗂️ Sidebar

- **A question plus working or done shows two overlapping dots,** like the Space button, and the question dot lines up with every other status dot.

### 🖥️ Terminal

- **Cmd+A selects the terminal screen and a plain shell no longer prints `7;9u`,** Select All on an empty screen selects nothing, and other Cmd keys behave as in Ghostty, thanks to @suatkocar (#278).

### 🌐 Browser and Files

- **Sign-in popups (Google included), site permission prompts and page notifications work in the Browser.**
- **Annotate uses Agentation 3.1.2, picks inside same-origin frames, and starts in annotating mode.**
- **In light mode the Files page stays neutral instead of taking the theme tint,** and it repaints when you change the theme colour.
- **Uninstalling the Chromium runtime while it runs removes it at the next launch,** with Keep installed to undo; Reinstall on Windows works the same way.

### 🎨 Theme

- **The sidebar and the main area share one transparency by default,** and the Strength slider moves both together, keeping any difference you set by hand.
- **A custom light colour is drawn as strongly as the light presets.**

### 📸 Screenshots

- **macOS: area selection lines up with the screen and covers the menu bar and the Dock,** so the frozen picture is no longer shifted down and a drag can start anywhere.

### 🤖 Agents

- **Antigravity: a turn stays working until agy is really done,** background commands included, and Combined status no longer ends it early.
- **Settings > Agents: Update available is a filled button** like the buttons beside it.

### 📱 On the phone

- **Published pages open in a page sheet** with Refresh, the phone's own picture viewer, and a scroll position shared with the computer.
- **On iPhone the chat box stays above the keyboard when the keyboard changes height while open.**

### 🧹 Projects

- **No more Chats: Quick Terminal, `ghostex create-chat` and the Chats group are gone,** and a project in the chats folder is an ordinary project.

### ⚡ Faster and lighter

- **The background service uses about half the CPU, and its startup memory peak drops from up to 650 MB to about 270 MB.**
- **Background windows stop redrawing, hidden chats pause their clocks, and a working chat updates only its newest turn** instead of rebuilding the whole transcript.
- **Typing and new messages in a long chat cost a fraction of what they did,** and saving a setting no longer freezes the window.
- **Chat images, diagrams and pictures stop piling up in memory.**

### 📦 Smaller downloads

- **The Windows installer is about 56 MB instead of 94 MB,** and releases no longer carry the Windows portable zip.
- **The macOS download is about 5 MB smaller.**

### 🔒 Security

- **Ghostex bundles Node 24.22.1 for the October 21 Node security release,** and its npm packages are past this week's advisories.

## 10.31.0 - 2026-10-22

**Ghostex 10.31.0 is out.** Pictures in floating pages open in the chat's image viewer, waiting messages can go back to the chat box, Windows repairs itself after an antivirus removes a helper program, downloads are smaller, and Read aloud arrives as an experimental feature.

### 💬 Chat

- **Click a picture in a floating page to open it in the chat's image viewer inside the page window,** with the same buttons and zoom, and arrows (or the Left and Right keys) to step through the page's pictures.
- **A floating page closes on a second click outside,** so a stray click no longer loses it; Escape and its close button still close it at once.
- **A floating page reopens where you left it scrolled,** and a Refresh button appears at its top right when the agent changes the page's file.
- **Floating pages, table previews and diagrams open at 80% of the window's width and 95% of its height.**
- **A waiting message gets a Back to composer arrow next to its play button** for Claude Code and Grok Build: it takes the message out of the agent's queue and puts it back in your chat box, unsent.
- **Back to composer and the Up key now work for Grok Build's queued messages too.**
- **The play button on a waiting message no longer cancels the message it was clicked for,** and does nothing once the agent has already started it.
- **Arabic and other right-to-left languages read right to left in the chat box and the transcript:** such a line is right-aligned, the arrow keys follow it, and Windows no longer draws it reversed.
- **Ghostex in a browser shows Arabic, Persian and Urdu text** instead of empty boxes.
- **The pink question dot counts only the questions still waiting for you.**

### 📱 On the phone

- **Tap a waiting message's play icon to interrupt the agent and send it now,** or tap the arrow beside it to bring the message back to the chat box.
- **Read aloud works on the phone in the chats of a computer that has it on,** with its own voice and speed, and Auto-read keeps going with the phone locked.

### 🔊 Read aloud

- **Read aloud speaks the agent's final replies on the Mac and in Ghostex in a browser,** as an experimental feature: turn on Enable Experimental Features, then switch on Read aloud in Settings > Extensions > Store.
- **Auto-read in the chat's ⋯ menu reads each finished reply of the chat on screen;** choose the voice and speed in Settings > Chat.

### 🪟 Windows

- **Ghostex repairs itself when an antivirus removes one of its helper programs** instead of failing every start, and when the installed copy is gone too, it names the file and how to restore it from Protection history.
- **A background service start that hangs for three minutes is replaced on the next launch.**

### 🩺 When something goes wrong

- **Ghostex waits while its background service is still starting** instead of failing with "Could not read gxserver response".
- **Send Bug / Feedback can open a GitHub issue in your browser when the background service is not answering,** with your message and the startup diagnostics filled in and your personal details removed.
- **`ghostex bundle` collects its diagnostics even when the background service is down.**
- **A saved key or sign-in that can no longer be read says so** in Settings > Voice agent and Settings > Connectors, with a way to paste or sign in again.

### 📦 Smaller downloads

- **Linux packages are much smaller:** the x64 rpm drops from 119 to 75 MB, the deb from 92 to 76 MB, and the built-in browser component from 366 to 152 MB.
- **The macOS download is about 9 MB smaller and the Windows zip about 4 MB smaller.**

### 🩹 Fixes and polish

- **The Linux deb installs on Debian 11 again.**
- **Japanese input picks conversion candidates with Up and Down again.**
- **Quick Access's project, scope and tag pickers have a proper search field.**
- **A folded coordinator in the sidebar shows the highlight when one of its hidden threads is the session on screen,** on the computer and the phone.
- **`ghostex settings open --tab` opens Voice agent, Connectors and Workspaces.**

## 10.30.0 - 2026-10-22

**Ghostex 10.30.0 is out.** Claude Code workflows get their own card in the chat, permission requests say what is asked and why, the phone selects text in chats and terminals and keeps crash reports, and Claude account sessions no longer start signed out.

### 💬 Chat

- **Claude Code workflows show as a Workflow card:** its name, description and agent counts, each phase with its progress and description, one row per agent with its status, a preview and a live timer, and the script behind Show full script; click an agent to open it.
- **Permission requests say what is asked and why:** the card shows what the agent wants to do, which agent is asking (background helpers included), the exact command or file, and Claude's warning when it flagged the request.
- **An answered permission card no longer flashes back for a moment.**
- **File edit and write approvals from Claude Code show in the chat again,** with the file and Claude's question.
- **When Claude switches itself into Plan mode, Ghostex switches the session back to your mode;** a Plan mode you pick yourself stays.
- **Opening a finished turn's "Worked for" card lists its tool calls in one flat list,** in the order they ran, with the agent's notes in between as plain text.
- **"+N previous tool calls" sits above the last tool call,** so the earlier calls open above it in the order they ran.
- **OpenCode 2 sessions get Chat View again,** and the dimmed Chat View button names the agents that support it.
- **Stash prompt in More actions is one line,** with the number of stashed prompts in a small circle.
- **Handover headings now default to 60% color strength;** change it in Settings > Chat > Handover heading color strength.
- **The microphone always stays next to Send,** even when the chat box is narrow.
- **A question card that could not be shown disappears once the agent asks again in a readable form.**
- **Picking a Claude mode the session does not offer stops after one try** instead of cycling your mode.

### 📱 On the phone

- **Hold a message's words to select them,** with Copy, Add to Chat and More (the message's actions) in the menu, on Android and iPhone.
- **Hold a word in the terminal to select it and drag to select more,** with Copy, Paste and Select All, on iPhone and Android.
- **The phone keeps a crash report and offers to send it on the next launch,** cleaned of personal data, through Send Bug / Feedback.
- **The image viewer zooms with pinch and double tap on Android** instead of crashing on a double tap.
- **Saving an edited computer no longer crashes the app on Android.**
- **The Docs screen is now called Files,** like the computer's Files view, and lists projects with deeply nested folders.
- **The Threads sheet's rows use the sheet's normal text size.**
- **The voice screen keeps your place when you scroll up,** with a button to jump back down, and Stop talking is now Stop.

### 🎧 Voice agent

- **Choose a call mode in Settings > Voice agent:** regular, or push-to-talk with a Hold to talk button, which costs less on Grok; Settings shows the price of each, on the computer and the phone.
- **The voice agent knows which session is on your screen** when you say "this one", and when each message was sent.

### 🔐 Claude accounts

- **Sessions on your added Claude accounts no longer start "Not logged in"** when your default Claude login is signed out.
- **A session on a turned-off or signed-out account moves to a working account when it wakes,** in the same conversation.
- **Waking a Claude session no longer hangs for minutes** searching old transcripts.

### 📸 Ghostex Capture

- **Write Prompt's Send to is a dropdown that starts on the session you last used in the main window,** with the same project list and filter inside it.
- **The prompt box hides during a capture** instead of leaving a strip of it on screen and in the picture.
- **The capture panel, editor, prompt box and notes keep their border all the way round the corners.**

### 🪟 Windows

- **Menus and pop-ups resize in place:** clicking "N more" in the Threads menu no longer closes it and brings another app in front of Ghostex, and a closing pop-up hands focus back to Ghostex.

### 🗂️ Sidebar and coordinators

- **Collapsed project and collection headers count sessions waiting on your answers in pink,** on the computer and the phone.
- **The selected Space's button no longer shows status dots,** on the computer and the phone.
- **Auto Sleep keeps a coordinator awake while one of its threads is working or waiting on a question.**
- **Close After Done is no longer offered for coordinators;** their threads keep it.

### ⚙️ Settings

- **Less-used settings move behind Show Advanced:** about 27 General rows and the Workspaces, Cloud Boxes, Open In and OS Integration pages, which search still finds.

### 🌐 Remote computers

- **Remote settings see a connected Tailscale** even when its command-line tool is not on your PATH, so the Tailscale phone code appears again.

### 🧰 Installed tools

- **Beads, gh and glab installed from Settings run by name in your terminals and agents right away,** and Settings shows them as installed only when a new terminal can find them.

### 📦 Smaller downloads

- **The Files view's drawing editor is about 5 MB smaller;** pasted Mermaid text now pastes as plain text instead of becoming a drawing.
- **The onboarding pictures are about 1 MB smaller.**

### 🩹 Fixes and polish

- **Input methods in the terminal show their candidate window at the cursor as it moves** (Fcitx5 on Linux and others), thanks to @bet4it.
- **Agents Hub no longer reports "Hook scripts: 1 to fix" when there is nothing to link,** and Windows paths show as ~/.claude/hooks.

## 10.29.0 - 2026-10-20

**Ghostex 10.29.0 is out.** The voice agent comes to Windows and Linux and learns to draft, remember your last talk and show you the session it is talking about, Dictate never loses a recording, Cmd-clicked file links open at their line, and the downloads get smaller.

### 🎧 Voice agent

- **The voice agent, its panel and Dictate now work on Windows and Linux,** with the same keys and settings as on the Mac.
- **It shows you the session it is talking about:** when it reads, answers or drafts in a session, Ghostex switches to it, in any project, unless you are typing; on the phone it shows an "Open <session>" chip instead.
- **It can draft a message into a session's chat without sending it,** for you to read and send, and it asks out loud before sending anything risky or destructive.
- **It remembers your last conversation:** a Continue / Start fresh card in the call picks up where you left off with a one-sentence recap; forget it from the panel or Settings > Voice agent.
- **It answers the chat's questions for you,** by option name, number or "the recommended one", and tells you what it recorded.
- **It sees an orchestrator's threads as sub-threads** and reads one when you ask for it by name.
- **The voice panel can be moved,** and clicking the voice's name picks another voice; a small speed button next to it sets how fast it talks, from 1× to 2×, on the computer and the phone.
- **New hotkeys:** ⌃⌥V starts or ends a voice call and ⌃⌥D dictates into the chat box (Ctrl+Alt+Shift+V and Ctrl+Alt+Shift+D on Windows and Linux).
- **The headset button now sits left of Start** in the work area header.

### 🎙️ Dictate

- **Dictate never loses a recording:** it records first and transcribes after, keeps every recording, and lists them at the bottom of Settings > Voice agent to play, download, recover or transcribe to the clipboard.
- **Dictating shows as a small pill right above the microphone** with the timer, Transcribing and a ✕ to cancel (Escape cancels too), and the chat box keeps its cursor so you see where the text will land.

### 💬 Chat

- **Find (Cmd+F) searches only what you can see** and highlights the matched words instead of whole blocks.
- **A reply stays in view when a watcher, a background task or a timer wakes the agent again,** instead of folding into the next turn.
- **Clicking anywhere on the working bar above the chat box scrolls to the bottom** while the ↓ button shows; on the phone, tap it.
- **Summary mode and Stash prompt leave the chat box toolbar:** Summary mode is under More actions > View and Stash prompt is a More actions row, with the same shortcuts.
- **Skip all on a question form or page now skips every question,** even ones you had already picked.
- **Handover heading color strength** is a new advanced Settings > Chat slider that fades the colored Done, Needs you and Untested headings toward plain text.
- **A question page shows when its questions were withdrawn,** like the chat's own forms.

### 🗂️ Sidebar and notifications

- **The New terminal button is back on every project header,** next to the agent button, on the computer and the phone.
- **Every status dot is the same size,** overlapping dots get a thin ring, and Spaces and computer tabs show a dot when a session in them is waiting on your answers.
- **In-progress notices pulse their dot and end with "…"** ("Committing…", "Pushing…", "Creating worktree…") until the result arrives.

### 🧭 Orchestrators

- **Show all effort levels** is a new advanced setting: the orchestrator effort pickers offer only High and XHigh unless you turn it on, since lower levels aren't recommended for orchestrators.

### 🛠 Code view

- **Cmd-clicking a `file:line` or `file:line:column` link in the terminal opens the Code view at that line,** and `ghostex edit --goto` no longer gets lost after the Code view was closed or reloaded; if no Code view takes an open within a minute, Ghostex says so instead of doing nothing (#266).

### 📦 Smaller downloads

- **The Mac download is about a third smaller** (about 76 MB instead of 110 MB).
- **Windows: the installer no longer carries the Code editor and the WSL terminal engine;** Ghostex downloads them the first time you need them, so the first Code view or WSL setup needs internet.

### 📱 On the phone

- **Easy Connect reconnects after the app sat in the background,** thanks to @banozz0 (#265).

### 🩹 Fixes and polish

- **Windows: chat menus and pickers have one clean corner,** with and without window transparency.
- **The Settings > Appearance theme previews are rounded on all four corners.**

## 10.28.0 - 2026-10-20

**Ghostex 10.28.0 is out.** Talk to your agents out loud with the new voice agent, see at a glance which sessions are waiting on your answers, run OpenCode as an orchestrator, and hear completion sounds on Linux and Windows.

### 🎧 Voice agent

- **Talk to your agents out loud:** on a Mac, the headset button in the work area header starts a spoken conversation with Ghostex's voice agent. Ask what your sessions are doing or which ones are waiting on you, have it read you a session's last messages, send a prompt, answer a question or approve a permission prompt, or start a new agent, and it tells you when a session finishes its turn.
- **A panel under the header shows live captions of both sides** and the tools the agent runs, with Mute, Stop talking and Hang up; talking over it stops it, and it hangs up by itself after 5 quiet minutes.
- **Bring your own key:** pick xAI Grok Voice (the default) or OpenAI GPT Realtime in Settings > Voice agent, choose a voice and paste the provider's key, which is checked before it is saved and stays encrypted on your computer. Not available on Windows or Linux yet.
- **On your Android phone too:** the headset button in the top bar starts the same conversation with the computer you are looking at, and it keeps going while you switch apps or lock the phone, with a "Voice mode on" notification that has Mute and Hang up.
- **Dictate is back on the Mac:** the microphone button in the chat box and on the terminal bar turns what you say into text with xAI's speech-to-text and never sends it by itself; on Android it sits in the chat box's toolbar.

### 🔔 Sessions and notifications

- **Sessions waiting on your answers stand out:** when an agent ends its turn with questions you have not answered, its sidebar row shows the question dot and you get one notification, on the computer and the phone; answering clears it, and `ghostex sessions` shows "answer requested".

### 💬 Chat

- **The Previous question pill and question links take you to the question itself,** even in a long reply, instead of the end of the message. On the computer and the phone.
- **Answered questions show their number in the same tile as the card's icon,** lined up under the title.

### 🧭 Orchestrators

- **OpenCode 2 can be an orchestrator:** pick it in New Orchestrator or run `ghostex orchestrator create --agent opencode` (#261), and the sidebar's Make Orchestrator now offers every agent an orchestrator can run on, on the computer and the phone.
- **A new OpenCode chat sends its first message** instead of leaving it waiting in the queue.

### 🖥 Terminal

- **Bold text uses a heavier font** is a new Settings > Terminal switch (on by default, under Show Advanced): turn it off and bold text keeps your normal font weight, so it stands out by color only. Ghostty's `bold-color`, `font-style-bold` and `font-variation-bold` in Ghostex's terminal config work too (#264).

### 🛠 Code view

- **When another program holds the Code view's port,** the card names it and offers "Close it and retry" instead of "VS Code didn't start in time"; on Windows it asks for permission when it needs to.
- **A Code editor another computer starts here gets its own port** and ends when that computer disconnects, so a dropped SSH connection no longer blocks your own Code view.

### 🐧 Linux and Windows

- **Completion and copy sounds play on Linux and Windows:** the packages now include the sounds and the app plays them (#260).
- **The Linux x64 .rpm installs with dnf again.**
- **The Resources panel lists your servers on Linux.**

## 10.27.0 - 2026-10-19

**Ghostex 10.27.0 is out.** Every one-line card in the chat gets one clean look, you can edit the orchestrator playbook for all projects or one project, Settings gathers its add-on pages into an Extensions group, and one notification bell covers every connected computer.

### 💬 Chat

- **Every one-line card looks the same:** "Worked for", "Previous turn", Message from and to, background commands, model and effort changes, compactions and answered questions are all 44px cards with one font and background and a square icon on the left; their status (Delivered, Queued, Waiting, Answered) is a pill on the right, and none has a chevron. On the computer and the phone.
- **Question cards show what is waiting on you:** a question waiting for your answer leads with a checklist and a yellow "?", an answered one with a question bubble and a green check, and the edited files card with a pencil.
- **Question numbers sit at the right of each question** in the chat's questions form and on pages, and an unanswered question on a page shows the yellow "?" too.
- **The conversation ends right above the working card,** without the extra gap that sat under the last message.

### 🧭 Orchestrators

- **Edit the orchestrator playbook:** Settings > Chat > Orchestrator playbook changes the playbook every orchestrator follows, a project can have its own in Settings > Projects that also covers its worktrees, and New Orchestrator shows and edits the one it will use.
- **Orchestrators you already have follow an edited playbook,** Reset to Default (click it twice) brings back Ghostex's own, a switch turns an edited copy off without losing it, and the editor tells you when Ghostex's playbook changed since you edited yours. From a terminal: `ghostex orchestrator playbook` and `ghostex orchestrator set-playbook`.

### 🧩 Settings and extensions

- **An Extensions group at the bottom of Settings:** Store (the page that was called Extensions), Workspaces, Cloud Boxes, Connectors, Actions, Open In, OS Integration and Debugging sit in one collapsible group that remembers whether it is open, and a search opens it.
- **Connectors, OS Integration and Debugging can be turned off in the Store** like the other built-in extensions; turning Connectors off also turns off `ghostex connectors`.
- **Popups inside Settings close with the same round X as Settings itself,** shown when the pointer is near their top.
- **Sections in the Settings sidebar fold with the same animation as projects in the sidebar.**

### 🌐 Remote computers

- **One notification bell for every connected computer,** with All, This computer and Remote tabs: a remote notification names its computer, and clicking it switches the sidebar to that computer.
- **Switching computers reopens the session you were last on there,** the way switching Spaces does.

### 🐧 Linux

- **Ghostex comes for Linux ARM64:** a .deb, an .rpm and a tarball for 64-bit ARM computers sit next to the x64 downloads.

### 🩹 Fixes and polish

- **The Subagents pill no longer counts stopped or interrupted subagents as running:** subagents stopped with TaskStop or Stop all, or finished in an unusual way, leave the count, so their timers no longer grow for hours (#259).
- **A codex typed in a plain zsh or fish terminal stays off Codex's shared server** even when mise or another tool reorders your PATH.
- **Bordered tab switches keep their rounded corners** when the first or last tab is selected.

## 10.26.0 - 2026-10-18

**Ghostex 10.26.0 is out.** The chat's working indicator becomes one steady card above the chat box, Copilot, Cursor, Grok and OMP work fully from the chat, model menus read each agent's own model list, orchestrators run from a skill with an agent of your choice, and a long list of fixes from your bug reports.

### 💬 Chat

- **One working card above the chat box:** what the agent is doing, the running tool's name and the pills (Threads, Background, the ? question button, Delayed Send) sit in one card right above the chat box, so the conversation never jumps or scrolls under it; each pill opens its own menu, and a finished turn reads "Worked for 12s".
- **The model pill shows the full model and effort:** it grows to fit the name when there's room, and hovering it names the provider, model, effort and context in full.
- **Indented lines in agent replies keep their indentation** in the chat on the computer and the phone, while lists, quotes and code blocks render as before.
- **Edit › Copy, Cut, Paste and Select All work in the terminal and the chat,** enabled only when they can act, so apps that read your selection through Edit › Copy get it, thanks to @banozz0.
- **With several computers connected, each computer's chats show that computer's own model lists.**
- **A Claude session's model chip reads only its own status line,** not another session's footer that an agent printed in the conversation.

### 🤖 Agents

- **Copilot, Cursor, Grok and OMP work from the chat:** approvals and questions show as chat cards, Stop really stops, and each gets its own slash commands, mode picker, context meter and Fork where the agent supports it. Copilot's conversation shows in the chat for the first time.
- **Model menus read each agent's own list:** Codex, Cursor, Grok and Antigravity menus come from the CLI's own model list, refreshed every 10 minutes, with older versions folded under Legacy models.
- **Hermes 0.22 works in the chat again,** and Pi's queue no longer cuts a running turn short.
- **Antigravity's Plan mode no longer lands in auto-approve,** Codex's plan mode turns off from the chat, and a model the agent refused shows what it's really running.
- **Long chat messages and briefs reach OMP 18.11 again:** Ghostex now recognises the 📄 chip OMP shows for a long paste.
- **ZCode:** answers to its question picker reach it from the chat, a Start Agent brief is sent instead of waiting forever, and moving a draft into the chat takes 2 seconds instead of failing after 16.
- **A codex you type by hand in a Ghostex session no longer locks its conversation:** it runs without Codex's shared background server, so Ghostex's own resume never shows "This conversation is open in another app".
- **Codex:** a session Codex didn't name gets a quiet name a minute after its first prompt, a session restored by its title finds its conversation, a session that never got a prompt wakes fresh, and "Enable full access?" shows in the chat.
- **Grok, Antigravity and OMP sessions slept before their first prompt wake properly,** and Copilot keeps its real conversation when it resumes.
- **Agent CLI updates never dead-end:** Ghostex falls back to the CLI's own updater, Copilot and Pi on Windows update, Hermes reports its update state, and OMP installs its own binary.
- **Search by prompt finds Antigravity, Hermes, Copilot, Droid, OMP and ZCode prompts** and resumes their sessions.

### 🧭 Orchestrators

- **Orchestrators run from the ghostex-orchestrator skill:** New Orchestrator gets an agent dropdown (Claude, Codex, ZCode, Cursor, Grok, Copilot, Antigravity, Pi and OMP), High or XHigh effort with XHigh by default, and an (i) that explains what an orchestrator is for.
- **An OMP thread reports to its orchestrator once per turn,** not at every pause while it works.
- **Orchestrators can answer without markers:** an advanced, not-recommended setting in Settings > Chat > Orchestrators answer without markers; Show marker emojis is an advanced setting now too.

### 🐞 Send Bug / Feedback

- **Large screenshots send:** up to five screenshots of 5 MB each, the same budget as `ghostex feedback send`.
- **The investigating agent starts on the computer that has the reported session,** including a remote one.
- **Images can be dropped onto the dialog,** and a screenshot copied from CleanShot pastes as an image instead of its file path.

### 🔌 Connectors

- **Each workspace can sign gws in with its own Google OAuth client:** Settings > Connectors > Choose client…, or `ghostex connectors gws client <client_secret.json>`.
- **Connector sign-ins copy their one-time code and show it large,** with Copy and Open page again, and Settings stays open when you switch back from the browser.
- **A Composio read whose tool name puts the noun first runs without an approval card.**

### 🐧 Linux and Windows

- **Linux: dead keys (~, ´) no longer freeze the terminal** on Wayland and X11.
- **Windows: the portable zip runs as its own Ghostex** beside an installed one, with its data, ports and lock next to itself, and it leaves the installed Ghostex's agent hooks alone.
- **Windows: a Claude Swap update, reinstall or uninstall waits for running sessions** instead of breaking cswap.
- **WSL: Ghostex no longer reads WSL as unavailable because of Windows PATH entries,** and WSL startup errors say what failed, thanks to @Dev-Mriganka.
- **Windows: commit message generation and Generate Name work under PowerShell.**

### 🩹 Fixes and polish

- **The Settings window no longer lights up the sidebar rows behind it** when you move over its menu.
- **Ghostex web: the New session menu starts agents,** and the chat's More actions (Rename, Sleep, Fork, Full Reload) work.
- **`ghostex board start-work` types the card's prompt into the new session,** so an agent started from the command line or an outside orchestrator gets to work instead of waiting at an empty prompt.
- **One-shot jobs (commit messages, Catch me up, titles) run on the session's account or your default,** never a disabled one.
- **The sidebar's bottom row sits a little higher above the window edge.**

## 10.25.1 - 2026-10-17

**Ghostex 10.25.1 is out.** Agents start without stopping at a "Do you trust this folder?" prompt, the chat status line stays on one row, and the phone's Catch me up moves into the ⋯ menu.

### 🤖 Agents

- **Agents no longer stop at a "Do you trust this folder?" prompt:** before Claude Code, Codex, Cursor CLI, Copilot, Gemini, Grok Build, Antigravity, Pi or Factory Droid starts, Ghostex marks its project or worktree folder as trusted, and it never trusts your home folder or anything above it. Turn it off in Settings > Agents > Trust project folders automatically.

### 💬 Chat

- **The status line stays on one row** and scrolls sideways instead of wrapping, on the computer and the phone; on the computer a vertical mouse wheel over it scrolls it too.

### 📱 On the phone

- **Catch me up moves into the ⋯ menu** above Side chat, and the chat box no longer shrinks while you scroll.
- **Move left and Move right in a machine tab's long-press menu** reorder the machine strip.

## 10.25.0 - 2026-10-17

**Ghostex 10.25.0 is out.** Update Ghostex on your other computers from anywhere, find any project file with Cmd+O, open CSV, JSON, PDF, SQLite and font files right in Files, chat with OpenCode 2 with live replies, and the phone reconnects on its own after being in the background.

### 📂 Files

- **Cmd+O finds files:** Quick Access has a Files tab that searches every project's files, the current project first, and Cmd+5 switches to it while Quick Access is open; Enter opens the file in Files.
- **Settings > File opening has two dropdowns,** one for HTML, Markdown and images and one for every other file, offering Files, the Code view, VS Code, Cursor, Zed, the JetBrains IDEs and the other editors you have installed; your earlier Markdown, HTML and image choices are kept.
- **CSV and TSV files open as a table** you can sort by column and copy from, with a Raw button for the text.
- **JSON files get Format, Minify and a Tree view** that folds, searches and copies a value's path.
- **PDFs open in the built-in PDF viewer,** AVIF pictures show like any other picture, and font files open as a preview with sample text, weights and their characters.
- **SQLite databases open read-only:** pick a table to page through its rows or run a SELECT query.
- **Dockerfiles, PowerShell scripts and Terraform files are coloured,** and so are .env, .gitignore, .ini and .jsonl files; notebooks open in the Code view.
- **Zip, Office and other files Files can't show open in their own app** instead of a "not previewed" message.
- **Typing in a long Markdown file with notes stays fast,** and undo and redo update the view.

### 💬 Chat

- **Every file in the chat has Copy Path and Open File Location,** and Open in Files works for every file except folders and media; on the phone Copy Path is in the long-press menu.
- **Grey thinking** (Settings > Chat, off by default) shows the agent's thinking in grey and its replies in the full text colour, on the computer and the phone.
- **The Previous question pill shows an amber dot that pings** when a question arrives and every 30 seconds while any is unanswered.
- **When an agent writes questions Ghostex can't show,** Ghostex asks it once to send them again in the right format.
- **An interrupted turn reads "Agent was interrupted",** and Skip all is a soft fill so it no longer looks disabled.
- **A Claude chat resumed after a while keeps its last reply visible** instead of folding it away behind a long Worked time.
- **Page questions show a green check as soon as you answer them,** and questions you already answered or that were asked again no longer come back as unanswered.

### 🌐 Remote computers

- **Update Ghostex on a remote computer** from Settings > Remote, its tab in the sidebar or `ghostex app-update`, with progress, while its agents keep running; turn on Keep this machine up to date to install updates while none of its agents is working. It works on computers running this release or later, so update older ones once by hand.
- **Pages, localhost pictures and Browser links from a remote session open through the connection to that computer** instead of this computer's 127.0.0.1.
- **The Code view opens for a session on a remote Mac** from any computer, including Windows.

### 🤖 Agents

- **OpenCode 2 chats stream replies and reasoning live,** and accept-all, resume, prompt search and the installer work with OpenCode 2; OpenCode 1 is no longer supported.
- **Windows: Hermes Agent can use its tools again;** its hooks no longer block every tool call.
- **An agent CLI install or update that is refused says why.**

### 🧭 Browser

- **A phone button in the Browser toolbar shows a tab at phone or tablet size,** with an optional mobile user agent, saved per tab.
- **macOS: clicking a saved autofill suggestion fills the field,** and so do Down and Enter.
- **The Browser no longer offers to save passwords** or sign you in automatically.

### ⚙️ Settings

- **Settings stays open when you switch apps** and reopens on the same page, scroll position and open form; the Add machine dialog keeps what you typed (not the password) across restarts.
- **The Actions, custom agent, Cloud Boxes, Extensions and Add account forms reopen with what you typed,** never a sign-in code or secret.
- **The Theme page's + square opens the colour picker,** and Colourfulness sets how deep a custom colour is.

### 📱 On the phone

- **The phone reconnects on its own after being in the background,** instead of needing a force-quit.
- **iOS terminal: a long press always shows Paste,** Cmd+V on a keyboard pastes without asking each time, and a multi-line paste into a program without safe paste asks first, on Android too.
- **Wide tables in chat scroll sideways.**
- **The web preview has a Desktop site switch,** remembered for each site.
- **The Machines list shows each computer's Ghostex version** and can update it.
- **Links, pages, pictures, video, audio, PDFs and drawings from a computer open through the phone's connection to it,** from anywhere on that computer.

### 🩹 Fixes and polish

- **Capture no longer crashes on a Mac without Screen Recording permission,** and its windows have no traffic lights.
- **Restarting or updating Ghostex never leaves gxserver stuck,** so the app reconnects within seconds.
- **Tooltip text is centred** instead of sitting low.
- **The Notifications panel has more room around its title.**
- **Start Agent in Send Bug / Feedback runs in the Ghostex settings folder,** which every agent trusts.
- **Connectors (experimental): Settings > Connectors, per-workspace gh and gws sign-ins,** and the Connectors helper no longer slows Ghostex down.

## 10.24.0 - 2026-10-16

**Ghostex 10.24.0 is out.** Send Bug / Feedback can hand your report to an agent that gathers the details and files it, and it's on the phone too; question cards send blank questions as skipped and can send on a single pick; programs can report their own session status; Windows gets a real choice of window blur and a transparency strength that goes all the way to clear; and Source opens again for WSL projects.

### 🐞 Send Bug / Feedback

- **Send Feedback is now Send Bug / Feedback,** and it asks for your GitHub username first, so GitHub tells you when your report is fixed: the issue opens with "Reported by @you".
- **Collect more data with an agent:** turn it on and Start Agent hands your description and screenshots to your default agent, which gathers the version, settings and logs that help, shows you the full draft in its chat, and files the issue only after you say yes.
- **The phone has Send Bug / Feedback too,** in the Sessions menu after Settings, with screenshots from your photos or the clipboard. It posts even when it can't reach your computer, and while a computer is connected it can start the agent there.
- **Agents can file an issue themselves** with `ghostex feedback send`, which also shows the exact request with `--dry-run`.

### 💬 Chat

- **A question you leave blank is sent as skipped,** so a form goes out with Send answers whatever you answered; the separate Skip row on every question is gone, and Skip all still sends at once. On the computer, the web and the phone.
- **Send when you pick an answer:** turn it on in Settings > Chat (under Show Advanced) and a card that asks one single-choice question sends the moment you pick, thanks to @larik-PR.
- **Skip all and Send answers show their shortcuts in a quick tooltip** instead of key labels, and Skip all wears Send's colours a shade darker.
- **Claude's dialogs become numbered choice cards,** with rare actions folded under More, and Claude menus with tabs or bullets get clickable rows.
- **A tool call or file edit waiting for your approval opens by itself,** showing its command or diff, and folds back once you answer.
- **The panels above the composer become one row of pills** on the Working line (scroll to bottom, previous question, threads, tasks, subagents, background work), each opening its own list.
- **Pictures in a reply show as a filmstrip,** pictures in your own messages are small thumbnails again, and question tags are outline pills that show whether they were answered.
- **A connection or server error retries quickly first** and shows as a card with the retry state, Retry now and Stop retrying.
- **An agent that couldn't start shows the start command's error lines** instead of waiting at Starting.
- **A described tool call reads its description in every mode,** and with Simple mode off the work under each reply starts open.
- **Files and links moves into the ⋯ menu,** and Maximize chat box and Files and links get hotkeys.
- **The zoom keys change one saved chat zoom** that every chat follows, and messages from or to an orchestrator show its crown.

### 🚦 Session status

- **Programs can report their own status:** Claude Code 2.1.295 and later, Codex betas and your own scripts send it, and Settings > Sidebar > Session status from picks Detection, Combined or Program status only.
- **A session whose program reports an error gets a red dot,** and its long-press menu on the phone shows the program's own status line.

### 🪟 Windows

- **Blur on Windows:** with Desktop and windows behind the glass, pick Acrylic, Plain blur or Off in Settings > Theme > Transparency; Acrylic now stays blurred when Ghostex isn't the focused window.
- **Transparency Strength goes from nearly solid to fully clear** across the whole window, including an empty work area and session placeholders (on every platform).
- **Source opens for WSL projects again:** Ghostex no longer gives up while it prepares the Linux editor, and it clears a VS Code an older version left holding its port.
- **Ghostex installs its own build of Claude Swap on Windows,** which shares history between accounts and adds folder trust to session profiles.

### 🎨 Theme and Settings

- **Background opacity and Background blur** in Settings > Terminal make terminals see-through on their own, and the strip under a terminal's agent bar matches them.
- **A calmer Theme page** with Appearance preview cards, one Transparency choice and smaller colour squares.
- **Every tooltip is a small menu with key caps,** frosted under window glass.
- **Settings opens bigger,** Accounts is a list with a detail page and one-click Reconnect, and an account's Enabled switch keeps it saved but out of use.
- **Dragging an expanded row in Settings > Agents collapses it first,** and the Extensions page links to the related settings.

### 🧭 Orchestrators

- **Threads rename themselves** after the area they own, and a thread can be unlinked into a standalone session.
- **Orchestrators can see and fix accounts:** `ghostex accounts` lists every account with its usage, and `ghostex agents switch-account` and `ghostex agents reload` move or restart a stuck thread.
- **A restored session keeps its launch settings, account and model,** and a restored orchestrator keeps its threads and notes.

### 📂 Files

- **A wrap button replaces Refresh,** and open files follow changes on disk, like VS Code.
- **A code file fills the view,** with the path shown once and a see-through gutter under window glass.

### 🔌 Connectors (experimental)

- **Each workspace can connect apps** through Executor or Composio, sign them in and run tools that only read. Turn on Enable Experimental Features to see Connectors.

### 📱 On the phone

- **On iOS a long press on the terminal opens Paste,** and it pastes without the Allow Paste prompt.
- **Easy Connect finds Ghostex on a Windows computer where it's installed just for you,** and shows a computer's errors as plain text.
- **The Android app is smaller:** the APK ships for 64-bit phones only, compressed.

### 🩹 Fixes and polish

- **An old file Ghostex can't move into its new storage folder no longer stops it from starting** (#231).
- **Leaving an empty chat never closes it** (#236), and the first-run intro opens on a still picture with Play and fits small screens (#237).
- **A session whose worktree folder was deleted reopens in the project folder.**
- **Skills show one row per skill in the Agents Hub,** with one shared copy in ~/.agents/skills.
- **ZCode sessions show Working and Attention again,** chat sends reach ZCode 3.15, and a failed ZCode turn no longer leaves its session working.
- **A Claude conversation moved into Claude's background daemon keeps its chat,** and taking back a queued Claude message works with an empty queue.
- **Codex's /status shows its output every time,** and /diff's text reaches the chat.
- **`ghostex logs` shows the newest lines,** and `ghostex remove-project` takes a project's name or path.
- **Cmd+Backspace clears the line like Ghostty,** and the web app on a Mac gets the Mac editing keys and Cmd shortcuts.
- **Linux: a click inside a page window no longer closes it.**

## 10.23.0 - 2026-10-14

**Ghostex 10.23.0 is out.** Question forms take a closing note and answers show at once, sent agent messages turn Delivered when they arrive, orchestrators get Sleep Inactive Threads and editors for their goal and memory, closed sessions come back from anywhere, SpaceO gets a guided permission setup on macOS, and agents start properly under WSL with zsh and Powerlevel10k.

### 💬 Chat

- **A question form with more than one question ends with an optional notes box** for anything else you want the agent to know; it arrives with your answers. On the computer, the web and the phone.
- **Answers show at once:** a question form or page reads Answered the moment you send, and your answers appear right away as a pending card that settles once the agent has them.
- **A Previous question ↑ button** beside Scroll to bottom jumps to the next unanswered question; turn it on in Settings > Chat > Previous question button. Q-tags in an agent's reply (Q80) link to the question they name.
- **A message one agent sends another now turns Delivered** once the other session takes it, or Not sent with the reason, instead of staying on Waiting for agent. On the computer, the web and the phone.
- **In summary mode the turn that is still running shows its work open,** with the running command visible, and folds into the reply once the turn ends.
- **Simple mode names a tool call by the description the agent gave it** instead of the tool's name.
- **An agent that can't start says why:** a missing account login becomes a card with Switch account, Sign in and Try again, and a message waiting for that agent goes out once it is up.
- **Antigravity CLI's Run this command? panel becomes a permission card in the chat,** and its folder-trust card answers from the chat again.
- **Cmd+Enter on a folder-trust card picks Yes, I trust,** and links in the chat no longer show an underline.
- **A marked section's label stands on its own line,** with the text that followed it underneath.

### 🧭 Orchestrators

- **Sleep Inactive Threads** on an orchestrator's right-click menu sleeps every one of its threads that is idle. On the computer and the phone.
- **Goal & Instructions and Memory editors** open from an orchestrator's menu, and a thread's menu has Mark Thread Done and Reopen Thread, each saying why when a session can't close or wake. On the computer and the phone.

### 🗂️ Sessions and sidebar

- **Closed sessions come back from anywhere:** `ghostex previous-sessions` lists them and `ghostex restore-previous-session` restores one, and the phone's session search lists closed sessions and restores the one you tap.
- **Every project always shows all its sessions:** the compact list and Show more / Show less are gone, on the computer and the phone.
- **Send Feedback moved into the sidebar's More options menu,** under Join Discord, and takes an optional GitHub username so you hear back on GitHub when your report is fixed.
- **Sleep Below skips sessions that are running a shell command or asking a question.**
- **A row's menu opens where you right-clicked,** or lined up with the button that opened it.
- **Session hotkeys act on a focused side-panel view:** with Kanban, Files or the Browser focused, Close, Previous/Next Tab, Sleep, Reload, Cmd+F and Cmd+R work on that view.

### 📂 Files

- **Open With starts with Show in Folder** for a file on your computer, which shows it selected in File Explorer (Finder on a Mac).

### 🔐 macOS permissions

- **SpaceO and Fast Computer & Browser Use each ask macOS for their own permissions,** and Settings > Integrations shows Granted, Not granted or Needs restart for each one.
- **A guide beside System Settings walks you through SpaceO's permissions:** drag SpaceO from the tag into the list, it moves to the other side when there is no room, and SpaceO restarts by itself once its agents are done.
- **SpaceO updates from the same Update button as Fast Computer & Browser Use.**

### 🤖 Agents and accounts

- **Codex's Update now updates the copy your sessions actually run,** with the tool that installed it, then restarts Codex once it is idle.
- **`ghostex create-agent` and the board set an Antigravity CLI worker's model and effort at launch.**
- **Switching a session's account answers the folder-trust prompt once,** including for an account that never ran in that folder, and Sign in again works when the default Claude profile is signed out.

### 🐧 Linux and Windows

- **Agents start properly under zsh with Powerlevel10k:** Codex no longer quits with "stdin is not a terminal" in WSL (and on Linux and macOS), thanks to @omegafrog for the report.
- **WSL sessions no longer take `C:\WINDOWS\system32\wsl.exe` as their name;** sessions already named that way go back to their usual name.

### 🖥️ Terminal

- **Ghostex terminals have their own Ghostty config,** so Settings > Terminal never writes to the Ghostty app's config again; your current look is copied over once, and Import from Ghostty brings over fonts, spacing and the cursor whenever you want.

### 🎨 Look

- **Every text button in the app's dialogs, Settings and chat cards is filled;** secondary actions take one neutral fill, on the computer and the phone.
- **Settings > Chat: Show marker emojis comes before Marked sections style,** and Preview is a labelled button beside the style dropdown.

### 🩹 Fixes and polish

- **`ghostex show` embeds images a page points to by a relative path,** and a page with inlined images is no longer refused for its source size.
- **Closing a session ends the messages it was holding,** so it can't come back by itself.
- **Starting an agent that is turned off in Settings > Agents from the CLI is refused** and names the agents you can use, unless you pass `--allow-hidden`.
- **Settings' scrollbars look and sit like the chat's,** and the rail's no longer covers its rows.
- **The Goal & Instructions and Memory editors stay open until the save succeeds.**

## 10.22.0 - 2026-10-13

**Ghostex 10.22.0 is out.** A Files and links card lists everything a session mentioned, the Kanban board scrolls with a mouse and opens in a window of its own, the Work panel shows formatted Markdown, and Ghostex has a new app icon.

### 💬 Chat

- **Files and links:** a link button in the chat box's toolbar opens a card listing every attachment, file and link the session mentioned, with a filter and counts; each row opens its item or jumps to the message that mentioned it. For an orchestrator it covers its own chat and every thread, grouped by thread. On the computer, the web and the phone.
- **Catch me up shows only while the chat box is empty,** fades in and out, and reads the last 30 agent messages.
- **A tool group's line** is the same thin hairline as the Worked fold's.
- **macOS: Cmd+V pastes into the chat box again** instead of into the terminal behind the chat, thanks to @banozz0.

### 🗂️ Kanban

- **The board scrolls sideways with a mouse:** drag the bar under the lanes or use Shift+wheel, each lane has its own scrollbar, and dragging a card to the board's edge scrolls the board, thanks to @landygg.
- **Open in New Window** moves the board to a window of its own, for a second screen; the panel shows Show Window and Bring Back, and the window follows the project you switch to, thanks to @landygg.

### 🧩 Work Mode

- **The Work panel shows Markdown:** ticket descriptions and comments, Slack messages and pull request bodies show headings, lists, checklists, tables, code and links.
- **Video comes right after PR** in the team flow, and "Working thread" is now an optional "Slack thread" step: an optional step shows a grey "?" until it's done. Mark any step optional in Settings > Workspaces > Team-flow steps; a team's owner runs `ghostex team deploy` once so the team can save it.
- **A session card shows its ticket's new pull request** within moments of it being opened, instead of minutes later.

### 🌸 App icon

- **A new seven-petal app icon** on the computer and the phone.

### 🩹 Fixes and polish

- **macOS: the sidebar button sits further from the window buttons,** thanks to @TheOrcDev.
- **An account is no longer marked disabled** because a CLI started by hand in a session's terminal was refused on a different login.

## 10.21.0 - 2026-10-12

**Ghostex 10.21.0 is out.** A Catch me up button tells you where a session stands, a message an agent queued comes back to your chat box with ↑, renaming a session shows the new name at once, messages between agents name the other session, and on Windows a restart or update takes over in seconds instead of showing the startup dialog.

### 💬 Chat

- **Catch me up:** a button at the top right of the chat box has a small model read the session's recent conversation and shows a coloured handover (what needs you, what's done, what's in progress) in a card above the chat box, with Refresh, Copy and Close. Off by default: turn it on in Settings > Chat > Catch me up button.
- **Back to composer:** a message Claude Code is still holding in its queue gets Back to composer in its actions, and ↑ in an empty chat box takes back the newest one, like ↑ in Claude's own input. Its text and pictures are added to your draft unsent, and any other queued messages stay queued.
- **Messages between agents name the other session:** a row reads Message from "<session>" or Message to "<session>" and opens to the whole message; one command that sends the same message to several sessions is one "Message to 2 sessions" row listing each recipient, and a session that can't be found reads "another agent" instead of a raw code.
- **Answered question forms fold to one line** with their title and Answered (click to open the form again), and your answers below read as plain text, each question with your answer under it; the agent's own question receipts look the same.
- **Needs you is orange** in marked handovers, so it no longer looks like In progress.
- **Section labels wrap and their links work:** a long marked label (for example a pull request title with its link) wraps like a paragraph and its link is clickable.
- **A `!` command's copy button** floats outside the bubble on its left, like other messages, and the bubble no longer says Running.
- **The Worked row** shows only its tool-call and file counts; a failed call still shows on its own row.
- **The text caret is 1 pixel wide** in the chat box and every text field.

### 📋 Sidebar

- **Renaming a session shows the new name at once,** and the agent's confirmation reaches every computer and the phone within about a quarter second instead of 3–4 seconds. If the agent never takes the name, the old one comes back with a shake and Rename reopens with the name you tried.

### 🧭 Orchestrators

- **A message to a sleeping thread wakes it** and keeps retrying until it's delivered, and reopening a thread wakes it too; a message that waited more than a day is marked Not sent instead of waking an old session.
- **Handovers list each thread once,** under Kicked off or In progress, and question numbers keep counting across the whole conversation.

### 📱 On the phone

- **Hold a message to see its actions:** copy, edit, Back to composer, rewind and the rest open on a long press, in the main chat and the subagent viewer, and nothing shows under a message until you do.

### 🪟 Windows

- **Restarts and updates take over in seconds:** a leftover gxserver of an older build is replaced right away, and startup checks only the sessions that need it, so Ghostex no longer waits minutes or shows the startup diagnostics dialog.
- **Hovering a message while typing no longer blinks** its buttons and time.

### 🩹 Fixes and polish

- **Sending feedback says why GitHub refused it** instead of a generic "try again", and a refused send can be sent again right away.

## 10.20.1 - 2026-10-11

**Ghostex 10.20.1 is out.** Messages with pictures wait for a slow-starting agent too, sessions move to an account with room faster when they hit a usage limit, and chat cards sit darker.

### 💬 Chat

- **Messages with pictures wait for a slow-starting agent** instead of giving up after 40 seconds; Ghostex keeps its own copy of each picture until the message goes through.
- **Chat cards sit darker** and question cards use a softer yellow, matching the question cards on agents' pages.
- **A finished background task just says it completed;** "exit code 0" and the duration are gone, and a failed task still shows its exit code.

### 🤖 Accounts

- **A session that hits its usage limit moves to an account with room within about 30 seconds,** instead of waiting minutes for a fresh usage reading.
- **A sleeping session wakes on an account with room** when its own account is already at its limit.
- **When every account is full,** the chat shows a card saying which limit was hit and when work continues, and the bell gets one notification.

## 10.20.0 - 2026-10-11

**Ghostex 10.20.0 is out.** Agents ask you questions as a form right in the chat, a message you send while an agent is still starting waits and goes through on its own, chat cards get a new look (questions, notices, agent messages, file changes, page previews), `/resume` and Ctrl+Enter interrupt-and-send work from the chat view, Ctrl+V no longer freezes Ghostex on Windows, and Work Mode can start Claude Code in the cloud.

⚠️ **Teams:** run `ghostex team deploy` once after updating, so teammates' cloud sessions show on the Work page (team functions version 5).

### 💬 Chat

- **Questions as a form in the chat:** agents ask several questions at once in one card: click to pick (nothing preselected), "Other / none of these" with a note, text answers, one Send (or Ctrl/Cmd+Enter). Your picks are kept until you send, answers wait while the agent is busy, and the card turns green once answered, with each answer under its question.
- **Question pages for visual choices:** agents' HTML pages can ask about mockups, with a Pick button and a note box at each mockup and each question right under what it asks about. The page's question cards look like the chat's and fill the page width; drafts are kept if you close the popup, and it closes once you send.
- **Messages wait for a slow-starting agent:** a message you send while the agent is still starting stays in the chat and goes through on its own as soon as the agent is ready, even if you close the app. You can edit or delete it while it waits.
- **Agent messages are one-line rows:** a message from or to another agent shows the agent's logo, the first line and its status (a check once delivered); click it to read the whole message, and Go to session appears on hover.
- **New notice and approval cards** with a coloured icon for info, warnings, errors and successes.
- **Page previews:** a page card shows a picture of the page once you've opened it in the floating window, and the file name until then.
- **File change cards:** a header with the files' folder, totals and a ratio bar, one row per file with a change bar (the icon's colour tells an edit, a new file and a deletion apart), opening to a diff with line numbers; Review all opens the Code view's Source Control.
- **Finished turns:** "Worked 16s" with rounded chips for tool calls, files and failures on the right, and tool rows with icons, durations and a status mark.
- **`/resume` from the chat:** Claude Code's and Codex's conversation picker shows as a card you drive with ↑/↓, typing, Enter or a click; other agents' pickers show live in a card that takes the keyboard until you hand it back.
- **Interrupt & Send:** Ctrl+Enter (or right-click Send) stops the agent's turn and sends your message right away; on the phone, hold Send while the agent works.
- **Background tasks:** each running task shows what it's doing with a timer (two or more fold into one row), the working indicator always comes first, and the finished row says how long the task ran.
- **Marked handovers:** orchestrators and their threads mark each part of their handover with ❓ ✅ ⚠️ 🛫 🔄 🧪 🚀 🛑 (Settings > Chat > Marked handovers in every session extends it to all agents), and the chat colours those sections in the style you pick. Settings > Chat > Show marker emojis (off by default) keeps the emojis next to the coloured titles.
- **The chat's scrollbar matches the Files view's:** inset from the edge, a little wider when you point at it.

### 🧭 Orchestrators

- **The Threads card lists working threads first;** Show recent adds the latest finished ones, then 30 more at a time.
- **Orchestrators ask you their questions as a form in the chat** (or a page when there's something to look at), bring their threads' questions and pages to you, and list every running thread in their handover.

### 🪟 Windows

- **Ctrl+V no longer freezes Ghostex** when the app you copied from is slow to hand over its data (Office, Remote Desktop, screenshot tools), in the chat, terminals, dialogs and Kanban.
- **Codex and Antigravity hooks run on Windows again,** so their activity and transcripts reach Ghostex.

### 🤖 Agents

- **Antigravity chat no longer stalls** while it has a background task running.
- **An agent started inside a session can no longer take that session over.**
- **The Claude usage card says when its numbers are old** ("Last known", "Last updated 3 days ago") instead of claiming live limits.

### 🧩 Workspaces and Work Mode

- **Start in cloud (Claude Code), Link to current session and Open in terminal** for cloud sessions on the Work page.
- **Linear pages stay in a workspace's browser,** and a site's question is a bar on its page instead of a dialog; Settings > Workspaces > Site permissions can forget your answers.
- **Assigned to me keeps all your tickets, Clean up works on Windows,** and work-card buttons never shrink.

### 🎨 Look and setup

- **The sidebar's bottom bar holds the workspace switcher and Send Feedback** on the left, and Commands (a lightning button) and Settings on the right; the Spaces row sits centred without a background.
- **Menus close when you click their button again,** and sidebar menus open 3px below their row (the workspace menu opens above its button).
- **Your views can be deleted** from the view editor, with a confirmation.
- **Setup's last page sets Agent approvals** (Keep default / Skip permissions).
- **The blue attention outline's top line shows** at any display scale, and an orchestrator's thread count lines up with the time.

### 📱 Phone

- **The phone chat matches:** question forms, agent message rows, notice and approval cards, page previews, file change cards, the new turn look, the `/resume` card, background task rows, Interrupt & Send on a held Send, coloured handovers and the marker emoji switch.

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
