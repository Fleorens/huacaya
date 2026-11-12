//! The Overview panels: the server-side ones (header, Model, Last request) and the machine-side ones
//! (GPU, CPU, Memory, Log). Every `render` draws only inside the `area` it is given (never outside,
//! never below the 3 × 3 minimum) and is tested by rendering into a `Buffer::empty`. See `docs/02-ui.md` § 2.2.

pub mod cpu;
pub mod gpu;
pub mod header;
pub mod log;
pub mod memory;
pub mod model;
pub mod request;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::ui::gauge::Gauge;
use crate::ui::theme::{Role, Theme};

/// What every panel needs besides the state.
#[derive(Debug, Clone, Copy)]
pub struct Ctx<'a> {
    pub theme: &'a Theme,
    /// `false` on narrow terminals: panels drop their braille graphs.
    pub graphs: bool,
    /// Local wall-clock time shown in the header, `HH:MM:SS`; given by the caller.
    pub clock: &'a str,
}

/// One `<label><gauge>  <value>` row at `(x, y)`, `width` columns: `label` (`Role::Dim`, written as
/// given, so pad it yourself), then a `gauge::Gauge` filling the columns left after two spaces and
/// `value` (`Role::Text`), then the value. When fewer than 4 columns are left for the gauge, it is
/// omitted and the value follows the label directly.
#[allow(clippy::too_many_arguments)] // the signature is the panel contract; callers pass one row's worth
pub fn gauge_row(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    label: &str,
    ratio: f64,
    accent: Role,
    thresholds: (f32, f32),
    value: &str,
    theme: &Theme,
) {
    let label_w = label.chars().count().min(width as usize) as u16;
    buf.set_line(x, y, &Line::from(Span::styled(&label[..label_w as usize], theme.fg(Role::Dim))), label_w);

    let value_w = value.chars().count() as u16;
    let gauge_w = width.saturating_sub(label_w).saturating_sub(2).saturating_sub(value_w);
    let mut gx = x + label_w;
    if gauge_w >= 4 {
        Gauge { ratio, accent, thresholds, theme }.render(Rect::new(gx, y, gauge_w, 1), buf);
        gx += gauge_w + 2;
    }
    let right = x + width;
    if gx < right {
        let vw = (right - gx).min(value_w);
        buf.set_line(
            gx,
            y,
            &Line::from(Span::styled(&value[..vw as usize], theme.fg(Role::Text))),
            right - gx,
        );
    }
}
