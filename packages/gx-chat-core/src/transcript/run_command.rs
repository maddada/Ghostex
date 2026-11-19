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
/// The scheme of a span that is a command only by its shape (`CommandKind::Shape`): drawn with the
/// same Copy and Paste in terminal, but never counted as the reply's one command for a Run
/// (the phone's message menu, `runCommands.ts`). A renderer that does not know it shows a plain
/// link and never a command it would type.
pub const SHAPE_COMMAND_HREF_PREFIX: &str = "ghostex-run-shape:";

/// Why a span counts as a command: a known program (or a span the author marked with `! ` or
/// `$ `), or only the shape of its command line (an unknown program with a flag or a path).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Known,
    Shape,
}

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
    // Shells, run on a script or with `-c`.
    "bash",
    "sh",
    "zsh",
    "pwsh",
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
    // An untagged fence of one line is the inline rule's to judge, and runs (Enter included) only
    // for a program it knows: a command by its shape alone is left for the reader to type.
    if body.lines().count() == 1 {
        return inline_command_kind(body)
            .filter(|(_, kind)| *kind == CommandKind::Known)
            .map(|(command, _)| command);
    }
    None
}

/// The command an inline code span runs, when the span holds one.
pub fn inline_command(code: &str) -> Option<String> {
    inline_command_kind(code).map(|(command, _)| command)
}

/// The command an inline code span runs, and why it counts.
pub fn inline_command_kind(code: &str) -> Option<(String, CommandKind)> {
    let code = code.trim();
    if code.is_empty() || code.contains(['\n', '\r']) {
        return None;
    }
    if let Some(rest) = code.strip_prefix("! ").or_else(|| code.strip_prefix("$ ")) {
        let rest = rest.trim();
        return (!rest.is_empty()).then(|| (rest.to_owned(), CommandKind::Known));
    }
    command_line_kind(code).map(|kind| (code.to_owned(), kind))
}

/// The link an inline code span holding a command becomes, under the scheme its kind takes.
pub fn inline_command_href(code: &str) -> Option<String> {
    let (command, kind) = inline_command_kind(code)?;
    Some(match kind {
        CommandKind::Known => command_href(&command),
        CommandKind::Shape => format!("{SHAPE_COMMAND_HREF_PREFIX}{}", encode_command(&command)),
    })
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
    decode_command(encoded_command(href)?).filter(|command| !command.chars().any(char::is_control))
}

/// The percent-encoded command a run link carries, under either scheme.
pub fn encoded_command(href: &str) -> Option<&str> {
    href.strip_prefix(RUN_COMMAND_HREF_PREFIX)
        .or_else(|| href.strip_prefix(SHAPE_COMMAND_HREF_PREFIX))
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
/// or a `&&` chain whose every step starts with a command name (`Known`), or an unknown program
/// written as a command line (`Shape`, `is_unknown_program_line`).
fn command_line_kind(code: &str) -> Option<CommandKind> {
    if is_command_line(code) {
        return Some(CommandKind::Known);
    }
    let words: Vec<&str> = code.split_whitespace().collect();
    let start = words.iter().position(|word| !is_assignment(word))?;
    let words = &words[start..];
    let generic = code.as_bytes().windows(2).any(|pair| {
        matches!(pair[1], b'<' | b'>') && !matches!(pair[0], b' ' | b'>' | b'2' | b'&')
    });
    if code.contains(['(', ')', '{', '}']) || generic || COMMAND_WORDS.contains(&words[0]) {
        return None;
    }
    is_unknown_program_line(words[0], &words[1..]).then_some(CommandKind::Shape)
}

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

/// A program the list above does not know, written the way a command line is: a command name
/// followed by a flag (`-config`, `--help`) or a path from the root or home (`/etc/x`,
/// `~/.config/x`), or a path to an executable (`~/.local/opt/hub/cli-proxy-api`, `./run.sh`)
/// followed by a flag or such a path (`./run.sh --fast`), or a script run from here followed by
/// anything (`./run.sh build`).
/// Plain words, one name, a path alone, a slash command (`/model opus`), a file (`main.rs --help`),
/// a path with spaces in it, code and prose stay code.
///
/// CDXC:SessionChat 2026-11-19 DECISION:
/// User, of an ask whose `cli-proxy-api -config ~/.config/cliproxyapi/config.yaml -claude-login`
/// had no Copy or Paste in terminal: "why dont we show the copy and the send to terminal buttons on
/// these commands here". A command line is recognised by its shape as well as by the known
/// programs, wherever the chat offers command buttons (replies, asks, the floating ask panel). Such
/// a span gets Copy and Paste in terminal only: it never makes a fence's Run, and the phone's
/// message menu does not count it as the reply's one command (`SHAPE_COMMAND_HREF_PREFIX`).
fn is_unknown_program_line(first: &str, arguments: &[&str]) -> bool {
    if arguments.is_empty() {
        return false;
    }
    // `-config`, `--help`; not `-1` or a class name's `-top-2`, `-mt-4`.
    let is_flag = |word: &str| {
        let name = word.trim_start_matches('-');
        word.starts_with('-')
            && word.len() - name.len() <= 2
            && name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && !name
                .rsplit('-')
                .next()
                .is_some_and(|last| last.bytes().all(|byte| byte.is_ascii_digit()))
    };
    let is_rooted_path = |word: &str| {
        let word = word.trim_matches(['"', '\'']);
        word == "~"
            || word.strip_prefix("~/").is_some_and(|rest| !rest.is_empty())
            || word
                .strip_prefix('/')
                .is_some_and(|rest| !rest.is_empty() && !rest.starts_with('/'))
    };
    let fits = |word: &&str| is_flag(word) || is_rooted_path(word);
    if let Some(start) = ["./", "../", "~/", "/"]
        .into_iter()
        .find(|start| first.starts_with(start))
    {
        // From the root, a second part at least: `/model opus` is a slash command.
        if start == "/" && !first[1..].contains('/') {
            return false;
        }
        // A later word with a `/` that is neither a flag nor a rooted path is the rest of a path
        // with spaces in it (`~/Documents/my notes/todo.md`).
        if arguments
            .iter()
            .any(|word| word.contains('/') && !fits(word))
        {
            return false;
        }
        // Its last part a command name, a script's extension at most.
        let name = first.rsplit('/').next().unwrap_or_default();
        let script = name.rsplit_once('.').is_none_or(|(_, extension)| {
            matches!(
                extension,
                "sh" | "bash" | "zsh" | "py" | "js" | "mjs" | "ts" | "rb" | "pl" | "ps1"
            )
        });
        // A flag or a rooted path must follow (`/etc/hosts file`, `/Users/me/my project`,
        // `./build output` are paths and words), except after a script run from here, which takes
        // any argument (`./run.sh build`).
        let script_here = start.starts_with('.') && name.contains('.');
        let arguments_fit = script_here || arguments.iter().any(fits);
        return name.len() >= 2 && is_command_name(name) && script && arguments_fit;
    }
    first.len() >= 2
        && is_command_name(first)
        && !first.contains(['/', '.'])
        && !NOT_PROGRAMS.contains(&first)
        && arguments.iter().any(fits)
}

/// First words that are a language's keyword or an English word, never a program an agent asks
/// the user to run (`import x from ~/y`, `see /etc/hosts`).
const NOT_PROGRAMS: &[&str] = &[
    "a", "an", "and", "as", "at", "by", "class", "const", "def", "else", "fetch", "fn", "for",
    "from", "function", "if", "import", "in", "include", "is", "let", "mod", "not", "of", "on",
    "or", "pub", "relative", "absolute", "require", "return", "root", "see", "the", "then", "to",
    "type", "use", "var", "when", "where", "with",
];

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
            // Programs it does not know, by their command line's shape.
            (
                "cli-proxy-api -config ~/.config/cliproxyapi/config.yaml -claude-login",
                "cli-proxy-api -config ~/.config/cliproxyapi/config.yaml -claude-login",
            ),
            ("mytool --verbose", "mytool --verbose"),
            ("hub-agent ~/hub.yaml", "hub-agent ~/hub.yaml"),
            (
                "~/.local/opt/cliproxyapi/8.0.78/cli-proxy-api -config ~/c.yaml",
                "~/.local/opt/cliproxyapi/8.0.78/cli-proxy-api -config ~/c.yaml",
            ),
            (
                "cli-proxy-api -config ~/.config/cliproxyapi/config.yaml -codex-device-login",
                "cli-proxy-api -config ~/.config/cliproxyapi/config.yaml -codex-device-login",
            ),
            ("./run.sh --fast", "./run.sh --fast"),
            ("./run.sh build", "./run.sh build"),
            ("../scripts/setup.ps1 dev", "../scripts/setup.ps1 dev"),
            ("../bin/hub -config ~/hub.yaml", "../bin/hub -config ~/hub.yaml"),
            ("bash ./install.sh", "bash ./install.sh"),
            ("sh -c 'echo hi'", "sh -c 'echo hi'"),
            ("zsh ~/setup.zsh", "zsh ~/setup.zsh"),
            ("pwsh -File ./setup.ps1", "pwsh -File ./setup.ps1"),
            ("/usr/local/bin/hub --serve", "/usr/local/bin/hub --serve"),
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
            // A program it does not know needs a command line's shape: a path alone, plain
            // words, a name and a number, a relative path or a file are not one.
            "~/.local/opt/cliproxyapi/8.0.78/cli-proxy-api",
            "./run.sh",
            "mytool build",
            "fix the bug",
            "count -1",
            "a - b",
            "see src/main.rs",
            "src/main.rs --help",
            "~/notes/todo.md later",
            "Option -x",
            // Slash commands and one-part routes.
            "/model opus",
            "/loop 5m /babysit-prs",
            "/code-review ultra",
            "/api/users GET",
            // Paths with spaces in them.
            "~/Documents/my notes/todo.md",
            "/Users/madda/dev/my project",
            "/etc/hosts file",
            // Keywords, English words and files first.
            "import x from ~/y",
            "from ~/lib import x",
            "return -value",
            "require ~/x",
            "use ~/crate",
            "include /usr/include/x.h",
            "the -flag",
            "see /etc/hosts",
            "fetch /api/x",
            "relative ~/x",
            "main.rs --help",
            // From here, a word after a path is not enough.
            "./build output",
            "./notes.md build",
            // Class names, not flags.
            "card -top-2",
            "box -mt-4",
        ] {
            assert_eq!(inline_command(span), None, "{span}");
        }
    }

    #[test]
    fn a_command_by_its_shape_only_pastes() {
        let shape = "cli-proxy-api -config ~/.config/cliproxyapi/config.yaml -claude-login";
        assert_eq!(
            inline_command_kind(shape).map(|(_, kind)| kind),
            Some(CommandKind::Shape)
        );
        assert_eq!(
            inline_command_kind("npm install").map(|(_, kind)| kind),
            Some(CommandKind::Known)
        );
        // Its link says so, and still carries the command.
        let href = inline_command_href(shape).unwrap();
        assert!(href.starts_with(SHAPE_COMMAND_HREF_PREFIX), "{href}");
        assert_eq!(command_from_href(&href).as_deref(), Some(shape));
        assert!(inline_command_href("npm install")
            .unwrap()
            .starts_with(RUN_COMMAND_HREF_PREFIX));
        // An untagged one-line fence runs only a program it knows.
        assert_eq!(fence_command("", shape), None);
        assert_eq!(fence_command("", "./run.sh build"), None);
        assert_eq!(
            fence_command("", "npm install").as_deref(),
            Some("npm install")
        );
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
