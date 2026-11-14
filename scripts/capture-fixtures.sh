#!/usr/bin/env bash
# Run ON THE AIBOX. Collects, in one tar.gz, what huacaya reads, so the test fixtures can be
# checked against the real machine: proc and sysfs files, 400 lines of the llama-server journal.
# Read-only. No secret is collected: the environment and the unit file are not read.
# The journal holds client IP addresses and request paths, nothing else.
set -euo pipefail
out="$(mktemp -d)"
r="$out/sysroot"
copy() { # copy <file>: keep the path, ignore unreadable files
  [[ -r "$1" ]] || return 0
  mkdir -p "$r$(dirname "$1")"
  cat "$1" > "$r$1" 2>/dev/null || true
}
for f in /proc/stat /proc/meminfo /proc/loadavg /proc/uptime /proc/net/dev /proc/sys/kernel/hostname; do copy "$f"; done
for f in /sys/devices/system/cpu/cpu[0-9]*/cpufreq/scaling_cur_freq; do copy "$f"; done
for d in /sys/class/hwmon/hwmon*; do
  for f in "$d"/name "$d"/*_input "$d"/*_label "$d"/*_average "$d"/*_cap "$d"/*_max "$d"/pwm[0-9]; do copy "$f"; done
done
for d in /sys/class/drm/card[0-9]*/device /sys/class/drm/renderD*/device; do
  [[ -d "$d" ]] || continue
  for f in vendor device gpu_busy_percent mem_busy_percent mem_info_vram_total mem_info_vram_used \
    mem_info_gtt_total mem_info_gtt_used current_link_speed current_link_width; do copy "$d/$f"; done
  for w in "$d"/hwmon/hwmon*; do
    [[ -d "$w" ]] || continue
    for f in "$w"/name "$w"/*_input "$w"/*_label "$w"/*_average "$w"/*_cap "$w"/*_max "$w"/pwm[0-9]; do copy "$f"; done
  done
done
ls -l /sys/class/drm /sys/class/hwmon > "$out/listing.txt" 2>&1 || true
for pid in $(pgrep -x llama-server || true); do
  # cmdline: arguments only (the API key is passed by environment, not by argument)
  for f in comm status cmdline; do copy "/proc/$pid/$f"; done
done
if journalctl -u llama-server -n 400 --no-pager -q -o short-iso-precise > "$out/journal.txt" 2> "$out/journal.err"; then
  echo "journal: $(wc -l < "$out/journal.txt") lines"
else
  echo "journal: NOT readable by $(id -un) — see journal.err (group systemd-journal or adm needed)"
fi
tar -C "$out" -czf "$PWD/huacaya-capture.tar.gz" .
rm -rf "$out"
echo "Written: $PWD/huacaya-capture.tar.gz"
