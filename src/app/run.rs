//! The dashboard run loop: terminal init/restore, collector threads, input and tick threads, and the
//! `--demo` event generator (`tasks/T15-run-loop.md`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::agent::client::AgentClient;
use crate::app::clock;
use crate::app::demo;
use crate::app::{App, Event};
use crate::collect::llama_poll::Poller;
use crate::collect::logs::{self, Level, LogLine};
use crate::collect::sampler::Sampler;
use crate::config::Cli;
use crate::http::{Client, is_loopback_url};
use crate::ui::draw;
use crate::ui::panels::Ctx;
use crate::ui::theme::{ColorMode, Theme};

/// Runs the dashboard until the user quits. The terminal is restored on every exit path.
pub fn run(cli: &Cli) -> Result<()> {
    let theme = Theme::new(ColorMode::detect());
    let offset = clock::local_utc_offset();
    let mut app = App::new(cli.interval_ms);

    // One channel, one stop flag, one interval shared with the data threads so `+`/`-` take effect
    // without a restart.
    let (tx, rx) = channel::<Event>();
    let stop = Arc::new(AtomicBool::new(false));
    let interval = Arc::new(AtomicU64::new(cli.interval_ms));
    // Lives until `run` returns: dropping it kills the log command.
    let mut _follower = None;

    // Data threads.
    if cli.demo {
        let demo_tx = tx.clone();
        let demo_stop = stop.clone();
        let demo_interval = interval.clone();
        thread::spawn(move || demo_loop(demo_tx, demo_stop, demo_interval));
    } else {
        // The sampler and poller loops are written here (not with `sampler::spawn` / `llama_poll::spawn`)
        // because their interval is fixed and read live from `interval`. Hoist the `cli` values so the
        // threads own them and do not borrow `cli` (which is not `'static`).
        let sysroot = cli.sysroot.clone();
        let mounts = cli.mounts.clone();
        let url = cli.url.clone();
        let api_key = cli.api_key();
        let log_cmd = cli.log_cmd.clone();
        let agent_url = cli.agent.clone();
        let agent_token = cli.agent_token();

        // The poller: unchanged (direct-instance mode needs a loopback URL, which a remote dashboard does
        // not have). It runs with or without an agent.
        let poller_stop = stop.clone();
        let poller_tx = tx.clone();
        let poller_interval = interval.clone();
        // On a loopback URL, ask the loaded instance directly; from another machine, only the router.
        let client = Client::new(&url, api_key, Duration::from_secs(2));
        // Only on the real machine: a test sysroot's ports belong to nothing, and the key must not go there.
        let poller = if is_loopback_url(&url) && cli.sysroot == std::path::Path::new("/") {
            Poller::new(client).with_local_instances(cli.sysroot.clone())
        } else {
            Poller::new(client)
        };
        thread::spawn(move || {
            let mut poller = poller;
            loop {
                if poller_stop.load(Ordering::Relaxed) {
                    break;
                }
                if poller_tx.send(Event::Llama(poller.poll(Instant::now()))).is_err() {
                    break;
                }
                sleep(&poller_stop, poller_interval.load(Ordering::Relaxed));
            }
        });

        // With an agent: one thread reads the server's machine panels and log (no sampler, no follower).
        // Without it: the sampler and the log follower, as before.
        if let Some(agent_url) = agent_url {
            let agent_tx = tx.clone();
            let agent_stop = stop.clone();
            let agent_interval = interval.clone();
            thread::spawn(move || agent_loop(agent_tx, agent_stop, agent_interval, agent_url, agent_token));
        } else {
            let sampler_stop = stop.clone();
            let sampler_tx = tx.clone();
            let sampler_interval = interval.clone();
            thread::spawn(move || {
                let mut sampler = Sampler::new(sysroot, mounts);
                loop {
                    if sampler_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    if sampler_tx.send(Event::System(sampler.sample(Instant::now()))).is_err() {
                        break;
                    }
                    sleep(&sampler_stop, sampler_interval.load(Ordering::Relaxed));
                }
            });

            // The log follower: keep it alive until `run` returns, so dropping it at the end kills the
            // command. When the command cannot start, report once and keep going.
            let log_tx = tx.clone();
            _follower = match logs::follow(&log_cmd, move |line| log_tx.send(Event::Log(line)).is_ok()) {
                Ok(follower) => Some(follower),
                Err(e) => {
                    let _ = tx.send(Event::Log(LogLine {
                        time: None,
                        source: None,
                        instance: None,
                        task: None,
                        level: Level::Warn,
                        text: format!("log command failed: {e}"),
                        event: None,
                    }));
                    None
                }
            };
        }
    }

    // Input and tick threads end on the stop flag.
    let input_stop = stop.clone();
    let input_tx = tx.clone();
    thread::spawn(move || input_loop(input_stop, input_tx));

    let tick_stop = stop.clone();
    let tick_tx = tx.clone();
    thread::spawn(move || tick_loop(tick_stop, tick_tx));

    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(e) => {
            stop.store(true, Ordering::Relaxed);
            return Err(e.into());
        }
    };
    let loop_result = dashboard_loop(&mut terminal, &mut app, &theme, offset, &interval, &rx);

    // Whatever happened in the loop: stop the threads, restore the terminal, then return its result.
    // The poller is not joined: an HTTP request may still be waiting for its 2 s timeout.
    stop.store(true, Ordering::Relaxed);
    ratatui::restore();
    loop_result
}

/// The main loop: draw once, then feed one event plus everything already waiting to `App::apply`,
/// redraw only when an apply asked for it, and stop when the user quits.
fn dashboard_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    theme: &Theme,
    offset: i32,
    interval: &Arc<AtomicU64>,
    rx: &Receiver<Event>,
) -> Result<()> {
    let mut shown_clock = draw_frame(terminal, app, theme, offset)?;
    loop {
        let mut drew = false;
        let event = match rx.recv() {
            Ok(event) => event,
            Err(_) => break, // every sender gone: stop.
        };
        drew |= apply_event(app, event, interval);
        // A burst of already-waiting events costs one frame.
        while let Ok(event) = rx.try_recv() {
            drew |= apply_event(app, event, interval);
        }
        if app.quit {
            break;
        }
        // The header clock must tick even when nothing else moves (idle server, long interval).
        if drew || wall_clock(offset) != shown_clock {
            shown_clock = draw_frame(terminal, app, theme, offset)?;
        }
    }
    Ok(())
}

/// Applies one event and, on a key, mirrors `app.interval_ms` into the shared interval so the data
/// threads pick the new refresh up without a restart. Returns whether the screen must be redrawn.
fn apply_event(app: &mut App, event: Event, interval: &Arc<AtomicU64>) -> bool {
    let is_key = matches!(&event, Event::Key(_));
    let drew = app.apply(event);
    if is_key {
        interval.store(app.interval_ms, Ordering::Relaxed);
    }
    drew
}

/// Draws one frame, computing the wall clock for this frame from the system time.
/// Returns the clock it showed.
fn draw_frame(
    terminal: &mut ratatui::DefaultTerminal,
    app: &App,
    theme: &Theme,
    offset: i32,
) -> Result<String> {
    let wall = wall_clock(offset);
    let ctx = Ctx { theme, graphs: true, clock: wall.as_str() };
    terminal.draw(|f| draw::draw(app, &ctx, f.area(), f.buffer_mut()))?;
    Ok(wall)
}

/// The local wall-clock time, `HH:MM:SS`.
fn wall_clock(offset: i32) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    clock::hms(now, offset)
}

/// The events of the live demo between two instants of the demo clock `t0 < t1` (seconds since start):
/// `Event::System(demo::system(t1, state))` and `Event::Llama(demo::llama(t1, state))` when a data
/// period of `interval_s` was crossed (`(t0 / interval_s).floor() != (t1 / interval_s).floor()`), then
/// one `Event::Log` per line of `demo::log_lines(t0, t1)`; `state = demo::state_at(t1)`.
pub fn demo_events(t0: f64, t1: f64, interval_s: f64) -> Vec<Event> {
    let mut events = Vec::new();
    if (t0 / interval_s).floor() != (t1 / interval_s).floor() {
        let state = demo::state_at(t1);
        events.push(Event::System(demo::system(t1, state)));
        events.push(Event::Llama(demo::llama(t1, state)));
    }
    for line in demo::log_lines(t0, t1) {
        events.push(Event::Log(line));
    }
    events
}

/// The `--demo` thread: every 100 ms, emit the demo events since the previous instant of its own clock.
fn demo_loop(tx: Sender<Event>, stop: Arc<AtomicBool>, interval: Arc<AtomicU64>) {
    let start = Instant::now();
    let mut prev = 0.0;
    while !stop.load(Ordering::Relaxed) {
        let now = start.elapsed().as_secs_f64();
        // The shared interval is in milliseconds, `demo_events` takes seconds.
        let interval_s = interval.load(Ordering::Relaxed) as f64 / 1000.0;
        for event in demo_events(prev, now, interval_s) {
            if tx.send(event).is_err() {
                return;
            }
        }
        prev = now;
        sleep(&stop, 100);
    }
}

/// The input thread: `poll` with a 100 ms timeout, then `read`; `Key` → `Event::Key`, `Resize` →
/// `Event::Resize`, everything else ignored. Ends on the stop flag.
fn input_loop(stop: Arc<AtomicBool>, tx: Sender<Event>) {
    while !stop.load(Ordering::Relaxed) {
        let ready = match crossterm::event::poll(Duration::from_millis(100)) {
            Ok(ready) => ready,
            Err(_) => {
                // No usable tty: back off instead of spinning.
                thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        if ready {
            match crossterm::event::read() {
                Ok(crossterm::event::Event::Key(key)) => {
                    let _ = tx.send(Event::Key(key));
                }
                Ok(crossterm::event::Event::Resize(_, _)) => {
                    let _ = tx.send(Event::Resize);
                }
                _ => {}
            }
        }
    }
}

/// The tick thread: one `Event::Tick` every 100 ms. Ends on the stop flag.
fn tick_loop(stop: Arc<AtomicBool>, tx: Sender<Event>) {
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(100));
        if tx.send(Event::Tick).is_err() {
            return;
        }
    }
}

/// Sleeps `interval_ms` in slices of at most 100 ms, waking early when `stop` is set.
fn sleep(stop: &Arc<AtomicBool>, interval_ms: u64) {
    let interval = Duration::from_millis(interval_ms);
    let mut slept = Duration::ZERO;
    while slept < interval {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let slice = (interval - slept).min(Duration::from_millis(100));
        thread::sleep(slice);
        slept += slice;
    }
}

/// The agent thread: every shared interval, `system()` → `Event::System`, then `log(since)` → one
/// `Event::Log` per line. Warns once on the transition to unreachable, says once on the return; never floods.
fn agent_loop(
    tx: Sender<Event>,
    stop: Arc<AtomicBool>,
    interval: Arc<AtomicU64>,
    url: String,
    token: Option<String>,
) {
    let client = AgentClient::new(&url, token);
    let mut since = 0u64;
    let mut reachable: Option<bool> = None;
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }

        // One reachability verdict per round (ok only if both calls succeed), so a half-working agent
        // cannot alternate `agent back` / `agent unreachable` every round.
        let mut events = Vec::new();
        let mut error = None;
        match client.system() {
            Ok(snapshot) => events.push(Event::System(snapshot)),
            Err(e) => error = Some(format!("{e:#}")),
        }
        match client.log(since) {
            Ok(page) if page.next < since => {
                // The agent restarted: its numbering began again at 1, below our cursor. Start over from its
                // whole ring next round rather than waiting forever for numbers it will not reach soon.
                since = 0;
            }
            Ok(page) => {
                events.extend(page.lines.into_iter().map(Event::Log));
                since = page.next;
                if page.dropped {
                    let line = agent_message("agent log: some lines were missed", Level::Warn);
                    events.push(Event::Log(line));
                }
            }
            Err(e) => {
                error.get_or_insert(format!("{e:#}"));
            }
        }
        if let Some(line) = reachability_line(&mut reachable, error) {
            events.insert(0, Event::Log(line));
        }
        for event in events {
            if tx.send(event).is_err() {
                return;
            }
        }

        sleep(&stop, interval.load(Ordering::Relaxed));
    }
}

/// The reachability transition line, if the state changed this round: `agent back` (Info) on return,
/// `agent unreachable: <error>` (Warn) on the way down. Never more than one per change.
fn reachability_line(reachable: &mut Option<bool>, error: Option<String>) -> Option<LogLine> {
    let prev = reachable.replace(error.is_none());
    match (prev, error) {
        (Some(false), None) => Some(agent_message("agent back", Level::Info)),
        (Some(true) | None, Some(e)) => Some(agent_message(&format!("agent unreachable: {e}"), Level::Warn)),
        _ => None,
    }
}

fn agent_message(text: &str, level: Level) -> LogLine {
    LogLine {
        time: None,
        source: None,
        instance: None,
        task: None,
        level,
        text: text.to_string(),
        event: None,
    }
}
