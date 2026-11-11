#!/usr/bin/env bash
# Run ON THE DEVBOX. Builds the release binary and installs it on the aibox over SSH.
#
#   scripts/deploy-aibox.sh <ssh-target> [remote path]
#
# The remote path defaults to ~/.local/bin/llamatop (relative paths are relative to the remote home).
# Extra ssh options go through SSH_OPTS, e.g. SSH_OPTS="-F $HOME/work/.aibox-access/ssh_config".
# The binary is copied next to its destination then renamed, so a running llamatop is not disturbed.
# The remote commands are built here on purpose: $dest is expanded client side.
# shellcheck disable=SC2029
set -euo pipefail
target="${1:?usage: $0 <ssh-target> [remote path]}"
dest="${2:-.local/bin/llamatop}"
read -r -a ssh_opts <<< "${SSH_OPTS:-}"
cd "$(dirname "$0")/.."

cargo build --release --locked
target_dir="$(cargo metadata --format-version 1 --no-deps | grep -o '"target_directory":"[^"]*"' | cut -d'"' -f4)"
bin="$target_dir/release/llamatop"
[[ -x "$bin" ]] || { echo "no binary at $bin" >&2; exit 1; }

ssh "${ssh_opts[@]}" "$target" "mkdir -p \"\$(dirname '$dest')\""
scp -q "${ssh_opts[@]}" "$bin" "$target:$dest.new"
ssh "${ssh_opts[@]}" "$target" "chmod 755 '$dest.new' && mv -f '$dest.new' '$dest' && '$dest' --version"
echo "installed on $target:$dest"
