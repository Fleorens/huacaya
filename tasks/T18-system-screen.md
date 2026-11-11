# T18 — System screen

| | |
|---|---|
| **Batch** | 2 — the dashboard |
| **Depends on** | T17 |
| **Read first** | `docs/02-ui.md` § 2 (System); `grep -n "pub " src/ui/*.rs src/ui/panels/*.rs src/ui/screens/*.rs src/collect/sampler.rs src/collect/process.rs` |
| **Deliverable** | `src/ui/screens/system.rs`, `tests/system_screen.rs`, `tests/snapshots/system-*.txt`; `pub mod system;` in `src/ui/screens/mod.rs`; the System branch of `src/ui/draw.rs` |

## Why

The Overview summarises the machine; the System screen shows all of it: every core, every sensor, every
disk, every interface, every `llama-server` process. Everything is already in `app.system`
(`SystemSnapshot`, T08).

## Contract — `src/ui/screens/system.rs`

```rust
/// The System screen in `area` (the body below the header).
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);

/// Warning / critical thresholds of one sensor: `amdgpu` → `GPU_JUNCTION_THRESHOLDS`; `coretemp` and
/// `k10temp` → `CPU_TEMP_THRESHOLDS`; `nvme` → `(70.0, 80.0)`; any other chip → `(80.0, 95.0)`.
pub fn sensor_thresholds(chip: &str) -> (f32, f32);
```

Layout of `area`: a left column of `area.width / 2` columns holding the **Cores** panel over its whole
height; a right column with, from the top, **Sensors** (`temps.len() + 2` rows), **Disks**
(`disks.len() + 2`), **Network** (`nets.len() + 2`) and **Processes** (`procs.len() + 3`). A right panel that
does not fit entirely in the rows left is cut to them when it keeps at least 3 rows, else dropped with
everything under it. Without a system snapshot: one `frame::panel("System", Role::Cpu, false, theme)` over
the whole area with a `Dim` row `no data yet`.

**Cores** — `frame::panel("Cores", Role::Cpu, false, theme)`. With `n = per_core_pct.len()` and `h` the
inner height: when `h >= n + 3`, one core per row; otherwise two per row (the inner width split in two
halves, core `i` on row `i / 2`, left half for even `i`). Each core:
`gauge_row("cpu<i>" padded to 6, pct / 100, Cpu, PCT_THRESHOLDS, pct(pct))`. Empty `per_core_pct` (first
sample): one row `waiting for a second sample` (`Dim`). After the cores, one blank row then:
`freq   <avg GHz> GHz avg   <max GHz> GHz max` (one decimal), `load   <one> <five> <fifteen>` (two
decimals), `uptime <util::duration>`; rows that do not fit are dropped.

**Sensors** — `frame::panel("Sensors", Role::Gpu, false, theme)`; one row per `TempReading` in order:
`<chip>/<label>` (`Dim`, padded to 24, cut with `text::fit` to 23 + one space) then
`<temp(celsius)><mark>` with `mark = hot_mark(severity(celsius, sensor_thresholds(chip)))`, the value in
`Warning` / `Critical` when the severity is, `Text` otherwise.

**Disks** — `frame::panel("Disks", Role::Memory, false, theme)`; per disk:
`gauge_row(<mount> padded to 13 (cut to 12 + space), used / total, Memory, PCT_THRESHOLDS,
bytes_pair(used, total))`.

**Network** — `frame::panel("Network", Role::Memory, false, theme)`; per interface:
`<iface padded to 10>↓ <rate(rx)>  ↑ <rate(tx)>   total ↓ <bytes(rx_total)>  ↑ <bytes(tx_total)>`, where
`rate(Some(x))` is `util::bytes(x as u64)` followed by `/s` and `rate(None)` is `n/a`.

**Processes** — `frame::panel("Processes", Role::Server, false, theme)`; a header row (`Dim`, bold)
`pid      role      port   rss       threads  model`, then per process: pid (9), `router` / `instance` (10),
port or `-` (7), `util::bytes(rss)` (10), threads (9), then `short_model_name(model).0` or `-`, cut at the width.

### `src/ui/draw.rs`

On `Screen::System` (tiers other than `TooSmall` / `Compact`): the header, then `screens::system::render` on
the body. The help overlay stays last.

## Pitfalls

- Demo values at `t = 119`, Generating: CPU usage `50.0`, so the cores read `44 %`, `48 %`, `52 %`, `56 %`
  repeating; network `↓ 2.1 K/s  ↑ 12.7 K/s`, totals `↓ 187 M  ↑ 188 M` (`util::bytes` has no decimal
  from 100 up); disk `/srv/models` `72.1/117.6 G`; processes 1194 (router, port 8080, `287 M`, 23 threads)
  and 6873 (instance, 35335, `19.2 G`, 30 threads, `Cyber-Tiel-Coder-35B-A3B`).
- `↓` and `↑` are one column each.
- Reuse `panels::gauge_row`, `text::*` and `frame::panel`; do not write new gauge code.

## Tests

`tests/system_screen.rs`, exactly these names; same helpers as `tests/panels_model.rs`. `body` = the screen
renderer alone into a `w × h` buffer.

- `thresholds_by_chip`: `amdgpu` → `(85, 95)`; `coretemp` → `(80, 95)`; `k10temp` → `(80, 95)`; `nvme` →
  `(70, 80)`; `acpitz` → `(80, 95)`.
- `cores_one_per_row`: `demo::app(Generating)`, body 140 × 37 → rows contain `cpu0`, `cpu15`, `44 %`,
  `56 %`, `freq   3.5 GHz avg   4.7 GHz max`, `load   8.33 7.14 5.56`, `uptime 4d 3h`; `cpu0` and `cpu1` are
  on different rows.
- `cores_two_per_row`: body 140 × 15 → `cpu0` and `cpu1` on the same row.
- `sensors_and_marks`: body 140 × 37 → rows contain `coretemp/Package id 0`, `amdgpu/junction`, `73 °C`; no
  `▲`; with `temps` entry `amdgpu/junction` set to `96.0` → a row contains `96 °C▲`.
- `disks_network_processes`: body 140 × 37 → rows contain `/srv/models`, `72.1/117.6 G`,
  `wlp9s0`, `↓ 2.1 K/s  ↑ 12.7 K/s`, `total ↓ 187 M  ↑ 188 M`, `1194`, `router`, `8080`, `287 M`, `6873`,
  `instance`, `35335`, `19.2 G`, `Cyber-Tiel-Coder-35B-A3B`.
- `system_without_data`: `App::new(1000)` → `no data yet`; `demo::app(Down)` → `router` present and
  `instance` absent.
- `system_small`: body 60 × 10 and 20 × 4: no panic, nothing outside the area; 60 × 10 still shows `cpu0`.
- `system_golden`: golden files `tests/snapshots/system-140x40.txt` and `system-100x30.txt` (full frames of
  `demo::app(Generating)`, `screen = System`), same `UPDATE_GOLDEN=1` mechanism as T14. Read them and describe
  them in the report.

## Acceptance criteria

- [ ] The files exist with the exact public API above; `draw.rs` routes the System screen.
- [ ] Nothing outside the deliverable changed (except `tasks/`).
- [ ] The 8 tests above exist under these names and pass; earlier golden files are unchanged.
- [ ] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) `--demo` then `4` shows the whole machine; on the aibox, the real one.
