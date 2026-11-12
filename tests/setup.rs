//! The first-run setup dialogue (`src/setup.rs`), per `tasks/T24-setup-wizard.md`.
//!
//! The dialogue is tested through `wizard` only: `std::io::Cursor` input, a `Vec<u8>` output, a closure
//! for `secret` that pops answers from a list, and a fake `Probe`. `read_secret` and `run` need a terminal.

use std::io::Cursor;

use anyhow::Result;
use llamatop::collect::llama::{ModelInfo, parse_models};
use llamatop::settings::{LlamaSettings, Settings};
use llamatop::setup::{self, Probe};

/// The log command the flag defaults to, when the URL is on loopback.
const LOG_CMD: &str = "journalctl -u llama-server -f -n 2000 --no-pager -q -o short-iso-precise";

/// A `Probe` for the tests: `health` fails for URLs containing `bad`; `models` answers `HTTP 401` for the
/// key `wrong`, else two models, one loaded.
struct FakeProbe;

impl Probe for FakeProbe {
    fn health(&self, url: &str) -> Result<()> {
        if url.contains("bad") {
            anyhow::bail!("connection refused");
        }
        Ok(())
    }

    fn models(&self, _url: &str, key: Option<&str>) -> Result<Vec<ModelInfo>> {
        if key == Some("wrong") {
            anyhow::bail!("GET /v1/models: HTTP 401");
        }
        let json = r#"{"data":[
            {"id":"other/Unloaded-Model:Q4_K_M","status":{"value":"unloaded"}},
            {"id":"peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M","status":{"value":"loaded"}}
        ]}"#;
        parse_models(json)
    }

    fn agent(&self, _url: &str, _token: Option<&str>) -> Result<String> {
        Ok("aibox-flo".to_string())
    }
}

/// Runs the dialogue with a cursor, a fixed secret answer, and the fake probe; returns the settings and the
/// whole output.
fn run_wizard(input: &str, key: &str, current: Settings) -> (Settings, String) {
    let mut input = Cursor::new(input.to_string());
    let mut out = Vec::new();
    let mut secret = |_: &str| -> Result<String> { Ok(key.to_string()) };
    let settings = setup::wizard(&mut input, &mut out, &mut secret, &FakeProbe, &current).unwrap();
    (settings, String::from_utf8(out).unwrap())
}

#[test]
fn normalize() {
    assert_eq!(setup::normalize_url("192.168.1.203:8080/"), "http://192.168.1.203:8080");
    assert_eq!(setup::normalize_url(" https://x/ "), "https://x");
}

#[test]
fn defaults_accepted() {
    let (settings, out) = run_wizard("\n\n\n", "", Settings::default());
    assert_eq!(settings.llama.url.as_deref(), Some("http://127.0.0.1:8080"));
    assert_eq!(settings.llama.api_key, None);
    assert_eq!(settings.llama.log_cmd.as_deref(), Some(LOG_CMD));
    assert!(out.contains("checking http://127.0.0.1:8080/health … ok"));
    assert!(out.contains("2 models, 1 loaded: Cyber-Tiel-Coder-35B-A3B"));
}

#[test]
fn remote_url_suggests_ssh_log() {
    let (settings, _) = run_wizard("192.168.1.203:8080\n\n\n", "", Settings::default());
    assert_eq!(
        settings.llama.log_cmd.as_deref(),
        Some("ssh 192.168.1.203 journalctl -u llama-server -f -n 2000 --no-pager -q -o short-iso-precise")
    );
}

#[test]
fn unreachable_then_other_url() {
    let (settings, _) = run_wizard("bad:1\nn\nhttp://ok:8080\n\n\n", "", Settings::default());
    assert_eq!(settings.llama.url.as_deref(), Some("http://ok:8080"));
}

#[test]
fn unreachable_kept() {
    let (settings, _) = run_wizard("bad:1\ny\n\n\n", "", Settings::default());
    assert_eq!(settings.llama.url.as_deref(), Some("http://bad:1"));
}

#[test]
fn key_refused_then_accepted() {
    let mut input = Cursor::new("\n\n\n".to_string());
    let mut out = Vec::new();
    let keys = ["wrong".to_string(), "right".to_string()];
    let mut idx = 0usize;
    let mut secret = move |_: &str| -> Result<String> {
        let k = keys[idx].clone();
        idx += 1;
        Ok(k)
    };
    let settings =
        setup::wizard(&mut input, &mut out, &mut secret, &FakeProbe, &Settings::default()).unwrap();
    assert_eq!(settings.llama.api_key.as_deref(), Some("right"));
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("the server refused the key"));
}

#[test]
fn existing_key_kept_and_never_shown() {
    let current = Settings {
        llama: LlamaSettings { api_key: Some("sk-OLD-123".to_string()), ..Default::default() },
        ..Default::default()
    };
    let (settings, out) = run_wizard("\n\n\n", "", current);
    assert_eq!(settings.llama.api_key.as_deref(), Some("sk-OLD-123"));
    assert!(!out.contains("sk-OLD"));
    assert!(!out.contains("right"));
}

#[test]
fn aborts_after_three_refusals() {
    let input = "bad:1\nn\nbad:2\nn\nbad:3\nn\n\n";
    let mut out = Vec::new();
    let mut secret = |_: &str| -> Result<String> { Ok(String::new()) };
    let result =
        setup::wizard(&mut Cursor::new(input), &mut out, &mut secret, &FakeProbe, &Settings::default());
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "setup aborted");
}

#[test]
fn end_of_input_aborts() {
    let input = "bad:1\n\n";
    let mut out = Vec::new();
    let mut secret = |_: &str| -> Result<String> { Ok(String::new()) };
    let result =
        setup::wizard(&mut Cursor::new(input), &mut out, &mut secret, &FakeProbe, &Settings::default());
    assert!(result.is_err());
}
