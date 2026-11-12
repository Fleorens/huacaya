//! Parsing of the four answers of the llama.cpp server: `/props`, `/v1/models`, `/metrics`,
//! `/slots`. Pure functions over strings and `serde_json::Value`; the network is T06.
//!
//! An unknown or missing field is never an error: only text that is not JSON, or a wrong top-level
//! shape, is. See `docs/01-architecture.md` § 2.1 and `tasks/T05-llama-parsing.md`.

use anyhow::anyhow;
use serde::Serialize;

/// State of a model as returned by `/v1/models`.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub enum ModelState {
    Unloaded,
    Loading,
    Loaded,
    /// Any other `status.value`, kept as sent.
    Other(String),
}

/// One entry of `/v1/models`: its id, state, runtime settings and (when loaded) its size.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct ModelInfo {
    pub id: String,
    /// From `status.value`. An entry with **no** `status` object (single-model server) is `Loaded`.
    pub state: ModelState,
    /// `context_length`: the training context.
    pub ctx_train: Option<u64>,
    /// The following five come from `status.args` (see `arg_value`).
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

/// The value that follows `flag` in a command line, `None` when the flag is absent or last.
pub fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

/// Parses `GET /v1/models`: the `data` array, in the order sent.
pub fn parse_models(json: &str) -> anyhow::Result<Vec<ModelInfo>> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|_| anyhow!("models response is not JSON"))?;
    let obj = v.as_object().ok_or_else(|| anyhow!("models response is not a JSON object"))?;
    let data = obj
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| arr.iter().map(parse_model).collect())
        .unwrap_or_default();
    Ok(data)
}

/// One model of `/v1/models`.
fn parse_model(m: &serde_json::Value) -> ModelInfo {
    let id = m.get("id").and_then(|s| s.as_str()).unwrap_or_default().to_string();

    let state = match m.get("status") {
        None => ModelState::Loaded,
        Some(status) => match status.get("value").and_then(|v| v.as_str()) {
            Some("unloaded") => ModelState::Unloaded,
            Some("loading") => ModelState::Loading,
            Some("loaded") => ModelState::Loaded,
            Some(other) => ModelState::Other(other.to_string()),
            None => ModelState::Unloaded,
        },
    };

    let args: Vec<String> = m
        .get("status")
        .and_then(|s| s.get("args"))
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();

    ModelInfo {
        id,
        state,
        ctx_train: m.get("context_length").and_then(|v| v.as_u64()),
        ctx_size: arg_value(&args, "--ctx-size").and_then(|s| s.parse::<u64>().ok()),
        n_cpu_moe: arg_value(&args, "--n-cpu-moe").and_then(|s| s.parse::<u32>().ok()),
        n_gpu_layers: arg_value(&args, "--n-gpu-layers").and_then(|s| s.parse::<u32>().ok()),
        threads: arg_value(&args, "--threads").and_then(|s| s.parse::<u32>().ok()),
        spec_type: arg_value(&args, "--spec-type").map(str::to_string),
        model_path: arg_value(&args, "--model").map(str::to_string),
        vision: m
            .get("architecture")
            .and_then(|a| a.get("input_modalities"))
            .and_then(|mods| mods.as_array())
            .map(|mods| mods.iter().any(|x| matches!(x.as_str(), Some("image"))))
            .unwrap_or(false),
        size_bytes: m.get("meta").and_then(|mm| mm.get("size")).and_then(|v| v.as_u64()),
        n_params: m.get("meta").and_then(|mm| mm.get("n_params")).and_then(|v| v.as_u64()),
        ftype: m.get("meta").and_then(|mm| mm.get("ftype")).and_then(|v| v.as_str()).map(str::to_string),
    }
}

/// The router flag and build id of `/props`.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct ServerProps {
    /// `true` when `role` is `"router"`.
    pub router: bool,
    pub build_info: Option<String>,
}

/// Parses `GET /props`.
pub fn parse_props(json: &str) -> anyhow::Result<ServerProps> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|_| anyhow!("props response is not JSON"))?;
    let obj = v.as_object().ok_or_else(|| anyhow!("props response is not a JSON object"))?;
    Ok(ServerProps {
        router: obj.get("role").and_then(|r| r.as_str()) == Some("router"),
        build_info: obj.get("build_info").and_then(|b| b.as_str()).map(str::to_string),
    })
}

/// Counters and gauges of `/metrics`. A metric absent from the text is `0.0` (or `None` for the two
/// speculative-decoding counters, absent when the model has no draft).
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize, Default)]
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
        let (name, value) = match line.rsplit_once(' ') {
            Some((name, value)) => (name, value),
            None => continue,
        };
        // A line whose name carries labels (`name{…} value`) is ignored, so `..._per_pos_total{…}`
        // is not mistaken for `..._accepted_tokens_total`.
        if name.contains('{') {
            continue;
        }
        let value: f64 = match value.parse() {
            Ok(value) => value,
            Err(_) => continue,
        };
        match name.strip_prefix("llamacpp:").unwrap_or(name) {
            "prompt_tokens_total" => m.prompt_tokens_total = value,
            "prompt_tokens_cached_total" => m.prompt_tokens_cached_total = value,
            "prompt_seconds_total" => m.prompt_seconds_total = value,
            "tokens_predicted_total" => m.tokens_predicted_total = value,
            "tokens_predicted_seconds_total" => m.tokens_predicted_seconds_total = value,
            "n_decode_total" => m.n_decode_total = value,
            "n_tokens_max" => m.n_tokens_max = value,
            "requests_processing" => m.requests_processing = value,
            "requests_deferred" => m.requests_deferred = value,
            "spec_decode_num_draft_tokens_total" => m.draft_tokens_total = Some(value),
            "spec_decode_num_accepted_tokens_total" => m.draft_accepted_total = Some(value),
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
        match (self.draft_accepted_total, self.draft_tokens_total) {
            (Some(accepted), Some(drafted)) if drafted != 0.0 => Some(accepted / drafted * 100.0),
            _ => None,
        }
    }
}

/// `num / den`, or `None` when the divisor is 0.
fn div(num: f64, den: f64) -> Option<f64> {
    if den == 0.0 { None } else { Some(num / den) }
}

/// One entry of `/slots`.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
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
pub fn parse_slots(json: &str) -> anyhow::Result<Vec<SlotInfo>> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|_| anyhow!("slots response is not JSON"))?;
    let arr = v.as_array().ok_or_else(|| anyhow!("slots response is not a JSON array"))?;
    Ok(arr.iter().map(parse_slot).collect())
}

/// One slot of `/slots`.
fn parse_slot(s: &serde_json::Value) -> SlotInfo {
    let next_token = s.get("next_token").and_then(|t| t.as_array()).and_then(|arr| arr.first());

    SlotInfo {
        id: s.get("id").and_then(|v| v.as_u64()).map(|n| n as u32).unwrap_or(0),
        id_task: s.get("id_task").and_then(|v| v.as_i64()).unwrap_or(0),
        n_ctx: s.get("n_ctx").and_then(|v| v.as_u64()).unwrap_or(0),
        is_processing: s.get("is_processing").and_then(|v| v.as_bool()).unwrap_or(false),
        n_prompt_tokens: s.get("n_prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        n_decoded: next_token.and_then(|o| o.get("n_decoded").and_then(|v| v.as_u64())).unwrap_or(0),
        n_remain: next_token.and_then(|o| o.get("n_remain").and_then(|v| v.as_i64())).unwrap_or(-1),
        speculative: s.get("speculative").and_then(|v| v.as_bool()).unwrap_or(false),
    }
}
