//! Compact rendering of log lines.
//!
//! The real log is dominated by `slot print_timing: id N | task T | …`, so `compact` drops that
//! prefix, shows the task as a short dim column and rewrites the known events in a few words; the
//! raw line stays one key away (`r`). `strip_prefix` removes the `slot`/`srv` prefix from any text.
//! Pure, no I/O — exercised by `tests/logtext.rs`.

use crate::collect::logs::{LogEvent, LogLine};
use crate::ui::text::{pct, tps};
use crate::util::{count, short_model_name};

/// `line.text` without its llama.cpp source prefix, runs of whitespace collapsed to one space,
/// trimmed.
///
/// Split on whitespace: when the first token is `slot` or `srv` and the next ends with `:` (the
/// function name), drop both, then drop `id <n> |` and `task <t> |` when they follow. Everything
/// else is kept whole.
pub fn strip_prefix(text: &str) -> String {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut i = 0usize;
    if tokens.len() >= 2 && (tokens[0] == "slot" || tokens[0] == "srv") && tokens[1].ends_with(':') {
        i = 2;
        i = drop_kw(&tokens, i, "id");
        i = drop_kw(&tokens, i, "task");
    }
    tokens[i..].join(" ")
}

/// After the prefix, skip `kw <value> |` (three tokens) when `tokens[i]` is `kw`.
fn drop_kw(tokens: &[&str], i: usize, kw: &str) -> usize {
    if tokens.get(i) == Some(&kw) { i + 3 } else { i }
}

/// The message of `line` in compact form (rules below). Never empty for a non-empty `line.text`.
pub fn compact(line: &LogLine) -> String {
    match line.event.as_ref() {
        Some(LogEvent::GenProgress { n_gen, tps: gen_tps, tps_3s }) => {
            format!("n_gen {}  tg {} t/s  3s {} t/s", n_gen, tps(Some(*gen_tps)), tps(Some(*tps_3s)))
        }
        Some(LogEvent::PromptProgress { n_tokens, progress, tps: prompt_tps }) => format!(
            "prompt {} tok  {}  {} t/s",
            n_tokens,
            pct(Some(*progress as f64 * 100.0)),
            tps(Some(*prompt_tps))
        ),
        Some(LogEvent::PromptTiming { tokens, tps: prompt_tps }) => {
            format!("prompt {} tok  {} t/s", tokens, tps(Some(*prompt_tps)))
        }
        Some(LogEvent::GenTiming { tokens, tps: gen_tps }) => {
            format!("gen {} tok  {} t/s", tokens, tps(Some(*gen_tps)))
        }
        Some(LogEvent::DraftAcceptance { accepted, generated }) => {
            if *generated == 0 {
                format!("draft {accepted}/{generated} accepted  n/a")
            } else {
                let p = pct(Some(*accepted as f64 / *generated as f64 * 100.0));
                format!("draft {accepted}/{generated} accepted  {p}")
            }
        }
        Some(LogEvent::Released { n_tokens }) => format!("released  ctx {}", count(*n_tokens)),
        Some(LogEvent::Request { method, path, status }) => format!("{method} {path} {status}"),
        Some(LogEvent::Proxy { model, port }) => format!("→ {} :{port}", short_model_name(model).0),
        None => strip_prefix(&line.text),
    }
}
