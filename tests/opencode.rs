//! Integration tests for `collect::opencode`: the parser, the poller, the base64 helper and the
//! `job_number` (per `tasks/T34-opencode-client.md`).

use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;

use llamatop::collect::opencode::*;
use llamatop::http::base64;
use llamatop::settings::Settings;

/// The `tests/fixtures/opencode` directory.
fn opencode_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opencode")
}

/// The whole body of a fixture file.
fn fixture(name: &str) -> String {
    std::fs::read_to_string(opencode_dir().join(name)).expect("fixture present")
}

#[test]
fn base64_vectors() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
}

#[test]
fn job_numbers() {
    assert_eq!(job_number("#136 Execute tasks/T30-nvidia.md"), Some(136));
    assert_eq!(job_number("Refactor the parser"), None);
    assert_eq!(job_number("#x"), None);
    assert_eq!(job_number("#136"), Some(136));
    assert_eq!(job_number("#136x"), Some(136));
}

#[test]
fn sessions_fixture() {
    let sessions = parse_sessions(&fixture("sessions.json")).unwrap();
    assert_eq!(sessions.len(), 3);

    // First: running, job 136, the tokens in the fixture.
    let first = &sessions[0];
    assert!(first.running);
    assert_eq!(first.job, Some(136));
    assert_eq!(first.tokens_in, 49133);
    assert_eq!(first.tokens_out, 35718);
    assert_eq!(first.cache_read, 1524584);
    assert_eq!(first.model.as_deref(), Some("example/Coder-35B-A3B-GGUF:Q4_K_M"));
    assert_eq!(first.agent.as_deref(), Some("nightshift"));
    assert_eq!(first.directory.as_deref(), Some("/work/.wt/demo-136"));

    // Second: succeeded, not running, job 131.
    let second = &sessions[1];
    assert_eq!(second.outcome.as_deref(), Some("succeeded"));
    assert!(!second.running);
    assert_eq!(second.job, Some(131));

    // Third: failed, job None (no leading `#`).
    let third = &sessions[2];
    assert_eq!(third.outcome.as_deref(), Some("failed"));
    assert_eq!(third.job, None);
}

#[test]
fn steps_fixture() {
    let steps = parse_steps(&fixture("messages.json"), Some("/work/.wt/demo-131")).unwrap();
    assert_eq!(steps.len(), 10);
    let s = |i: usize| &steps[i];

    assert_eq!(s(0).kind, StepKind::Think);
    assert_eq!(s(0).summary, "Read the sheet first, then the cited sections.");
    assert_eq!(s(0).ok, None);

    assert_eq!(s(1).kind, StepKind::Read);
    assert_eq!(s(1).summary, "tasks/T24-setup-wizard.md");
    assert_eq!(s(1).ok, Some(true));

    assert_eq!(s(2).kind, StepKind::Search);
    assert_eq!(s(2).summary, "pub fn in src");
    assert_eq!(s(2).ok, Some(true));

    assert_eq!(s(3).kind, StepKind::Read);
    assert_eq!(s(3).summary, "src/missing.rs");
    assert_eq!(s(3).ok, Some(false));
    assert_eq!(s(3).detail.as_deref(), Some("File not found"));

    assert_eq!(s(4).kind, StepKind::Write);
    assert_eq!(s(4).summary, "tests/setup.rs");
    assert_eq!(s(4).detail.as_deref(), Some("3 lines"));

    assert_eq!(s(5).kind, StepKind::Edit);
    assert_eq!(s(5).summary, "src/setup.rs");
    assert_eq!(s(5).detail.as_deref(), Some("+3 −1"));

    assert_eq!(s(6).kind, StepKind::Shell);
    assert_eq!(s(6).summary, "cargo test --test setup");
    assert_eq!(s(6).ok, Some(false));
    assert_eq!(s(6).detail.as_deref(), Some("test result: FAILED. 8 passed; 1 failed"));

    assert_eq!(s(7).kind, StepKind::Shell);
    assert_eq!(s(7).summary, "bash .devbox/check");
    assert_eq!(s(7).ok, Some(true));
    assert_eq!(s(7).detail.as_deref(), Some("test result: ok. 9 passed"));

    assert_eq!(s(8).kind, StepKind::Think);
    assert_eq!(
        s(8).summary,
        "All tests pass and the check is green. Now tick the acceptance criteria and set the status to livrée."
    );
    assert_eq!(s(8).ok, None);

    assert_eq!(s(9).kind, StepKind::Say);
    assert_eq!(s(9).summary, "Rapport : la fiche est livrée, les 9 tests passent.");
    assert_eq!(s(9).ok, None);
}

/// A `Fetch` that serves the two fixtures: any session id gets `messages.json`.
struct FakeFetch {
    sessions: String,
    messages: String,
}

impl llamatop::collect::llama_poll::Fetch for FakeFetch {
    fn get(&self, path: &str, _query: &[(&str, &str)]) -> anyhow::Result<String> {
        if path == "/api/session" { Ok(self.sessions.clone()) } else { Ok(self.messages.clone()) }
    }
}

#[test]
fn poller_focus() {
    let fetch = FakeFetch { sessions: fixture("sessions.json"), messages: fixture("messages.json") };
    let mut poller = OpenCodePoller::new(fetch);

    // No focus: the focus is the running session's id.
    let no_focus = poller.poll(None);
    assert_eq!(no_focus.focus.as_deref(), Some("ses_running0000000000000000"));
    assert_eq!(no_focus.sessions.len(), 3);

    // An explicit focus that is one of the sessions: that one.
    let focused = poller.poll(Some("ses_done000000000000000000"));
    assert_eq!(focused.focus.as_deref(), Some("ses_done000000000000000000"));
    assert_eq!(focused.steps.len(), 10);
}

#[test]
fn poller_error_hides_the_password() {
    let fetch =
        llamatop::http::Client::new_basic("http://127.0.0.1:9", "u", "pw-SECRET", Duration::from_secs(1));
    let mut poller = OpenCodePoller::new(fetch);
    let snapshot = poller.poll(None);
    let error = snapshot.error.expect("an error");
    assert!(error.contains("unreachable") || error.contains("connection"));
    assert!(!error.contains("SECRET"));
}

#[test]
fn settings_and_password() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/tmp/opencode-settings-{}.toml", std::process::id()));
    let settings = Settings {
        opencode: llamatop::settings::OpenCodeSettings {
            url: Some("http://127.0.0.1:8080".to_string()),
            user: Some("dev".to_string()),
            password: Some("pw".to_string()),
            password_file: None,
        },
        ..Default::default()
    };
    llamatop::settings::save(&path, &settings).unwrap();
    assert_eq!(llamatop::settings::load(&path).unwrap().unwrap(), settings);
    let _ = std::fs::remove_file(&path);

    // `opencode_password()` reads a temp file `  pw \n` as `pw`.
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/tmp/opencode-pw-{}.txt", std::process::id()));
    std::fs::write(&file, "  pw \n").unwrap();
    let cli =
        llamatop::config::Cli::parse_from(["llamatop", "--opencode-password-file", file.to_str().unwrap()]);
    assert_eq!(cli.opencode_password().as_deref(), Some("pw"));
    let _ = std::fs::remove_file(&file);
}

/// A `Probe` for the opencode setup test: `opencode` answers with a session count, `HTTP 401` for the
/// password `bad`.
struct FakeProbe;

impl llamatop::setup::Probe for FakeProbe {
    fn health(&self, _url: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn models(
        &self,
        _url: &str,
        _key: Option<&str>,
    ) -> anyhow::Result<Vec<llamatop::collect::llama::ModelInfo>> {
        let json = r#"{"data":[{"id":"m/Model:Q4_K_M","status":{"value":"loaded"}}]}"#;
        llamatop::collect::llama::parse_models(json)
    }
    fn agent(&self, _url: &str, _token: Option<&str>) -> anyhow::Result<String> {
        Ok("aibox-flo".to_string())
    }
    fn opencode(&self, _url: &str, _user: &str, password: Option<&str>) -> anyhow::Result<usize> {
        if password == Some("bad") {
            anyhow::bail!("the server refused the password");
        }
        Ok(3)
    }
}

#[test]
fn setup_asks_for_opencode() {
    // The Cursor only feeds the `answer()` reads (URL, log, agent URL, opencode URL, opencode user);
    // the `secret()` reads (key, password) come from `idx`, not the Cursor.
    let answers = "\n\n\nhttp://opencode:8080\ndev\n";
    let mut input = std::io::Cursor::new(answers.to_string());
    let mut out = Vec::new();
    // Secrets: API key (idx 0, empty), opencode password (idx 1, the real one). The agent token is not
    // asked because the agent URL is empty.
    let mut idx = 0usize;
    let mut secret = move |_: &str| -> anyhow::Result<String> {
        let k = if idx == 1 { "pw-SECRET".to_string() } else { String::new() };
        idx += 1;
        Ok(k)
    };
    let settings =
        llamatop::setup::wizard(&mut input, &mut out, &mut secret, &FakeProbe, &Settings::default()).unwrap();
    assert_eq!(settings.opencode.url.as_deref(), Some("http://opencode:8080"));
    assert_eq!(settings.opencode.user.as_deref(), Some("dev"));
    assert_eq!(settings.opencode.password.as_deref(), Some("pw-SECRET"));
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("ok, 3 sessions"));
    assert!(!out.contains("pw-SECRET"), "the password must not reach the output: {out:?}");
}
