# T04 — Disks, network, llama-server processes

| | |
|---|---|
| **Batch** | 1 — data layer |
| **Depends on** | T02 |
| **Read first** | `docs/01-architecture.md` § 2 (table only) and § 3 |
| **Deliverable** | `src/collect/disk.rs`, `src/collect/net.rs`, `src/collect/process.rs`, `tests/disk_net_process.rs`; three `pub mod` lines in `src/collect/mod.rs` |

## Why

Three small panels: how full the model volume is, how much traffic the box sees, and how much RAM the
`llama-server` processes hold (a MoE model keeps its experts in RAM: that number matters on a 32 GB box).

## Fixtures (do not edit) — under `tests/fixtures/sysroot/proc/`

- `net/dev`: `lo`, `wlp9s0` (rx 9876543210, tx 1234567890), `enp8s0` (all zeros).
- `1201/` — the router: `comm` = `llama-server`, `cmdline` (NUL-separated) has `--models-preset` and
  `--port 8080`, `status` has `VmRSS: 81920 kB`, `Threads: 12`.
- `48211/` — a model instance: `cmdline` has `--port 35335` and
  `--alias peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M`; `VmRSS: 9437184 kB`, `Threads: 34`.
- `77/` — `kthreadd`: not ours.

## Contract

Every struct and enum derives `Debug, Clone, PartialEq, serde::Serialize`.

### `src/collect/disk.rs`

```rust
/// Bytes. `used` = (blocks − bfree) × frsize; `available` = bavail × frsize; `total` = blocks × frsize.
pub struct DiskSample { pub mount: String, pub total: u64, pub used: u64, pub available: u64 }

/// `statvfs(path)`. `None` when the path does not exist, the call fails, or `total` is 0.
/// NOT relative to the sysroot: it describes a real mount point.
pub fn disk_usage(path: &Path) -> Option<DiskSample>;

/// `disk_usage` of each path, skipping the `None`s, keeping the order.
pub fn read_disks(mounts: &[PathBuf]) -> Vec<DiskSample>;
```

This is the single `unsafe` block of the project: `libc::statvfs` with a `CString` built from
`path.as_os_str().as_bytes()` (`std::os::unix::ffi::OsStrExt`) and a `std::mem::MaybeUninit<libc::statvfs>`.
Write a `// SAFETY:` comment. The `libc` crate is already a dependency.

### `src/collect/net.rs`

```rust
pub struct NetCounters { pub iface: String, pub rx_bytes: u64, pub tx_bytes: u64 }

/// Parses `/proc/net/dev`, skipping the two header lines and the `lo` interface, keeping file order.
/// Columns after `iface:` — 1st is rx bytes, 9th is tx bytes.
pub fn parse_net_dev(text: &str) -> Vec<NetCounters>;

pub struct NetSample {
    pub iface: String,
    /// Bytes per second; `None` on the first sample of an interface.
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
    pub rx_total: u64,
    pub tx_total: u64,
}

/// Keeps one pair of `util::rate::Rate` per interface.
#[derive(Debug, Default)]
pub struct NetReader { /* private */ }

impl NetReader {
    pub fn new() -> Self;
    /// Reads `proc/net/dev` under `sysroot`. Interfaces with `rx_total + tx_total == 0` are left out.
    pub fn sample(&mut self, sysroot: &Path, now: Instant) -> Vec<NetSample>;
    /// Same computation from already-parsed counters (what `sample` calls; used by tests).
    pub fn rates(&mut self, counters: Vec<NetCounters>, now: Instant) -> Vec<NetSample>;
}
```

### `src/collect/process.rs`

```rust
pub enum ServerRole { Router, Instance }

pub struct ProcSample {
    pub pid: u32,
    /// `Instance` when the command line has `--alias`, else `Router`.
    pub role: ServerRole,
    /// Value following `--alias`.
    pub model: Option<String>,
    /// Value following `--port`.
    pub port: Option<u16>,
    /// `VmRSS`, in bytes; 0 when the line is missing.
    pub rss: u64,
    pub threads: u32,
}

/// Every `proc/<pid>` under `sysroot` whose `comm` is exactly `llama-server`, ordered by pid.
pub fn read_server_procs(sysroot: &Path) -> Vec<ProcSample>;
```

## Pitfalls

- `wlp9s0: 9876543210 …` — the name ends at the **first** `:`; the kernel may print `eth0:123` with no space
  after the colon: split on `:` first, then on whitespace.
- `cmdline` is NUL-separated, often with a trailing NUL: split on `\0`, drop empty items. Read it with
  `std::fs::read` (bytes) and `String::from_utf8_lossy`.
- Only directories whose name is digits only are processes (`proc/net`, `proc/sys` are not).
- A process can disappear between `read_dir` and the read: any failed read just skips that process.

## Tests — `tests/disk_net_process.rs`

- `disk_usage_of_root`: `disk_usage(Path::new("/"))` is `Some`, `total > 0`, `used <= total`, `mount` = `/`.
- `disk_usage_of_missing_path`: `/nonexistent-llamatop` → `None`; `read_disks` of `["/", "/nonexistent-llamatop"]`
  → length 1.
- `net_dev_parsing`: fixture → 2 entries, `wlp9s0` (9876543210 / 1234567890) then `enp8s0` (0 / 0).
- `net_dev_without_space_after_colon`: a line `eth0:100 0 0 0 0 0 0 0 200 0 0 0 0 0 0 0` after two header
  lines → rx 100, tx 200.
- `net_rates`: `rates` with `wlp9s0` rx 1000 / tx 500 at t0 → one sample, both rates `None`; then rx 3000 /
  tx 1500 at t0 + 2 s → rx 1000.0, tx 500.0.
- `silent_interfaces_are_hidden`: `sample` on the fixture sysroot → only `wlp9s0`.
- `server_processes`: 2 entries. First: pid 1201, `Router`, model `None`, port 8080, rss 83886080,
  threads 12. Second: pid 48211, `Instance`, model `peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M`,
  port 35335, rss 9663676416, threads 34.
- `no_processes_on_empty_root`: `/nonexistent-llamatop` → empty, no panic.

## Acceptance criteria

- [x] Exact public API above; exactly one `unsafe` block in the crate, with its `// SAFETY:` comment.
- [x] The 8 tests exist under these names and pass.
- [x] No fixture modified.
- [x] `bash .devbox/check` is green.
