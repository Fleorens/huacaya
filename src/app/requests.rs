//! The second `impl App` block: merging the llama log's timing lines into `LastRequest` (T31).
//! Kept apart from `state` so that file stays under the line limit.

use crate::app::LastRequest;
use crate::collect::logs::{LogEvent, LogLine};

use super::App;
use super::REQUEST_HISTORY;

impl App {
    /// Merges one classified line into `last_request`, following the timing-line rules.
    pub(super) fn update_last_request(&mut self, line: &LogLine) {
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
                self.sync_request(false);
            }
            Some(LogEvent::GenTiming { tokens, tps }) => {
                if let Some(req) = &mut self.last_request
                    && req.task == line.task
                {
                    req.gen_tokens = Some(*tokens);
                    req.gen_tps = Some(*tps);
                    self.sync_request(true);
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
                    self.sync_request(false);
                }
            }
            Some(LogEvent::DraftAcceptance { accepted, generated }) => {
                let draft_pct =
                    if *generated == 0 { None } else { Some(*accepted as f64 / *generated as f64 * 100.0) };
                if let Some(req) = &mut self.last_request
                    && req.task == line.task
                {
                    req.draft_pct = draft_pct;
                    self.sync_request(true);
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
                    self.sync_request(false);
                }
            }
            Some(LogEvent::Released { n_tokens }) => {
                if let Some(req) = &mut self.last_request
                    && req.task == line.task
                {
                    req.ctx_tokens = Some(*n_tokens);
                    self.sync_request(true);
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
                    self.sync_request(false);
                }
            }
            _ => {}
        }
    }

    /// Seconds since `task_started`, from the 100 ms ticks (`(frame - start) / 10`); `None` without a task.
    pub fn task_elapsed_s(&self) -> Option<u64> {
        let (_, start) = self.task_started?;
        Some(self.frame.saturating_sub(start) / 10)
    }

    /// Keep `requests` in step with `last_request`: replace the back when filling, else push a new one.
    pub(super) fn sync_request(&mut self, fill: bool) {
        let Some(req) = self.last_request.clone() else {
            return;
        };
        if fill {
            self.requests.back_mut().into_iter().for_each(|back| *back = req.clone());
        } else {
            self.requests.push_back(req);
            while self.requests.len() > REQUEST_HISTORY {
                self.requests.pop_front();
            }
        }
    }
}
