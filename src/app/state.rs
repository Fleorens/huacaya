//! The dashboard's pure state machine: what the UI reads and never changes.
//!
//! `App` is fed `Event`s (snapshots, log lines, keys, ticks) by `apply`; the UI only reads it.
//! Nothing here draws or starts a thread, so every behaviour is exercised by `tests/app_state.rs`.

use std::collections::VecDeque;

use crate::collect::llama::ModelState;
use crate::collect::llama_poll::LlamaSnapshot;
use crate::collect::logs::{LogEvent, LogLine};
use crate::collect::opencode::OpenCodeSnapshot;
use crate::collect::sampler::SystemSnapshot;
use crate::util::History;
use crossterm::event::KeyEvent;

use super::log_view::{LevelFilter, LogView};
use super::net_rates::update_net_rates;

/// Log lines kept in memory; the oldest is dropped beyond.
pub const LOG_CAPACITY: usize = 2000;
/// Samples kept per graph (5 min at the default 1 s interval).
pub const HISTORY_CAPACITY: usize = 300;
/// Refresh periods offered by `+` / `-`, in milliseconds.
pub const INTERVAL_STEPS: [u64; 6] = [200, 500, 1000, 2000, 5000, 10_000];
/// Lines moved by `PgUp` / `PgDn`.
pub const PAGE: usize = 10;
/// Requests kept in `App::requests`.
pub const REQUEST_HISTORY: usize = 50;

/// What reaches the UI thread.
#[derive(Debug, Clone)]
pub enum Event {
    System(SystemSnapshot),
    Llama(LlamaSnapshot),
    OpenCode(Box<OpenCodeSnapshot>),
    Log(LogLine),
    Key(KeyEvent),
    /// The terminal changed size; the next frame is laid out again.
    Resize,
    /// The 100 ms animation tick.
    Tick,
}

/// The five screens, in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Overview,
    Models,
    Logs,
    System,
    /// The agents screen (T35): the OpenCode queue, the followed session, its timeline.
    Agents,
}

impl Screen {
    pub const ALL: [Screen; 5] =
        [Screen::Overview, Screen::Models, Screen::Logs, Screen::System, Screen::Agents];

    fn index(self) -> usize {
        match self {
            Screen::Overview => 0,
            Screen::Models => 1,
            Screen::Logs => 2,
            Screen::System => 3,
            Screen::Agents => 4,
        }
    }

    /// Next in tab order, wrapping (`Agents` → `Overview`).
    pub fn next(self) -> Self {
        Screen::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    /// Previous in tab order, wrapping (`Overview` → `Agents`).
    pub fn prev(self) -> Self {
        Screen::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// `"Overview"`, `"Models"`, `"Logs"`, `"System"`, `"Agents"`.
    pub fn title(self) -> &'static str {
        match self {
            Screen::Overview => "Overview",
            Screen::Models => "Models",
            Screen::Logs => "Logs",
            Screen::System => "System",
            Screen::Agents => "Agents",
        }
    }
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
    /// The OpenCode sessions and steps, or `None` until the first round.
    pub opencode: Option<OpenCodeSnapshot>,
    /// The session the Agents screen follows; set by T35.
    pub agent_focus: Option<String>,
    /// Index into `opencode.sessions` on the Agents screen (T35).
    pub agent_selected: usize,
    /// Whether the Agents screen shows the map view instead of the list (T44); toggled by `m`.
    pub agent_map: bool,
    /// Wall-clock Unix ms of the frame being drawn, for the map's running clock (T44); 0 in tests.
    pub now_ms: i64,
    /// Seconds east of UTC, for `HH:MM:SS` on the Agents timeline (T35).
    pub utc_offset_s: i32,
    pub logs: VecDeque<LogLine>,
    /// `busy_pct` of the first GPU.
    pub gpu_load: History,
    /// `cpu.usage_pct`.
    pub cpu_usage: History,
    /// `live_gen_tps` of the first active model (0 when it is not generating).
    pub gen_tps: History,
    /// Per interface, the receive and transmit rates (bytes/s), in the snapshot's order.
    pub net_rates: Vec<(String, History, History)>,
    /// Every sensor shown instead of the grouped summary, on the System screen (T38).
    pub all_sensors: bool,
    pub last_request: Option<LastRequest>,
    /// Every request seen in the log, oldest first, at most REQUEST_HISTORY; the last one is last_request.
    pub requests: VecDeque<LastRequest>,
    /// (id_task, frame) when the first active model's slot 0 started processing that task.
    pub task_started: Option<(i64, u64)>,
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
    /// The data shown while paused (a copy taken when `p` was pressed); `None` when not paused.
    pub(super) frozen: Option<Box<App>>,
    /// How far `log_view.scroll` moved for lines that arrived while paused: the frozen log has none of them.
    pub(super) paused_scroll: usize,
}

impl App {
    /// Empty state: no snapshot, no line, `Screen::Overview`, follow on, filter empty, level `All`,
    /// `task` `None`, `tasks_focus` false, `show_proxy` false, `raw` off, histories of `HISTORY_CAPACITY`,
    /// `frame` 0, everything else false / 0.
    pub fn new(interval_ms: u64) -> Self {
        App {
            system: None,
            llama: None,
            opencode: None,
            agent_focus: None,
            agent_selected: 0,
            agent_map: true,
            now_ms: 0,
            utc_offset_s: 0,
            logs: VecDeque::new(),
            gpu_load: History::new(HISTORY_CAPACITY),
            cpu_usage: History::new(HISTORY_CAPACITY),
            gen_tps: History::new(HISTORY_CAPACITY),
            net_rates: Vec::new(),
            all_sensors: false,
            last_request: None,
            requests: VecDeque::new(),
            task_started: None,
            screen: Screen::Overview,
            help: false,
            paused: false,
            interval_ms,
            log_view: LogView {
                follow: true,
                scroll: 0,
                filter: String::new(),
                editing_filter: false,
                raw: false,
                task: None,
                tasks_focus: false,
                task_selected: 0,
                level: LevelFilter::All,
                show_proxy: false,
            },
            model_selected: 0,
            frame: 0,
            quit: false,
            frozen: None,
            paused_scroll: 0,
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
                update_net_rates(&mut self.net_rates, &s.nets);
                self.system = Some(s);
                !self.paused
            }
            Event::Llama(s) => {
                if !s.active.is_empty() {
                    self.gen_tps.push(s.active[0].live_gen_tps.unwrap_or(0.0) as f32);
                }
                // A new id_task on slot 0 restarts the timer, even without an idle snapshot in between.
                self.task_started = match s.active.first().and_then(|m| m.slots.first()) {
                    Some(slot) if slot.is_processing => match self.task_started {
                        Some((t, start)) if t == slot.id_task => Some((t, start)),
                        _ => Some((slot.id_task, self.frame)),
                    },
                    _ => None,
                };
                // The first list of models selects the loaded one: it is the model one wants to read about.
                let first_list = self.llama.as_ref().is_none_or(|l| l.models.is_empty());
                if first_list && let Some(i) = s.models.iter().position(|m| m.state == ModelState::Loaded) {
                    self.model_selected = i;
                }
                self.model_selected = self.model_selected.min(s.models.len().saturating_sub(1));
                self.llama = Some(s);
                !self.paused
            }
            Event::OpenCode(s) => {
                // Keep the queue selection valid and on the session shown: the followed one, else the
                // fetched focus. The list may reorder or lose sessions between two fetches.
                self.agent_selected = self.agent_selected.min(s.sessions.len().saturating_sub(1));
                let shown = self.agent_focus.as_deref().or(s.focus.as_deref());
                if let Some(i) = shown.and_then(|id| s.sessions.iter().position(|x| x.id == id)) {
                    self.agent_selected = i;
                }
                self.opencode = Some(*s);
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
                    if self.paused {
                        self.paused_scroll = self.paused_scroll.saturating_add(1);
                    }
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
        if let Some(task) = self.log_view.task
            && line.task.as_ref() != Some(&task)
        {
            return false;
        }
        true
    }

    /// The log lines the log view shows, oldest first: accepted level, not a hidden `Proxy`, filtered by text or task.
    pub fn visible_logs(&self) -> Vec<&LogLine> {
        let filter_lower = self.log_view.filter.to_ascii_lowercase();
        self.logs.iter().filter(|l| self.line_visible(l, &filter_lower)).collect()
    }

    /// `true` when something on screen moves by itself: a slot of an active model `is_processing`, or a
    /// model in state `Loading`; or the Agents map, while its activity has a pulse or a running station.
    pub fn animating(&self) -> bool {
        let llama_anim = self.llama.as_ref().is_some_and(|llama| {
            llama.active.iter().any(|m| m.slots.iter().any(|s| s.is_processing))
                || llama.models.iter().any(|m| m.state == ModelState::Loading)
        });
        llama_anim
            || self.screen == Screen::Agents
                && self.agent_map
                && self.opencode.as_ref().is_some_and(|s| {
                    !s.activity.pulses.is_empty() || s.activity.stations.iter().any(|x| x.running > 0)
                })
    }
}
