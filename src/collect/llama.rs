//! Parsing of the four answers of a llama.cpp server, no network here (T06 does the HTTP).
//!
//! Every parse is pure: an unknown or missing field is never an error. Only text that is not JSON,
//! or a wrong top-level shape, is an error. See `docs/01-architecture.md` § 2.1 and § 3.

use anyhow::{Context, Result};

/// The load state of a model, from `status.value`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum ModelState {
    Unloaded,
    Loading,
    Loaded,
    /// Any other `status.value`, kept as sent.
    Other(String),
}

/// A model as listed by `GET /v1/models`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ModelInfo {
    /// The model id, e.g. `peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M`.
    pub id: String,
    /// From `status.value`. An entry with **no** `status` object (single-model server) is `Loaded`.
    pub state: ModelState,
    /// `context_length`: the training context.
    pub ctx_train: Option<u64>,
    /// From `status.args` (see `arg_value`).
    pub ctx_size: Option<u64>, // --ctx-size
    pub n_cpu_moe: Option<u32>,     // --n-cpu-moe
    pub n_gpu_layers: Option<u32>,  // --n-gpu-layers
    pub threads: Option<u32>,       // --threads
    pub spec_type: Option<String>,  // --spec-type
    pub model_path: Option<String>, // --model
    /// `architecture.input_modalities` contains `"image"`.
    pub vision: bool,
    /// From `meta`, present only when loaded: `size`, `n_params`, `ftype`.
    pub size_bytes: Option<u64>,
    pub n_params: Option<u64>,
    pub ftype: Option<String>,
}

/// The value that follows `flag` in a command line, `None` when absent or last.
pub fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(|v| v.as_str())
}

/// Parses `GET /v1/models`: the `data` array, in the order sent.
pub fn parse_models(json: &str) -> Result<Vec<ModelInfo>> {
    let value: serde_json::Value = serde_json::from_str(json).context("parse_models: not JSON")?;
    let data = match value.get("data") {
        // No `data` key (e.g. `{}`): valid, just empty.
        None => return Ok(Vec::new()),
        Some(d) => d.as_array().context("parse_models: `data` is not an array")?,
    };
    let mut models = Vec::with_capacity(data.len());
    for entry in data {
        let id = entry.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let state = match entry.get("status").and_then(|s| s.get("value")).and_then(|v| v.as_str()) {
            Some("unloaded") => ModelState::Unloaded,
            Some("loading") => ModelState::Loading,
            Some("loaded") => ModelState::Loaded,
            Some(other) => ModelState::Other(other.to_string()),
            // No status object (single-model server): the model is loaded.
            None => ModelState::Loaded,
        };
        let args: Vec<String> = entry
            .get("status")
            .and_then(|s| s.get("args"))
            .and_then(|a| a.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let vision = entry
            .get("architecture")
            .and_then(|a| a.get("input_modalities"))
            .and_then(|v| v.as_array())
            .map(|mods| mods.iter().any(|m| m.as_str() == Some("image")))
            .unwrap_or(false);
        let meta = entry.get("meta");
        models.push(ModelInfo {
            id,
            state,
            ctx_train: entry.get("context_length").and_then(|v| v.as_u64()),
            ctx_size: arg_value(&args, "--ctx-size").and_then(|v| v.parse().ok()),
            n_cpu_moe: arg_value(&args, "--n-cpu-moe").and_then(|v| v.parse().ok()),
            n_gpu_layers: arg_value(&args, "--n-gpu-layers").and_then(|v| v.parse().ok()),
            threads: arg_value(&args, "--threads").and_then(|v| v.parse().ok()),
            spec_type: arg_value(&args, "--spec-type").map(str::to_string),
            model_path: arg_value(&args, "--model").map(str::to_string),
            vision,
            size_bytes: meta.and_then(|m| m.get("size")).and_then(|v| v.as_u64()),
            n_params: meta.and_then(|m| m.get("n_params")).and_then(|v| v.as_u64()),
            ftype: meta.and_then(|m| m.get("ftype")).and_then(|v| v.as_str()).map(str::to_string),
        });
    }
    Ok(models)
}

/// The server's role and build, from `GET /props`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ServerProps {
    pub router: bool,
    pub build_info: Option<String>,
}

/// `router` is true when `role` is `"router"`.
pub fn parse_props(json: &str) -> Result<ServerProps> {
    let value: serde_json::Value = serde_json::from_str(json).context("parse_props: not JSON")?;
    Ok(ServerProps {
        router: value.get("role").and_then(|v| v.as_str()) == Some("router"),
        build_info: value.get("build_info").and_then(|v| v.as_str()).map(str::to_string),
    })
}

/// Counters and gauges of `/metrics`. A metric absent from the text is 0.0 (or `None` for the two
/// speculative-decoding counters, absent when the model has no draft).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct Metrics {
    pub prompt_tokens_total: f64,
    pub prompt_tokens_cached_total: f64,
    pub prompt_seconds_total: f64,
    pub tokens_predicted_total: f64,
    pub tokens_predicted_seconds_total: f64,
    pub n_decode_total: f64,
    pub n_tokens_max: f64,
    pub requests_processing: f64,
    pub requests_deferred: f64,
    pub draft_tokens_total: Option<f64>, // spec_decode_num_draft_tokens_total
    pub draft_accepted_total: Option<f64>, // spec_decode_num_accepted_tokens_total
}

/// Parses Prometheus text. Never fails: comment lines (`#`), unknown metrics, lines with labels
/// (`name{…} value`) and unparsable values are ignored.
pub fn parse_metrics(text: &str) -> Metrics {
    let mut m = Metrics::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `name{labels} value`: drop the label line before splitting on the last space.
        let (name, value) = match line.rsplit_once(' ') {
            Some((n, v)) => (n, v),
            None => continue,
        };
        if name.contains('{') {
            continue;
        }
        let val: f64 = match value.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let name = name.strip_prefix("llamacpp:").unwrap_or(name);
        match name {
            "prompt_tokens_total" => m.prompt_tokens_total = val,
            "prompt_tokens_cached_total" => m.prompt_tokens_cached_total = val,
            "prompt_seconds_total" => m.prompt_seconds_total = val,
            "tokens_predicted_total" => m.tokens_predicted_total = val,
            "tokens_predicted_seconds_total" => m.tokens_predicted_seconds_total = val,
            "n_decode_total" => m.n_decode_total = val,
            "n_tokens_max" => m.n_tokens_max = val,
            "requests_processing" => m.requests_processing = val,
            "requests_deferred" => m.requests_deferred = val,
            "spec_decode_num_draft_tokens_total" => m.draft_tokens_total = Some(val),
            "spec_decode_num_accepted_tokens_total" => m.draft_accepted_total = Some(val),
            _ => {}
        }
    }
    m
}

impl Metrics {
    /// tokens_predicted_total / tokens_predicted_seconds_total; `None` when the divisor is 0.
    pub fn avg_gen_tps(&self) -> Option<f64> {
        div(self.tokens_predicted_total, self.tokens_predicted_seconds_total)
    }

    /// prompt_tokens_total / prompt_seconds_total; `None` when the divisor is 0.
    pub fn avg_prompt_tps(&self) -> Option<f64> {
        div(self.prompt_tokens_total, self.prompt_seconds_total)
    }

    /// cached / (cached + prompt_tokens_total) × 100; `None` when the sum is 0.
    pub fn cache_reuse_pct(&self) -> Option<f64> {
        let cached = self.prompt_tokens_cached_total;
        div(cached * 100.0, cached + self.prompt_tokens_total)
    }

    /// accepted / drafted × 100; `None` when either counter is absent or drafted is 0.
    pub fn draft_acceptance_pct(&self) -> Option<f64> {
        let (accepted, drafted) = match (self.draft_accepted_total, self.draft_tokens_total) {
            (Some(a), Some(d)) => (a, d),
            _ => return None,
        };
        div(accepted * 100.0, drafted)
    }
}

/// `numerator / denominator`, or `None` when the divisor is 0.
fn div(numerator: f64, denominator: f64) -> Option<f64> {
    if denominator == 0.0 { None } else { Some(numerator / denominator) }
}

/// One generation slot, from `GET /slots`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SlotInfo {
    pub id: u32,
    pub id_task: i64,
    pub n_ctx: u64,
    pub is_processing: bool,
    pub n_prompt_tokens: u64,
    /// `next_token[0].n_decoded` (0 when `next_token` is absent or empty).
    pub n_decoded: u64,
    /// `next_token[0].n_remain` (−1 when absent).
    pub n_remain: i64,
    pub speculative: bool,
}

/// Parses `GET /slots`: a JSON array.
pub fn parse_slots(json: &str) -> Result<Vec<SlotInfo>> {
    let value: serde_json::Value = serde_json::from_str(json).context("parse_slots: not JSON")?;
    let arr = value.as_array().context("parse_slots: not a JSON array")?;
    let mut slots = Vec::with_capacity(arr.len());
    for entry in arr {
        let (n_decoded, n_remain) = match entry.get("next_token").and_then(|v| v.as_array()) {
            Some(nt) if !nt.is_empty() => {
                let first = &nt[0];
                (
                    first.get("n_decoded").and_then(|v| v.as_u64()).unwrap_or(0),
                    first.get("n_remain").and_then(|v| v.as_i64()).unwrap_or(-1),
                )
            }
            _ => (0, -1),
        };
        slots.push(SlotInfo {
            id: entry.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            id_task: entry.get("id_task").and_then(|v| v.as_i64()).unwrap_or(0),
            n_ctx: entry.get("n_ctx").and_then(|v| v.as_u64()).unwrap_or(0),
            is_processing: entry.get("is_processing").and_then(|v| v.as_bool()).unwrap_or(false),
            n_prompt_tokens: entry.get("n_prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            n_decoded,
            n_remain,
            speculative: entry.get("speculative").and_then(|v| v.as_bool()).unwrap_or(false),
        });
    }
    Ok(slots)
}
