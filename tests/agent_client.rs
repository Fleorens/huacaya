//! The dashboard's client for the agent (`src/agent/client.rs`), the config/token resolution
//! (`src/config.rs`), the `[agent]` settings round-trip (`src/settings.rs`), and the setup question
//! (`src/setup.rs`), per `tasks/T28-agent-client.md`.
//!
//! The client tests start a real agent with `agent::server::start` on `127.0.0.1:0` (fixture sysroot,
//! `cat` log command, token `t`), as in `tests/agent_server.rs`.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{CommandFactory, FromArgMatches, Parser};
use llamatop::agent::client::AgentClient;
use llamatop::agent::server::{self, AgentConfig};
use llamatop::collect::llama::{ModelInfo, parse_models};
use llamatop::config::Cli;
use llamatop::settings::{self, AgentSettings, LlamaSettings, Settings};
use llamatop::setup::{self, Probe};

/// The agent config pointed at the fixture machine and log, listening on an OS-assigned port.
fn config() -> AgentConfig {
    AgentConfig {
        listen: "127.0.0.1:0".parse().expect("a loopback address with an OS-assigned port"),
        token: Some("t".to_string()),
        sysroot: PathBuf::from(format!("{}/tests/fixtures/sysroot", env!("CARGO_MANIFEST_DIR"))),
        mounts: vec![PathBuf::from("/")],
        log_cmd: format!("cat {}/tests/fixtures/journal/llama-server.txt", env!("CARGO_MANIFEST_DIR")),
        interval: Duration::from_millis(200),
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temp path under the system temp dir, so parallel tests do not clash.
fn temp_path() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("llamatop-agent-test-{}-{}", std::process::id(), n))
}

#[test]
fn client_reads_system_and_log() {
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, _handle) = server::start(config(), stop).expect("the agent starts");
    let port = addr.port();
    let url = format!("http://127.0.0.1:{port}");
    let client = AgentClient::new(&url, Some("t".to_string()));

    // The first sample arrives after at most one interval: retry up to 3 s.
    let start = Instant::now();
    let snapshot;
    loop {
        match client.system() {
            Ok(s) => {
                snapshot = s;
                break;
            }
            Err(_) => {
                if start.elapsed() > Duration::from_secs(3) {
                    panic!("no /v1/system sample within 3 s");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    assert_eq!(snapshot.hostname.as_deref(), Some("aibox-flo"));

    // The ring holds the whole fixture once: 35 fixture lines plus the source-ended line.
    let start = Instant::now();
    let mut page;
    loop {
        page = client.log(0).expect("log(0)");
        if page.lines.len() == 36 {
            break;
        }
        if start.elapsed() > Duration::from_secs(3) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(page.lines.len(), 36);

    assert_eq!(client.version().expect("version"), env!("CARGO_PKG_VERSION"));
}

#[test]
fn client_wrong_token() {
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, _handle) = server::start(config(), stop).expect("the agent starts");
    let port = addr.port();
    let url = format!("http://127.0.0.1:{port}");
    let client = AgentClient::new(&url, Some("x".to_string()));
    let err = client.system().expect_err("a wrong token is an error");
    assert!(err.to_string().contains("401"), "got: {err}");
}

#[test]
fn agent_token_order() {
    // The file's first line wins, trimmed.
    let path = temp_path();
    fs::write(&path, "  tok \n").unwrap();
    let mut cli = Cli::parse_from(["llamatop"]);
    cli.agent_token_file = Some(path.clone());
    assert_eq!(cli.agent_token(), Some("tok".to_string()));
    let _ = fs::remove_file(&path);

    // No file: the inline token is used, trimmed.
    let mut cli = Cli::parse_from(["llamatop"]);
    cli.agent_token_inline = Some("  inline  ".to_string());
    assert_eq!(cli.agent_token(), Some("inline".to_string()));
}

#[test]
fn settings_agent_round_trip() {
    let settings = Settings {
        llama: LlamaSettings::default(),
        agent: AgentSettings {
            url: Some("http://a:9099".to_string()),
            token: Some("tok".to_string()),
            token_file: Some(PathBuf::from("/etc/llamatop/agent-token")),
        },
    };
    let path = temp_path();
    settings::save(&path, &settings).unwrap();
    assert_eq!(settings::load(&path).unwrap().unwrap(), settings);
    let _ = fs::remove_file(&path);

    // `apply_settings` from the default fills `cli.agent` from the file.
    let matches = Cli::command().try_get_matches_from(["llamatop"]).unwrap();
    let mut cli = Cli::from_arg_matches(&matches).unwrap();
    cli.apply_settings(&matches, &settings);
    assert_eq!(cli.agent, Some("http://a:9099".to_string()));
}

/// A `Probe` for the setup test: `health` and `models` answer, `agent` answers the fixture hostname.
struct FakeProbe;

impl Probe for FakeProbe {
    fn health(&self, _url: &str) -> Result<()> {
        Ok(())
    }

    fn models(&self, _url: &str, _key: Option<&str>) -> Result<Vec<ModelInfo>> {
        let json = r#"{"data":[{"id":"m/Model:Q4_K_M","status":{"value":"loaded"}}]}"#;
        parse_models(json)
    }

    fn agent(&self, _url: &str, _token: Option<&str>) -> Result<String> {
        Ok("aibox-flo".to_string())
    }
}

#[test]
fn setup_asks_for_the_agent() {
    let mut input = Cursor::new("\n\nhttp://a:9099\n".to_string());
    let mut out = Vec::new();
    // First secret: the llama key (""); second: the agent token.
    let mut idx = 0usize;
    let keys = ["".to_string(), "tok-SECRET".to_string()];
    let mut secret = move |_: &str| -> Result<String> {
        let k = keys[idx].clone();
        idx += 1;
        Ok(k)
    };
    let settings =
        setup::wizard(&mut input, &mut out, &mut secret, &FakeProbe, &Settings::default()).unwrap();
    assert_eq!(settings.agent.url.as_deref(), Some("http://a:9099"));
    assert_eq!(settings.agent.token.as_deref(), Some("tok-SECRET"));
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("ok, aibox-flo"));
    assert!(!out.contains("SECRET"), "the token must not reach the output: {out:?}");
}

#[test]
fn setup_enter_keeps_the_agent_dash_removes_it() {
    let mut current = Settings::default();
    current.agent.url = Some("http://a:9099".to_string());
    current.agent.token = Some("tok".to_string());
    // Enter at the agent question keeps it (then Enter keeps the token).
    let mut secret = |_: &str| -> Result<String> { Ok(String::new()) };
    let mut input = Cursor::new("\n\n\n".to_string());
    let kept = setup::wizard(&mut input, &mut Vec::new(), &mut secret, &FakeProbe, &current).unwrap();
    assert_eq!(kept.agent.url.as_deref(), Some("http://a:9099"));
    assert_eq!(kept.agent.token.as_deref(), Some("tok"));
    // `-` removes it.
    let mut input = Cursor::new("\n\n-\n".to_string());
    let removed = setup::wizard(&mut input, &mut Vec::new(), &mut secret, &FakeProbe, &current).unwrap();
    assert_eq!(removed.agent.url, None);
    assert_eq!(removed.agent.token, None);
}
