# T05 — Parsing the llama.cpp server answers

| | |
|---|---|
| **Batch** | 1 — data layer |
| **Depends on** | T02 |
| **Read first** | `docs/01-architecture.md` § 2.1 and § 3 |
| **Deliverable** | `src/collect/llama.rs`, `tests/llama_parsing.rs`; one `pub mod` line in `src/collect/mod.rs` |

## Why

Pure parsing of four answers of the server, no network here (T06 does the HTTP). The fixtures are **real
answers captured on the aibox on 2026-11-11**: trust them over your memory of the llama.cpp API.

## Fixtures (do not edit) — `tests/fixtures/llama/`

| File | Endpoint | Content |
|---|---|---|
| `props-router.json` | `GET /props` | `"role":"router"`, `"build_info":"b12151-10d4846a5"` |
| `models.json` | `GET /v1/models` | 5 models, the 4th is `loaded` |
| `metrics.prom` | `GET /metrics?model=…` | Prometheus text |
| `metrics-t1.prom` | same, later | counters moved, `requests_processing 1` |
| `slots-idle.json` | `GET /slots?model=…` | 1 slot, idle |
| `slots-busy.json` | same | 1 slot, generating (`n_decoded` 427) |

Look at them with `head -c 1500` or `python3 -m json.tool | head -80`; do not load `models.json` whole more
than once.

## Contract — `src/collect/llama.rs`

Every struct and enum derives `Debug, Clone, PartialEq, serde::Serialize`. Parse JSON through
`serde_json::Value` (or private `Deserialize` structs with `#[serde(default)]` everywhere): **an unknown or
missing field is never an error**. Only text that is not JSON, or a wrong top-level shape, is an error.

```rust
pub enum ModelState {
    Unloaded,
    Loading,
    Loaded,
    /// Any other `status.value`, kept as sent.
    Other(String),
}

pub struct ModelInfo {
    pub id: String,
    /// From `status.value`. An entry with **no** `status` object (single-model server) is `Loaded`.
    pub state: ModelState,
    /// `context_length`: the training context.
    pub ctx_train: Option<u64>,
    /// The following five come from `status.args` (see `arg_value`).
    pub ctx_size: Option<u64>,       // --ctx-size
    pub n_cpu_moe: Option<u32>,      // --n-cpu-moe
    pub n_gpu_layers: Option<u32>,   // --n-gpu-layers
    pub threads: Option<u32>,        // --threads
    pub spec_type: Option<String>,   // --spec-type
    pub model_path: Option<String>,  // --model
    /// `architecture.input_modalities` contains `"image"`.
    pub vision: bool,
    /// From `meta`, present only when loaded: `size`, `n_params`, `ftype`.
    pub size_bytes: Option<u64>,
    pub n_params: Option<u64>,
    pub ftype: Option<String>,
}

/// The value that follows `flag` in a command line, `None` when absent or last.
pub fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str>;

/// Parses `GET /v1/models`: the `data` array, in the order sent.
pub fn parse_models(json: &str) -> anyhow::Result<Vec<ModelInfo>>;

pub struct ServerProps { pub router: bool, pub build_info: Option<String> }
/// `router` is true when `role` is `"router"`.
pub fn parse_props(json: &str) -> anyhow::Result<ServerProps>;

/// Counters and gauges of `/metrics`. A metric absent from the text is 0.0 (or `None` for the two
/// speculative-decoding counters, absent when the model has no draft).
#[derive(Default)]
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
    pub draft_tokens_total: Option<f64>,     // spec_decode_num_draft_tokens_total
    pub draft_accepted_total: Option<f64>,   // spec_decode_num_accepted_tokens_total
}

/// Parses Prometheus text. Never fails: comment lines (`#`), unknown metrics, lines with labels
/// (`name{…} value`) and unparsable values are ignored.
pub fn parse_metrics(text: &str) -> Metrics;

impl Metrics {
    /// tokens_predicted_total / tokens_predicted_seconds_total; `None` when the divisor is 0.
    pub fn avg_gen_tps(&self) -> Option<f64>;
    /// prompt_tokens_total / prompt_seconds_total; `None` when the divisor is 0.
    pub fn avg_prompt_tps(&self) -> Option<f64>;
    /// cached / (cached + prompt_tokens_total) × 100; `None` when the sum is 0.
    pub fn cache_reuse_pct(&self) -> Option<f64>;
    /// accepted / drafted × 100; `None` when either counter is absent or drafted is 0.
    pub fn draft_acceptance_pct(&self) -> Option<f64>;
}

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
pub fn parse_slots(json: &str) -> anyhow::Result<Vec<SlotInfo>>;
```

## Pitfalls

- Metric names are prefixed `llamacpp:` — strip the prefix, then compare the **whole** name.
  `spec_decode_num_accepted_tokens_per_pos_total{position="0"} 6654` must **not** be read as
  `spec_decode_num_accepted_tokens_total`: a line whose name part contains `{` is ignored.
- Values use exponent notation: `2.67344e+06`. `str::parse::<f64>` handles it.
- `next_token` is an **array** holding one object, not an object.
- `status.args` is the whole command line; flags and values are separate items.
- `meta` exists only on the loaded model; `sidecars` only on some. Neither is required.

## Tests — `tests/llama_parsing.rs`

Read fixtures with `std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/llama/…"))`.
Float tolerance 0.01.

- `props_router`: router true, build `b12151-10d4846a5`. `parse_props("{}")` → router false, build `None`.
- `models_list`: 5 models; ids in order: `bartowski/Ornith-1.5-35B-A3B-GGUF:Q4_K_M`, `gsq-qwen3.8-27b`,
  `muse-glimmer-30b`, `peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M`, `unsloth/Qwen3-8B-GGUF:Q4_K_M`;
  states: `Unloaded`, `Unloaded`, `Unloaded`, `Loaded`, `Unloaded`.
- `loaded_model_details`: 4th model: ctx_train 262144, ctx_size 196608, n_cpu_moe 21, n_gpu_layers 99,
  threads 8, spec_type `draft-mtp`, model_path
  `/srv/models/direct/Cyber-Tiel-Coder-35B-A3B-MTP-UD-Q4_K_M.gguf`, vision false, size_bytes 22512939520,
  n_params 35505251456, ftype `Q4_K - Medium`.
- `unloaded_model_details`: 1st model: ctx_size 163840, n_cpu_moe 16, spec_type `None`, model_path `None`,
  vision true, size_bytes `None`. 3rd model: spec_type `draft-dflash`, n_cpu_moe `None`.
- `model_without_status_is_loaded`: `{"data":[{"id":"solo"}]}` → one model, `Loaded`, every option `None`.
- `models_bad_json_is_error`: `parse_models("nope")` is `Err`; `parse_models("{}")` → `Ok` and empty.
- `other_state_is_kept`: `{"data":[{"id":"x","status":{"value":"sleeping"}}]}` → `Other("sleeping")`;
  `"loading"` → `Loading`.
- `metrics_values`: `metrics.prom` → prompt_tokens_total 108881, cached 2673440, prompt_seconds 169.371,
  tokens_predicted_total 24107, predicted_seconds 384.361, n_decode_total 8005, n_tokens_max 49353,
  requests_processing 0, draft_tokens_total `Some(23280)`, draft_accepted_total `Some(16281)`.
- `metrics_derived`: avg_gen_tps 62.72; avg_prompt_tps 642.86; cache_reuse_pct 96.09;
  draft_acceptance_pct 69.94.
- `metrics_later_sample`: `metrics-t1.prom` → tokens_predicted_total 24231, requests_processing 1.
- `metrics_empty_text`: `parse_metrics("")` equals `Metrics::default()`; all four derived values `None`.
- `slots_idle`: 1 slot: id 0, id_task 8021, n_ctx 196608, is_processing false, n_prompt_tokens 49353,
  n_decoded 0, n_remain −1, speculative true.
- `slots_busy`: id_task 8040, is_processing true, n_prompt_tokens 61250, n_decoded 427, n_remain 32341.
- `slots_bad_shape`: `parse_slots("{}")` is `Err`; `parse_slots("[{}]")` → one slot, all zero / false, n_remain −1.
- `arg_value_lookup`: on `["a","--x","1","--y"]`: `--x` → `Some("1")`, `--y` → `None`, `--z` → `None`.

## Acceptance criteria

- [ ] Exact public API above.
- [ ] The 15 tests exist under these names and pass.
- [ ] No fixture modified.
- [ ] `bash .devbox/check` is green.
