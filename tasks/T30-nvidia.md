# T30 — NVIDIA GPUs (through `nvidia-smi`)

| | |
|---|---|
| **Batch** | 6 — any machine |
| **Depends on** | T29 |
| **Read first** | `src/collect/gpu.rs` (`GpuSample`); `src/collect/sampler.rs`; `src/ui/panels/gpu.rs`; `grep -n "temp_junction\|temp_edge\|Radeon" src/ui -r` |
| **Deliverable** | `src/collect/nvidia.rs`, `tests/nvidia.rs`, `tests/fixtures/nvidia/smi.csv`; `pub mod nvidia;` in `src/collect/mod.rs`; changes to `src/collect/sampler.rs` and `src/ui/panels/gpu.rs`, `src/ui/compact.rs`; README |

## Why

Most llama.cpp machines run NVIDIA cards. NVIDIA exposes nothing useful in sysfs: its numbers come from
`nvidia-smi`, which every driver installation ships. Read it once per sample, into the same `GpuSample` the AMD
collector fills, so every screen works unchanged.

## Contract — `src/collect/nvidia.rs`

```rust
/// The query passed to `nvidia-smi`, in this column order.
pub const QUERY: &str = "index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,\
power.limit,fan.speed,clocks.sm,pcie.link.gen.current,pcie.link.width.current";

/// Parses the output of `nvidia-smi --query-gpu=<QUERY> --format=csv,noheader,nounits`: one line per GPU,
/// comma-separated, each field trimmed. A field that is empty, `[N/A]`, `[Not Supported]` or not a number is
/// `None`. Lines with fewer than 12 fields are skipped. Never panics.
pub fn parse_smi(text: &str) -> Vec<GpuSample>;

/// Runs `nvidia-smi` with that query, waits at most 2 s (then kills it), and parses its output. Empty when the
/// program is missing, fails, or times out.
pub fn read_nvidia() -> Vec<GpuSample>;
```

Field mapping (`MiB` × 1 048 576 → bytes):

| `GpuSample` | from |
|---|---|
| `card` | `nvidia<index>` |
| `name` | `name` as given (e.g. `NVIDIA GeForce RTX 4090`) |
| `busy_pct` | `utilization.gpu` |
| `vram_used`, `vram_total` | `memory.used`, `memory.total` |
| `temp_edge_c` | `temperature.gpu` (the only temperature NVIDIA reports) |
| `temp_junction_c`, `temp_mem_c` | `None` |
| `power_w`, `power_cap_w` | `power.draw`, `power.limit` |
| `fan_pct` | `fan.speed`; `fan_rpm` `None` |
| `sclk_mhz` | `clocks.sm` |
| `vddgfx_mv`, `gtt_used` | `None` |
| `link` | `PCIe gen <gen> x<width>` when both are known, else `None` |

### `src/collect/sampler.rs`

`gpus` = the AMD cards (`read_gpus(sysroot)`) followed by `nvidia::read_nvidia()` — the latter **only** when the
sysroot is `/` (a test sysroot describes a fake machine; never run the real `nvidia-smi` for it). Remember, in
the `Sampler`, that `nvidia-smi` is missing after the first failed spawn with `NotFound`, and stop trying.

### UI

- `src/ui/panels/gpu.rs`, row 3: when `temp_junction_c` is `None` and `temp_edge_c` is `Some`, write
  `temp  <temp(edge)><mark>` with the mark from `GPU_JUNCTION_THRESHOLDS`; otherwise as today.
- The title strips a leading `NVIDIA GeForce `, `NVIDIA ` or `Radeon ` from the name (`GPU  RTX 4090`).
- `src/ui/compact.rs`: the GPU temperature is the junction one, else the edge one.

### README

"GPU support: AMD (amdgpu sysfs) and NVIDIA (`nvidia-smi`, shipped with the driver)."

## Pitfalls

- Write `tests/fixtures/nvidia/smi.csv` by hand, two lines:
  `0, NVIDIA GeForce RTX 4090, 97, 20480, 24564, 71, 352.47, 450.00, 63, 2520, 4, 16` and
  `1, NVIDIA RTX A2000 12GB, [N/A], 512, 12282, 38, [Not Supported], [Not Supported], 30, 210, 3, 8`.
- `std::process::Command` has no timeout: spawn, poll `try_wait` every 20 ms up to 2 s, then `kill` and `wait`.
- No `unwrap` on parsing; `f32` / `u64` parse failures are `None`.
- Never run `nvidia-smi` in tests (the devbox has none): test `parse_smi` with the fixture, and the sampler only
  with the fixture sysroot (which must not call it).

## Tests

`tests/nvidia.rs`, exactly these names.

- `parse_two_gpus`: the fixture → 2 samples; first: card `nvidia0`, name `NVIDIA GeForce RTX 4090`, busy 97,
  vram 20 480 MiB / 24 564 MiB in bytes, edge 71, junction `None`, power 352.47 / 450, fan 63 %, sclk 2520, link
  `PCIe gen 4 x16`; second: busy `None`, power and cap `None`, link `PCIe gen 3 x8`.
- `parse_garbage`: `""`, `"\n\n"`, `"a, b"`, `"x,y,z,1,2,3,4,5,6,7,8,9,10"` → never panics; the last gives one
  sample whose numeric fields that are not numbers are `None`.
- `fixture_sysroot_never_calls_nvidia_smi`: a `Sampler` on `tests/fixtures/sysroot` gives exactly the one AMD
  card of the fixture.
- `gpu_panel_nvidia`: the GPU panel (60 × 9) of an `App` whose system snapshot has the first fixture GPU → the
  top border contains `GPU  RTX 4090`, a row contains `temp  71 °C` and none contains `junction`.

## Acceptance criteria

- [ ] The public API above exists; NVIDIA GPUs appear in every screen through the same `GpuSample`.
- [ ] The 4 tests above exist under these names and pass; every earlier test passes.
- [ ] `bash .devbox/check` is green.
- [ ] (Not by the model) checked on a machine with an NVIDIA card.
