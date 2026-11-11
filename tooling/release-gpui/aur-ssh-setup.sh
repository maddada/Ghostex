# shellcheck shell=bash
# Sourced (never executed) by the AUR steps of .github/workflows/release-gpui-publish.yml and by
# .github/workflows/release-aur-dry-run.yml, so the dry run proves the very SSH setup a release uses.
# Reads AUR_SSH_PRIVATE_KEY, installs it as ~/.ssh/aur, pins the aur.archlinux.org host keys, and
# exports GIT_SSH_COMMAND. It never prints the private key: only its public fingerprint.
#
# CDXC:Release 2026-11-10 WHY: a key pasted into the secret from a Windows clipboard carries CR line
# endings, which ssh rejects as "invalid format" and the AUR step would report as a refused push, so
# they are stripped, and an unusable key (bad format, a passphrase) fails here with that reason.

install -d -m 700 "$HOME/.ssh"
printf '%s\n' "${AUR_SSH_PRIVATE_KEY}" | tr -d '\r' >"$HOME/.ssh/aur"
chmod 600 "$HOME/.ssh/aur"
if ! ssh-keygen -y -P '' -f "$HOME/.ssh/aur" >"$HOME/.ssh/aur.pub" 2>/dev/null; then
  echo "AUR_SSH_PRIVATE_KEY is not a usable OpenSSH private key (wrong format, or it has a passphrase)." >&2
  return 1
fi
echo "AUR key: $(ssh-keygen -l -f "$HOME/.ssh/aur.pub")"
# Pinned rather than ssh-keyscan'd: a blind keyscan trusts whatever answers on port 22 the first
# time. These are the aur.archlinux.org host keys published in archlinux/infrastructure
# docs/ssh-hostkeys.txt (checked against the live server on 2026-11-10):
#   ECDSA   SHA256:uTa/0PndEgPZTf76e1DFqXKJEXKsn7m9ivhLQtzGOCI
#   ED25519 SHA256:RFzBCUItH9LZS0cKB5UE6ceAYhBD5C8GeOBip8Z11+4
#   RSA     SHA256:5s5cIyReIfNNVGRFdDbe3hdYiI5OelHGpw2rOUud3Q8
cat >"$HOME/.ssh/aur_known_hosts" <<'EOF'
aur.archlinux.org ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEuBKrPzbawxA/k2g6NcyV5jmqwJ2s+zpgZGZ7tpLIcN
aur.archlinux.org ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBLMiLrP8pVi5BFX2i3vepSUnpedeiewE5XptnUnau+ZoeUOPkpoCgZZuYfpaIQfhhJJI5qgnjJmr4hyJbe/zxow=
aur.archlinux.org ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQDKF9vAFWdgm9Bi8uc+tYRBmXASBb5cB5iZsB7LOWWFeBrLp3r14w0/9S2vozjgqY5sJLDPONWoTTaVTbhe3vwO8CBKZTEt1AcWxuXNlRnk9FliR1/eNB9uz/7y1R0+c1Md+P98AJJSJWKN12nqIDIhjl2S1vOUvm7FNY43fU2knIhEbHybhwWeg+0wxpKwcAd/JeL5i92Uv03MYftOToUijd1pqyVFdJvQFhqD4v3M157jxS5FTOBrccAEjT+zYmFyD8WvKUa9vUclRddNllmBJdy4NyLB8SvVZULUPrP3QOlmzemeKracTlVOUG1wsDbxknF1BwSCU7CmU6UFP90kpWIyz66bP0bl67QAvlIc52Yix7pKJPbw85+zykvnfl2mdROsaT8p8R9nwCdFsBc9IiD0NhPEHcyHRwB8fokXTajk2QnGhL+zP5KnkmXnyQYOCUYo3EKMXIlVOVbPDgRYYT/XqvBuzq5S9rrU70KoI/S5lDnFfx/+lPLdtcnnEPk=
EOF
chmod 600 "$HOME/.ssh/aur_known_hosts"
export GIT_SSH_COMMAND="ssh -i $HOME/.ssh/aur -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile=$HOME/.ssh/aur_known_hosts -o BatchMode=yes -o ConnectTimeout=20 -o ServerAliveInterval=15 -o ServerAliveCountMax=4"
