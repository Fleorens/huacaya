#!/usr/bin/env bash
# Publishes a GitHub release of huacaya from a clean, pushed main:
#
#   scripts/release.sh 0.3.1        # the version must equal Cargo.toml's
#
# Assets: huacaya-x86_64-linux.tar.gz (+ .sha256), the static binary with README, LICENSE and the agent's unit;
# huacaya_<version>_amd64.deb and huacaya_amd64.deb (same file, stable name for releases/latest/download).
# install.sh downloads the tarball from releases/latest. Needs gh, logged in with push rights.
set -euo pipefail
cd "$(dirname "$0")/.."
version="${1:?usage: $0 <version>}"
version="${version#v}"
tag="v$version"

[[ "$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)" == "$version" ]] \
  || { echo "Cargo.toml's version is not $version" >&2; exit 1; }
[[ -z "$(git status --porcelain)" ]] || { echo "the working tree is not clean" >&2; exit 1; }
[[ "$(git branch --show-current)" == "main" ]] || { echo "not on main" >&2; exit 1; }
git fetch -q github main
[[ "$(git rev-parse HEAD)" == "$(git rev-parse github/main)" ]] || { echo "main is not pushed to github" >&2; exit 1; }
bash .devbox/check

target=x86_64-unknown-linux-musl
cargo build --release --locked --target "$target"
target_dir="$(cargo metadata --format-version 1 --no-deps | grep -o '"target_directory":"[^"]*"' | cut -d'"' -f4)"
out="$target_dir/release-assets/$tag"
rm -rf "$out" && mkdir -p "$out/huacaya"
install -m 755 "$target_dir/$target/release/huacaya" "$out/huacaya/huacaya"
install -m 644 README.md LICENSE packaging/huacaya-agent.service "$out/huacaya/"
tar -C "$out" -czf "$out/huacaya-x86_64-linux.tar.gz" huacaya
(cd "$out" && sha256sum huacaya-x86_64-linux.tar.gz > huacaya-x86_64-linux.tar.gz.sha256)

deb="$(scripts/build-deb.sh | tail -1)"
cp "$deb" "$out/"
cp "$deb" "$out/huacaya_amd64.deb"

previous="$(git describe --tags --abbrev=0 2>/dev/null || true)"
notes="$out/notes.md"
{
  echo "Install or update: \`curl -fsSL https://raw.githubusercontent.com/Fleorens/huacaya/main/install.sh | sh\`"
  echo
  echo "Changes:"
  git log --no-merges --format='- %s' ${previous:+"$previous"..}HEAD | grep -v '^- docs: .*task sheet' || true
} > "$notes"

git tag -a "$tag" -m "huacaya $version"
git push -q github "$tag"
gh release create "$tag" --repo Fleorens/huacaya --title "huacaya $version" --notes-file "$notes" \
  "$out/huacaya-x86_64-linux.tar.gz" "$out/huacaya-x86_64-linux.tar.gz.sha256" \
  "$out/huacaya_${version}_amd64.deb" "$out/huacaya_amd64.deb"
echo "released $tag"
