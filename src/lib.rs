//! llamatop — terminal dashboard for a llama.cpp server and the machine that runs it.
//!
//! Architecture: `docs/01-architecture.md`. One module per data source; nothing here draws.

pub mod app;
pub mod collect;
pub mod config;
pub mod dump;
pub mod http;
pub mod settings;
pub mod ui;
pub mod util;
