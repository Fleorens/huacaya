# T24 — First-run setup: `llamatop setup`

| | |
|---|---|
| **Batch** | 4 — a native Linux tool |
| **Depends on** | T23 |
| **Read first** | `src/settings.rs`; `src/config.rs`; `src/main.rs`; `grep -n "pub fn" src/http.rs src/collect/llama.rs src/util/format.rs` |
| **Deliverable** | `src/setup.rs`, `tests/setup.rs`; `pub mod setup;` in `src/lib.rs`; a `setup` subcommand in `src/config.rs`; the setup branches in `src/main.rs`; README |

## Why

Installed as a package, `llamatop` should work the first time it is typed: when there is no configuration yet,
it asks where the llama.cpp server is and its API key, checks them against the server, saves the answers
(T23's file) and starts. `llamatop setup` runs the same dialogue again at any time. It is a plain
question-and-answer dialogue on the terminal, before the dashboard starts: no TUI.

## Contract — `src/setup.rs`

```rust
/// How the dialogue checks a server: `HttpProbe` for real, a fake in the tests.
pub trait Probe {
    /// `GET <url>/health` without key; `Ok` on a 2xx answer.
    fn health(&self, url: &str) -> anyhow::Result<()>;
    /// `GET <url>/v1/models` with `key`, parsed by `collect::llama::parse_models`.
    fn models(&self, url: &str, key: Option<&str>) -> anyhow::Result<Vec<ModelInfo>>;
}

/// `Probe` over `http::Client`, 3 s timeout.
pub struct HttpProbe;

/// The dialogue (below) over any input and output. `secret` reads the API key without echo.
/// Returns the settings to save; `current` gives the defaults shown in brackets.
pub fn wizard(
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    secret: &mut dyn FnMut(&str) -> anyhow::Result<String>,
    probe: &dyn Probe,
    current: &Settings,
) -> anyhow::Result<Settings>;

/// Reads one line from the terminal without echo: crossterm raw mode, `Enter` ends, `Backspace` erases,
/// `Ctrl-C` or `Esc` is an error `setup cancelled`. The prompt is written first; raw mode is disabled again on
/// every path (a guard whose `Drop` calls `disable_raw_mode`).
pub fn read_secret(prompt: &str) -> anyhow::Result<String>;

/// `wizard` on stdin / stdout with `read_secret` and `HttpProbe`, then `settings::save(path, …)`, then prints
/// `saved <path>`. Returns the saved settings.
pub fn run(path: &Path, current: &Settings) -> anyhow::Result<Settings>;

/// `url` with `http://` added when it has no scheme and without trailing `/`; surrounding spaces trimmed.
pub fn normalize_url(url: &str) -> String;
```

### The dialogue

Defaults: URL = `current.llama.url`, else `http://127.0.0.1:8080`. Lines end with `\n`; prompts do not.

1. `llamatop setup\n\n`
2. URL: prompt `llama.cpp server URL [<default>]: `; an empty answer takes the default; `normalize_url`.
   Then `checking <url>/health … ` and `ok\n`, or `unreachable: <error>\n` followed by the prompt
   `use it anyway? [y/N] `: `y` / `yes` keeps it, anything else asks the URL again. Three refused URLs in a
   row → `Err("setup aborted")`.
3. Key: `secret("API key (empty: <keep the current one | none>): ")`. Empty keeps `current.llama.api_key`
   (or none). Then `checking models … ` and:
   - success: `<n> models, <k> loaded` then, when `k > 0`, `: <short names of the loaded ones, comma-separated>`,
     then `\n`;
   - an error containing `HTTP 401`: `the server refused the key\n`, and the key is asked again (three
     refusals → `Err("setup aborted")`);
   - any other error: `could not list the models: <error>\n`, and the dialogue goes on.
4. Log command: default = `current.llama.log_cmd`, else, when the URL's host is loopback
   (`http::is_loopback_url`), the flag's default command; otherwise
   `ssh <host> journalctl -u llama-server -f -n 2000 --no-pager -q -o short-iso-precise` with `<host>` the URL's
   host. Prompt `log command [<default>]: `; empty takes the default.
5. Returns `Settings { llama: LlamaSettings { url, api_key, api_key_file: current's, log_cmd } }`.

The key is **never** written to `out`, not even masked.

### `src/config.rs`

`#[command(subcommand)] pub command: Option<Command>`, with `pub enum Command { /// Ask for the server
address and key, check them, save the configuration file. Setup }`.

### `src/main.rs`

After loading the file (T23), with `path` = the configuration file's path:

- `Some(Command::Setup)` → `setup::run(&path, &current_or_default)` and exit 0.
- The dashboard branch (not `--dump`, not `--snapshot`): when the file does not exist, `url` was not given
  by flag or environment, `--demo` is off, and both stdin and stdout are terminals
  (`std::io::IsTerminal`), print `no configuration yet: <path>\n`, run `setup::run`, apply the returned
  settings with `apply_settings`, then start the dashboard.

## Pitfalls

- Test the dialogue through `wizard` only, with `std::io::Cursor` input, a `Vec<u8>` output, a closure for
  `secret` that pops answers from a list, and a fake `Probe`; `read_secret` and `run` need a terminal.
- `parse_models` gives `ModelInfo` with `state`; count `ModelState::Loaded`. Short names come from
  `util::short_model_name`.
- Reading a line: `read_line` returns `Ok(0)` at end of input: treat it as an empty answer the first time,
  and as `Err("setup aborted")` if the dialogue still needs an answer after the input ended (so a test can
  never loop forever).
- No new dependency: crossterm already provides raw mode and key events.

## Tests

`tests/setup.rs`, exactly these names. The fake probe: `health` fails for URLs containing `bad`; `models`
answers `HTTP 401` for the key `wrong`, else two models, one loaded
(`peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M`).

- `normalize`: `192.168.1.203:8080/` → `http://192.168.1.203:8080`; ` https://x/ ` → `https://x`.
- `defaults_accepted`: input `"\n\n"`, secret `""`, empty current → url `http://127.0.0.1:8080`, no key,
  log_cmd the flag's default; output contains `checking http://127.0.0.1:8080/health … ok` and
  `2 models, 1 loaded: Cyber-Tiel-Coder-35B-A3B`.
- `remote_url_suggests_ssh_log`: input `"192.168.1.203:8080\n\n"` → log_cmd
  `ssh 192.168.1.203 journalctl -u llama-server -f -n 2000 --no-pager -q -o short-iso-precise`.
- `unreachable_then_other_url`: input `"bad:1\nn\nhttp://ok:8080\n\n"` → url `http://ok:8080`.
- `unreachable_kept`: input `"bad:1\ny\n\n"` → url `http://bad:1`.
- `key_refused_then_accepted`: secrets `wrong`, `right` → api_key `Some("right")`; output contains
  `the server refused the key`.
- `existing_key_kept_and_never_shown`: current api_key `sk-OLD-123`, secret `""` → api_key
  `Some("sk-OLD-123")`; output contains neither `sk-OLD` nor `right`.
- `aborts_after_three_refusals`: input `"bad:1\nn\nbad:2\nn\nbad:3\nn\n"` → `Err` whose message is
  `setup aborted`.
- `end_of_input_aborts`: input `"bad:1\n"` (then nothing) → `Err`.

## Acceptance criteria

- [ ] The public API above exists; `llamatop setup` runs the dialogue; the dashboard runs it when there is no
      configuration, under the conditions above.
- [ ] The 9 tests above exist under these names and pass.
- [ ] README: the first run and `llamatop setup` are described.
- [ ] `bash .devbox/check` is green.
- [ ] (On a terminal, not by the model) `llamatop setup` hides the key while typing.
