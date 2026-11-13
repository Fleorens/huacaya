//! The log view's state: which levels it shows and where it stands.

use crate::collect::logs::Level;

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
    /// `true`: log lines are shown as printed; `false`: in compact form.
    pub raw: bool,
    /// Only the lines of this task are visible; `None`: every task.
    pub task: Option<i64>,
    /// `true`: the Tasks list has the keys (`Up`/`Down`/`Enter`); the global keys still come first.
    pub tasks_focus: bool,
    /// Index into `log_tasks(&app.logs, 50)` while the Tasks list is focused.
    pub task_selected: usize,
    pub level: LevelFilter,
    /// `false`: lines whose event is `LogEvent::Proxy` are hidden. The router prints two of them for
    /// every polling round of llamatop itself, so they are noise by default.
    pub show_proxy: bool,
}
