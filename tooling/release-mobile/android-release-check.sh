#!/usr/bin/env bash
# Checks a release APK for the size settings of
# apps/mobile/app/plugins/withAndroidReleaseSize.js without a device:
# R8 ran, native libraries are compressed, and every class that native code
# (JNI, JNA) or reflection reaches by name survived shrinking under its own name.
#
# Usage: android-release-check.sh <apk> <r8-mapping-dir> <build-tools-dir>
set -euo pipefail
# dexdump prints raw UTF-8 from string pools; byte-wise text tools never choke on it.
export LC_ALL=C

APK="${1:?usage: android-release-check.sh <apk> <r8-mapping-dir> <build-tools-dir>}"
MAPPING_DIR="${2:?missing R8 mapping directory}"
BUILD_TOOLS="${3:?missing Android build-tools directory}"
DEXDUMP="$BUILD_TOOLS/dexdump"
[[ -x "$DEXDUMP" ]] || {
	echo "dexdump is missing from $BUILD_TOOLS" >&2
	exit 1
}

fail() {
	echo "Android release check: $*" >&2
	exit 1
}

# R8 ran: it writes the mapping, and it renamed at least one class.
MAPPING="$MAPPING_DIR/mapping.txt"
[[ -s "$MAPPING" ]] || fail "no R8 mapping at $MAPPING (minify did not run)"
grep -Eq '^[^ #][^ ]* -> [^ ]+:$' "$MAPPING" || fail "R8 mapping lists no classes"
awk '/^[^ #]/ && $2 == "->" { sub(":$", "", $3); if ($1 != $3) renamed = 1 } END { exit renamed ? 0 : 1 }' "$MAPPING" \
	|| fail "R8 renamed no class (obfuscation is off)"
if [[ -s "$MAPPING_DIR/missing_rules.txt" ]]; then
	cat "$MAPPING_DIR/missing_rules.txt" >&2
	fail "R8 reported missing classes"
fi

# Native libraries are compressed (useLegacyPackaging), not stored.
STORED_LIBS="$(unzip -v "$APK" 'lib/*' | awk '$8 ~ /^lib\// && $2 == "Stored" { print $8 }')"
[[ -z "$STORED_LIBS" ]] || fail "native libraries are stored uncompressed: $STORED_LIBS"

DEX_DIR="$(mktemp -d)"
trap 'rm -rf "$DEX_DIR"' EXIT
unzip -q -o "$APK" 'classes*.dex' -d "$DEX_DIR"
DUMP="$DEX_DIR/dump.txt"
for dex in "$DEX_DIR"/classes*.dex; do
	"$DEXDUMP" "$dex" >>"$DUMP"
done

# Class descriptor -> methods that must keep their names ("" = class only).
# go.Seq / Tailcatbridge: the gomobile bridge calls them through JNI.
# BouncyCastle: the JCA instantiates provider SPIs from class-name strings.
# SshConfigRecord: Expo Records are filled by reflection.
REQUIRED=(
	"Lgo/Seq;|incRefnum decRef"
	"Ldev/ghostex/tailcatbridge/Tailcatbridge;|"
	"Lorg/bouncycastle/jce/provider/BouncyCastleProvider;|"
	"Lorg/bouncycastle/jcajce/provider/symmetric/AES\$ECB;|"
	"Lnet/schmizz/sshj/SSHClient;|"
	"Lexpo/modules/ghostexnative/SshConfigRecord;|"
)
for entry in "${REQUIRED[@]}"; do
	descriptor="${entry%%|*}"
	methods="${entry#*|}"
	members="$(awk -v want="'$descriptor'" '
		/^  Class descriptor/ { inside = ($4 == want); if (inside) found = 1; next }
		inside && $1 == "name" { gsub("\047", "", $3); print $3 }
		END { if (!found) exit 1 }
	' "$DUMP")" || fail "class $descriptor was removed or renamed by R8"
	for method in $methods; do
		grep -qx "$method" <<<"$members" || fail "$descriptor.$method was removed or renamed by R8"
	done
done

echo "Android release check passed: R8 on, native libraries compressed, ${#REQUIRED[@]} by-name classes intact"
