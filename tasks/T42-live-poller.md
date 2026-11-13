# T42 — OpenCode poller: whole history once, then a light round every second

| | |
|---|---|
| **Batch** | 8 — the Agents map (design validated by Florian on 2026-11-13) |
| **Depends on** | T41 |
| **Read first** | `src/collect/opencode.rs` (`OpenCodePoller`, `OpenCodeSnapshot`, `parse_steps`); `src/collect/opencode/raw.rs`; `src/collect/activity.rs` (`Activity::update`); `src/app/threads.rs`; `tests/opencode.rs` (`FakeFetch`, `poller_focus`) |
| **Deliverable** | `tests/opencode_poller.rs`; changes to `src/collect/opencode.rs`, `src/collect/opencode/raw.rs`, `src/app/threads.rs` |

## Why

The map (T43) animates a pulse when a step starts or ends, so the poller must see steps within about a
second. Today each round fetches the default page (50 messages, about 390 KB on a real job) every 2 s, and
counts would cover those 50 messages only. Measured on the real OpenCode 2.2.7 server on 2026-11-13:

- `GET /api/session/<id>/message` takes `limit` (at most **200**; more is `HTTP 400`) and `cursor`; pages are
  newest first; the answer's `cursor.next` (an opaque string) gives the next **older** page.
- `limit=12` is about 140 KB; `limit=3` about 32 KB.

So: when the followed session changes, load its whole history once (pages of 200, at most 10 pages); then,
every second, fetch only the newest 12 messages and merge them by step id (`Step::id`, T41).

## Contract

### `src/collect/opencode/raw.rs` (private)

`RawMessages` gains `#[serde(default)] cursor: Option<RawCursor>`, with
`struct RawCursor { #[serde(default)] next: Option<String> }`.

### `src/collect/opencode.rs`

```rust
/// The `cursor.next` of a messages page: the token for the next older page, `None` when absent, null or
/// when the JSON does not parse.
pub fn next_cursor(json: &str) -> Option<String>;

/// Messages per page when loading a session's history (the server's maximum).
pub const HISTORY_PAGE: usize = 200;
/// Pages loaded at most for one history (2000 messages).
pub const HISTORY_PAGES: usize = 10;
/// Messages fetched by a live round.
pub const LIVE_LIMIT: usize = 12;
/// Steps kept in `OpenCodeSnapshot::steps` (the newest).
pub const SNAPSHOT_STEPS: usize = 200;
```

`OpenCodeSnapshot` gains `#[serde(skip)] pub activity: Activity` (`crate::collect::activity::Activity`).

`OpenCodePoller<F>` keeps, privately: the id of the session whose history is loaded (`None` at first), that
session's steps (oldest first), and an `Activity`. It gains:

```rust
/// One round at wall-clock `now_ms`; `poll` calls it with the current time.
pub fn poll_at(&mut self, focus: Option<&str>, now_ms: i64) -> OpenCodeSnapshot;
```

`poll(focus)` keeps its signature and documented behaviour and becomes `self.poll_at(focus, now_ms())`.
A round:

1. `GET /api/session` and the choice of the focus exactly as today (errors as today).
2. **History** when the focus differs from the loaded session: `GET /api/session/<id>/message` with query
   `[("limit", "200")]`; while the page held `HISTORY_PAGE` messages, `next_cursor` gives a token and fewer
   than `HISTORY_PAGES` pages were read, the next page with `[("limit", "200"), ("cursor", <token>)]`. Each page
   goes through `parse_steps` (same `directory` as today); the older pages' steps come first. The loaded
   steps replace the kept ones, and `activity.update(id, &steps, now_ms)` folds them in (a new session: a
   history round, no pulses).
3. **Live** otherwise: one `GET` with `[("limit", "12")]`; its steps are merged into the kept ones — a step
   whose id is already kept replaces it in place, a new one is appended — and
   `activity.update(id, &round_steps, now_ms)` gets this round's steps only.
4. The snapshot: `sessions`, `focus`, `steps` = the newest `SNAPSHOT_STEPS` kept steps (oldest first),
   `activity` = a clone of the poller's, `error: None`, `fetched_ms: now_ms`.
5. Any error (sessions, a page, parsing) gives the snapshot of today's error case and forgets the loaded
   session, so the next successful round loads the history again.

No focus (no sessions): kept steps and activity are cleared, `steps` empty, `activity` default.

### `src/app/threads.rs`

`opencode_loop` sleeps **1000 ms** between rounds instead of 2000 (doc comment updated).

## Pitfalls

- The query goes in the `query` argument of `Fetch::get`, never in the path (`http::Client::get` encodes
  it). Tests check the exact pairs.
- `parse_steps` is oldest first within a page; pages arrive newest first: reverse the page order, not the
  steps inside a page.
- `FakeFetch` in `tests/opencode.rs` ignores the query and serves `messages.json` (11 steps, `cursor.next`
  null): `poller_focus` must keep passing unchanged.
- Keep `src/collect/opencode.rs` under 400 lines: if needed, move the poller into
  `src/collect/opencode/poller.rs` (`pub use` it from `opencode.rs` so `collect::opencode::OpenCodePoller`
  keeps its path).

## Tests

`tests/opencode_poller.rs`, exactly these names. Write a `ScriptFetch` double in the test file: it records
every `(path, query)` it receives (owned strings, in a `RefCell<Vec<…>>` or a `Mutex`), answers
`/api/session` with `sessions.json`, and answers message requests from a queue of bodies given by the test.
Build message bodies in the test with a helper: `fn page(msgs: &[(&str /*message id*/, &str /*tool id*/,
&str /*tool name*/, i64 /*created*/, Option<i64> /*completed*/)], next: Option<&str>) -> String` producing
assistant messages, each with one tool part (`state.status` `"completed"` when `completed` is set, else
`"running"`).

- `next_cursor_values`: `{"data": [], "cursor": {"next": "abc", "previous": null}}` → `Some("abc")`;
  `{"data": [], "cursor": {"next": null}}` → `None`; `{"data": []}` → `None`; `not json` → `None`.
- `history_then_live`: round 1 `poll_at(Some(<done session id>), 1_000)` with a 3-message page → the requests
  are `/api/session` then `/api/session/<id>/message` with `[("limit","200")]`; `steps.len() == 3`;
  `activity.pulses` empty. Round 2 at `2_000` with a page holding the same 3 messages plus a new running
  `shell` step → the message request has `[("limit","12")]`; `steps.len() == 4`; one outbound pulse at
  `2_000` at `Station::Shell` (the helper's shell part has no command). Round 3 at `3_000` with that step completed
  → `steps.len() == 4` (replaced in place, the last step is now ended), one inbound pulse at `3_000`.
- `history_pages`: a first page of exactly 200 messages with `next` = `"c1"`, then a page of 5 messages
  → two message requests, the second with `[("limit","200"), ("cursor","c1")]`; `steps.len() == 205`; the
  first step is the oldest of the second page.
- `history_page_cap`: every page has 200 messages and a `next` token → exactly `HISTORY_PAGES` message
  requests, then the round ends normally.
- `focus_change_reloads`: after a history and a live round on session A, a round focused on session B makes
  a `[("limit","200")]` request again; its activity holds only B's steps.
- `error_reloads`: a live round whose messages body is `not json` → `error` set; the next round makes a
  `[("limit","200")]` request.
- `snapshot_window`: a history of 250 steps → `snapshot.steps.len() == SNAPSHOT_STEPS`, ending with the
  newest step.

## Acceptance criteria

- [ ] The contract above is implemented; `poll` keeps its signature and documented behaviour.
- [ ] Every earlier test passes; the tests above pass.
- [ ] `bash .devbox/check` is green.
- [ ] (Real server, by the orchestrator) a running job's map counts match its whole history and new steps show
      within about a second.
