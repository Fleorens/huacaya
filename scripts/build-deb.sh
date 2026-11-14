#!/usr/bin/env bash
# Builds the Debian package of huacaya: <target dir>/debian/huacaya_<version>_<arch>.deb
# Run on a Debian-like machine (needs dpkg-deb). Install with: sudo apt install ./huacaya_<version>_<arch>.deb
#
# The package holds the static (musl) binary in /usr/bin/huacaya, the README and the agent's systemd unit
# (installed disabled). Static: no library dependency, so it installs on any Debian or Ubuntu release.
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
arch=amd64   # the binary is built for x86_64 below, whatever this machine is
target=x86_64-unknown-linux-musl

cargo build --release --locked --target "$target"
target_dir="$(cargo metadata --format-version 1 --no-deps | grep -o '"target_directory":"[^"]*"' | cut -d'"' -f4)"
out="$target_dir/debian"
stage="$out/huacaya_${version}_${arch}"
rm -rf "$stage"
install -d "$stage/DEBIAN" "$stage/usr/bin" "$stage/usr/share/doc/huacaya" "$stage/usr/lib/systemd/system"
install -m 755 "$target_dir/$target/release/huacaya" "$stage/usr/bin/huacaya"
install -m 644 README.md "$stage/usr/share/doc/huacaya/README.md"
# The agent's unit is shipped disabled: it needs a token first (see the README, "Agent").
install -m 644 packaging/huacaya-agent.service "$stage/usr/lib/systemd/system/huacaya-agent.service"

size_kb="$(du -sk "$stage/usr" | cut -f1)"
cat > "$stage/DEBIAN/control" <<EOF
Package: huacaya
Version: $version
Architecture: $arch
Maintainer: Fleorens
Homepage: https://github.com/Fleorens/huacaya
Recommends: openssh-client
Conflicts: llamatop
Replaces: llamatop
Section: utils
Priority: optional
Installed-Size: $size_kb
Description: terminal dashboard for a llama.cpp server
 Shows the loaded model, generation speed, context fill, prompt-cache reuse,
 GPU / CPU / RAM / temperatures and the live server log. Read-only: it never
 loads or unloads a model. Run "huacaya setup" to configure it.
EOF

dpkg-deb --root-owner-group --build "$stage" "$out/huacaya_${version}_${arch}.deb" >/dev/null
echo "$out/huacaya_${version}_${arch}.deb"
