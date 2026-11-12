//! The server-side panels of the Overview screen: header, Model and Last request.
//!
//! Every `render` draws only inside the `area` it is given (never outside, never below the 3 × 3
//! minimum) and is tested by rendering into a `Buffer::empty`. See `docs/02-ui.md` § 2.2.

pub mod header;
pub mod model;
pub mod request;

use crate::ui::theme::Theme;

/// What every panel needs besides the state.
#[derive(Debug, Clone, Copy)]
pub struct Ctx<'a> {
    pub theme: &'a Theme,
    /// `false` on narrow terminals: panels drop their braille graphs.
    pub graphs: bool,
    /// Local wall-clock time shown in the header, `HH:MM:SS`; given by the caller.
    pub clock: &'a str,
}
