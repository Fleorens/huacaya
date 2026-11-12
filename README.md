# llamatop

A terminal dashboard for a [llama.cpp](https://github.com/ggml-org/llama.cpp) server and the machine that
runs it: the loaded model and its state, live generation and prompt speed, context fill, prompt-cache reuse,
speculative-decoding acceptance, GPU / CPU / RAM / temperatures, disks, network, and the server log.

It works with a single `llama-server` or a router serving several models, on the server itself or from another
machine. It is **read-only**: it never loads, unloads or configures a model. A value it cannot read shows
`n/a`, so it runs without a GPU, without `journalctl`, or with the server down.

```
╭ llamatop ─ gpu-box ─ up 4d 3h ─────────────────────────────────── ● server ok  b12151 ─ 10:21:03 ╮
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
╭ CPU ───────────────────────────────────────────╮╭ Memory ────────────────────────────────────────╮
```

(`llamatop --snapshot 100x30`, demo data — try `llamatop --demo`.)

## Install

Linux x86_64, any distribution (a static binary, no dependency):

```sh
curl -fsSL https://raw.githubusercontent.com/Fleorens/llamatop/main/install.sh | sh
```

It installs `llamatop` into `/usr/local/bin` (with `sudo` when needed) or `~/.local/bin` (`--user`).
Debian and Ubuntu can use the package of the [latest release](https://github.com/Fleorens/llamatop/releases/latest)
instead:

```sh
curl -fsSLO https://github.com/Fleorens/llamatop/releases/latest/download/llamatop_amd64.deb
sudo apt install ./llamatop_amd64.deb
```

From source (Rust 1.88+): `cargo install --git https://github.com/Fleorens/llamatop`.

## First run

```sh
llamatop
```

With no configuration yet, llamatop asks where the server is (`http://127.0.0.1:8080` by default), its API
key if it has one (typed without echo, checked against the server), and how to read its log, then saves the
answers to `~/.config/llamatop/config.toml` (mode 600) and starts. `llamatop setup` asks again anytime.

## What it shows

| Screen | |
|---|---|
| `1` Overview | model state (idle, prompt, generating, loading), live t/s, context fill, last request, GPU, CPU, memory, log |
| `2` Models | every model the server knows, its settings, the details of the selected one |
| `3` Logs | the server log, full screen: follow, scroll, text and level filters |
| `4` System | per-core usage, every temperature sensor, disks, network, `llama-server` processes |

The machine panels describe the machine llamatop runs on. To see the server's machine from elsewhere, run the
[agent](#agent) on the server. GPU support: AMD (amdgpu sysfs) and NVIDIA (`nvidia-smi`, shipped with the
driver).

The log comes from any command whose output is the server's log: by default
`journalctl -u llama-server -f -n 2000 --no-pager -q -o short-iso-precise`, which suits llama.cpp run as a
systemd unit named `llama-server` (the user needs the `systemd-journal` group). Others: `tail -F
/path/to/server.log`, `docker logs -f <container>`, `ssh host journalctl …`.

## Options

| Flag | Default | |
|---|---|---|
| `--url` | `http://127.0.0.1:8080` | the server, router or single model (`LLAMATOP_URL`) |
| `--api-key-file` | — | file holding the key (`LLAMATOP_API_KEY_FILE`); `LLAMA_API_KEY` also works |
| `--log-cmd` | `journalctl -u llama-server -f …` | the command whose output is the log (`LLAMATOP_LOG_CMD`) |
| `--interval-ms` | `1000` | refresh period, 200–10000 (`+` / `-` change it live) |
| `--mount` | every real disk | a mount point to show, repeatable |
| `--agent` | — | URL of a llamatop agent on the server (`LLAMATOP_AGENT`), see below |
| `--config` | `~/.config/llamatop/config.toml` | the configuration file (`LLAMATOP_CONFIG`) |
| `--demo` | | generated data, no server needed |
| `--dump` | | one JSON report of every data source, then exit |
| `--snapshot WxH` | | one frame of the demo on stdout (`--demo-state`, `--screen`, `--ansi`) |

Order of precedence: flag, then environment variable, then configuration file, then default.

```toml
# ~/.config/llamatop/config.toml
[llama]
url = "http://192.168.1.10:8080"
api_key = "sk-..."              # or api_key_file = "/path/to/key"
log_cmd = "ssh 192.168.1.10 journalctl -u llama-server -f -n 2000 --no-pager -q -o short-iso-precise"

[agent]
url = "http://192.168.1.10:9099"
token = "..."
```

## Keys

| Key | Action |
|---|---|
| `1`–`4`, `Tab`, `Shift-Tab` | switch screen |
| `q`, `Ctrl-C` | quit |
| `?` | help |
| `p` | pause the display (data keeps flowing) |
| `+` / `-` | faster / slower refresh |
| `↑ ↓ PgUp PgDn Home End` | scroll the log, select a model |
| `f` | follow the log |
| `/` | filter the log (`Enter` keeps it, `Esc` clears it) |
| `e` | level filter: all → warnings + errors → errors |
| `r` | raw / compact log lines |
| `x` | show the router's `proxying request` lines (hidden by default) |

## Agent

To watch the server's GPU, sensors and log from another machine, run `llamatop agent` on the server. It
samples that machine and follows its log like the dashboard does, and serves them over HTTP to remote
dashboards (`/v1/health`, `/v1/system`, `/v1/log`). A token is required as soon as it listens beyond loopback.

With the Debian package, the agent is a systemd service, shipped disabled:

```sh
sudo install -d -m 700 /etc/llamatop
head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n' | sudo tee /etc/llamatop/agent-token >/dev/null
sudo chmod 600 /etc/llamatop/agent-token
sudo systemctl enable --now llamatop-agent          # listens on 0.0.0.0:9099; open the port if a firewall filters it
```

Without the package: `llamatop agent --token-file /path/to/token` (`--listen 0.0.0.0:9099` by default).
Then, on the other machine, `llamatop setup` asks for the agent's URL (`http://<server>:9099`) and the token.

## The safety rule

On a llama.cpp router with autoload, a request naming a model that is not loaded **loads it**: tens of GB read,
the running model evicted. llamatop therefore asks `/metrics` and `/slots` only about models that the same
polling round's `/v1/models` lists as `loaded`, and, on the server itself, asks the loaded instance directly
(which also keeps llamatop out of the server's log). A test guards the rule.

## Development

- `bash .devbox/check`: `cargo fmt --check`, `clippy -D warnings`, tests, 400-line limit per file.
- `cargo run -- --sysroot tests/fixtures/sysroot --dump`: the collectors on a fake machine.
- `UPDATE_GOLDEN=1 cargo test` rewrites the golden frames in `tests/snapshots/` after an intended change.
- `scripts/release.sh <version>`: static binary, `.deb`, GitHub release.
- Design: `docs/01-architecture.md` (data), `docs/02-ui.md` (screens). The project was built sheet by sheet
  by a local model driven by Claude Code: `tasks/` holds every sheet.

## License

MIT.
