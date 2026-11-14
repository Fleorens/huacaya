//! The configuration file (`src/settings.rs` and `src/config.rs`), per `tasks/T23-config-file.md`.
//! Tests build their own matches and use temp files: they never read the real `~/.config` nor set
//! environment variables (tests run in parallel).

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use clap::{CommandFactory, FromArgMatches};

use huacaya::config::Cli;
use huacaya::settings::{self, AgentSettings, LlamaSettings, OpenCodeSettings, Settings};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temp path under the system temp dir, so parallel tests do not clash.
fn temp_path() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("huacaya-test-{}-{}", std::process::id(), n))
}

fn all_fields() -> Settings {
    Settings {
        llama: LlamaSettings {
            url: Some("http://10.0.0.1:8080".to_string()),
            api_key: Some("sk-TEST".to_string()),
            api_key_file: Some(PathBuf::from("/etc/huacaya/key")),
            log_cmd: Some("cat /tmp/log".to_string()),
        },
        agent: AgentSettings {
            url: Some("http://10.0.0.2:9099".to_string()),
            token: Some("tok-TEST".to_string()),
            token_file: Some(PathBuf::from("/etc/huacaya/agent-token")),
        },
        opencode: OpenCodeSettings {
            url: Some("http://10.0.0.3:8080".to_string()),
            user: Some("dev".to_string()),
            password: Some("pw-TEST".to_string()),
            password_file: Some(PathBuf::from("/etc/huacaya/opencode-password")),
        },
    }
}

#[test]
fn paths() {
    assert_eq!(settings::path_from(Some("/x"), Some("/h")), Some(PathBuf::from("/x/huacaya/config.toml")));
    assert_eq!(
        settings::path_from(Some(""), Some("/h")),
        Some(PathBuf::from("/h/.config/huacaya/config.toml"))
    );
    assert_eq!(settings::path_from(None, None), None);
    assert_eq!(settings::path_from(None, Some("")), None, "an empty HOME gives no default");
}

#[test]
fn round_trip() {
    use std::os::unix::fs::PermissionsExt;

    let path = temp_path();
    let settings = all_fields();
    settings::save(&path, &settings).unwrap();
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(settings::load(&path).unwrap().unwrap(), settings);
    let _ = fs::remove_file(&path);
}

#[test]
fn missing_file_is_none() {
    let path = temp_path();
    assert_eq!(settings::load(&path).unwrap(), None);
}

#[test]
fn invalid_file_is_an_error() {
    let path = temp_path();
    fs::write(&path, "[llama]\nulr = \"x\"\napi_key = \"sk-SECRET\"\n").unwrap();
    let message = format!("{}", settings::load(&path).unwrap_err());
    assert!(message.contains(path.to_str().unwrap()));
    assert!(!message.contains("SECRET"));
    let _ = fs::remove_file(&path);
}

#[test]
fn settings_fill_defaults() {
    let matches = Cli::command().try_get_matches_from(["huacaya"]).unwrap();
    let mut cli = Cli::from_arg_matches(&matches).unwrap();
    let settings = Settings {
        llama: LlamaSettings {
            url: Some("http://10.0.0.1:8080".to_string()),
            log_cmd: Some("cat x".to_string()),
            api_key: Some("k".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    cli.apply_settings(&matches, &settings);
    assert_eq!(cli.url, "http://10.0.0.1:8080");
    assert_eq!(cli.log_cmd, "cat x");
    assert_eq!(cli.api_key_inline, Some("k".to_string()));
}

#[test]
fn flags_win() {
    let matches = Cli::command()
        .try_get_matches_from(["huacaya", "--url", "http://127.0.0.1:8080", "--log-cmd", "true"])
        .unwrap();
    let mut cli = Cli::from_arg_matches(&matches).unwrap();
    let settings = Settings {
        llama: LlamaSettings {
            url: Some("http://10.0.0.1:8080".to_string()),
            log_cmd: Some("cat x".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    cli.apply_settings(&matches, &settings);
    assert_eq!(cli.url, "http://127.0.0.1:8080");
    assert_eq!(cli.log_cmd, "true");
}

#[test]
fn api_key_order() {
    // `LLAMA_API_KEY` wins over the inline key, so only run when it is unset.
    if std::env::var_os("LLAMA_API_KEY").is_some() {
        return;
    }
    let matches = Cli::command().try_get_matches_from(["huacaya"]).unwrap();
    let mut cli = Cli::from_arg_matches(&matches).unwrap();
    cli.api_key_inline = Some("inline".to_string());
    assert_eq!(cli.api_key(), Some("inline".to_string()));
}

/// The project was renamed from llamatop to huacaya (0.5): its configuration file is copied once, mode 600,
/// and the old one is left in place; an existing new file is never overwritten.
#[test]
fn legacy_configuration_is_taken_over() {
    use std::os::unix::fs::PermissionsExt;
    let base = temp_path();
    let legacy = base.join("llamatop").join("config.toml");
    let new = base.join("huacaya").join("config.toml");
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    fs::write(&legacy, "[llama]\nurl = \"http://aibox:8080\"\n").unwrap();

    assert_eq!(settings::migrate_legacy(&new).unwrap(), Some(legacy.clone()));
    assert_eq!(fs::read_to_string(&new).unwrap(), "[llama]\nurl = \"http://aibox:8080\"\n");
    assert_eq!(fs::metadata(&new).unwrap().permissions().mode() & 0o777, 0o600);
    assert!(legacy.exists());

    // Once the new file exists, nothing happens.
    fs::write(&legacy, "changed").unwrap();
    assert_eq!(settings::migrate_legacy(&new).unwrap(), None);
    assert_ne!(fs::read_to_string(&new).unwrap(), "changed");
    let _ = fs::remove_dir_all(&base);
}
