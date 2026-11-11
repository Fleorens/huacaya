//! The `llama-server` processes, read from `proc/<pid>/` under the sysroot.
//!
//! Parsing (`parse_proc_sample`) is pure; `read_server_procs` only does I/O.

use std::path::Path;

use serde::Serialize;

use crate::collect::read_trimmed;

/// Whether a `llama-server` is the router or one of the models it serves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum ServerRole {
    Router,
    Instance,
}

/// One `llama-server` process: its role, the model it serves (if any), its port, RAM and threads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProcSample {
    pub pid: u32,
    /// `Instance` when the command line has `--alias`, else `Router`.
    pub role: ServerRole,
    /// Value following `--alias`.
    pub model: Option<String>,
    /// Value following `--port`.
    pub port: Option<u16>,
    /// `VmRSS`, in bytes; 0 when the line is missing.
    pub rss: u64,
    pub threads: u32,
}

/// Every `proc/<pid>` under `sysroot` whose `comm` is exactly `llama-server`, ordered by pid.
pub fn read_server_procs(sysroot: &Path) -> Vec<ProcSample> {
    let Ok(entries) = std::fs::read_dir(sysroot.join("proc")) else {
        return Vec::new();
    };

    let mut procs = Vec::new();
    for entry in entries.flatten() {
        // Only numeric directory names are processes (`proc/net`, `proc/sys` are skipped).
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let comm = read_trimmed(sysroot, &format!("proc/{}/comm", pid)).unwrap_or_default();
        if comm != "llama-server" {
            continue;
        }
        // A process can exit between `read_dir` and these reads: skip it rather than publish zeros.
        let Some(status) = read_trimmed(sysroot, &format!("proc/{}/status", pid)) else {
            continue;
        };
        let Some(cmdline) = read_cmdline(sysroot, pid) else {
            continue;
        };
        procs.push(parse_proc_sample(pid, &status, &cmdline));
    }

    procs.sort_by_key(|p| p.pid);
    procs
}

/// `llama-server` arguments, split from the NUL-separated `cmdline` (which often has a trailing NUL).
fn read_cmdline(sysroot: &Path, pid: u32) -> Option<Vec<String>> {
    let bytes = std::fs::read(sysroot.join(format!("proc/{}/cmdline", pid))).ok()?;
    Some(
        String::from_utf8_lossy(&bytes)
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
    )
}

/// Builds a sample from a process's `status` and parsed `cmdline`.
fn parse_proc_sample(pid: u32, status: &str, cmdline: &[String]) -> ProcSample {
    ProcSample {
        pid,
        role: if cmdline.iter().any(|a| a == "--alias") { ServerRole::Instance } else { ServerRole::Router },
        model: next_arg(cmdline, "--alias"),
        port: next_arg(cmdline, "--port").and_then(|p| p.parse::<u16>().ok()),
        rss: parse_rss(status),
        threads: parse_threads(status),
    }
}

/// The argument following `flag`, if present.
fn next_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

/// `VmRSS` (kB) in bytes; 0 when the line is missing.
fn parse_rss(status: &str) -> u64 {
    status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u64>().ok())
        .map(|kb| kb.saturating_mul(1024))
        .unwrap_or(0)
}

/// `Threads:` count; 0 when the line is missing.
fn parse_threads(status: &str) -> u32 {
    status
        .lines()
        .find(|l| l.starts_with("Threads:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0)
}
