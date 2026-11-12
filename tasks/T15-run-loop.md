# T15 — Run loop: terminal, threads, live demo

| | |
|---|---|
| **Batch** | 2 — the dashboard |
| **Depends on** | T14 |
| **Read first** | `docs/01-architecture.md` § 4; `grep -n "pub " src/app/*.rs src/ui/draw.rs src/ui/panels/mod.rs src/collect/sampler.rs src/collect/llama_poll.rs src/collect/logs.rs src/config.rs` |
| **Deliverable** | `src/app/run.rs`, `src/app/clock.rs`, `tests/run_loop.rs`; `pub mod clock; pub mod run;` in `src/app/mod.rs`; the dashboard branch of `src/main.rs` |

## Why

Everything is written except the part that makes it a program: put the terminal in raw mode on the
alternate screen, start the collector threads, turn key presses and the 100 ms tick into `Event`s, feed them
to `App::apply`, redraw when asked, and give the terminal back in every case — quit, error or panic.
`--demo` runs the same loop on the T11 generator, so the dashboard can be tried without a server.

## Contract

### `src/app/clock.rs`

```rust
/// Parses the output of `date +%z` (`+0100`, `-0530`, with or without a trailing newline) into seconds
/// east of UTC. `None` on anything else.
pub fn parse_utc_offset(text: &str) -> Option<i32>;

/// The local UTC offset, read **once** by running `date +%z`; 0 (UTC) when it cannot be run or parsed.
pub fn local_utc_offset() -> i32;

/// `HH:MM:SS` of `unix_seconds + offset_s`, modulo one day (negative results wrap).
pub fn hms(unix_seconds: i64, offset_s: i32) -> String;
```

### `src/app/run.rs`

```rust
/// Runs the dashboard until the user quits. The terminal is restored on every exit path.
pub fn run(cli: &Cli) -> anyhow::Result<()>;

/// The events of the live demo between two instants of the demo clock `t0 < t1` (seconds since start):
/// `Event::System(demo::system(t1, state))` and `Event::Llama(demo::llama(t1, state))` when a data
/// period of `interval_s` was crossed (`(t0 / interval_s).floor() != (t1 / interval_s).floor()`), then
/// one `Event::Log` per line of `demo::log_lines(t0, t1)`; `state = demo::state_at(t1)`.
pub fn demo_events(t0: f64, t1: f64, interval_s: f64) -> Vec<Event>;
```

What `run` does, in order:

1. `ColorMode::detect()`, `clock::local_utc_offset()`, `App::new(cli.interval_ms)`.
2. One `mpsc::channel::<Event>()`, one `Arc<AtomicBool>` stop flag, one `Arc<AtomicU64>` holding the
   current interval in ms (shared with the data threads so `+` / `-` take effect without a restart).
3. Data threads (each sleeps in slices of at most 100 ms and ends when the stop flag is set or `send`
   fails):
   - **real**: a sampler thread (`Sampler::new(cli.sysroot.clone(), cli.mounts.clone())`, one `sample`
     per interval → `Event::System`); a poller thread (`Poller::new(Client::new(&cli.url, cli.api_key(),
     Duration::from_secs(2)))`, one `poll` per interval → `Event::Llama`); the log follower
     (`logs::follow(&cli.log_cmd, …)` → `Event::Log`, keep the `Follower` alive until the end). When
     `follow` fails, send one `Event::Log` with level `Warn` and text `log command failed: <error>`.
     The two loops are written here, not with `sampler::spawn` / `llama_poll::spawn`, because their
     interval is fixed.
   - **`--demo`**: one thread that, every 100 ms, sends `demo_events(previous t, current t, interval)` of
     its own clock (`Instant` since the thread started).
4. An input thread: `crossterm::event::poll(100 ms)` then `read()`; `Key` → `Event::Key`, `Resize` →
   `Event::Resize`, others ignored; ends on the stop flag.
5. A tick thread: `Event::Tick` every 100 ms; ends on the stop flag.
6. `ratatui::try_init()?` (raw mode, alternate screen, panic hook that restores), then the loop:
   draw once; then `recv()` one event, `apply` it, apply every event already waiting (`try_recv`) so a burst
   costs one frame; after each key, store `app.interval_ms` into the shared interval; break when
   `app.quit`; draw when any `apply` returned `true`. Drawing:
   `terminal.draw(|f| draw::draw(&app, &ctx, f.area(), f.buffer_mut()))` with `ctx.clock` =
   `clock::hms(now, offset)` computed for this frame.
7. Whatever happened in the loop (an error included): set the stop flag, `ratatui::restore()`, then return
   the loop's result. Do not join the poller thread: an HTTP request may still be waiting for its 2 s timeout.

### `src/main.rs`

The last branch becomes `llamatop::app::run::run(&cli)`. `--dump` and `--snapshot` keep priority.

## Pitfalls

- `ratatui::try_init` installs the restoring panic hook itself; do not add a second one.
- Raw mode swallows `Ctrl-C` as a key: it arrives as `KeyCode::Char('c')` with `CONTROL`, handled by T09.
- `crossterm::event::read` blocks: always `poll` first with a timeout, or the input thread never sees the
  stop flag.
- The `Follower` kills its command when dropped: keep it in a variable that lives until `run` returns.
- `SystemTime::now().duration_since(UNIX_EPOCH)` returns a `Result`: no `unwrap()`, fall back to 0.
- `run` itself cannot be tested without a terminal: keep its logic in `demo_events`, `clock` and small
  private functions, and test those.

## Tests

`tests/run_loop.rs`, exactly these names.

- `utc_offsets`: `parse_utc_offset("+0100\n")` → `Some(3600)`; `"-0530"` → `Some(-19800)`; `"+0000"` →
  `Some(0)`; `"0100"`, `"+01"`, `""`, `"+01:00"` → `None`.
- `clock_format`: `hms(0, 0)` → `00:00:00`; `hms(3_725, 0)` → `01:02:05`; `hms(86_399, 3600)` →
  `00:59:59`; `hms(0, -3600)` → `23:00:00`; `hms(1_762_934_400, 3600)` → `09:00:00`.
- `local_offset_is_sane`: `local_utc_offset()` is within ±14 h.
- `demo_events_data_period`: `demo_events(0.95, 1.05, 1.0)` holds exactly one `System` and one `Llama`;
  `demo_events(1.05, 1.15, 1.0)` holds none of them.
- `demo_events_logs`: summing the `Log` events of `demo_events(k * 0.1, (k + 1) * 0.1, 1.0)` for `k` in
  `0..400` gives the same count as `demo::log_lines(0.0, 40.0)`; the `Log` events of
  `demo_events(7.95, 8.05, 1.0)` are the two lines at `c = 8` (a `Proxy`, then a launch line).
- `demo_drives_app`: an `App::new(1000)` fed every event of `demo_events` from 0 to 20 s in 0.1 s steps
  (Generating by then) → `apply(Event::Tick)` returns `true`; fed on to 40 s → `last_request.gen_tokens ==
  Some(1040)` and `gen_tps.len() >= 39`.

## Acceptance criteria

- [x] The files exist with the exact public API above; `src/main.rs` runs the dashboard.
- [x] The 6 tests above exist under these names and pass.
- [x] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) `cargo run -- --demo` shows a moving dashboard, `q` quits and gives the
      shell back clean; on the aibox `llamatop --api-key-file …` shows the real server.
