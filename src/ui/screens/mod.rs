//! The per-screen renders, drawn by `draw` on the body under the header (the table + details on the
//! Models screen here, the Logs screen and help overlay on T17 and T18, the Agents screen on T35). See `docs/02-ui.md` § 2.

pub mod agents;
pub mod logs;
pub mod logs_side;
pub mod models;
pub mod models_details;
pub mod models_details_panels;
pub mod system;
pub mod system_left;
