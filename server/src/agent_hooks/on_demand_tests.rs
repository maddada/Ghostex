use super::*;
use std::fs;

/// Omarchy's `~/.local/bin` launchers, verbatim (read from an Omarchy install, 2026-10-28).
const OMARCHY_CURSOR_AGENT: &str = r#"#!/bin/bash
export MISE_MINIMUM_RELEASE_AGE=0
mise use -g --quiet "cursor-agent" || exit 1
exec mise x "cursor-agent" -- "cursor-agent" "$@"
"#;
const OMARCHY_GROK: &str = r#"#!/bin/bash
export MISE_MINIMUM_RELEASE_AGE=0
mise use -g --quiet "npm:@xai-official/grok" || exit 1
exec mise x "npm:@xai-official/grok" -- "grok" "$@"
"#;
const OMARCHY_HERMES: &str = r#"#!/bin/bash

# Written by omarchy-install-hermes-cli.

export UV_PYTHON="3.13"
export MISE_MINIMUM_RELEASE_AGE=0

if ! [[ -d "$(mise where 'pipx:hermes-agent[extras=all]' 2>/dev/null)/hermes-agent/lib/python3.13" ]]; then
  echo "Installing Hermes on Python 3.13 (this takes a minute)..." >&2
  if omarchy-cmd-missing uv && ! mise where uv >/dev/null 2>&1; then
    mise use -g --quiet uv@latest || exit 1
  fi
  mise use -g --quiet --force 'pipx:hermes-agent[extras=all]' || exit 1
fi

exec env -u UV_PYTHON mise x 'pipx:hermes-agent[extras=all]' -- hermes "$@"
"#;
/// The bin shim mise's npm backend installs for Grok: it runs the installed package, nothing more.
const MISE_NPM_BIN_SHIM: &str = r#"#!/bin/sh
# aube-bin-shim v2 target=../.mise/@xai-official+grok@1.0.70/node_modules/@xai-official/grok/bin/grok
link="$0"
basedir=$(dirname "$link")
export NODE_PATH="$basedir/..:$basedir/../.mise/node_modules"
if [ -x "$basedir/node" ]; then
  exec "$basedir/node" "$basedir/../.mise/@xai-official+grok@1.0.70/node_modules/@xai-official/grok/bin/grok" "$@"
else
  exec node "$basedir/../.mise/@xai-official+grok@1.0.70/node_modules/@xai-official/grok/bin/grok" "$@"
fi
"#;

fn write(dir: &Path, name: &str, contents: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

fn kind(dir: &Path, name: &str, contents: &str) -> Kind {
    classify(&write(dir, name, contents.as_bytes()), dir)
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn omarchy_launchers_are_wrappers() {
    let scratch = tempfile::tempdir().unwrap();
    let bin = scratch.path().join(".local/bin");
    assert_eq!(
        kind(&bin, "cursor-agent", OMARCHY_CURSOR_AGENT),
        Kind::Wrapper
    );
    assert_eq!(kind(&bin, "grok", OMARCHY_GROK), Kind::Wrapper);
    assert_eq!(kind(&bin, "hermes", OMARCHY_HERMES), Kind::Wrapper);
}

#[test]
fn package_runner_shims_are_wrappers() {
    let scratch = tempfile::tempdir().unwrap();
    let dir = scratch.path();
    for (name, script) in [
        (
            "gemini",
            "#!/bin/sh\nexec npx -y @google/gemini-cli \"$@\"\n",
        ),
        (
            "codex",
            "#!/usr/bin/env bash\nbunx @openai/codex@latest \"$@\"\n",
        ),
        (
            "amp",
            "#!/usr/bin/env -S zsh -f\npnpm dlx @sourcegraph/amp \"$@\"\n",
        ),
        ("qwen", "#!/bin/sh\nyarn dlx @qwen-code/qwen-code \"$@\"\n"),
        ("droid", "#!/bin/sh\n/usr/local/bin/bun x droid \"$@\"\n"),
        ("aider", "#!/bin/sh\nexec uvx aider-chat \"$@\"\n"),
        ("crush", "#!/bin/sh\nexec uv tool run crush \"$@\"\n"),
        ("kimi", "#!/bin/sh\nexec pipx run kimi-cli \"$@\"\n"),
        (
            "pi",
            "#!/bin/sh\nnpm exec --yes -- @mariozechner/pi \"$@\"\n",
        ),
        (
            "opencode",
            "#!/bin/sh\nasdf install opencode latest && exec opencode \"$@\"\n",
        ),
    ] {
        assert_eq!(kind(dir, name, script), Kind::Wrapper, "{name}");
    }
    // Windows launchers: a cmd script and a PowerShell script.
    assert_eq!(
        kind(
            dir,
            "gemini.cmd",
            "@echo off\r\nnpx.cmd -y @google/gemini-cli %*\r\n"
        ),
        Kind::Wrapper
    );
    assert_eq!(
        kind(dir, "grok.ps1", "& mise.exe x grok -- grok @args\r\n"),
        Kind::Wrapper
    );
}

#[test]
fn real_installs_are_not_wrappers() {
    let scratch = tempfile::tempdir().unwrap();
    let dir = scratch.path();
    // A native binary (ELF header, then bytes that happen to spell a runner).
    let mut elf = b"\x7fELF\x02\x01\x01\0".to_vec();
    elf.extend_from_slice(b"\0\0 mise use npx bunx \0");
    let binary = write(dir, "claude", &elf);
    assert_eq!(classify(&binary, dir), Kind::Real);
    // Mach-O.
    let macho = write(dir, "cursor-agent", b"\xcf\xfa\xed\xfe\x07\0\0\x01npx");
    assert_eq!(classify(&macho, dir), Kind::Real);
    // npm's global bin: a node script, whatever its text mentions.
    assert_eq!(
        kind(
            dir,
            "codex.js",
            "#!/usr/bin/env node\n// run `npx codex` to try it\nrequire('./bin');\n"
        ),
        Kind::Real
    );
    // mise's npm-backend bin shim and pnpm's global shim run the installed package.
    assert_eq!(kind(dir, "grok", MISE_NPM_BIN_SHIM), Kind::Real);
    assert_eq!(
        kind(
            dir,
            "gemini",
            "#!/bin/sh\nbasedir=$(dirname \"$0\")\nexport NODE_PATH=\"/Users/me/Library/pnpm/global/5/.pnpm/node_modules\"\nexec node \"$basedir/global/5/node_modules/@google/gemini-cli/dist/index.js\" \"$@\"\n"
        ),
        Kind::Real
    );
    // npm's Windows shim.
    assert_eq!(
        kind(
            dir,
            "codex.cmd",
            "@ECHO off\r\nSETLOCAL\r\n\"%_prog%\"  \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n"
        ),
        Kind::Real
    );
    // A comment that names a runner is not a command.
    assert_eq!(
        kind(
            dir,
            "amp",
            "#!/bin/sh\n# Installed with: npx @sourcegraph/amp\nexec /opt/amp/bin/amp \"$@\"\n"
        ),
        Kind::Real
    );
    // `mise where` and `mise which` only read.
    assert_eq!(
        kind(
            dir,
            "opencode",
            "#!/bin/bash\nexec \"$(mise where opencode)/opencode\" \"$@\"\n"
        ),
        Kind::Real
    );
    // A program larger than any launcher.
    let large = format!(
        "#!/bin/bash\n{}\nnpx something\n",
        "# filler\n".repeat(4096)
    );
    assert_eq!(kind(dir, "big", &large), Kind::Real);
    // Missing file: nothing to judge, the caller already found it on PATH.
    assert_eq!(classify(&dir.join("missing"), dir), Kind::Real);
}

#[test]
fn mise_shims_need_an_installed_version() {
    let scratch = tempfile::tempdir().unwrap();
    let home = scratch.path();
    let shim = write(home, ".local/share/mise/shims/cursor-agent", b"#!/bin/sh\n");
    assert_eq!(classify(&shim, home), Kind::MiseShim);
    // mise's own shims are symlinks to the mise binary.
    #[cfg(unix)]
    {
        let mise = write(home, "usr/bin/mise", b"\x7fELF");
        let link = home.join("bin/grok");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&mise, &link).unwrap();
        assert_eq!(classify(&link, home), Kind::MiseShim);
    }
    let installed = write(
        home,
        ".local/share/mise/installs/cursor-agent/1.0/cursor-agent",
        b"\x7fELF",
    );
    // Not installed yet: no probe.
    assert_eq!(probe_target_with(&text(&shim), home, || None), None);
    // Installed: the shim runs as today.
    assert_eq!(
        probe_target_with(&text(&shim), home, || Some(text(&installed))),
        Some(text(&shim))
    );
}

#[test]
fn wrappers_probe_only_the_real_install() {
    let scratch = tempfile::tempdir().unwrap();
    let home = scratch.path();
    let wrapper = write(
        home,
        ".local/bin/cursor-agent",
        OMARCHY_CURSOR_AGENT.as_bytes(),
    );
    // Before the user starts it: not installed, nothing runs.
    assert_eq!(probe_target_with(&text(&wrapper), home, || None), None);
    // After the wrapper installed it, probes run mise's install, never the wrapper again.
    let installed = write(
        home,
        ".local/share/mise/installs/cursor-agent/2026.10.1/cursor-agent",
        b"\x7fELF",
    );
    assert_eq!(
        probe_target_with(&text(&wrapper), home, || Some(text(&installed))),
        Some(text(&installed))
    );
    // A lookup that answers with the wrapper itself, or another wrapper, is no install.
    assert_eq!(
        probe_target_with(&text(&wrapper), home, || Some(text(&wrapper))),
        None
    );
    let other = write(
        home,
        "other/cursor-agent",
        b"#!/bin/sh\nexec npx cursor-agent \"$@\"\n",
    );
    assert_eq!(
        probe_target_with(&text(&wrapper), home, || Some(text(&other))),
        None
    );
    // A real binary never asks mise.
    let real = write(home, "bin/codex", b"\x7fELF");
    assert_eq!(
        probe_target_with(&text(&real), home, || panic!(
            "asked mise for a real binary"
        )),
        Some(text(&real))
    );
}

#[test]
fn asdf_shims_need_an_installed_version() {
    let scratch = tempfile::tempdir().unwrap();
    let home = scratch.path();
    let shim =
        "#!/usr/bin/env bash\n# asdf-plugin: gemini-cli 0.9.0\nexec asdf exec \"gemini\" \"$@\"\n";
    assert_eq!(
        kind(&home.join(".asdf/shims"), "gemini", shim),
        Kind::AsdfShimMissing
    );
    fs::create_dir_all(home.join(".asdf/installs/gemini-cli/0.9.0")).unwrap();
    assert_eq!(classify(&home.join(".asdf/shims/gemini"), home), Kind::Real);
}

#[test]
fn runner_words_inside_other_words_do_not_count() {
    assert!(!installs_on_demand("exec /opt/npx-free/bin/agent \"$@\"\n"));
    assert!(!installs_on_demand("export MISE_USE=1\nexec agent\n"));
    assert!(!installs_on_demand("mise which agent\n"));
    assert!(installs_on_demand("mise --quiet x agent -- agent\n"));
    assert!(installs_on_demand("rtx exec agent -- agent\n"));
}

#[test]
fn probing_a_wrapper_never_runs_it() {
    let scratch = tempfile::tempdir().unwrap();
    let home = scratch.path();
    let marker = home.join("installed");
    let binary = "ghostex-on-demand-test-agent";
    let wrapper = write(
        home,
        &format!(".local/bin/{binary}"),
        format!(
            "#!/bin/bash\ntouch '{}'\nmise use -g --quiet \"{binary}\" || exit 1\nexec mise x \"{binary}\" -- \"{binary}\" \"$@\"\n",
            marker.display()
        )
        .as_bytes(),
    );
    // The real lookup: `mise which` (when mise is on this computer) knows no such tool.
    assert_eq!(probe_target(&text(&wrapper), binary, home), None);
    assert!(!marker.exists(), "the wrapper ran");
}
