#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
REPO_ROOT="$(release_gpui_repo_root)"
REFERENCES_ROOT="$REPO_ROOT/.dependencies"

reference_url() {
	case "$1" in
	zed) printf '%s\n' "https://github.com/maddada/zed.git" ;;
	cef-rs) printf '%s\n' "https://github.com/tauri-apps/cef-rs.git" ;;
	gpui-component) printf '%s\n' "https://github.com/maddada/gpui-component.git" ;;
	esac
}

reference_revision() {
	case "$1" in
	zed) printf '%s\n' "5775362fbd422f00ef7ca3e7a88b088a65d7c22b" ;;
	cef-rs) printf '%s\n' "0ddbc2accc06a3ac7f18e1543f752c3fb65161f2" ;;
	gpui-component) printf '%s\n' "d34717841a8f52cd7fcf6a27a81e60704f45754c" ;;
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

dependency_git() {
	local destination="$1"
	shift
	git -c "safe.directory=$destination" -C "$destination" "$@"
}

mkdir -p "$REFERENCES_ROOT"
for name in zed cef-rs gpui-component; do
	if [[ "${GHOSTEX_RELEASE_ANDROID_ONLY:-0}" == "1" ]]; then
		break
	fi
	if [[ "${GHOSTEX_RELEASE_SKIP_GPUI_REFERENCES:-0}" == "1" ]]; then
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
		if [[ -n "$(dependency_git "$destination" status --porcelain --untracked-files=all)" ]]; then
			echo "GPUI reference checkout is dirty; refusing a non-reproducible release build: $destination" >&2
			exit 1
		fi
		printf 'Verified clean pinned GPUI reference %s at %s\n' "$name" "$revision"
		continue
	fi
	echo "Dependency submodule is unavailable: $destination" >&2
	exit 1
done

if [[ "${GHOSTEX_RELEASE_SKIP_SUBMODULES:-0}" != "1" ]]; then
	if [[ "${GHOSTEX_RELEASE_ANDROID_ONLY:-0}" == "1" ]]; then
		requested=(mobile)
	else
		requested=(.dependencies/code-server .dependencies/zmx)
	fi
	if [[ "${GHOSTEX_RELEASE_INCLUDE_ANDROID:-0}" == "1" && "${GHOSTEX_RELEASE_ANDROID_ONLY:-0}" != "1" ]]; then
		requested+=(mobile)
	fi
	git -c "safe.directory=$REPO_ROOT" -C "$REPO_ROOT" submodule update --init --depth=1 -- "${requested[@]}"
	if [[ "${GHOSTEX_RELEASE_ANDROID_ONLY:-0}" != "1" ]]; then
		git -C "$REPO_ROOT/.dependencies/code-server" submodule update --init --depth=1 -- lib/vscode
	fi
fi

printf 'Prepared pinned GPUI dependencies under %s\n' "$REFERENCES_ROOT"
