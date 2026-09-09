use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::ghostex_cli::{skills, usage};

/// CDXC:AgentSkills 2026-09-09 WHY:
/// The guide chapters are embedded from `skills/ghostex-help/references/` at
/// build time so `ghostex guide` from an installed binary always prints the
/// docs that match its own CLI and settings catalog, even when the copy of the
/// skill folder in the user's global skills directory is stale.
const GUIDE_CHAPTERS: &[(&str, &str, &str)] = &[
    (
        "overview",
        "What Ghostex is, its modes, and how the Help skill works",
        include_str!("../../../skills/ghostex-help/references/overview.md"),
    ),
    (
        "features",
        "Every user-facing feature and where to find it",
        include_str!("../../../skills/ghostex-help/references/features.md"),
    ),
    (
        "settings",
        "Every setting with its key, type, default, and meaning",
        include_str!("../../../skills/ghostex-help/references/settings.md"),
    ),
    (
        "hotkeys",
        "Every hotkey action with its default binding",
        include_str!("../../../skills/ghostex-help/references/hotkeys.md"),
    ),
];

pub fn guide_command(args: &[String]) -> CliResult<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("overview");
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();
    if subcommand == "help" || subcommand == "-h" || subcommand == "--help" {
        println!("{}", usage::guide_usage());
        return Ok(());
    }
    match subcommand {
        "install-skill" => skills::install_help_skill_command(&rest),
        "list" | "chapters" => {
            println!("Ghostex guide chapters (ghostex guide <chapter>):");
            for (name, description, _) in GUIDE_CHAPTERS {
                println!("  {name:<10} {description}");
            }
            Ok(())
        }
        chapter => {
            if rest.iter().any(|arg| arg == "-h" || arg == "--help") {
                println!("{}", usage::guide_usage());
                return Ok(());
            }
            let Some((_, _, content)) = GUIDE_CHAPTERS.iter().find(|(name, _, _)| *name == chapter)
            else {
                let names: Vec<&str> = GUIDE_CHAPTERS.iter().map(|(name, _, _)| *name).collect();
                return Err(CliError::Other(format!(
                    "Unknown guide chapter \"{chapter}\". Chapters: {}. Run `ghostex guide list` for descriptions.",
                    names.join(", ")
                )));
            };
            print!("{content}");
            if !content.ends_with('\n') {
                println!();
            }
            Ok(())
        }
    }
}
