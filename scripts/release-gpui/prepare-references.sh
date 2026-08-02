#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
REFERENCES_ROOT="$REPO_ROOT/.dependencies"
GPUI_COMPONENT_PATCH="$SCRIPT_DIR/patches/gpui-component-managed-tooltip-placement.patch"
GPUI_COMPONENT_SCROLLBAR_PATCH="$SCRIPT_DIR/patches/gpui-component-scrollbar-options.patch"
ZED_WINDOWS_CHILD_KEY_PATCH="$SCRIPT_DIR/patches/zed-windows-native-child-key-dispatch.patch"

reference_url() {
  case "$1" in
    zed) printf '%s\n' "https://github.com/zed-industries/zed.git" ;;
    cef-rs) printf '%s\n' "https://github.com/tauri-apps/cef-rs.git" ;;
    gpui-component) printf '%s\n' "https://github.com/longbridge/gpui-component.git" ;;
    beads) printf '%s\n' "https://github.com/steveyegge/beads.git" ;;
  esac
}

reference_revision() {
  case "$1" in
    zed) printf '%s\n' "1a246efd7e1b83ab568ec5e3e6c1a43a42e1abba" ;;
    cef-rs) printf '%s\n' "0ddbc2accc06a3ac7f18e1543f752c3fb65161f2" ;;
    gpui-component) printf '%s\n' "bc174a7ec4534b2a4174fddde314b38d30d69093" ;;
    beads) printf '%s\n' "672d942083a1fd0c8603fa1e77620c58ba9d47c8" ;;
  esac
}

if [[ "${1:-}" == "--reference-metadata" ]]; then
  name="${2:-}"
  url="$(reference_url "$name")"
  revision="$(reference_revision "$name")"
  if [[ -z "$url" || -z "$revision" ]]; then
    echo "Unknown release reference: ${name:-<missing>}" >&2
    exit 1
  fi
  printf '%s\t%s\n' "$url" "$revision"
  exit 0
fi

has_only_expected_changes() {
  local destination="$1"
  shift
  local expected actual untracked
  expected="$(printf '%s\n' "$@" | LC_ALL=C sort)"
  actual="$(dependency_git "$destination" diff --name-only --no-ext-diff | LC_ALL=C sort)"
  untracked="$(dependency_git "$destination" ls-files --others --exclude-standard)"
  [[ "$actual" == "$expected" && -z "$untracked" ]]
}

dependency_git() {
  local destination="$1"
  shift
  git -c "safe.directory=$destination" -C "$destination" "$@"
}

mkdir -p "$REFERENCES_ROOT"
for name in zed cef-rs gpui-component beads; do
  if [[ "${GHOSTEX_RELEASE_ANDROID_ONLY:-0}" == "1" ]]; then
    break
  fi
  if [[ "${GHOSTEX_RELEASE_SKIP_GPUI_REFERENCES:-0}" == "1" && "$name" != "beads" ]]; then
    continue
  fi
  destination="$REFERENCES_ROOT/$name"
  revision="$(reference_revision "$name")"
  if [[ ! -e "$destination/.git" ]]; then
    git -c "safe.directory=$REPO_ROOT" -C "$REPO_ROOT" submodule update --init --depth=1 -- ".dependencies/$name"
  fi
  if dependency_git "$destination" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    current="$(dependency_git "$destination" rev-parse HEAD)"
    if [[ "$current" != "$revision" ]]; then
      cat >&2 <<EOF
GPUI reference $destination is at $current, expected $revision.
Refusing to alter an existing checkout because it may contain user work.
Use a clean CI checkout or update this reference manually.
EOF
      exit 1
    fi
    if [[ "$name" == "gpui-component" ]] && cmp -s \
      <(dependency_git "$destination" diff --no-ext-diff --binary --abbrev=7 -- crates/ui/src/tooltip.rs) \
      "$GPUI_COMPONENT_PATCH" && cmp -s \
      <(dependency_git "$destination" diff --no-ext-diff --binary --abbrev=7 -- \
        crates/ui/src/menu/popup_menu.rs crates/ui/src/scroll/scrollbar.rs) \
      "$GPUI_COMPONENT_SCROLLBAR_PATCH" && has_only_expected_changes "$destination" \
        crates/ui/src/menu/popup_menu.rs \
        crates/ui/src/scroll/scrollbar.rs \
        crates/ui/src/tooltip.rs; then
      printf 'Verified Ghostex gpui-component patch in %s\n' "$destination"
      continue
    fi
    if [[ "$name" == "zed" ]] && cmp -s \
      <(dependency_git "$destination" diff --no-ext-diff --abbrev=10 -- crates/gpui_windows/src/platform.rs) \
      "$ZED_WINDOWS_CHILD_KEY_PATCH" && has_only_expected_changes "$destination" \
        crates/gpui_windows/src/platform.rs; then
      printf 'Verified Ghostex Zed Windows child-key patch in %s\n' "$destination"
      continue
    fi
    if [[ -n "$(dependency_git "$destination" status --porcelain --untracked-files=all)" ]]; then
      echo "GPUI reference checkout is dirty; refusing a non-reproducible release build: $destination" >&2
      exit 1
    fi
    if [[ "$name" == "gpui-component" ]]; then
      dependency_git "$destination" apply --check "$GPUI_COMPONENT_PATCH"
      dependency_git "$destination" apply "$GPUI_COMPONENT_PATCH"
      dependency_git "$destination" apply --check "$GPUI_COMPONENT_SCROLLBAR_PATCH"
      dependency_git "$destination" apply "$GPUI_COMPONENT_SCROLLBAR_PATCH"
      printf 'Applied Ghostex gpui-component patch in %s\n' "$destination"
    elif [[ "$name" == "zed" ]]; then
      dependency_git "$destination" apply --check "$ZED_WINDOWS_CHILD_KEY_PATCH"
      dependency_git "$destination" apply "$ZED_WINDOWS_CHILD_KEY_PATCH"
      printf 'Applied Ghostex Zed Windows child-key patch in %s\n' "$destination"
    fi
    continue
  fi
  echo "Dependency submodule is unavailable: $destination" >&2
  exit 1
done

if [[ "${GHOSTEX_RELEASE_SKIP_SUBMODULES:-0}" != "1" ]]; then
  if [[ "${GHOSTEX_RELEASE_ANDROID_ONLY:-0}" == "1" ]]; then
    requested=(mobile)
  else
    requested=(code-server t3code zehn zmx)
  fi
  if [[ "${GHOSTEX_RELEASE_INCLUDE_ANDROID:-0}" == "1" && "${GHOSTEX_RELEASE_ANDROID_ONLY:-0}" != "1" ]]; then
    requested+=(mobile)
  fi
  git -c "safe.directory=$REPO_ROOT" -C "$REPO_ROOT" submodule update --init --depth=1 -- "${requested[@]}"
  if [[ "${GHOSTEX_RELEASE_ANDROID_ONLY:-0}" != "1" ]]; then
    git -C "$REPO_ROOT/code-server" submodule update --init --depth=1 -- lib/vscode
  fi
fi

printf 'Prepared pinned GPUI dependencies under %s\n' "$REFERENCES_ROOT"
