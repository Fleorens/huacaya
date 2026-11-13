//! Summarising the tasks (requests) seen in the log, from their classified lines (T37).
//!
//! Pure, no I/O — exercised by `tests/log_tasks.rs`. A task is one `task` number; a line that has no
//! task, or a negative one, is ignored. Never panics.

use std::collections::HashMap;

use crate::collect::logs::{Level, LogEvent, LogLine};

/// How a task seen in the log ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskOutcome {
    Running,
    Done,
    Failed,
}

/// One task (request) summarised from its log lines.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSummary {
    pub task: i64,
    pub outcome: TaskOutcome,
    /// Seconds between its first and last line, from their `time` (`HH:MM:SS`); `None` without both times.
    pub seconds: Option<u64>,
    /// `Released { n_tokens }` when seen, else the last `GenProgress { n_gen }`, else `None`.
    pub tokens: Option<u64>,
}

/// The seconds of an `HH:MM:SS` string, or `None` when it is not shaped like that.
fn hms_to_secs(time: &Option<String>) -> Option<u64> {
    let parts: Vec<&str> = time.as_deref()?.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: u64 = parts[0].parse().ok()?;
    let m: u64 = parts[1].parse().ok()?;
    let s: u64 = parts[2].parse().ok()?;
    Some(h * 3600 + m * 60 + s)
}

/// The tasks of `lines` (only `task >= 0`), most recently seen first (by their last line), at most `max`.
/// `Failed` when one of its lines is `Level::Error`, else `Done` when a `Released` event was seen, else
/// `Running`. A last time earlier than the first (midnight passed) adds 86 400 s. Never panics.
pub fn log_tasks<'a>(lines: impl IntoIterator<Item = &'a LogLine>, max: usize) -> Vec<TaskSummary> {
    // Per task: the accumulated facts, and the index of its last line (for the "newest first" order).
    struct Acc {
        first_secs: Option<u64>,
        last_secs: Option<u64>,
        last_index: usize,
        saw_released: Option<u64>,
        last_gen: Option<u64>,
        saw_error: bool,
    }
    let mut accs: HashMap<i64, Acc> = HashMap::new();
    for (i, line) in lines.into_iter().enumerate() {
        let Some(task) = line.task.filter(|t| *t >= 0) else {
            continue;
        };
        let acc = accs.entry(task).or_insert_with(|| Acc {
            first_secs: None,
            last_secs: None,
            last_index: i,
            saw_released: None,
            last_gen: None,
            saw_error: false,
        });
        acc.last_index = i;
        if let Some(secs) = hms_to_secs(&line.time) {
            acc.first_secs.get_or_insert(secs);
            acc.last_secs = Some(secs);
        }
        match line.event.as_ref() {
            Some(LogEvent::Released { n_tokens }) => acc.saw_released = Some(*n_tokens),
            Some(LogEvent::GenProgress { n_gen, .. }) => acc.last_gen = Some(*n_gen),
            _ => {}
        }
        if line.level == Level::Error {
            acc.saw_error = true;
        }
    }

    // Most recently seen first (largest last-line index), then take `max`.
    let mut tasks: Vec<(i64, Acc)> = accs.into_iter().collect();
    tasks.sort_by_key(|a| std::cmp::Reverse(a.1.last_index));
    tasks
        .into_iter()
        .take(max)
        .map(|(task, a)| {
            let outcome = if a.saw_error {
                TaskOutcome::Failed
            } else if a.saw_released.is_some() {
                TaskOutcome::Done
            } else {
                TaskOutcome::Running
            };
            let seconds = match (a.first_secs, a.last_secs) {
                (Some(first), Some(last)) => {
                    Some(if last < first { last + 86_400 - first } else { last - first })
                }
                _ => None,
            };
            let tokens = a.saw_released.or(a.last_gen);
            TaskSummary { task, outcome, seconds, tokens }
        })
        .collect()
}
