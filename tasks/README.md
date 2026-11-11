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
- Batch 1: **ready** (T01 → T08).
- Batch 2: **outlined** below, sheets to be written after the review of batch 1 and after `--dump` has been
  compared with the real machine.

## Batch 1 — data layer (no screen)

| Sheet | Subject | Depends on | Status |
|---|---|---|---|
| [T01](T01-util.md) | History ring, rates, formatting | — | livrée |
| [T02](T02-system.md) | CPU, memory, load, uptime | T01 | livrée |
| [T03](T03-sensors-gpu.md) | Temperatures, AMD GPU | T02 | prête |
| [T04](T04-disk-net-process.md) | Disks, network, llama-server processes | T02 | prête |
| [T05](T05-llama-parsing.md) | Parsing the server answers | T02 | prête |
| [T06](T06-llama-poller.md) | Polling the server, the safety rule | T05 | prête |
| [T07](T07-logs.md) | Following and classifying the log | T02 | prête |
| [T08](T08-sampler-dump.md) | System sampler, `--dump` | T03, T04, T06, T07 | prête |

Statuses: `prête` → `livrée` (set by the model at the end of its session) → `validée` (set by the
orchestrator after review).

**Gate between the batches** — on the aibox:
1. ~~`bash scripts/capture-fixtures.sh`~~ done on 2026-11-11, see above;
2. `llamatop --dump --api-key-file <file>`: every value checked against `amdgpu_top` / `sensors` / `free`.

## Batch 2 — the dashboard (outline, from `docs/02-ui.md`)

| Sheet | Subject |
|---|---|
| T09 | App state, event loop (data tick + animation tick), terminal setup / restore / panic hook, `--demo` data generator |
| T10 | Theme (true colour / 16 colours / `NO_COLOR`), widgets: gradient gauge, braille graph, badge, panel frame |
| T11 | Responsive layout engine + Overview screen, `--snapshot WxH`, golden files at 4 sizes |
| T12 | Models screen (table + details) |
| T13 | Logs screen: follow, scroll, text filter, level filter; "Last request" from the timing lines |
| T14 | System screen: per-core bars, all sensors, disks, network, processes |
| T15 | Help overlay, pause, refresh speed, compact mode, "terminal too small" |
| T16 | Release build, install script for the aibox, README with screenshots |
