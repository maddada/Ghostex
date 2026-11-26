use crate::data::*;
use crate::json::opt;
use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "integrations",
        title: "Integrations",
        sections: vec![integrations()],
    }
}

pub(crate) fn integrations() -> Section {
    section(
        "integrations",
        "Integrations",
        vec![
            row("ghostexCapture", "Floating Capture", "A small button that floats over every app and shows how many agents are working, waiting for you, or asking a question. Screenshot an area, an app or the whole screen, mark it up, and send a prompt to any project or session without switching to Ghostex. Cmd+Ctrl+Shift+S (Alt+Ctrl+Shift+S on Windows and Linux) opens it; A, Space, F or T instead of S capture an area, the current app or the full screen, or write a prompt, right away."),
            row("ghostexCli", "Ghostex CLI", "Ghostex keeps the app-bundled ghostex command linked automatically for mobile apps and CLI-backed integration setup."),
            row("bundledAgentSkills", "Bundled Agent Skills", "Install the Ghostex skill: type /ghostex ($ghostex in Codex) and any agent reads what Ghostex can do, and the guide for each job, from the ghostex CLI, which always matches this Ghostex. It is copied to ~/.agents/skills and can be updated or uninstalled. For computer and browser use, agents read the cua-driver skill that Fast Computer & Browser Use installs.").options_of(BUNDLED_GHOSTEX_AGENT_SKILLS, "name", "skillName"),
            row("cuaPermissions", "Fast Computer & Browser Use Permissions", "Fast Computer & Browser Use needs Accessibility to click and type in apps, and Screen Recording to understand what is visible on the desktop."),
            row("spaceo", "SpaceO", "On an Apple Silicon Mac with macOS 14 or later, install SpaceO so agents can use Mac apps on their own hidden screen while you keep your own screen, pointer and focus. Ghostex runs the official installer and keeps SpaceO running in the background, and agents read ghostex spaceo guide to use it; once installed, update, reinstall or uninstall it from the same row."),
            row("spaceoPermissions", "SpaceO Permissions", "SpaceO needs Accessibility to click and type in apps, and Screen Recording to take screenshots of the apps it runs."),
            row("managedTools", "Tools", "Install, update, reinstall or uninstall the tools Ghostex sets up for you: Node.js and npm, uv, Homebrew, Linux system tools, Beads, the GitHub and GitLab CLIs, and Google's Gemini API CLI. Each Install button says how it installs; your own copies are used when you have them.").options(&[opt("Node.js and npm", "node"), opt("uv (Python)", "uv"), opt("Homebrew", "homebrew"), opt("System tools (curl, unzip, git)", "systemTools"), opt("Beads (bd)", "beads"), opt("GitHub CLI (gh)", "gh"), opt("GitLab CLI (glab)", "glab"), opt("Gemini API CLI (gemini-api)", "geminiApi")]),
            // The Chromium runtime (CEF) row moved to Settings > Browser (CDXC:Settings 2026-11-26
            // in general/browser.rs); the page keeps a line pointing there.
        ],
    )
}
