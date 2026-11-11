//! Pure helpers shared by every panel: a history ring, counter-to-rate conversion, and formatting.
//!
//! See `docs/01-architecture.md` § 3 "Derived values". No I/O, no display strings in the data model.

pub mod format;
pub mod history;
pub mod rate;

pub use format::{bytes, count, duration, short_model_name};
pub use history::History;
pub use rate::Rate;
