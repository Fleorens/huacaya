//! The dashboard's pure state machine (`App`), fed by events and read by the UI.

pub mod demo;
mod demo_log;
pub mod keys;
pub mod state;

pub use state::*;
