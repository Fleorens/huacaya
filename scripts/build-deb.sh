#!/usr/bin/env bash
# Builds the Debian package of llamatop: <target dir>/debian/llamatop_<version>_<arch>.deb
# Run on a Debian-like machine (needs dpkg-deb). Install with: sudo apt install ./llamatop_<version>_<arch>.deb
#
# The package holds the release binary (/usr/bin/llamatop), the README and the agent's systemd unit
# (installed disabled). The binary links against the C library and libgcc_s (unwinding): libc6 at the
# version it was built with, and libgcc-s1.
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
arch="$(dpkg --print-architecture)"
glibc="$(getconf GNU_LIBC_VERSION | cut -d' ' -f2)"

cargo build --release --locked
target_dir="$(cargo metadata --format-version 1 --no-deps | grep -o '"target_directory":"[^"]*"' | cut -d'"' -f4)"
out="$target_dir/debian"
stage="$out/llamatop_${version}_${arch}"
rm -rf "$stage"
install -d "$stage/DEBIAN" "$stage/usr/bin" "$stage/usr/share/doc/llamatop" "$stage/usr/lib/systemd/system"
install -m 755 "$target_dir/release/llamatop" "$stage/usr/bin/llamatop"
install -m 644 README.md "$stage/usr/share/doc/llamatop/README.md"
# The agent's unit is shipped disabled: it needs a token first (see the README, "Agent").
install -m 644 packaging/llamatop-agent.service "$stage/usr/lib/systemd/system/llamatop-agent.service"

size_kb="$(du -sk "$stage/usr" | cut -f1)"
cat > "$stage/DEBIAN/control" <<EOF
Package: llamatop
Version: $version
Architecture: $arch
Maintainer: Florian
Depends: libc6 (>= $glibc), libgcc-s1
Recommends: openssh-client
Section: utils
Priority: optional
Installed-Size: $size_kb
Description: terminal dashboard for a llama.cpp server
 Shows the loaded model, generation speed, context fill, prompt-cache reuse,
 GPU / CPU / RAM / temperatures and the live server log. Read-only: it never
 loads or unloads a model. Run "llamatop setup" to configure it.
EOF

dpkg-deb --root-owner-group --build "$stage" "$out/llamatop_${version}_${arch}.deb" >/dev/null
echo "$out/llamatop_${version}_${arch}.deb"
