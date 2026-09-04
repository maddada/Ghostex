# shellcheck shell=bash
# Content-hash build stamps shared by prepare-macos-runtime.sh and
# build-macos-app.sh. The sourcing script must define REPO_ROOT and
# BUILD_CACHE_DIR before sourcing this file.

# CDXC:Build 2026-06-07-16:23: Local starts should rebuild expensive bundled resources only when their runtime inputs change. Store content-hash stamps under build/<arch> so repeated `bun run start` calls do not churn source files or rely on generated folders that may be deleted by other build steps.
fingerprint_inputs() {
	"${GXSERVER_NODE_BIN:-node}" "$REPO_ROOT/tooling/fingerprint-build-inputs.mjs" "$@"
}

cache_stamp_path() {
	printf '%s/%s.sha256\n' "$BUILD_CACHE_DIR" "$1"
}

cache_matches() {
	local key="$1"
	local digest="$2"
	shift 2
	local stamp
	stamp="$(cache_stamp_path "$key")"
	if [[ ! -f "$stamp" || "$(<"$stamp")" != "$digest" ]]; then
		return 1
	fi
	local output_path
	for output_path in "$@"; do
		if [[ ! -e "$output_path" ]]; then
			return 1
		fi
	done
	return 0
}

write_cache_stamp() {
	local key="$1"
	local digest="$2"
	mkdir -p "$BUILD_CACHE_DIR"
	printf '%s\n' "$digest" >"$(cache_stamp_path "$key")"
}

path_identity() {
	local candidate="$1"
	if [[ -e "$candidate" ]]; then
		stat -f '%m:%z:%N' "$candidate"
	else
		printf 'missing:%s\n' "$candidate"
	fi
}
