# T10 — Theme and widgets

| | |
|---|---|
| **Batch** | 2 — the dashboard |
| **Depends on** | — (only `ratatui`; nothing from batch 1) |
| **Read first** | `docs/02-ui.md` § 1 and § 4 |
| **Deliverable** | `src/ui/mod.rs`, `src/ui/theme.rs`, `src/ui/gauge.rs`, `src/ui/graph.rs`, `src/ui/frame.rs`, `tests/ui_widgets.rs`; `pub mod ui;` in `src/lib.rs` |

## Why

Every panel of the dashboard is drawn from the same four pieces: a colour palette that degrades from true
colour to 16 colours to no colour, a horizontal gauge, a braille graph and a rounded panel frame with a state
badge. They are written first, alone, and tested cell by cell against a ratatui `Buffer`, because nobody can
look at the screen: the tests are the only eyes.

This sheet touches nothing of `src/collect/`, `src/util/`, `src/dump.rs`, `src/main.rs` or `src/config.rs`.

## Contract

`src/ui/mod.rs` declares `pub mod frame; pub mod gauge; pub mod graph; pub mod theme;` and a one-line module
doc. Imports come from `ratatui` itself: `ratatui::buffer::Buffer`, `ratatui::layout::Rect`,
`ratatui::style::{Color, Modifier, Style}`, `ratatui::text::Span`,
`ratatui::widgets::{Block, BorderType, Borders, Widget}`.

### `src/ui/theme.rs`

```rust
/// How many colours the terminal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode { TrueColor, Ansi16, NoColor }

impl ColorMode {
    /// `no_color` present **and not empty** → `NoColor`; else `colorterm` equal to `truecolor` or `24bit`
    /// (ASCII case-insensitive) → `TrueColor`; else `Ansi16`.
    pub fn from_env_values(no_color: Option<&str>, colorterm: Option<&str>) -> Self;
    /// `from_env_values` on the `NO_COLOR` and `COLORTERM` environment variables.
    pub fn detect() -> Self;
}

/// What a colour is used for. Every colour of the dashboard goes through a role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role { Border, BorderFocused, Text, Dim, Server, Gpu, Cpu, Memory, Good, Warning, Critical }

/// The true-colour palette of `docs/02-ui.md` § 4, independent of the mode (used for gradients).
pub fn rgb(role: Role) -> (u8, u8, u8);

/// The palette for one colour mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme { pub mode: ColorMode }

impl Theme {
    pub fn new(mode: ColorMode) -> Self;
    /// `TrueColor`: `Color::Rgb` of `rgb(role)`; `Ansi16`: the table below; `NoColor`: `None`
    /// (leave the terminal's own colour).
    pub fn color(&self, role: Role) -> Option<Color>;
    /// `Style::default()` with `fg` = `color(role)` when there is one; exactly `Style::default()` otherwise.
    pub fn fg(&self, role: Role) -> Style;
}

/// Where a value stands against its two thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity { Normal, Warning, Critical }

/// `value >= critical` → `Critical`; `value >= warning` → `Warning`; else (NaN included) `Normal`.
pub fn severity(value: f32, warning: f32, critical: f32) -> Severity;

/// `"▲"` for `Warning` and `Critical`, `""` for `Normal`: colour never carries the information alone.
pub fn hot_mark(severity: Severity) -> &'static str;

/// Thresholds of `docs/02-ui.md` § 4, `(warning, critical)`.
pub const PCT_THRESHOLDS: (f32, f32) = (75.0, 90.0);
pub const GPU_JUNCTION_THRESHOLDS: (f32, f32) = (85.0, 95.0);
pub const CPU_TEMP_THRESHOLDS: (f32, f32) = (80.0, 95.0);

/// Per channel `from + (to - from) * t` in `f32`, `t` clamped to 0..=1, then `.round()` to `u8`.
pub fn lerp_rgb(from: (u8, u8, u8), to: (u8, u8, u8), t: f32) -> (u8, u8, u8);

/// Colour of a gauge cell whose centre sits at `pct` (0–100) of the gauge's width.
/// `pct < warning` → the accent; `warning <= pct < critical` → `TrueColor`: `lerp_rgb(rgb(accent),
/// rgb(Warning), (pct - warning) / (critical - warning))`, `Ansi16`: the `Warning` colour;
/// `pct >= critical` → the `Critical` colour. `NoColor`: always `None`.
pub fn gauge_color(theme: &Theme, accent: Role, pct: f32, thresholds: (f32, f32)) -> Option<Color>;
```

Palette (`rgb` and `TrueColor`) and its 16-colour mapping (`Ansi16`), one line per `Role`:

| Role | `rgb` | `Ansi16` |
|---|---|---|
| `Border` | `#3b4261` | `Color::DarkGray` |
| `BorderFocused` | `#7aa2f7` | `Color::LightBlue` |
| `Text` | `#c0caf5` | `Color::Gray` |
| `Dim` | `#565f89` | `Color::DarkGray` |
| `Server` | `#ff9e64` | `Color::Yellow` |
| `Gpu` | `#bb9af7` | `Color::Magenta` |
| `Cpu` | `#7aa2f7` | `Color::Blue` |
| `Memory` | `#2ac3de` | `Color::Cyan` |
| `Good` | `#9ece6a` | `Color::Green` |
| `Warning` | `#e0af68` | `Color::LightYellow` |
| `Critical` | `#f7768e` | `Color::Red` |

### `src/ui/gauge.rs`

```rust
/// The bar's characters: `width` cells. With `e = (ratio * width * 8).round()` eighths (`ratio` clamped
/// to 0..=1, NaN counts as 0): `e / 8` cells `█`, then, when `e % 8 > 0`, one cell from
/// `▏▎▍▌▋▊▉` (index `e % 8`, 1-based), then `░` up to `width`. Always exactly `width` characters.
pub fn bar_cells(ratio: f64, width: u16) -> String;

/// A one-row horizontal gauge. Draws `bar_cells(ratio, area.width)` on the **first row** of `area`
/// (nothing when the area has no width or no height); other rows are untouched.
/// Cell `i` of the filled part (`█` and the partial cell) gets its foreground from
/// `gauge_color(theme, accent, (i as f32 + 0.5) / width as f32 * 100.0, thresholds)`; `░` cells take
/// `theme.color(Role::Dim)`. With no colour (`None`), the cell keeps the buffer's own foreground.
pub struct Gauge<'t> {
    pub ratio: f64,
    pub accent: Role,
    pub thresholds: (f32, f32),
    pub theme: &'t Theme,
}

impl Widget for Gauge<'_> { fn render(self, area: Rect, buf: &mut Buffer); }
```

### `src/ui/graph.rs`

```rust
/// A filled braille graph, `height` rows of `width` characters, top row first. Each cell holds two samples
/// (left and right dot column), so the graph shows the **last** `2 * width` values, newest at the right;
/// with fewer values, the left side is blank. A value's dot height is
/// `(min(v / max, 1) * height * 4).round()`, at least 1 when `v > 0`; it is 0 when `v` is NaN or `<= 0`,
/// and every height is 0 when `max` is NaN or `<= 0`. Dots fill from the bottom of the graph upward.
/// In one cell, from its bottom dot row to its top: left column bits `0x40, 0x04, 0x02, 0x01`, right column
/// bits `0x80, 0x20, 0x10, 0x08`; the character is `U+2800 + bits`, and a cell with no dot is a plain space
/// `' '` (not `U+2800`). `width == 0` or `height == 0` → empty `Vec`.
pub fn braille_rows(values: &[f32], max: f32, width: u16, height: u16) -> Vec<String>;

/// Draws `braille_rows(values, max, area.width, area.height)` into `area`, every non-space cell with
/// `theme.fg(accent)`. Space cells are left untouched (symbol and style).
pub struct BrailleGraph<'a> {
    pub values: &'a [f32],
    pub max: f32,
    pub accent: Role,
    pub theme: &'a Theme,
}

impl Widget for BrailleGraph<'_> { fn render(self, area: Rect, buf: &mut Buffer); }
```

### `src/ui/frame.rs`

```rust
/// A panel frame: all borders, `BorderType::Rounded`, border style `theme.fg(Role::BorderFocused)` when
/// `focused` else `theme.fg(Role::Border)`, title `" <title> "` (one space each side) as a `Span` styled
/// `theme.fg(accent)` plus `Modifier::BOLD`.
pub fn panel<'a>(title: &'a str, accent: Role, focused: bool, theme: &Theme) -> Block<'a>;

/// A state badge: `"◉ <text>"` when `lit`, `"○ <text>"` otherwise, styled `theme.fg(role)` plus
/// `Modifier::BOLD`. The UI toggles `lit` to make a badge pulse.
pub fn badge(text: &str, role: Role, theme: &Theme, lit: bool) -> Span<'static>;
```

## Pitfalls

- `Buffer::empty(Rect::new(0, 0, w, h))` gives a buffer of spaces whose cells have `fg == Color::Reset`;
  read a cell with `buf[(x, y)]` and its text with `.symbol()`.
- The partial and empty characters are multi-byte: build strings with `char`s, never slice them by bytes,
  and count them with `.chars().count()`.
- `(0.97_f64 * 160.0).round()` is 155 and `(0.06_f64 * 80.0).round()` is 5: compute in `f64` as written,
  do not add an epsilon.
- `f32::round` rounds half away from zero: `lerp_rgb((0,0,0), (255,255,255), 0.5)` is `(128,128,128)`.
- The block title starts right after the top-left corner: a 20-wide panel titled `GPU` has the top row
  `╭ GPU ─────────────╮`.
- No `unwrap()` outside tests: `char::from_u32(0x2800 + bits)` returns an `Option`; fall back to `' '`.

## Tests

`tests/ui_widgets.rs`, exactly these names. Write a helper in the test file:
`fn lines(buf: &Buffer) -> Vec<String>` (each row's symbols concatenated). `T` = `Theme::new(TrueColor)`,
`A` = `Theme::new(Ansi16)`, `N` = `Theme::new(NoColor)`.

Theme
- `mode_from_env`: `(Some("1"), Some("truecolor"))` → `NoColor`; `(Some(""), Some("truecolor"))` →
  `TrueColor`; `(None, Some("24bit"))` → `TrueColor`; `(None, Some("TrueColor"))` → `TrueColor`;
  `(None, Some("xterm"))` → `Ansi16`; `(None, None)` → `Ansi16`.
- `palette_by_mode`: `rgb(Role::Gpu)` = `(0xbb, 0x9a, 0xf7)`; `T.color(Role::Critical)` =
  `Some(Color::Rgb(0xf7, 0x76, 0x8e))`; `A.color(Role::Gpu)` = `Some(Color::Magenta)`;
  `A.color(Role::Warning)` = `Some(Color::LightYellow)`; `N.color(Role::Gpu)` = `None`;
  `N.fg(Role::Gpu)` = `Style::default()`; `T.fg(Role::Cpu)` = `Style::default().fg(Color::Rgb(0x7a, 0xa2, 0xf7))`.
- `severity_and_mark`: `severity(74.9, 75.0, 90.0)` = `Normal`; `75.0` → `Warning`; `89.9` → `Warning`;
  `90.0` → `Critical`; `f32::NAN` → `Normal`; `hot_mark(Critical)` = `"▲"`, `hot_mark(Warning)` = `"▲"`,
  `hot_mark(Normal)` = `""`.
- `lerp_clamps_and_rounds`: `lerp_rgb((0,0,0), (255,255,255), 0.5)` = `(128,128,128)`; with `t = -1.0` →
  `(0,0,0)`; with `t = 2.0` → `(255,255,255)`.
- `gauge_colour_zones`: accent `Gpu`, thresholds `PCT_THRESHOLDS`. `T` at 50 → `Some(Color::Rgb(0xbb,
  0x9a, 0xf7))`; `T` at 82.5 → `Some(Color::Rgb(206, 165, 176))`; `T` at 95 → `Some(Color::Rgb(0xf7, 0x76,
  0x8e))`; `A` at 50 → `Some(Color::Magenta)`, at 82.5 → `Some(Color::LightYellow)`, at 95 →
  `Some(Color::Red)`; `N` at 95 → `None`.

Gauge
- `bar_cells_values`: `(0.5, 10)` → `"█████░░░░░"`; `(0.97, 20)` → `"███████████████████▍"` (19 `█` then
  `▍`); `(0.06, 10)` → `"▋░░░░░░░░░"`; `(0.0, 5)` → `"░░░░░"`; `(1.5, 4)` → `"████"`; `(f64::NAN, 3)` →
  `"░░░"`; `(0.5, 0)` → `""`.
- `gauge_renders_first_row`: `Gauge { ratio: 0.5, accent: Role::Cpu, thresholds: PCT_THRESHOLDS, theme: &T }`
  rendered into `Buffer::empty(Rect::new(0, 0, 10, 2))` → `lines` = `["█████░░░░░", "          "]`;
  `buf[(0, 0)].fg` = `Color::Rgb(0x7a, 0xa2, 0xf7)`; `buf[(9, 0)].fg` = `Color::Rgb(0x56, 0x5f, 0x89)`.
- `gauge_colours_past_thresholds`: ratio `1.0`, width 10, accent `Gpu`, theme `T` → `buf[(9, 0)].fg` =
  `Color::Rgb(0xf7, 0x76, 0x8e)` (cell centre at 95 %) and `buf[(0, 0)].fg` = `Color::Rgb(0xbb, 0x9a, 0xf7)`.
- `gauge_without_colour`: same as `gauge_renders_first_row` with `N` → same `lines`, `buf[(0, 0)].fg` =
  `Color::Reset`.

Graph
- `braille_one_row`: `braille_rows(&[0.0, 1.0, 2.0, 3.0, 4.0], 4.0, 3, 1)` = `[" ⣠⣾"]`.
- `braille_two_rows`: `braille_rows(&[8.0, 4.0, 1.0, 0.0], 8.0, 2, 2)` = `["⡇ ", "⣿⡀"]`.
- `braille_small_and_clamped`: `(&[0.1], 100.0, 1, 1)` → `["⢀"]`; `(&[20.0], 10.0, 1, 1)` → `["⢸"]`.
- `braille_degenerate`: `(&[5.0], 0.0, 2, 1)` → `["  "]`; `(&[f32::NAN, -3.0], 10.0, 1, 1)` → `[" "]`;
  `(&[1.0], 1.0, 0, 3)` → empty.
- `graph_renders_with_accent`: `BrailleGraph { values: &[0.0, 1.0, 2.0, 3.0, 4.0], max: 4.0, accent:
  Role::Gpu, theme: &T }` into a 3×1 buffer → `lines` = `[" ⣠⣾"]`; `buf[(1, 0)].fg` = `Color::Rgb(0xbb,
  0x9a, 0xf7)`; `buf[(0, 0)].fg` = `Color::Reset`.

Frame
- `panel_rounded_frame`: `panel("GPU", Role::Gpu, false, &T)` rendered into a 20×3 buffer → `lines` =
  `["╭ GPU ─────────────╮", "│                  │", "╰──────────────────╯"]`; `buf[(0, 0)].fg` =
  `Color::Rgb(0x3b, 0x42, 0x61)`; `buf[(2, 0)].fg` = `Color::Rgb(0xbb, 0x9a, 0xf7)` and its modifier contains
  `Modifier::BOLD`.
- `panel_focused`: same with `focused = true` → `buf[(0, 0)].fg` = `Color::Rgb(0x7a, 0xa2, 0xf7)`.
- `badge_states`: `badge("GENERATING", Role::Server, &T, true)` → content `"◉ GENERATING"`, style
  `Style::default().fg(Color::Rgb(0xff, 0x9e, 0x64)).add_modifier(Modifier::BOLD)`; with `lit = false` →
  `"○ GENERATING"`; with `N` → style `Style::default().add_modifier(Modifier::BOLD)`.

## Acceptance criteria

- [ ] The five `src/ui/` files exist with the exact public API above; `src/lib.rs` declares `pub mod ui;`.
- [ ] Nothing outside `src/ui/`, `src/lib.rs` and `tests/ui_widgets.rs` changed (except `tasks/`).
- [ ] The 17 tests above exist under these names and pass.
- [ ] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) the gauge gradient and the braille graph look right in a real terminal.
