# Ghostex terms

What the words we use for Ghostex mean, so users, agents and code reviews stay on the same page. Add or update an entry whenever the user defines a new term or renames a concept.

- **Orchestrator**: the agent session the user talks to about a stream of work. It plans the work, hands it to threads, keeps track of them and reports back. Formerly called Coordinator (renamed 2026-10-10); code, API routes and saved data still say `coordinator`, and `ghostex coordinator …` still works as a hidden alias of `ghostex orchestrator …`.
- **Thread**: an agent session an orchestrator starts and briefs to do one piece of work. It sits under its orchestrator in the sidebar, and its final message is its report to the orchestrator.
- **Current session** (Work page): the session selected in the window that hosts the Work view. The ticket's Start chat ▾ menu offers **Link to current session**, which adds the ticket to that session's links.
- **Start in cloud**: starting a ticket's work as a cloud session (Claude Code on the web for now) from the Work page or `ghostex work-mode start <ticket> --cloud`, with a task Ghostex drafts from the ticket. The list of clouds it offers is the cloud runner's provider list (`server/src/cloud_runner.rs`).
