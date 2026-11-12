# T21 — More GPU and memory details when there is room

| | |
|---|---|
| **Batch** | 3 — after the first look at the real screen |
| **Depends on** | T20 |
| **Read first** | `grep -n "pub \|fn " src/ui/panels/gpu.rs src/ui/panels/memory.rs src/ui/text.rs src/collect/gpu.rs src/collect/system.rs` |
| **Deliverable** | changes to `src/ui/panels/gpu.rs` and `src/ui/panels/memory.rs`, `tests/panels_details.rs`; regenerated golden files |

## Why

On a large terminal the GPU and Memory panels are mostly empty: the GPU panel shows four rows then a graph,
and the Memory panel three rows then nothing. The collectors already read more (`GpuSample`: `sclk_mhz`,
`vddgfx_mv`, `fan_pct`, `temp_mem_c`, `gtt_used`, `link`; `MemSample`: `available`, `cached`; the router's
own process). Show them when the panel is tall enough, and keep the small layouts exactly as they are.

## Contract

No public signature changes. In `src/ui/panels/gpu.rs`, with `h` the inner height of the panel: when
`h >= 10`, three rows are inserted after row 4 (`power …`), and the graph (when `ctx.graphs`) takes the rows
left after them:

5. `clock <sclk> MHz   <vddgfx> mV   fan <fan_pct>` — sclk with no decimal, fan as `text::pct`; each missing
   value `n/a`.
6. `mem   <temp(temp_mem_c)>   GTT <util::bytes(gtt_used)>` (`n/a` when missing).
7. `link  <link>` (`n/a` when missing).

Labels (`clock`, `mem`, `GTT`, `fan`, `link`) in `Role::Dim`, values in `Role::Text`, labels padded to 6
like the rows above. When `h < 10`, nothing changes.

In `src/ui/panels/memory.rs`, after row 3 (`model …` / `server …`), when the panel has the rows:

4. `avail <util::bytes(available)>   cached <util::bytes(cached)>`
5. `router <util::bytes(rss)>` for the `ServerRole::Router` process, only when row 3 showed an instance
   (otherwise row 3 already is the router).

## Pitfalls

- Expected values (T11 demo, Generating, `t = 119`): sclk `2204 MHz`, `917 mV`, fan `34 %`, mem `76 °C`, GTT
  `175 M` (`util::bytes` has no decimal from 100 up), link `16.0 GT/s PCIe x16`; memory avail `9.5 G`,
  cached `18.2 G`, router `287 M`.
- The existing tests of `tests/panels_machine.rs` render the GPU panel at 50 × 9 (inner height 7): they must
  pass unchanged.
- Regenerate the golden files with `UPDATE_GOLDEN=1` and read the 140 × 40 ones: only the GPU and Memory
  panels may change.

## Tests

`tests/panels_details.rs`, exactly these names; same helpers as `tests/panels_model.rs`.

- `gpu_details_when_tall`: `demo::app(Generating)`, GPU panel 70 × 16 → rows contain
  `clock 2204 MHz   917 mV   fan 34 %`, `mem   76 °C   GTT 175 M`, `link  16.0 GT/s PCIe x16`, and the last
  inner row still holds a braille character.
- `gpu_no_details_when_short`: GPU panel 70 × 11 (inner height 9) → no row contains `clock`.
- `memory_details_when_tall`: Memory panel 40 × 8 → rows contain `avail 9.5 G   cached 18.2 G` and
  `router 287 M`; at 40 × 5 neither.
- `memory_down_has_no_router_row`: `demo::app(Down)`, Memory panel 40 × 8 → `server 287 M` present,
  no row starts with `│router`.

## Acceptance criteria

- [ ] GPU and Memory panels show the extra rows only when tall enough; no public API changed.
- [ ] The 4 tests above exist under these names and pass; every earlier test passes unchanged.
- [ ] The golden files are regenerated and read; only GPU and Memory panels changed.
- [ ] `bash .devbox/check` is green.
