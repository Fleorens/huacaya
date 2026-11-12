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

Re-split on 2026-11-12 into smaller sheets, one per session. Sheets are written one or two ahead of the queue,
each against the code its dependencies actually delivered.

| Sheet | Subject | Depends on | Status |
|---|---|---|---|
| [T09](T09-app-state.md) | App state, events, keys (pure, no terminal) | T08 | livrée |
| [T10](T10-theme-widgets.md) | Theme (true colour / 16 colours / `NO_COLOR`), gauge, braille graph, badge, panel frame | — | validée |
| [T11](T11-demo.md) | `--demo` data: deterministic states (idle, prompt, generating, loading, down) and a live generator | T09 | livrée |
| [T12](T12-panels-model.md) | Text helpers, header + tabs, Model and Last request panels | T10, T11 | livrée |
| [T13](T13-panels-machine.md) | GPU, CPU, Memory and Log panels | T12 | livrée |
| [T14](T14-layout-overview.md) | Responsive layout, Overview screen, `ui::draw`, "terminal too small", `--snapshot WxH`, golden files | T13 | livrée |
| [T15](T15-run-loop.md) | Run loop: terminal init / restore, collector threads, input and tick threads, `--demo` live | T14 | livrée |
| [T16](T16-models-screen.md) | Models screen (table + details) | T14 | livrée |
| [T17](T17-logs-help.md) | Logs screen (status bar, filter typing) and help overlay | T16 | livrée |
| [T18](T18-system-screen.md) | System screen (per-core bars, sensors, disks, network, processes) | T17 | livrée |
| T19 | Release build, install script for the aibox, README | T15–T18 | outlined |
