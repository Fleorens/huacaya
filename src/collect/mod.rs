//! Data sources: one module per source, reading everything under `--sysroot`.
//!
//! See `docs/01-architecture.md` § 2. A collector never panics and never returns an error for a
//! missing file: an unreadable value is `None`.

use std::path::Path;

/// Reads a file under `sysroot` and trims it. `None` on any error.
pub(crate) fn read_trimmed(sysroot: &Path, relative: &str) -> Option<String> {
    let content = std::fs::read_to_string(sysroot.join(relative)).ok()?;
    Some(content.trim().to_string())
}

/// `read_trimmed` then `parse::<T>()`.
pub(crate) fn read_parsed<T: std::str::FromStr>(sysroot: &Path, relative: &str) -> Option<T> {
    let text = read_trimmed(sysroot, relative)?;
    text.parse().ok()
}

pub mod disk;
pub mod gpu;
pub mod llama;
pub mod llama_poll;
pub mod logs;
pub mod net;
pub mod process;
pub mod sampler;
pub mod sensors;
pub mod system;
