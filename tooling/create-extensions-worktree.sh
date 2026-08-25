#!/usr/bin/env bash
# create-extensions-worktree.sh — create a LIGHT Ghostex worktree dedicated to
# the extensions wiring work (new root `extensions/` folder backed by the
# public Ghostex-extensions repo).
#
# "Light" means the worktree checks out app source plus the small vendored
# trees (~100 MB) and symlinks every heavy or generated tree back into this
# main checkout instead of duplicating it (~150 GB saved):
#   - the big submodules under `.dependencies/` (cef-rs, code-server,
#     gpui-component, zed, zehn, zmx) — symlinked + `--skip-worktree` so git
#     status stays clean; vendored ghostty/ghostty-patches check out normally
#   - ghostty's gitignored build caches (zig-out, .zig-cache, zig-pkg)
#   - `node_modules/`, `apps/desktop/node_modules/` (bun install output)
#   - every Cargo `target/` dir (apps/desktop 76 GB, server 42 GB, …)
#   - `apps/desktop/runtime|build|dist` (generated native/web staging)
#   - `docs/` (local-only planning notes, gitignored in the main checkout)
# The `apps/mobile/app` submodule is left uninitialized (empty dir) — the
# extensions work never touches mobile.
#
# Because gitignore directory patterns (`build/`) do not match symlinks, the
# created symlinks are ignored via the shared `.git/info/exclude` (guarded,
# appended once). The `extensions/` folder inside the worktree is a symlink to
# a sibling checkout of the Ghostex-extensions repo (cloned or initialized on
# first run), so it survives worktree deletion.
#
# Usage:
#   tooling/create-extensions-worktree.sh [worktree-path] [branch]
# Defaults:
#   worktree-path  ~/dev/_worktrees/ghostex-extensions
#   branch         feat/extensions   (created from main if missing)
# Env overrides:
#   GHOSTEX_EXTENSIONS_DIR     sibling checkout path
#                              (default ~/dev/_active/Ghostex-extensions)
#   GHOSTEX_EXTENSIONS_REMOTE  clone URL for the extensions repo
#                              (default https://github.com/maddada/Ghostex-extensions.git)

set -euo pipefail

MAIN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WT_PATH="${1:-$HOME/dev/_worktrees/ghostex-extensions}"
BRANCH="${2:-feat/extensions}"
EXT_DIR="${GHOSTEX_EXTENSIONS_DIR:-$HOME/dev/_active/Ghostex-extensions}"
EXT_REMOTE="${GHOSTEX_EXTENSIONS_REMOTE:-https://github.com/maddada/Ghostex-extensions.git}"

say() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }

if [ -e "$WT_PATH" ]; then
  echo "error: $WT_PATH already exists — remove it first (git worktree remove \"$WT_PATH\")" >&2
  exit 1
fi

# --- 1. Worktree on its own branch (never touches the main checkout's branch) ---
say "Creating worktree at $WT_PATH on branch $BRANCH"
mkdir -p "$(dirname "$WT_PATH")"
if git -C "$MAIN_ROOT" show-ref --verify --quiet "refs/heads/$BRANCH"; then
  git -C "$MAIN_ROOT" worktree add "$WT_PATH" "$BRANCH"
else
  git -C "$MAIN_ROOT" worktree add -b "$BRANCH" "$WT_PATH" main
fi

# --- 2. Replace the big .dependencies submodules with symlinks ---
# The worktree never initializes these submodules, so their dirs are empty;
# swap each for a symlink into the main checkout and mark the gitlink
# --skip-worktree so status/diff never report the symlink as a typechange.
DEP_SUBMODULES=(cef-rs code-server gpui-component zed zehn zmx)
for sm in "${DEP_SUBMODULES[@]}"; do
  src="$MAIN_ROOT/.dependencies/$sm" dst="$WT_PATH/.dependencies/$sm"
  [ -d "$src" ] || { say "skip .dependencies/$sm (missing in main checkout)"; continue; }
  rmdir "$dst" 2>/dev/null || { say "skip .dependencies/$sm (worktree dir not empty)"; continue; }
  ln -s "$src" "$dst"
  git -C "$WT_PATH" update-index --skip-worktree ".dependencies/$sm"
  say "linked .dependencies/$sm"
done
# Belt and braces: never surface submodule state in this worktree's status.
git -C "$MAIN_ROOT" config extensions.worktreeConfig true
git -C "$WT_PATH" config --worktree diff.ignoreSubmodules all
git -C "$WT_PATH" config --worktree status.submoduleSummary false

# --- 3. Symlink heavy/generated (gitignored) trees back into the main checkout ---
link() { # link <repo-relative-path>  (target must be gitignored or excluded)
  local rel="$1" src="$MAIN_ROOT/$1" dst="$WT_PATH/$1"
  [ -e "$src" ] || { say "skip $rel (missing in main checkout)"; return 0; }
  [ -e "$dst" ] && rm -rf "$dst"
  mkdir -p "$(dirname "$dst")"
  ln -s "$src" "$dst"
  say "linked $rel"
}

link node_modules
link docs
link apps/desktop/node_modules
link apps/desktop/target
link apps/desktop/runtime
link apps/desktop/build
link apps/desktop/dist
link server/target
link apps/history-cli/target
link packages/find/target
link packages/paths/target
link .dependencies/ghostty/zig-out
link .dependencies/ghostty/.zig-cache
link .dependencies/ghostty/zig-pkg

# --- 4. extensions/ -> sibling Ghostex-extensions checkout ---
if [ ! -d "$EXT_DIR/.git" ]; then
  say "Setting up Ghostex-extensions checkout at $EXT_DIR"
  if git ls-remote "$EXT_REMOTE" >/dev/null 2>&1; then
    git clone "$EXT_REMOTE" "$EXT_DIR"
  else
    say "Remote $EXT_REMOTE not reachable — initializing a fresh local repo"
    mkdir -p "$EXT_DIR"
    git -C "$EXT_DIR" init -b main
    git -C "$EXT_DIR" remote add origin "$EXT_REMOTE"
  fi
fi
ln -s "$EXT_DIR" "$WT_PATH/extensions"
say "linked extensions -> $EXT_DIR"

# --- 5. Ignore the symlinks (dir-only gitignore patterns miss symlinks) ---
EXCLUDE_FILE="$MAIN_ROOT/.git/info/exclude"
MARKER="# ghostex-light-worktree symlinks"
if ! grep -qF "$MARKER" "$EXCLUDE_FILE" 2>/dev/null; then
  cat >>"$EXCLUDE_FILE" <<EOF
$MARKER (added by tooling/create-extensions-worktree.sh)
/extensions
/docs
/node_modules
/apps/desktop/node_modules
/apps/desktop/build
/apps/desktop/dist
/apps/desktop/runtime
/apps/desktop/target
/apps/history-cli/target
/packages/find/target
/packages/paths/target
/server/target
/.dependencies/ghostty/zig-out
/.dependencies/ghostty/.zig-cache
/.dependencies/ghostty/zig-pkg
EOF
  say "appended symlink excludes to .git/info/exclude"
fi

# --- 6. Report ---
say "Worktree ready — git status should be empty:"
git -C "$WT_PATH" status --short | head -20
cat <<EOF

  Worktree:    $WT_PATH  (branch $BRANCH)
  Extensions:  $WT_PATH/extensions -> $EXT_DIR
  Remove with: git worktree remove --force "$WT_PATH"

  Notes:
  - bun/cargo builds reuse the main checkout's node_modules and target dirs.
    Avoid building the same crate in both checkouts at the same time.
  - The big .dependencies submodules are shared with the main checkout via
    symlinks; do not edit them from here (zmx edits belong in the main tree).
  - apps/mobile/app stays uninitialized here (mobile is out of scope).
EOF
