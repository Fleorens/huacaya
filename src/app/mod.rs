//! The dashboard's pure state machine (`App`), fed by events and read by the UI.

pub mod clock;
pub mod demo;
pub mod demo_agents;
mod demo_log;
pub mod keys;
pub mod log_tasks;
pub mod log_view;
mod net_rates;
mod pause;
mod requests;
pub mod run;
pub mod state;
mod threads;

pub use log_view::*;
pub use state::*;
