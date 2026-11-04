#!/usr/bin/env bash
# Rebuilds the appimage_signature.rs test fixture: a tiny ELF64 holding the two AppImage signature
# sections, signed the way tooling/release-gpui/linux-appimage.sh signs a release, but with a
# throwaway test key made here (never the Ghostex signing key).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'gpgconf --homedir "$WORK/gpg" --kill gpg-agent 2>/dev/null || true; rm -rf "$WORK"' EXIT
mkdir -m 700 "$WORK/gpg"
export GNUPGHOME="$WORK/gpg"
gpg --batch --quiet --pinentry-mode loopback --passphrase '' \
	--quick-generate-key "Ghostex AppImage signature test key" rsa2048 sign never
FPR="$(gpg --batch --with-colons --list-secret-keys | awk -F: '$1 == "fpr" { print $10; exit }')"
gpg --batch --armor --export "$FPR" >"$HERE/test-key.asc"
printf '%s\n' "$FPR" >"$HERE/test-key.fpr"
python3 - "$WORK/unsigned.elf" <<'PY'
import struct, sys
names = b"\0.shstrtab\0.sha256_sig\0.sig_key\0.payload\0"
def name(n): return names.index(n.encode())
payload = b"pretend squashfs payload\n" * 8
sig_size, key_size = 1024, 8192
body = names + bytes(sig_size) + bytes(key_size) + payload
names_off = 64
sig_off = names_off + len(names)
key_off = sig_off + sig_size
payload_off = key_off + key_size
shoff = 64 + len(body)
sections = [
    (0, 0, 0, 0),
    (name(".shstrtab"), 3, names_off, len(names)),
    (name(".sha256_sig"), 1, sig_off, sig_size),
    (name(".sig_key"), 1, key_off, key_size),
    (name(".payload"), 1, payload_off, len(payload)),
]
header = b"\x7fELF" + bytes([2, 1, 1, 0]) + bytes(8)
header += struct.pack("<HHIQQQIHHHHHH", 2, 62, 1, 0, 0, shoff, 0, 64, 0, 0, 64, len(sections), 1)
table = b"".join(struct.pack("<IIQQQQIIQQ", n, t, 0, 0, off, size, 0, 0, 1, 0) for n, t, off, size in sections)
open(sys.argv[1], "wb").write(header + body + table)
PY
digest() { node "$HERE/../../../../tooling/release-gpui/appimage-signature.mjs" digest "$1"; }
digest "$WORK/unsigned.elf" >"$WORK/digest.txt"
gpg --batch --yes --local-user "$FPR!" --digest-algo SHA256 --armor --textmode \
	--detach-sign --output "$WORK/signature.asc" "$WORK/digest.txt"
cp "$WORK/unsigned.elf" "$HERE/signed.elf"
node "$HERE/../../../../tooling/release-gpui/appimage-signature.mjs" embed "$HERE/signed.elf" "$WORK/signature.asc" "$HERE/test-key.asc"
digest "$HERE/signed.elf" >"$HERE/signed-digest.txt"
cmp -s "$WORK/digest.txt" "$HERE/signed-digest.txt"
gpg --batch --verify "$WORK/signature.asc" "$HERE/signed-digest.txt"
