//! Integration tests for `collect::llama`: parsing of the four server answers.
//!
//! Values come from `tests/fixtures/llama/` (real answers captured on the aibox). Floats compare
//! within 0.01.

use std::path::Path;

use llamatop::collect::llama::*;

/// Reads a fixture directly under `tests/fixtures/llama/`.
fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/llama/{name}"));
    std::fs::read_to_string(path).expect("fixture present")
}

/// True when two floats differ by less than the 0.01 tolerance.
fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn props_router() {
    let props = parse_props(&fixture("props-router.json")).expect("props parses");
    assert!(props.router);
    assert_eq!(props.build_info.as_deref(), Some("b12151-10d4846a5"));

    let empty = parse_props("{}").expect("empty props parses");
    assert!(!empty.router);
    assert_eq!(empty.build_info, None);
}

#[test]
fn models_list() {
    let models = parse_models(&fixture("models.json")).expect("models parse");
    assert_eq!(models.len(), 5);

    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "bartowski/Ornith-1.5-35B-A3B-GGUF:Q4_K_M",
            "gsq-qwen3.8-27b",
            "muse-glimmer-30b",
            "peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M",
            "unsloth/Qwen3-8B-GGUF:Q4_K_M",
        ]
    );

    let states: Vec<ModelState> = models.iter().map(|m| m.state.clone()).collect();
    assert_eq!(
        states,
        [
            ModelState::Unloaded,
            ModelState::Unloaded,
            ModelState::Unloaded,
            ModelState::Loaded,
            ModelState::Unloaded,
        ]
    );
}

#[test]
fn loaded_model_details() {
    let models = parse_models(&fixture("models.json")).expect("models parse");
    let m = &models[3];
    assert_eq!(m.ctx_train, Some(262144));
    assert_eq!(m.ctx_size, Some(196608));
    assert_eq!(m.n_cpu_moe, Some(21));
    assert_eq!(m.n_gpu_layers, Some(99));
    assert_eq!(m.threads, Some(8));
    assert_eq!(m.spec_type.as_deref(), Some("draft-mtp"));
    assert_eq!(
        m.model_path.as_deref(),
        Some("/srv/models/direct/Cyber-Tiel-Coder-35B-A3B-MTP-UD-Q4_K_M.gguf")
    );
    assert!(!m.vision);
    assert_eq!(m.size_bytes, Some(22512939520));
    assert_eq!(m.n_params, Some(35505251456));
    assert_eq!(m.ftype.as_deref(), Some("Q4_K - Medium"));
}

#[test]
fn unloaded_model_details() {
    let models = parse_models(&fixture("models.json")).expect("models parse");

    let m = &models[0];
    assert_eq!(m.ctx_size, Some(163840));
    assert_eq!(m.n_cpu_moe, Some(16));
    assert_eq!(m.spec_type, None);
    assert_eq!(m.model_path, None);
    assert!(m.vision);
    assert_eq!(m.size_bytes, None);

    let m3 = &models[2];
    assert_eq!(m3.spec_type.as_deref(), Some("draft-dflash"));
    assert_eq!(m3.n_cpu_moe, None);
}

#[test]
fn model_without_status_is_loaded() {
    let models = parse_models(r#"{"data":[{"id":"solo"}]}"#).expect("parses");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "solo");
    assert_eq!(models[0].state, ModelState::Loaded);
    assert_eq!(models[0].ctx_train, None);
    assert_eq!(models[0].ctx_size, None);
    assert_eq!(models[0].n_cpu_moe, None);
    assert_eq!(models[0].n_gpu_layers, None);
    assert_eq!(models[0].threads, None);
    assert_eq!(models[0].spec_type, None);
    assert_eq!(models[0].model_path, None);
    assert!(!models[0].vision);
    assert_eq!(models[0].size_bytes, None);
    assert_eq!(models[0].n_params, None);
    assert_eq!(models[0].ftype, None);
}

#[test]
fn models_bad_json_is_error() {
    assert!(parse_models("nope").is_err());
    assert!(parse_models("{}").expect("shape ok").is_empty());
}

#[test]
fn other_state_is_kept() {
    let models = parse_models(r#"{"data":[{"id":"x","status":{"value":"sleeping"}}]}"#).expect("parses");
    assert_eq!(models[0].state, ModelState::Other("sleeping".to_string()));

    let models = parse_models(r#"{"data":[{"id":"y","status":{"value":"loading"}}]}"#).expect("parses");
    assert_eq!(models[0].state, ModelState::Loading);
}

#[test]
fn metrics_values() {
    let m = parse_metrics(&fixture("metrics.prom"));
    assert_eq!(m.prompt_tokens_total, 108881.0);
    assert_eq!(m.prompt_tokens_cached_total, 2673440.0);
    assert_eq!(m.prompt_seconds_total, 169.371);
    assert_eq!(m.tokens_predicted_total, 24107.0);
    assert_eq!(m.tokens_predicted_seconds_total, 384.361);
    assert_eq!(m.n_decode_total, 8005.0);
    assert_eq!(m.n_tokens_max, 49353.0);
    assert_eq!(m.requests_processing, 0.0);
    assert_eq!(m.draft_tokens_total, Some(23280.0));
    assert_eq!(m.draft_accepted_total, Some(16281.0));
}

#[test]
fn metrics_derived() {
    let m = parse_metrics(&fixture("metrics.prom"));
    assert!(approx(m.avg_gen_tps().unwrap(), 62.72));
    assert!(approx(m.avg_prompt_tps().unwrap(), 642.86));
    assert!(approx(m.cache_reuse_pct().unwrap(), 96.09));
    assert!(approx(m.draft_acceptance_pct().unwrap(), 69.94));
}

#[test]
fn metrics_later_sample() {
    let m = parse_metrics(&fixture("metrics-t1.prom"));
    assert_eq!(m.tokens_predicted_total, 24231.0);
    assert_eq!(m.requests_processing, 1.0);
}

#[test]
fn metrics_empty_text() {
    let m = parse_metrics("");
    assert_eq!(m, Metrics::default());
    assert_eq!(m.avg_gen_tps(), None);
    assert_eq!(m.avg_prompt_tps(), None);
    assert_eq!(m.cache_reuse_pct(), None);
    assert_eq!(m.draft_acceptance_pct(), None);
}

#[test]
fn slots_idle() {
    let slots = parse_slots(&fixture("slots-idle.json")).expect("slots parse");
    assert_eq!(slots.len(), 1);
    let s = &slots[0];
    assert_eq!(s.id, 0);
    assert_eq!(s.id_task, 8021);
    assert_eq!(s.n_ctx, 196608);
    assert!(!s.is_processing);
    assert_eq!(s.n_prompt_tokens, 49353);
    assert_eq!(s.n_decoded, 0);
    assert_eq!(s.n_remain, -1);
    assert!(s.speculative);
}

#[test]
fn slots_busy() {
    let slots = parse_slots(&fixture("slots-busy.json")).expect("slots parse");
    let s = &slots[0];
    assert_eq!(s.id_task, 8040);
    assert!(s.is_processing);
    assert_eq!(s.n_prompt_tokens, 61250);
    assert_eq!(s.n_decoded, 427);
    assert_eq!(s.n_remain, 32341);
}

#[test]
fn slots_bad_shape() {
    assert!(parse_slots("{}").is_err());
    let slots = parse_slots("[{}]").expect("empty slot parses");
    assert_eq!(slots.len(), 1);
    let s = &slots[0];
    assert_eq!(s.id, 0);
    assert_eq!(s.id_task, 0);
    assert_eq!(s.n_ctx, 0);
    assert!(!s.is_processing);
    assert_eq!(s.n_prompt_tokens, 0);
    assert_eq!(s.n_decoded, 0);
    assert_eq!(s.n_remain, -1);
    assert!(!s.speculative);
}

#[test]
fn arg_value_lookup() {
    let args: Vec<String> = ["a", "--x", "1", "--y"].iter().map(|s| s.to_string()).collect();
    assert_eq!(arg_value(&args, "--x"), Some("1"));
    assert_eq!(arg_value(&args, "--y"), None);
    assert_eq!(arg_value(&args, "--z"), None);
}
