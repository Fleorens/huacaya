# Implementation plan — llamatop

Each task sheet fits in **one OpenCode session** of the local model. The orchestrator (Claude) writes the
sheets **by batch**: a batch is written against the code the previous one actually delivered, because a sheet
that cites signatures not yet written ends up lying.

Run a sheet: `dev add llamatop @tasks/T01-util.md`. Order: the table's, top to bottom.

## Where it stands (2026-11-11)

- Skeleton written and green by the orchestrator: `Cargo.toml` (all dependencies), `src/config.rs` (CLI),
  `src/http.rs` (HTTP client), `.devbox/check`, fixtures (`tests/fixtures/`: real API answers captured on the
  aibox, a synthetic `/proc` + `/sys` tree, a journal excerpt).
- 2026-11-11, capture of the real machine (`scripts/capture-fixtures.sh`) compared with the fixtures: the
  `/proc` and `/sys` layout matches (amdgpu on `card1`, sensors under `device/hwmon/hwmon5`); the journal
  format did not, so the journal fixture is now 30 real lines and T07 was rewritten against them. Added from
  the capture: unconnected probes at −55 °C are skipped (T03).
- Batch 1: **validée** (T01 → T08), reviewed by the orchestrator on 2026-11-11. Fixed in the review:
  `--dump` hung forever on a live but silent log stream (the 700 ms deadline was checked only when a line
  arrived); the GPU temperature lookup also matched `freq<N>_label` files.
- Batch 2: **outlined** below, sheets to be written after `--dump` has been compared with the real machine.
  Exception: T10 (theme and widgets) uses nothing from batch 1, so it was written ahead.

## Batch 1 — data layer (no screen)

| Sheet | Subject | Depends on | Status |
|---|---|---|---|
| [T01](T01-util.md) | History ring, rates, formatting | — | validée |
| [T02](T02-system.md) | CPU, memory, load, uptime | T01 | validée |
| [T03](T03-sensors-gpu.md) | Temperatures, AMD GPU | T02 | validée |
| [T04](T04-disk-net-process.md) | Disks, network, llama-server processes | T02 | validée |
| [T05](T05-llama-parsing.md) | Parsing the server answers | T02 | validée |
| [T06](T06-llama-poller.md) | Polling the server, the safety rule | T05 | validée |
| [T07](T07-logs.md) | Following and classifying the log | T02 | validée |
| [T08](T08-sampler-dump.md) | System sampler, `--dump` | T03, T04, T06, T07 | validée |

Statuses: `prête` → `livrée` (set by the model at the end of its session) → `validée` (set by the
orchestrator after review).

**Gate between the batches** — on the aibox:
1. ~~`bash scripts/capture-fixtures.sh`~~ done on 2026-11-11, see above;
2. ~~`llamatop --dump --api-key-file <file>`~~ done on 2026-11-12 on the aibox: every value matches `free`,
   `sensors`, `df`, `ps` and `/proc/loadavg` (GPU power is `power1_average`, the same file `sensors` reads as
   PPT). Finding: each polling round makes the router print two `proxying request` lines, so the log view
   hides `Proxy` lines by default (T09).

## Batch 2 — the dashboard (from `docs/02-ui.md`)

Re-split on 2026-11-12 into smaller sheets, one per session. All delivered and reviewed on 2026-11-12, then
run on the aibox against the real server (every screen, 140×40, 80×24, 50×15): what remains is Florian's
judgement on screen (the unticked "on screen" criteria of each sheet).

| Sheet | Subject | Depends on | Status |
|---|---|---|---|
| [T09](T09-app-state.md) | App state, events, keys (pure, no terminal) | T08 | validée |
| [T10](T10-theme-widgets.md) | Theme (true colour / 16 colours / `NO_COLOR`), gauge, braille graph, badge, panel frame | — | validée |
| [T11](T11-demo.md) | `--demo` data: deterministic states (idle, prompt, generating, loading, down) and a live generator | T09 | validée |
| [T12](T12-panels-model.md) | Text helpers, header + tabs, Model and Last request panels | T10, T11 | validée |
| [T13](T13-panels-machine.md) | GPU, CPU, Memory and Log panels | T12 | validée |
| [T14](T14-layout-overview.md) | Responsive layout, Overview screen, `ui::draw`, "terminal too small", `--snapshot WxH`, golden files | T13 | validée |
| [T15](T15-run-loop.md) | Run loop: terminal init / restore, collector threads, input and tick threads, `--demo` live | T14 | validée |
| [T16](T16-models-screen.md) | Models screen (table + details) | T14 | validée |
| [T17](T17-logs-help.md) | Logs screen (status bar, filter typing) and help overlay | T16 | validée |
| [T18](T18-system-screen.md) | System screen (per-core bars, sensors, disks, network, processes) | T17 | validée |
| T19 | Release build, install script for the aibox, README — done by the orchestrator, no sheet | T15–T18 | validée |

## Batch 3 — after the first look at the real screen (2026-11-12)

Florian's screenshot of the real dashboard: borders invisible on a dark background, VRAM red whenever a model
is loaded, dead-looking idle graphs, a log panel full of the same prefix, a mostly empty GPU panel. The
first three are fixed by the orchestrator (commit "first on-screen review"); the rest are these sheets.

| Sheet | Subject | Depends on | Status |
|---|---|---|---|
| [T20](T20-compact-log.md) | Compact log lines (`r` toggles raw) | T18 | validée |
| [T21](T21-gpu-memory-details.md) | More GPU and memory details when there is room | T20 | validée |
| [T22](T22-direct-instance.md) | Poll the loaded instance directly (no proxy lines in the journal) | T21 | validée |

## Batch 4 — a native Linux tool (2026-11-12)

Florian: run it from any Linux machine, installed as a package, with a first-run setup that asks for the
llama.cpp address and key (and later OpenCode's). The remote machine panels need an agent on the server: next.

| Sheet | Subject | Depends on | Status |
|---|---|---|---|
| [T23](T23-config-file.md) | Configuration file `~/.config/llamatop/config.toml` | T22 | validée |
| [T24](T24-setup-wizard.md) | First-run setup, `llamatop setup` | T23 | validée |
| T25 | `.deb` package (`scripts/build-deb.sh`) — by the orchestrator | T24 | validée |

## Batch 5 — the agent: the server's machine panels on any machine (2026-11-12)

| Sheet | Subject | Depends on | Status |
|---|---|---|---|
| [T26](T26-agent-protocol.md) | Agent protocol: request parsing, routing, log ring, token (pure) | T24 | validée |
| [T27](T27-agent-server.md) | `llamatop agent`: the server | T26 | validée |
| [T28](T28-agent-client.md) | Dashboard side: `--agent`, `[agent]`, setup question | T27 | validée |

## Batch 6 — any machine (2026-11-12)

The repository goes public: llamatop must suit any llama.cpp machine, not only the aibox.

| Sheet | Subject | Depends on | Status |
|---|---|---|---|
| [T29](T29-auto-mounts.md) | Show the machine's real disks by default | T28 | livrée |
| [T30](T30-nvidia.md) | NVIDIA GPUs through `nvidia-smi` | T29 | prête |
