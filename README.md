# llamatop

A terminal dashboard for a [llama.cpp](https://github.com/ggml-org/llama.cpp) server and the machine that
runs it: the loaded model and its state, generation and prompt speed, context fill, prompt-cache reuse,
speculative-decoding acceptance, GPU / CPU / RAM / temperatures, disks, network, and the live server log.

It is written for one machine (an AMD Radeon box running llama.cpp in router mode) but runs anywhere: a
value it cannot read shows `n/a`, and it works without an AMD GPU, without `journalctl` and with the
server down. It is **read-only**: it never loads, unloads or configures a model.

```
╭ llamatop ─ aibox-flo ─ up 4d 3h ───────────────────────────────── ● server ok  b12151 ─ 10:21:03 ╮
│ 1 Overview   2 Models   3 Logs   4 System                                              ? help  q │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭ Model ─────────────────────────────────────────╮╭ GPU  RX 7800 XT ───────────────────────────────╮
│Cyber-Tiel-Coder-35B-A3B   Q4_K_M               ││load  ██████████████████████████████████▏░  95 %│
│◉ GENERATING   task 9000                        ││VRAM  ████████████████████████████▉  15.9/16.0 G│
│gen    49.3 t/s  ⣿⣷⣾⣿⣶⣿⣷⣿⣿⣶⣿⣶⣾⣷⣾⣿⣷⣿⣷⣾⣿⣶⣿⣷⣿⣿⣾⣿⣶⣾⣷││temp  73 °C junction   62 °C edge               │
│prompt 591 t/s   cache 98 %                     ││power 117 W / 212 W   fan 1233 rpm              │
│ctx    ████████▏░░░░░░░░░░░░░░░░  63.8K / 196.6K││⣿⣿⣾⣿⣿⣿⣿⣿⣿⣿⣿⣷⣿⣿⣿⣿⣿⣿⣿⣾⣿⣾⣿⣿⣿⣿⣿⣿⣷⣿⣿⣿⣿⣿⣿⣿⣾⣿⣾⣿⣿⣿⣿⣿⣿⣷⣿⣿│
│draft  64 %   ctx-size 196.6K   n-cpu-moe 21   …││⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿│
╰────────────────────────────────────────────────╯╰────────────────────────────────────────────────╯
```

(`llamatop --snapshot 100x30`, demo data. More frames in `tests/snapshots/`.)

## Install

On the build machine (Rust 2024, no system library needed):

```sh
cargo build --release          # the binary is target/release/llamatop
```

To the aibox, over SSH, from the devbox:

```sh
scripts/deploy-aibox.sh <ssh-target>                 # installs ~/.local/bin/llamatop on the target
SSH_OPTS="-F $HOME/work/.aibox-access/ssh_config" scripts/deploy-aibox.sh aibox
```

## Run

```sh
llamatop --api-key-file ~/llama-api-key       # on the aibox
llamatop --demo                               # generated data, no server needed
llamatop --url http://192.168.1.203:8080 --api-key-file key \
         --log-cmd "ssh aibox journalctl -u llama-server -f -n 2000 --no-pager -q -o short-iso-precise"
```

The machine panels describe the machine llamatop runs on; run it on the server's machine to see the GPU.
The API key is read from `LLAMA_API_KEY`, else from the first line of `--api-key-file`; it is never printed.
The user running llamatop needs to read the journal (group `systemd-journal`) for the log panel.

| Flag | Default | |
|---|---|---|
| `--url` | `http://127.0.0.1:8080` | server, router or single model (`LLAMATOP_URL`) |
| `--api-key-file` | — | `LLAMATOP_API_KEY_FILE` |
| `--log-cmd` | `journalctl -u llama-server -f -n 2000 …` | any command whose output is the log (`LLAMATOP_LOG_CMD`) |
| `--interval-ms` | `1000` | refresh period, 200–10000 |
| `--mount` | `/ /srv/models /srv/data` | disks shown (missing ones are skipped) |
| `--sysroot` | `/` | where `proc/` and `sys/` are read (tests) |
| `--dump` | | one JSON report of every data source, then exit |
| `--demo` | | generated data |
| `--snapshot WxH` | | one frame of the demo on stdout (`--demo-state`, `--screen`, `--ansi`) |

## Keys

| Key | Action |
|---|---|
| `1`–`4`, `Tab`, `Shift-Tab` | Overview, Models, Logs, System |
| `q`, `Ctrl-C` | quit |
| `?` | help |
| `p` | pause the display (data keeps flowing) |
| `+` / `-` | faster / slower refresh |
| `↑ ↓ PgUp PgDn Home End` | scroll the log, select a model |
| `f` | follow the log |
| `/` | filter the log (`Enter` keeps it, `Esc` clears it) |
| `e` | level filter: all → warnings + errors → errors |
| `x` | show the router's `proxying request` lines (hidden: llamatop's own polling prints two per second) |

## The safety rule

On a llama.cpp router with autoload, a request naming a model that is not loaded **loads it** (tens of GB,
evicting the running model). llamatop therefore queries `/metrics` and `/slots` only for models that the same
polling round's `/v1/models` lists as `loaded`. See `docs/01-architecture.md` § 2.2; a test guards it.

## Development

- `bash .devbox/check`: `cargo fmt --check`, `clippy -D warnings`, tests, 400-line limit per file.
- `cargo run -- --sysroot tests/fixtures/sysroot --dump`: the collectors on a fake machine.
- Golden files: `UPDATE_GOLDEN=1 cargo test` rewrites `tests/snapshots/*.txt` after an intended change.
- Design: `docs/01-architecture.md` (data), `docs/02-ui.md` (screens); history of the work: `tasks/`.
