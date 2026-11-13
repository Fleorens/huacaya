//! Colour palette and colour mode, shared by every widget.

use ratatui::style::{Color, Style};

/// How many colours the terminal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    Ansi16,
    NoColor,
}

impl ColorMode {
    /// `no_color` present **and not empty** → `NoColor`; else `colorterm` equal to `truecolor` or
    /// `24bit` (ASCII case-insensitive) → `TrueColor`; else `Ansi16`.
    pub fn from_env_values(no_color: Option<&str>, colorterm: Option<&str>) -> Self {
        if no_color.is_some_and(|v| !v.is_empty()) {
            return ColorMode::NoColor;
        }
        match colorterm {
            Some(c) if c.eq_ignore_ascii_case("truecolor") || c.eq_ignore_ascii_case("24bit") => {
                ColorMode::TrueColor
            }
            _ => ColorMode::Ansi16,
        }
    }

    /// `from_env_values` on the `NO_COLOR` and `COLORTERM` environment variables.
    pub fn detect() -> Self {
        ColorMode::from_env_values(
            std::env::var("NO_COLOR").ok().as_deref(),
            std::env::var("COLORTERM").ok().as_deref(),
        )
    }
}

/// What a colour is used for. Every colour of the dashboard goes through a role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Border,
    BorderFocused,
    Text,
    Dim,
    Server,
    Gpu,
    Cpu,
    Memory,
    Good,
    Warning,
    Critical,
    /// The selected row band of the Models table.
    Selection,
}

/// The true-colour palette of `docs/02-ui.md` § 4, independent of the mode (used for gradients).
pub fn rgb(role: Role) -> (u8, u8, u8) {
    match role {
        Role::Border => (0x54, 0x5c, 0x7e),
        Role::BorderFocused => (0x7a, 0xa2, 0xf7),
        Role::Text => (0xc0, 0xca, 0xf5),
        Role::Dim => (0x56, 0x5f, 0x89),
        Role::Server => (0xff, 0x9e, 0x64),
        Role::Gpu => (0xbb, 0x9a, 0xf7),
        Role::Cpu => (0x7a, 0xa2, 0xf7),
        Role::Memory => (0x2a, 0xc3, 0xde),
        Role::Good => (0x9e, 0xce, 0x6a),
        Role::Warning => (0xe0, 0xaf, 0x68),
        Role::Critical => (0xf7, 0x76, 0x8e),
        Role::Selection => (0x28, 0x34, 0x57),
    }
}

/// The palette for one colour mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub mode: ColorMode,
}

impl Theme {
    /// The palette for `mode`.
    pub fn new(mode: ColorMode) -> Self {
        Theme { mode }
    }

    /// `TrueColor`: `Color::Rgb` of `rgb(role)`; `Ansi16`: the table below; `NoColor`: `None`
    /// (leave the terminal's own colour).
    pub fn color(&self, role: Role) -> Option<Color> {
        match (self.mode, role) {
            (ColorMode::TrueColor, role) => {
                let (r, g, b) = rgb(role);
                Some(Color::Rgb(r, g, b))
            }
            (ColorMode::Ansi16, Role::Border | Role::Dim | Role::Selection) => Some(Color::DarkGray),
            (ColorMode::Ansi16, Role::BorderFocused) => Some(Color::LightBlue),
            (ColorMode::Ansi16, Role::Text) => Some(Color::Gray),
            (ColorMode::Ansi16, Role::Server) => Some(Color::Yellow),
            (ColorMode::Ansi16, Role::Gpu) => Some(Color::Magenta),
            (ColorMode::Ansi16, Role::Cpu) => Some(Color::Blue),
            (ColorMode::Ansi16, Role::Memory) => Some(Color::Cyan),
            (ColorMode::Ansi16, Role::Good) => Some(Color::Green),
            (ColorMode::Ansi16, Role::Warning) => Some(Color::LightYellow),
            (ColorMode::Ansi16, Role::Critical) => Some(Color::Red),
            (ColorMode::NoColor, _) => None,
        }
    }

    /// `Style::default()` with `fg` = `color(role)` when there is one; exactly `Style::default()` otherwise.
    pub fn fg(&self, role: Role) -> Style {
        match self.color(role) {
            Some(color) => Style::default().fg(color),
            None => Style::default(),
        }
    }
}

/// Where a value stands against its two thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Normal,
    Warning,
    Critical,
}

/// `value >= critical` → `Critical`; `value >= warning` → `Warning`; else (NaN included) `Normal`.
pub fn severity(value: f32, warning: f32, critical: f32) -> Severity {
    if value >= critical {
        Severity::Critical
    } else if value >= warning {
        Severity::Warning
    } else {
        Severity::Normal
    }
}

/// `"▲"` for `Warning` and `Critical`, `""` for `Normal`: colour never carries the information alone.
pub fn hot_mark(severity: Severity) -> &'static str {
    match severity {
        Severity::Normal => "",
        Severity::Warning | Severity::Critical => "▲",
    }
}

/// Thresholds of `docs/02-ui.md` § 4, `(warning, critical)`: percentages.
pub const PCT_THRESHOLDS: (f32, f32) = (75.0, 90.0);
/// GPU junction temperature, °C.
pub const GPU_JUNCTION_THRESHOLDS: (f32, f32) = (85.0, 95.0);
/// CPU package temperature, °C.
pub const CPU_TEMP_THRESHOLDS: (f32, f32) = (80.0, 95.0);
/// VRAM, percent: a loaded model fills it by design, so only a really full card warns.
pub const VRAM_THRESHOLDS: (f32, f32) = (98.0, 99.5);

/// Per channel `from + (to - from) * t` in `f32`, `t` clamped to 0..=1, then `.round()` to `u8`.
pub fn lerp_rgb(from: (u8, u8, u8), to: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        (from.0 as f32 + (to.0 as f32 - from.0 as f32) * t).round() as u8,
        (from.1 as f32 + (to.1 as f32 - from.1 as f32) * t).round() as u8,
        (from.2 as f32 + (to.2 as f32 - from.2 as f32) * t).round() as u8,
    )
}

/// Colour of a gauge cell whose centre sits at `pct` (0–100) of the gauge's width.
/// `pct < warning` → the accent; `warning <= pct < critical` → `TrueColor`: `lerp_rgb(rgb(accent),
/// rgb(Warning), (pct - warning) / (critical - warning))`, `Ansi16`: the `Warning` colour;
/// `pct >= critical` → the `Critical` colour. `NoColor`: always `None`.
pub fn gauge_color(theme: &Theme, accent: Role, pct: f32, thresholds: (f32, f32)) -> Option<Color> {
    let (warning, critical) = thresholds;
    match theme.mode {
        ColorMode::NoColor => None,
        ColorMode::Ansi16 => match severity(pct, warning, critical) {
            Severity::Normal => theme.color(accent),
            Severity::Warning => theme.color(Role::Warning),
            Severity::Critical => theme.color(Role::Critical),
        },
        ColorMode::TrueColor => {
            if pct < warning {
                theme.color(accent)
            } else if pct < critical {
                let t = (pct - warning) / (critical - warning);
                let (r, g, b) = lerp_rgb(rgb(accent), rgb(Role::Warning), t);
                Some(Color::Rgb(r, g, b))
            } else {
                theme.color(Role::Critical)
            }
        }
    }
}
