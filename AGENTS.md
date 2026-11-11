# llamatop — shared rules for agents

This file is read by OpenCode (AGENTS.md) and by Claude Code (CLAUDE.md, a
symlink to this file). Single source of truth: only this file is edited.

## Roles

- **OpenCode**, on the aibox's local model: writes the code. In a job, it
  works alone in a worktree, on a `gen/<id>-*` branch.
- **Claude Code**: reviews, fixes, decides. Nothing reaches `main` without
  review: automatic merge when the checks and Claude Code's review pass,
  otherwise by Florian (`dev merge`).

## Language

- Everything written for an agent (this file, task sheets, briefs) is in
  English. Code, comments and commit messages too.
- Your final report is in **French**: Florian reads it.

## Rules

- Never `git push`, never rewrite history, never commit on `main`.
- Small, atomic commits, messages in English: `type: description`
  (feat, fix, refactor, test, docs, chore).
- Before finishing: if `.devbox/check` exists, run `bash .devbox/check`
  and fix until it passes.
- No dependency added without a justification in the commit message.
- Never build artifacts, binaries or large files in git.
- Ambiguity: pick the simplest, reversible option and explain it in the
  commit.

## Project

**llamatop** is a terminal dashboard (TUI) for a llama.cpp server and the machine that runs it
(`aibox-flo`: Debian 13, i7-11700K, Radeon RX 7800 XT, llama.cpp in router mode on port 8080).
It shows the loaded model, generation speed, context fill, GPU / CPU / RAM / temperatures, and the live
server log. Architecture: `docs/01-architecture.md`. Screens: `docs/02-ui.md`. Plan: `tasks/README.md`.

- Stack: Rust 2024, `ratatui` 0.30 + `crossterm` 0.29 (UI), `ureq` 3 (blocking HTTP, no TLS), `serde`,
  `clap`. **No async runtime** (no tokio): plain threads and `std::sync::mpsc`.
- Run in dev: `cargo run -- --sysroot tests/fixtures/sysroot` (fake machine), `cargo run -- --dump`.
- Tests: `bash .devbox/check` (= `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, 400-line limit).
- The dependencies are already in `Cargo.toml`. Add none unless the task sheet names it.

## Mandatory session flow (task sheets)

You carry out **ONE sheet** (`tasks/Txx-*.md`) per session, nothing more.

1. Read the sheet once, in full, then only the sections of `docs/` it cites under "Read first".
2. `git status` must be clean, and the sheets under "Depends on" must be `livrée` or `validée` in
   `tasks/README.md`. Otherwise STOP and say so. Never write a stub to stand in for a missing sheet.
3. The "Contract" section is binding: module paths, type names, field names, function signatures and
   documented behaviour are exactly those of the sheet. Other sheets are written against them.
   You may add private helpers. You may not change a public signature: if one cannot work, STOP and report.
4. Write the tests listed under "Tests" (same file names, same test names, same expected values).
   Expected values come from the fixtures in `tests/fixtures/`: **never edit a fixture** to make a test pass.
5. Finish with `cargo fmt`, then `bash .devbox/check`: everything green.
6. Tick the acceptance criteria you verified (`- [x]`) in the sheet, set its status to `livrée` in
   `tasks/README.md`, and commit: `feat: <summary> (Txx)`.
7. Final report **in French**: files, commands run and results, deviations from the sheet and why, questions.

## Conventions

- One module per data source under `src/collect/`. A collector **never panics** and never returns an
  error for a missing file: a value that cannot be read is `None` (the UI shows "n/a"). The dashboard must
  run on a machine without an AMD GPU, without `journalctl`, and with the server down.
- Every function that reads `/proc` or `/sys` takes the root as `sysroot: &Path` and joins **relative**
  paths to it (`sysroot.join("proc/stat")`), so the tests run on `tests/fixtures/sysroot`.
- Parsing is separated from I/O: `parse_*(&str) -> …` functions are pure and are what the tests exercise.
- No `unwrap()` / `expect()` outside tests, except on a `Mutex` lock. No `unsafe`, except the single
  `libc::statvfs` call of T04.
- Units in the data model: bytes (`u64`), degrees Celsius (`f32`), watts (`f32`), percent 0–100 (`f32`),
  MHz (`f32`), seconds. The UI formats; collectors never produce display strings.
- The API key never appears in a log, an error message, a test, a fixture or `--dump` output.
- A source file stays under 400 lines; write files in chunks of at most ~300 lines per write.
- Public items carry a one-line doc comment. Comments say why, not what.
- You have **no visual access**. A criterion judged on screen stays unticked; flag it in your report.

## Limited context

Your context is about 130,000 tokens. Read just what you need: the sheet once, the cited doc sections,
and existing code through `grep -n "pub " src/<file>.rs` rather than whole files. Short thinking, no
global plan: one file, one write, the next. After a compaction, `git status --short` shows what is done.
