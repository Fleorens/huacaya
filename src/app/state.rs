//! The dashboard's pure state machine: what the UI reads and never changes.
//!
//! `App` is fed `Event`s (snapshots, log lines, keys, ticks) by `apply`; the UI only reads it.
//! Nothing here draws or starts a thread, so every behaviour is exercised by `tests/app_state.rs`.

use std::collections::VecDeque;

use crate::collect::llama::ModelState;
use crate::collect::llama_poll::LlamaSnapshot;
use crate::collect::logs::{Level, LogEvent, LogLine};
use crate::collect::sampler::SystemSnapshot;
use crate::util::History;
use crossterm::event::KeyEvent;

/// Log lines kept in memory; the oldest is dropped beyond.
pub const LOG_CAPACITY: usize = 2000;
/// Samples kept per graph (5 min at the default 1 s interval).
pub const HISTORY_CAPACITY: usize = 300;
/// Refresh periods offered by `+` / `-`, in milliseconds.
pub const INTERVAL_STEPS: [u64; 6] = [200, 500, 1000, 2000, 5000, 10_000];
/// Lines moved by `PgUp` / `PgDn`.
pub const PAGE: usize = 10;

/// What reaches the UI thread.
#[derive(Debug, Clone)]
pub enum Event {
    System(SystemSnapshot),
    Llama(LlamaSnapshot),
    Log(LogLine),
    Key(KeyEvent),
    /// The terminal changed size; the next frame is laid out again.
    Resize,
    /// The 100 ms animation tick.
    Tick,
}

/// The four screens, in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Overview,
    Models,
    Logs,
    System,
}

impl Screen {
    pub const ALL: [Screen; 4] = [Screen::Overview, Screen::Models, Screen::Logs, Screen::System];

    fn index(self) -> usize {
        match self {
            Screen::Overview => 0,
            Screen::Models => 1,
            Screen::Logs => 2,
            Screen::System => 3,
        }
    }

    /// Next in tab order, wrapping (`System` → `Overview`).
    pub fn next(self) -> Self {
        Screen::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    /// Previous in tab order, wrapping (`Overview` → `System`).
    pub fn prev(self) -> Self {
        Screen::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// `"Overview"`, `"Models"`, `"Logs"`, `"System"`.
    pub fn title(self) -> &'static str {
        match self {
            Screen::Overview => "Overview",
            Screen::Models => "Models",
            Screen::Logs => "Logs",
            Screen::System => "System",
        }
    }
}

/// Which levels the log shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelFilter {
    All,
    WarnAndErrors,
    Errors,
}

impl LevelFilter {
    /// `All` → `WarnAndErrors` → `Errors` → `All`.
    pub fn next(self) -> Self {
        match self {
            LevelFilter::All => LevelFilter::WarnAndErrors,
            LevelFilter::WarnAndErrors => LevelFilter::Errors,
            LevelFilter::Errors => LevelFilter::All,
        }
    }

    /// `All`: every level; `WarnAndErrors`: `Warn` and `Error`; `Errors`: `Error` only.
    pub fn accepts(self, level: Level) -> bool {
        matches!(
            (self, level),
            (LevelFilter::All, _)
                | (LevelFilter::WarnAndErrors, Level::Warn | Level::Error)
                | (LevelFilter::Errors, Level::Error)
        )
    }
}

/// What the log view shows and where it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct LogView {
    /// `true`: the view sticks to the newest line.
    pub follow: bool,
    /// Visible lines hidden below the view; 0 = the newest line is the last one shown.
    pub scroll: usize,
    /// Case-insensitive substring searched in `LogLine::text`; empty = no filter.
    pub filter: String,
    /// `true` while the user types the filter after `/`.
    pub editing_filter: bool,
    pub level: LevelFilter,
    /// `false`: lines whose event is `LogEvent::Proxy` are hidden. The router prints two of them for
    /// every polling round of llamatop itself, so they are noise by default.
    pub show_proxy: bool,
}

/// The last request, assembled from the timing lines of one task.
#[derive(Debug, Clone, PartialEq)]
pub struct LastRequest {
    pub task: Option<i64>,
    /// `HH:MM:SS` of the line that started this request.
    pub time: Option<String>,
    pub prompt_tokens: Option<u64>,
    pub prompt_tps: Option<f64>,
    pub gen_tokens: Option<u64>,
    pub gen_tps: Option<f64>,
    /// `accepted / generated × 100` of the `DraftAcceptance` line.
    pub draft_pct: Option<f64>,
    /// `n_tokens` of the `Released` line: the context size at the end of the request.
    pub ctx_tokens: Option<u64>,
}

/// The whole state of the dashboard. The UI reads it and never changes it.
#[derive(Debug, Clone)]
pub struct App {
    pub system: Option<SystemSnapshot>,
    pub llama: Option<LlamaSnapshot>,
    pub logs: VecDeque<LogLine>,
    /// `busy_pct` of the first GPU.
    pub gpu_load: History,
    /// `cpu.usage_pct`.
    pub cpu_usage: History,
    /// `live_gen_tps` of the first active model (0 when it is not generating).
    pub gen_tps: History,
    pub last_request: Option<LastRequest>,
    pub screen: Screen,
    pub help: bool,
    pub paused: bool,
    pub interval_ms: u64,
    pub log_view: LogView,
    /// Index into `llama.models` on the Models screen.
    pub model_selected: usize,
    /// Number of `Tick`s received, drives the animations.
    pub frame: u64,
    pub quit: bool,
}

impl App {
    /// Empty state: no snapshot, no line, `Screen::Overview`, follow on, filter empty, level `All`,
    /// `show_proxy` false, histories of `HISTORY_CAPACITY`, `frame` 0, everything else false / 0.
    pub fn new(interval_ms: u64) -> Self {
        App {
            system: None,
            llama: None,
            logs: VecDeque::new(),
            gpu_load: History::new(HISTORY_CAPACITY),
            cpu_usage: History::new(HISTORY_CAPACITY),
            gen_tps: History::new(HISTORY_CAPACITY),
            last_request: None,
            screen: Screen::Overview,
            help: false,
            paused: false,
            interval_ms,
            log_view: LogView {
                follow: true,
                scroll: 0,
                filter: String::new(),
                editing_filter: false,
                level: LevelFilter::All,
                show_proxy: false,
            },
            model_selected: 0,
            frame: 0,
            quit: false,
        }
    }

    /// Applies one event (rules below) and returns `true` when the screen must be redrawn.
    pub fn apply(&mut self, event: Event) -> bool {
        match event {
            Event::System(s) => {
                if let Some(busy) = s.gpus.first().and_then(|g| g.busy_pct) {
                    self.gpu_load.push(busy);
                }
                if let Some(cpu) = s.cpu.usage_pct {
                    self.cpu_usage.push(cpu);
                }
                self.system = Some(s);
                !self.paused
            }
            Event::Llama(s) => {
                if !s.active.is_empty() {
                    self.gen_tps.push(s.active[0].live_gen_tps.unwrap_or(0.0) as f32);
                }
                self.model_selected = self.model_selected.min(s.models.len().saturating_sub(1));
                self.llama = Some(s);
                !self.paused
            }
            Event::Log(l) => {
                let visible = self.line_visible(&l, &self.log_view.filter.to_ascii_lowercase());
                self.update_last_request(&l);
                self.logs.push_back(l);
                if self.logs.len() > LOG_CAPACITY {
                    self.logs.pop_front();
                }
                if !self.log_view.follow && visible {
                    self.log_view.scroll = self.log_view.scroll.saturating_add(1);
                }
                !self.paused
            }
            Event::Key(k) => {
                crate::app::keys::handle(self, k);
                true
            }
            Event::Resize => true,
            Event::Tick => {
                self.frame += 1;
                !self.paused && self.animating()
            }
        }
    }

    /// Whether a single line passes the current log view (level, hidden `Proxy`, text filter).
    /// `filter_lower` is `log_view.filter` already lower-cased, so a whole list costs one allocation.
    fn line_visible(&self, line: &LogLine, filter_lower: &str) -> bool {
        if !self.log_view.level.accepts(line.level) {
            return false;
        }
        if matches!(line.event, Some(LogEvent::Proxy { .. })) && !self.log_view.show_proxy {
            return false;
        }
        if !filter_lower.is_empty() && !line.text.to_ascii_lowercase().contains(filter_lower) {
            return false;
        }
        true
    }

    /// The log lines the log view shows, oldest first: level accepted, not a hidden `Proxy` line, and
    /// containing `log_view.filter` (ASCII case-insensitive) when it is not empty.
    pub fn visible_logs(&self) -> Vec<&LogLine> {
        let filter_lower = self.log_view.filter.to_ascii_lowercase();
        self.logs.iter().filter(|l| self.line_visible(l, &filter_lower)).collect()
    }

    /// `true` when something on screen moves by itself: a slot of an active model `is_processing`, or a
    /// model in state `Loading`. `false` without a llama snapshot.
    pub fn animating(&self) -> bool {
        let Some(llama) = &self.llama else {
            return false;
        };
        llama.active.iter().any(|m| m.slots.iter().any(|s| s.is_processing))
            || llama.models.iter().any(|m| m.state == ModelState::Loading)
    }

    /// Merges one classified line into `last_request`, following the timing-line rules.
    fn update_last_request(&mut self, line: &LogLine) {
        match line.event.as_ref() {
            Some(LogEvent::PromptTiming { tokens, tps }) => {
                self.last_request = Some(LastRequest {
                    task: line.task,
                    time: line.time.clone(),
                    prompt_tokens: Some(*tokens),
                    prompt_tps: Some(*tps),
                    gen_tokens: None,
                    gen_tps: None,
                    draft_pct: None,
                    ctx_tokens: None,
                });
            }
            Some(LogEvent::GenTiming { tokens, tps }) => {
                if let Some(req) = &mut self.last_request
                    && req.task == line.task
                {
                    req.gen_tokens = Some(*tokens);
                    req.gen_tps = Some(*tps);
                } else {
                    self.last_request = Some(LastRequest {
                        task: line.task,
                        time: line.time.clone(),
                        prompt_tokens: None,
                        prompt_tps: None,
                        gen_tokens: Some(*tokens),
                        gen_tps: Some(*tps),
                        draft_pct: None,
                        ctx_tokens: None,
                    });
                }
            }
            Some(LogEvent::DraftAcceptance { accepted, generated }) => {
                let draft_pct =
                    if *generated == 0 { None } else { Some(*accepted as f64 / *generated as f64 * 100.0) };
                if let Some(req) = &mut self.last_request
                    && req.task == line.task
                {
                    req.draft_pct = draft_pct;
                } else {
                    self.last_request = Some(LastRequest {
                        task: line.task,
                        time: line.time.clone(),
                        prompt_tokens: None,
                        prompt_tps: None,
                        gen_tokens: None,
                        gen_tps: None,
                        draft_pct,
                        ctx_tokens: None,
                    });
                }
            }
            Some(LogEvent::Released { n_tokens }) => {
                if let Some(req) = &mut self.last_request
                    && req.task == line.task
                {
                    req.ctx_tokens = Some(*n_tokens);
                } else {
                    self.last_request = Some(LastRequest {
                        task: line.task,
                        time: line.time.clone(),
                        prompt_tokens: None,
                        prompt_tps: None,
                        gen_tokens: None,
                        gen_tps: None,
                        draft_pct: None,
                        ctx_tokens: Some(*n_tokens),
                    });
                }
            }
            _ => {}
        }
    }
}
