# T38 — System screen v2: core heatmap, grouped sensors, network graphs

| | |
|---|---|
| **Batch** | 7 — the TUI v2 (mockups validated by Florian on 2026-11-12) |
| **Depends on** | T37 (T36 for `Role::Selection`) |
| **Read first** | `src/ui/screens/system.rs` (whole file); `src/ui/gauge.rs`; `src/ui/theme.rs` (`rgb`, `Role::Selection`, `gauge_color`); `src/ui/graph.rs`; `src/collect/sampler.rs` (`SystemSnapshot`); `src/collect/sensors.rs` (`TempReading`); `src/collect/net.rs` (`NetSample`); `src/app/state.rs` (`Event::System`); `src/app/keys.rs`; `tests/system_screen.rs` |
| **Deliverable** | changes to `src/ui/screens/system.rs` plus `src/ui/screens/system_left.rs` (new: CPU and Sensors panels), `src/app/state.rs`, `src/app/keys.rs`, `src/ui/help.rs`; `tests/system_screen.rs`; golden files regenerated |

## Why

Sixteen gauge rows for sixteen cores fill half the screen and say little. The validated mockup shows the cores
as a compact heatmap (brighter = busier), groups the sensors by what they measure (GPU, CPU, storage, board),
and gives each network interface a small rate graph.

## Contract

### State and keys

- `App` gains `pub net_rates: Vec<(String, History, History)>`: per interface, the receive and transmit rates
  (bytes/s as `f32`), capacity `HISTORY_CAPACITY`. On `Event::System`, for each `NetSample` with both rates,
  push them into the entry of its `iface` (created when missing, in the snapshot's order); entries whose
  interface is no longer in the snapshot are removed.
- `App` gains `pub all_sensors: bool` (`false`). On the System screen, `s` toggles it.
- `src/ui/help.rs`: a row `s` → `System: every sensor, or the summary` (box height + 1).

### Layout

`render` keeps its signature and the `no data yet` case. Two equal columns (left `area.width / 2`). The last row
of the area is a `Dim` hint row `cores: brighter = busier · sensors grouped by chip · s: all sensors`
(`text::fit_groups` split at ` · `), when the area has at least 12 rows.

**Left column** — the **CPU** panel at its own height, the **Sensors** panel on the rest.

- **CPU** — title `CPU  <cpu_info.model> · <cores> cores / <threads> threads` (the parts that exist; just
  `CPU`), accent `Cpu`; right title `<temp> · <freq> GHz` (`Dim`; `text::temp` of `cpu_temp_c`, the average
  frequency with one decimal; a missing part left out). Inside:
  - The core heatmap: cells of `w = (inner.width − 7) / 8` columns (at least 3), 8 per row, one column gap,
    as many rows as needed. A cell is the core's `per_core_pct` rounded, centred. True colour: background
    `lerp(rgb(Selection), rgb(Cpu), pct / 100)` (per channel, rounded), foreground `rgb(Text)`, or
    `(0x1a, 0x1b, 0x26)` from 80 %. 16 colours: no background, foreground `Text` below 70, `Warning` below 90,
    else `Critical`. No colour: the number alone, bold from 90. Before the second sample: one `Dim` row
    `cores n/a`.
  - Then a 3-row `BrailleGraph` of `app.cpu_usage` (accent `Cpu`, max 100), when `ctx.graphs`.
  - Then `load <one> <five> <fifteen>   uptime <util::duration(uptime_s)>` (labels `Dim`).
  - Height: borders + heatmap rows + 3 (graph, when shown) + 1.
- **Sensors** — title `Sensors`, accent `Gpu`, right title `<n> sensors · s: all` (or `s: summary` when
  `all_sensors`). The readings are grouped, groups in this order, each with a bold `Dim` header row:
  `GPU · amdgpu` (chip `amdgpu`, accent `Gpu`), `CPU · <chip>` (`coretemp` or `k10temp`, accent `Cpu`),
  `Storage · nvme` (accent `Memory`), `Board · <the other chips, comma-separated, in reading order>` (accent
  `Good`). An empty group is not shown. Rows under a header, as
  `  <label padded to 15><Gauge, 16 columns>  <temp right-aligned to 6><hot mark>`, the gauge with
  `ratio = celsius / 100`, the group's accent, and `thresholds = sensor_thresholds(chip)` (a temperature is
  its own percentage of 100 °C, so the gauge turns warning / critical at the sensor's thresholds):
  - summary (`!all_sensors`): GPU — every reading, labels lowercased; CPU — `package` (the reading whose label
    starts with `Package`, or `Tctl`, else the first) and `hottest core` (the highest reading whose label
    starts with `Core`, row absent without one); Storage — one row per nvme chip, `composite` (label
    `Composite`, else the first reading of that chip); Board — `hottest` (the highest).
  - all: every reading of each group, label lowercased.
  - Rows that do not fit are dropped from the bottom.

**Right column** — stacked like today (a panel that does not fit keeps at least 3 rows or is dropped with
the ones under it): **Disks**, **Network**, **Processes**.

- **Disks** — as today (`mount  <gauge>  used/total`), mount padded to 15.
- **Network** — two rows per interface: `<iface padded to 8>↓ <rate>  <graph>   total <bytes(rx_total)>` and
  `        ↑ <rate>  <graph>   total <bytes(tx_total)>`, where `<rate>` is `<bytes(v)>/s` padded to 10 (`n/a`)
  and `<graph>` a one-row `BrailleGraph` of the interface's `net_rates` history (receive accent `Memory`,
  transmit `Gpu`; `max` = the larger of the two histories' maxima, at least 1), 22 columns, omitted when
  `!ctx.graphs` or when fewer than 22 columns remain for it after the rest of the row.
- **Processes** — header `pid     role      port    rss      thr  model`, rows as today in those columns;
  then, when a GPU is known and a row is left, a blank row and
  `GPU  <name>  <link> · <sclk> MHz · <vddgfx> mV · fan <fan_pct> · GTT <bytes(gtt)>` of `gpus[0]` — `GPU  <name>`
  in `Gpu` (the name without the `NVIDIA GeForce ` / `NVIDIA ` / `Radeon ` prefix, as the GPU panel's title:
  share that code), the rest `Dim`, only the parts that exist, `text::fit_groups` split at ` · `.

## Tests

Update in `tests/system_screen.rs`, never weaken: `thresholds_by_chip` (unchanged), `sensors_and_marks`,
`disks_network_processes`, `system_without_data`, `system_small`, `system_golden` (regenerate, read every
golden file). `cores_one_per_row` and `cores_two_per_row` are replaced by:

- `cpu_title_and_heatmap` (demo `Generating`, 140 × 40): the title contains
  `CPU  Intel Core i7-11700K · 8 cores / 16 threads`, the right title `60 °C · 3.5 GHz`; a row contains the 8
  first `per_core_pct` values of the snapshot, rounded, in order (build the expected numbers in the test).
- `heatmap_colours`: in true colour, a cell of a 0 % core has the background `rgb(Selection)` and a 100 % core
  `rgb(Cpu)`; with `ColorMode::NoColor` a 95 % core's number is bold.
- `sensors_grouped`: the headers `GPU · amdgpu`, `CPU · coretemp`, `Storage · nvme` appear in this order;
  `junction` with `73 °C` on one row; `package` with `60 °C`.
- `sensors_all_toggle`: `s` on the System screen sets `all_sensors`; the right title shows `s: summary`.
- `net_rates_history`: two `Event::System` with rates for `wlp9s0` → `net_rates` holds one entry of length 2;
  a snapshot without it removes the entry.
- `network_two_rows`: rows contain `wlp9s0  ↓ ` and `        ↑ ` and `total`.
- `gpu_summary_line`: the Processes panel shows `GPU  RX 7800 XT` and `917 mV`.

## Pitfalls

- `system.rs` is 295 lines: put CPU and Sensors in `src/ui/screens/system_left.rs` (`pub(crate)` functions).
- The heatmap writes a background: `cell.set_bg(color)` for every cell of the block, the gap columns excluded.
- `lerp` per channel: `a + (b − a) × t`, `t` clamped to 0..=1, rounded to `u8`.
- Sensor labels in the fixtures: read `tests/fixtures/sysroot/sys/class/hwmon/*/temp*_label`; never edit them.

## Acceptance criteria

- [ ] The contract above is implemented.
- [ ] The new and updated tests pass; golden files regenerated and read.
- [ ] `bash .devbox/check` is green.
- [ ] On screen (Florian): the System screen looks like the validated mockup at 140 × 40.
