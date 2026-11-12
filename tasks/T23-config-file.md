# T23 — Configuration file

| | |
|---|---|
| **Batch** | 4 — a native Linux tool |
| **Depends on** | T22 |
| **Read first** | `src/config.rs`; `src/main.rs`; `grep -n "cli\." src/app/run.rs src/dump.rs` |
| **Deliverable** | `src/settings.rs`, `tests/settings.rs`; `pub mod settings;` in `src/lib.rs`; changes to `src/config.rs` and `src/main.rs` |

## Why

llamatop is meant to be installed as a package and run from any Linux machine, not only on the server with
the right flags. Its settings (where the llama.cpp server is, its API key, how to read its log) go in one
file, written by the first-run setup of T24. Flags and environment variables keep priority, so nothing that
works today changes. The `toml` dependency is already in `Cargo.toml` (added by the orchestrator: TOML is the
usual format for such a file, and serde already describes the data).

## Contract

### `src/settings.rs`

```rust
/// The content of the configuration file. Every field is optional: a missing one falls back to the flag's
/// default.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub llama: LlamaSettings,
}

/// The `[llama]` table.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LlamaSettings {
    /// Base URL of the server, like `--url`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The API key itself (the file is created with mode 600).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// A file holding the key, like `--api-key-file`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_file: Option<PathBuf>,
    /// Like `--log-cmd`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_cmd: Option<String>,
}

/// `$XDG_CONFIG_HOME/llamatop/config.toml` when `XDG_CONFIG_HOME` is set and not empty, else
/// `$HOME/.config/llamatop/config.toml`; `None` without either variable.
pub fn default_path() -> Option<PathBuf>;

/// Same rule from given values, so it can be tested without touching the environment.
pub fn path_from(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf>;

/// Reads and parses `path`. A missing file is `Ok(None)`; an unreadable file or invalid TOML is an error
/// whose message names the file and never contains the key.
pub fn load(path: &Path) -> anyhow::Result<Option<Settings>>;

/// Writes `settings` as TOML to `path`, creating the parent directories (mode 700 for a directory it
/// creates) and the file with mode 600 **from the start** (`OpenOptionsExt::mode`, then `set_permissions`
/// to 600 again in case the file already existed with wider rights). Replaces the whole file.
pub fn save(path: &Path, settings: &Settings) -> anyhow::Result<()>;
```

### `src/config.rs`

- New flag: `#[arg(long, env = "LLAMATOP_CONFIG")] pub config: Option<PathBuf>` — "Configuration file;
  default `~/.config/llamatop/config.toml`".
- New field, not a flag: `#[arg(skip)] pub api_key_inline: Option<String>` — "The key from the
  configuration file".
- `api_key()` becomes: `LLAMA_API_KEY`, else the first line of `--api-key-file`, else `api_key_inline`, else
  `None` (doc comment updated). The key is still never printed.
- New method:

```rust
/// Fills, from `settings`, every value the user did not give by flag or environment variable: `url` and
/// `log_cmd` when `matches.value_source(id)` is `Some(ValueSource::DefaultValue)`; `api_key_file` when it is
/// `None`; `api_key_inline` always (it is only used last). Values given by flag or environment win.
pub fn apply_settings(&mut self, matches: &clap::ArgMatches, settings: &Settings);
```

### `src/main.rs`

Parse with `let matches = Cli::command().get_matches(); let mut cli = Cli::from_arg_matches(&matches)?;`
(`clap::{CommandFactory, FromArgMatches}`), then load `cli.config.clone().or_else(settings::default_path)`
with `settings::load`; when it gives `Some(s)`, `cli.apply_settings(&matches, &s)`. A load error is fatal
(`anyhow` context `configuration file <path>`). Then the existing branches.

### `README.md` and `docs/01-architecture.md`

README: a "Configuration" section with the file's path, an example, and the order "flag > environment >
file > default". Architecture § 6: replace "no configuration file: flags and environment only" by "one
optional configuration file (`~/.config/llamatop/config.toml`, mode 600); flags and environment win".

## Pitfalls

- `value_source` is about `ArgMatches`, so `apply_settings` needs the matches: the `Cli` struct alone cannot
  tell a default from a value given equal to the default.
- In tests, build matches with `Cli::command().try_get_matches_from([...])`; never read the real
  `~/.config` nor set environment variables (tests run in parallel): use `path_from` and temp files under
  `std::env::temp_dir()` with a unique name.
- `deny_unknown_fields` makes a typo (`[llama] ulr = …`) an error that names the file, rather than a silent
  default. The error must not echo the file's content (it may hold the key), and `toml::de::Error`'s
  `Display` quotes the offending source line: never use it. Build the message from `err.message()` (it names
  the field, not the value) and the line number computed from `err.span()`:
  `<path>: line <n>: <message>`.
- `LLAMATOP_URL` set in the environment counts as given (`ValueSource::EnvVariable`), so it wins over the file.

## Tests

`tests/settings.rs`, exactly these names.

- `paths`: `path_from(Some("/x"), Some("/h"))` → `/x/llamatop/config.toml`; `(Some(""), Some("/h"))` →
  `/h/.config/llamatop/config.toml`; `(None, None)` → `None`.
- `round_trip`: `save` then `load` of a `Settings` with all four fields → equal; the file's mode is `0o600`
  (`metadata().permissions().mode() & 0o777`).
- `missing_file_is_none`: `load` of a path that does not exist → `Ok(None)`.
- `invalid_file_is_an_error`: a file `[llama]\nulr = "x"\napi_key = "sk-SECRET"\n` → `Err`, whose message
  contains the path and not `SECRET`.
- `settings_fill_defaults`: matches of `["llamatop"]` + settings with url `http://10.0.0.1:8080`, log_cmd
  `cat x`, api_key `k` → `cli.url`, `cli.log_cmd` take them; `cli.api_key_inline` is `Some("k")`.
- `flags_win`: matches of `["llamatop", "--url", "http://127.0.0.1:8080", "--log-cmd", "true"]` (values equal
  or not to the defaults) + the same settings → `url` and `log_cmd` stay the flags' values.
- `api_key_order`: with `api_key_inline = Some("inline")` and no file → `api_key()` is `Some("inline")`
  (run this test only when `LLAMA_API_KEY` is unset; skip with an early return otherwise).

## Acceptance criteria

- [x] The public API above exists; `main.rs` loads the file; a missing file changes nothing.
- [x] The 7 tests above exist under these names and pass; every earlier test passes.
- [x] README and architecture § 6 updated.
- [x] `bash .devbox/check` is green.
