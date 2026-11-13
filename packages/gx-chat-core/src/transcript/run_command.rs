//! Which shell commands in an agent's reply the chat offers to run in a terminal, and the text it
//! runs for each.
//!
//! CDXC:SessionChat 2026-10-24 DECISION:
//! User: "when i see this in the chat view, i want hovering on it to show a button to be able to run this in the terminal (we need to remove "! " from the start before sending it to terminal if it's there (because that's for the agent)". Of an inline span holding `cd /Users/madda/dev/_active/maddada-com && npx void auth login` with no `!`: inline code spans that hold a shell command qualify too, decided conservatively, never for a plain path, an identifier or a single word. Then, trying it: "run in terminal should be here for multi line when it's a command basically" (a block's Run button sits in its header beside wrap and copy), and "for inline it shouldn't be showing overlapping the command text. instead we should have 2 buttons (copy and paste in terminal (just pastes it doesn't run it) appear there above/below (keep it consistent with other things we show in chat view basically)". So a shell fence (bash, sh, zsh, shell, console), or an untagged or `text` fence that opens with `!` or `$ ` or is one command line, gets Run (types it and presses Enter); an inline command gets a hover bar above it with Copy and Paste in terminal (types it, Enter left to the user). Both go to a terminal of the Commands panel (User: "please make clicking on the floating terminal button that we added add that command to a terminal in the commands panel not a new terminal session in the sidebar"; the host picks the tab, `session_chat/run_in_terminal.rs`), never the agent's terminal. The hover bar sits above the span and flips below it when the transcript has no room above (User: "please fix this floating thingy overlapping other elements near the top and near the bottom (needs to flip to other side in that case").
//! SEE-ALSO: `native_markdown.rs` carries the decision to the renderers (the fence header's `run`, the
//! `ghostex-run:` link around an inline span); apps/desktop/src/app/native_chat/code_block.rs and
//! markdown_links.rs draw the desktop's buttons, apps/mobile/app/src/chat/native/transcript/markdown/
//! the phone's; the run itself is the `runInTerminal` host action (apps/desktop/src/app/session_chat/
//! run_in_terminal.rs, `ghostex run-in-terminal` in server/src/ghostex_cli/run_in_terminal.rs).

/// The scheme an inline command span's link carries: the command, percent-encoded, follows it.
pub const RUN_COMMAND_HREF_PREFIX: &str = "ghostex-run:";

/// Fence languages whose body is a shell session or script.
const SHELL_LANGUAGES: [&str; 8] = [
    "bash",
    "sh",
    "zsh",
    "shell",
    "console",
    "shellsession",
    "shell-session",
    "terminal",
];

/// First words that make a span a command once it has an argument: the shell builtins agents ask
/// users to type and the CLIs their instructions name.
const COMMAND_WORDS: &[&str] = &[
    // Shell builtins and core utilities.
    "cd",
    "export",
    "source",
    "unset",
    "alias",
    "echo",
    "printf",
    "ls",
    "cat",
    "less",
    "head",
    "tail",
    "grep",
    "rg",
    "find",
    "fd",
    "sed",
    "awk",
    "mkdir",
    "rmdir",
    "rm",
    "cp",
    "mv",
    "ln",
    "touch",
    "chmod",
    "chown",
    "sudo",
    "env",
    "which",
    "kill",
    "pkill",
    "killall",
    "ps",
    "lsof",
    "tar",
    "unzip",
    "zip",
    "curl",
    "wget",
    "ssh",
    "scp",
    "rsync",
    "open",
    "xattr",
    "codesign",
    "defaults",
    "launchctl",
    "diskutil",
    "say",
    "pbcopy",
    "pbpaste",
    "du",
    "df",
    "tree",
    "jq",
    "watch",
    "time",
    "nohup",
    "xargs",
    "tee",
    "wc",
    "sort",
    "diff",
    "patch",
    "ping",
    "dig",
    "systemctl",
    "journalctl",
    "apt",
    "apt-get",
    "dnf",
    "yum",
    "pacman",
    "brew",
    "port",
    "nix",
    // Version control and hosting.
    "git",
    "gh",
    "glab",
    "jj",
    // JavaScript.
    "npm",
    "npx",
    "pnpm",
    "pnpx",
    "yarn",
    "bun",
    "bunx",
    "node",
    "deno",
    "corepack",
    "nvm",
    "volta",
    "tsc",
    "vite",
    "vercel",
    "netlify",
    "wrangler",
    "firebase",
    "expo",
    "eas",
    // Rust, Go, Zig, Swift and the JVM.
    "cargo",
    "rustup",
    "rustc",
    "go",
    "zig",
    "swift",
    "xcodebuild",
    "xcrun",
    "xcode-select",
    "pod",
    "java",
    "mvn",
    "gradle",
    "./gradlew",
    "kotlin",
    // Python, Ruby, PHP.
    "python",
    "python3",
    "pip",
    "pip3",
    "pipx",
    "uv",
    "uvx",
    "poetry",
    "conda",
    "ruby",
    "gem",
    "bundle",
    "rails",
    "rake",
    "php",
    "composer",
    // Build tools, containers and clouds.
    "make",
    "cmake",
    "ninja",
    "just",
    "docker",
    "docker-compose",
    "podman",
    "kubectl",
    "helm",
    "terraform",
    "tofu",
    "aws",
    "gcloud",
    "az",
    "fly",
    "flyctl",
    "heroku",
    "doctl",
    "supabase",
    "railway",
    "render",
    "stripe",
    "ollama",
    // Agents and Ghostex itself.
    "ghostex",
    "gxserver",
    "claude",
    "codex",
    "opencode",
    "cswap",
    "zmx",
    "agentbox",
    // The virtual display agents test windows on (CDXC:Coordinators 2026-11-12: a dock row asked
    // the user to run `spaceo safety clear-host-health --operator` and offered no Copy).
    "spaceo",
    // Editors launched from a shell.
    "code",
    "cursor",
    "vim",
    "nvim",
    "nano",
    "subl",
    "zed",
];

/// The command a fenced block runs, when the block holds one: `language` is the fence's first
/// info word, `code` its body without the fence lines.
pub fn fence_command(language: &str, code: &str) -> Option<String> {
    let language = language.trim().to_ascii_lowercase();
    let body = code.trim_matches(['\n', '\r']);
    if body.trim().is_empty() {
        return None;
    }
    let first = body.lines().next().unwrap_or_default().trim_start();
    if language == "console" || language == "shellsession" || language == "shell-session" {
        return Some(console_commands(body));
    }
    if SHELL_LANGUAGES.contains(&language.as_str()) {
        return Some(without_bang(body));
    }
    if !(language.is_empty() || language == "text" || language == "txt" || language == "plain") {
        return None;
    }
    if first.starts_with('!') {
        return Some(without_bang(body));
    }
    if first.starts_with("$ ") {
        return Some(console_commands(body));
    }
    // An untagged fence of one line is the inline rule's to judge.
    if body.lines().count() == 1 {
        return inline_command(body);
    }
    None
}

/// The command an inline code span runs, when the span holds one.
pub fn inline_command(code: &str) -> Option<String> {
    let code = code.trim();
    if code.is_empty() || code.contains(['\n', '\r']) {
        return None;
    }
    if let Some(rest) = code.strip_prefix("! ").or_else(|| code.strip_prefix("$ ")) {
        let rest = rest.trim();
        return (!rest.is_empty()).then(|| rest.to_owned());
    }
    is_command_line(code).then(|| code.to_owned())
}

/// A command as it travels inside the marked Markdown: percent-encoded, so it holds no space,
/// quote or backslash for a Markdown parser to split on or unescape.
pub fn encode_command(command: &str) -> String {
    let mut encoded = String::with_capacity(command.len());
    for byte in command.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The command `encode_command` wrote, or `None` when the text is not one.
pub fn decode_command(encoded: &str) -> Option<String> {
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = encoded.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded)
        .ok()
        .filter(|command| !command.trim().is_empty())
}

/// The href of the link an inline command span becomes.
pub fn command_href(command: &str) -> String {
    format!("{RUN_COMMAND_HREF_PREFIX}{}", encode_command(command))
}

/// The command a `ghostex-run:` href carries, or `None` for any other link. The core only writes
/// one-line commands there (`inline_command`), so one holding a line break or other control
/// character is not the core's and is refused.
pub fn command_from_href(href: &str) -> Option<String> {
    decode_command(href.strip_prefix(RUN_COMMAND_HREF_PREFIX)?)
        .filter(|command| !command.chars().any(char::is_control))
}

/// The block as typed, without the `!` the agent's own shell prefix needs on its first line.
fn without_bang(body: &str) -> String {
    let trimmed = body.trim_start_matches(['\n', '\r']);
    let indent = trimmed.len() - trimmed.trim_start().len();
    let rest = &trimmed[indent..];
    let rest = rest
        .strip_prefix("! ")
        .or_else(|| rest.strip_prefix('!'))
        .unwrap_or(rest);
    rest.trim_end().to_owned()
}

/// A console transcript's commands: the lines a `$ ` prompt marks, without it; the output lines in
/// between are not typed. A block without prompts is run as typed.
fn console_commands(body: &str) -> String {
    let commands: Vec<&str> = body
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("$ "))
        .collect();
    if commands.is_empty() {
        return without_bang(body);
    }
    commands.join("\n").trim_end().to_owned()
}

/// A word an assignment like `FOO=bar` is: the shell sets it for the command after it.
fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        && !name.as_bytes()[0].is_ascii_digit()
}

/// A plausible command name: lowercase letters, digits and `-._/`, starting with a letter or `./`.
fn is_command_name(word: &str) -> bool {
    let name = word.strip_prefix("./").unwrap_or(word);
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_lowercase()
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.' | b'/')
        })
}

/// One of the commands an agent tells a user to type: a known command with at least one argument,
/// or a `&&` chain whose every step starts with a command name.
fn is_command_line(code: &str) -> bool {
    let words: Vec<&str> = code.split_whitespace().collect();
    let Some(start) = words.iter().position(|word| !is_assignment(word)) else {
        return false;
    };
    let words = &words[start..];
    let Some(first) = words.first() else {
        return false;
    };
    // A span that reads like code rather than a command line.
    let bytes = code.as_bytes();
    let generic = bytes.iter().enumerate().any(|(index, byte)| {
        matches!(byte, b'<' | b'>')
            && index > 0
            && !matches!(bytes[index - 1], b' ' | b'>' | b'2' | b'&')
    });
    if code.contains(['(', ')', '{', '}']) || generic {
        return false;
    }
    if COMMAND_WORDS.contains(first) {
        return words.len() >= 2 && command_arguments_fit(first, &words[1..]);
    }
    let steps: Vec<&str> = code.split("&&").map(str::trim).collect();
    steps.len() >= 2
        && words.len() >= 2
        && !words[1].starts_with("&&")
        && steps.iter().all(|step| {
            step.split_whitespace()
                .find(|word| !is_assignment(word))
                .is_some_and(|name| name.len() >= 2 && is_command_name(name))
        })
}

/// Arguments that keep an English phrase written as code (`source code`, `open questions`) from
/// passing as a command.
fn command_arguments_fit(command: &str, arguments: &[&str]) -> bool {
    let looks_like_argument = |word: &&str| {
        word.starts_with('-')
            || word.contains(['/', '.', '~', '$', '=', ':', '@', '"', '\''])
            || matches!(*word, "&&" | "||" | "|" | ";")
    };
    match command {
        // Builtins whose next word is a path or a name, which prose never writes after them.
        "cd" | "source" | "open" | "ls" | "cat" | "rm" | "mkdir" | "touch" | "code" | "cursor"
        | "vim" | "nvim" | "nano" | "subl" | "zed" | "time" | "watch" | "env" | "which" | "say"
        | "tree" | "sort" | "diff" | "patch" | "head" | "tail" | "less" | "find" | "make"
        | "just" => arguments.iter().any(looks_like_argument) || arguments.len() >= 2,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_spans_from_real_chats_hold_commands() {
        for (span, command) in [
            (
                "cd /Users/madda/dev/_active/maddada-com && npx void auth login",
                "cd /Users/madda/dev/_active/maddada-com && npx void auth login",
            ),
            (
                "! cd /Users/madda/dev/_active/maddada-com && npx void auth login",
                "cd /Users/madda/dev/_active/maddada-com && npx void auth login",
            ),
            ("$ npm install", "npm install"),
            ("gh auth login", "gh auth login"),
            ("cargo xtask start", "cargo xtask start"),
            ("brew install sccache", "brew install sccache"),
            ("git push origin main", "git push origin main"),
            ("npx vercel login", "npx vercel login"),
            ("ghostex sessions", "ghostex sessions"),
            (
                "GHOSTEX_AGENT_SKILLS_REMOTE=off gxserver agent-skills install --offline",
                "GHOSTEX_AGENT_SKILLS_REMOTE=off gxserver agent-skills install --offline",
            ),
            ("cd apps/desktop", "cd apps/desktop"),
            ("source ~/.zshrc", "source ~/.zshrc"),
            ("export PATH=$HOME/bin:$PATH", "export PATH=$HOME/bin:$PATH"),
            ("claude /login", "claude /login"),
        ] {
            assert_eq!(inline_command(span).as_deref(), Some(command), "{span}");
        }
    }

    #[test]
    fn inline_spans_that_are_not_commands_stay_code() {
        for span in [
            "cargo",
            "cd",
            "git",
            "apps/desktop/src/app/native_chat/code_block.rs",
            "/Users/madda/dev/_active/Ghostex",
            "~/.local/share/agent-work",
            "native_markdown",
            "ChatAppearance",
            "set_code_wrap(key, !wrapped, cx)",
            "Option<String>",
            "string | null",
            "chrome_ink().opacity(0.12)",
            "--json",
            "source code",
            "open questions",
            "make sure",
            "!important",
            "TODO: fix this",
            "! ",
            "a && b",
            "Foo && Bar",
        ] {
            assert_eq!(inline_command(span), None, "{span}");
        }
    }

    #[test]
    fn a_chained_command_of_unknown_tools_still_qualifies() {
        assert_eq!(
            inline_command("mytool build && mytool deploy").as_deref(),
            Some("mytool build && mytool deploy")
        );
    }

    #[test]
    fn a_bang_fence_runs_without_its_bang() {
        assert_eq!(
            fence_command(
                "",
                "! cd /Users/madda/dev/_active/maddada-com && npx void auth login"
            )
            .as_deref(),
            Some("cd /Users/madda/dev/_active/maddada-com && npx void auth login")
        );
        assert_eq!(
            fence_command("", "!npm run build").as_deref(),
            Some("npm run build")
        );
    }

    #[test]
    fn a_shell_fence_runs_whole_as_typed() {
        assert_eq!(
            fence_command("bash", "cd apps/desktop\ncargo check\n").as_deref(),
            Some("cd apps/desktop\ncargo check")
        );
        assert_eq!(
            fence_command("sh", "! brew install sccache").as_deref(),
            Some("brew install sccache")
        );
        assert_eq!(
            fence_command("zsh", "echo $SHELL").as_deref(),
            Some("echo $SHELL")
        );
        // A shell fence is a command whatever its words.
        assert_eq!(fence_command("shell", "mytool").as_deref(), Some("mytool"));
    }

    #[test]
    fn a_console_fence_runs_only_its_prompted_lines() {
        assert_eq!(
            fence_command("console", "$ npm install\nadded 12 packages\n$ npm test").as_deref(),
            Some("npm install\nnpm test")
        );
        assert_eq!(
            fence_command("console", "ls -la").as_deref(),
            Some("ls -la")
        );
    }

    #[test]
    fn a_prompt_marker_is_kept_outside_console_fences() {
        assert_eq!(
            fence_command("bash", "echo \"$ cost\"").as_deref(),
            Some("echo \"$ cost\"")
        );
    }

    #[test]
    fn code_fences_never_run() {
        assert_eq!(fence_command("rust", "fn main() {}"), None);
        assert_eq!(fence_command("json", "{\"a\": 1}"), None);
        assert_eq!(fence_command("ts", "npm install"), None);
        assert_eq!(fence_command("bash", "\n  \n"), None);
        // Untagged prose over several lines is not a script.
        assert_eq!(fence_command("", "first line\nsecond line"), None);
        assert_eq!(fence_command("", "apps/desktop/src/main.rs"), None);
        assert_eq!(
            fence_command("text", "npm run dev").as_deref(),
            Some("npm run dev")
        );
    }

    #[test]
    fn an_href_carries_the_command_both_ways() {
        let command = "cd /tmp/a b && echo \"100%\" | tee (x) ; ls ü";
        let href = command_href(command);
        assert!(href.starts_with(RUN_COMMAND_HREF_PREFIX));
        assert!(!href.contains([' ', '(', ')', '<', '>']));
        assert_eq!(command_from_href(&href).as_deref(), Some(command));
        assert_eq!(command_from_href("https://example.com"), None);
        assert_eq!(command_from_href("ghostex-run:%zz"), None);
    }
}

#[cfg(test)]
mod marking_tests {
    use super::*;
    use crate::transcript::native_markdown::native_markdown;

    /// The command a marked fence line's header carries.
    fn fence_run(line: &str) -> Option<String> {
        let (_, header) = line.split_once('\u{e000}')?;
        let header: serde_json::Value = serde_json::from_str(header).ok()?;
        decode_command(header["run"].as_str()?)
    }

    #[test]
    fn an_inline_command_in_a_reply_becomes_a_run_link_that_keeps_its_code() {
        let reply = "- Void sign-in: Run `cd /Users/madda/dev/_active/maddada-com && npx void auth login` in a normal terminal.";
        let marked = native_markdown(reply, false);
        let command = "cd /Users/madda/dev/_active/maddada-com && npx void auth login";
        assert!(
            marked.contains(&format!("[`{command}`]({})", command_href(command))),
            "{marked}"
        );
        // Ordinary code and a path stay as they were.
        let marked = native_markdown("Call `set_code_wrap(key)` on `ChatAppearance`.", false);
        assert!(!marked.contains(RUN_COMMAND_HREF_PREFIX), "{marked}");
    }

    #[test]
    fn a_typed_prompt_offers_no_command() {
        let marked = native_markdown("please run `npm install` for me", true);
        assert!(!marked.contains(RUN_COMMAND_HREF_PREFIX), "{marked}");
    }

    #[test]
    fn a_finished_shell_fence_carries_its_command_in_the_header() {
        let reply = "Run this:\n\n```\n! cd /Users/madda/dev/_active/maddada-com && npx void auth login\n```\n";
        let marked = native_markdown(reply, false);
        let open = marked.lines().find(|line| line.starts_with("```")).unwrap();
        assert_eq!(
            fence_run(open).as_deref(),
            Some("cd /Users/madda/dev/_active/maddada-com && npx void auth login"),
            "{open}"
        );
        // An untagged fence's header is its only info word, so it holds no space.
        assert!(
            !open.trim_start_matches('`').trim_start().contains(' '),
            "{open}"
        );
        // A list item's fence loses its indent before it is judged.
        let marked = native_markdown("1. Step\n   ```bash\n   cargo check\n   ```\n", false);
        let open = marked.lines().find(|line| line.contains("```")).unwrap();
        assert_eq!(fence_run(open).as_deref(), Some("cargo check"), "{marked}");
        // A quoted command survives the trip.
        let marked = native_markdown("```bash\necho \"hi there\" \\\n```\n", false);
        assert_eq!(
            fence_run(marked.lines().next().unwrap()).as_deref(),
            Some("echo \"hi there\" \\"),
            "{marked}"
        );
        // A named file keeps its label beside the command.
        let marked = native_markdown("```bash scripts/setup.sh\n./setup.sh --all\n```\n", false);
        let open = marked.lines().next().unwrap();
        assert!(open.contains(r#""label":"scripts/setup.sh""#), "{open}");
        assert_eq!(
            fence_run(open).as_deref(),
            Some("./setup.sh --all"),
            "{open}"
        );
    }

    #[test]
    fn a_streaming_or_code_fence_carries_no_command() {
        let marked = native_markdown("```bash\nnpm install\n", false);
        assert!(!marked.contains("\"run\""), "{marked}");
        let marked = native_markdown("```rust\nfn main() {}\n```\n", false);
        assert!(!marked.contains("\"run\""), "{marked}");
    }
}
