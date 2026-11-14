#!/bin/sh
# Installs huacaya from its latest GitHub release:
#
#   curl -fsSL https://raw.githubusercontent.com/Fleorens/huacaya/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/Fleorens/huacaya/main/install.sh | sh -s -- --user
#
# Default: /usr/local/bin (with sudo when it is not writable). --user: ~/.local/bin, no sudo.
# HUACAYA_VERSION=0.3.0 pins a version instead of the latest.
set -eu

repo="Fleorens/huacaya"
dest="/usr/local/bin"
user_install=0
for arg in "$@"; do
  case "$arg" in
    --user) user_install=1; dest="$HOME/.local/bin" ;;
    -h|--help) sed -n '2,8p' "$0" 2>/dev/null || true; exit 0 ;;
    *) echo "huacaya install: unknown option $arg" >&2; exit 2 ;;
  esac
done

os="$(uname -s)"; arch="$(uname -m)"
if [ "$os" != "Linux" ] || [ "$arch" != "x86_64" ]; then
  echo "huacaya install: prebuilt binaries exist for Linux x86_64 only (this is $os $arch)." >&2
  echo "Build from source instead: cargo install --git https://github.com/$repo" >&2
  exit 1
fi

asset="huacaya-x86_64-linux.tar.gz"
if [ -n "${HUACAYA_VERSION:-}" ]; then
  base="https://github.com/$repo/releases/download/v${HUACAYA_VERSION#v}"
else
  base="https://github.com/$repo/releases/latest/download"
fi

fetch() { # fetch <url> <file>
  if command -v curl >/dev/null 2>&1; then curl -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then wget -qO "$2" "$1"
  else echo "huacaya install: needs curl or wget" >&2; exit 1
  fi
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM
echo "downloading $base/$asset"
fetch "$base/$asset" "$tmp/$asset"
fetch "$base/$asset.sha256" "$tmp/$asset.sha256"
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$tmp" && sha256sum -c --quiet "$asset.sha256") || { echo "huacaya install: checksum mismatch" >&2; exit 1; }
fi
tar -xzf "$tmp/$asset" -C "$tmp"

if [ "$user_install" -eq 1 ]; then mkdir -p "$dest"; fi
if [ -w "$dest" ]; then
  install -m 755 "$tmp/huacaya/huacaya" "$dest/huacaya"
elif command -v sudo >/dev/null 2>&1; then
  sudo install -m 755 "$tmp/huacaya/huacaya" "$dest/huacaya"
else
  echo "huacaya install: $dest is not writable and sudo is missing; try --user" >&2
  exit 1
fi

echo "installed $("$dest/huacaya" --version) in $dest"
case ":$PATH:" in
  *":$dest:"*) ;;
  *) echo "note: $dest is not in your PATH" ;;
esac
echo "run: huacaya        (the first run asks for your llama.cpp server)"
