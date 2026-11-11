//! Disk usage, read from the host with `statvfs`.
//!
//! Unlike the other collectors this one is not relative to the sysroot: it describes a real mount
//! point. It holds the crate's single `unsafe` block, for the `libc::statvfs` syscall.

use std::ffi::CString;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// One mount point's usage. `used` = (blocks − bfree) × frsize; `available` = bavail × frsize.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiskSample {
    pub mount: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
}

/// Usage of `path` via `statvfs`. `None` when the path does not exist, the call fails, or `total`
/// is 0. Not relative to the sysroot: it describes a real mount point.
pub fn disk_usage(path: &Path) -> Option<DiskSample> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();

    unsafe {
        // SAFETY: `c_path` is a null-terminated string valid for the whole call, and `stat` is a
        // correctly aligned buffer of `libc::statvfs`; libc writes it only on success and returns
        // non-zero (leaving the buffer untouched) on failure.
        if libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) != 0 {
            return None;
        }
        // libc populated every field before returning 0.
        let stat = stat.assume_init();
        if stat.f_blocks == 0 {
            return None;
        }

        let frsize = stat.f_frsize;
        let total = stat.f_blocks * frsize;
        let used = (stat.f_blocks - stat.f_bfree) * frsize;
        let available = stat.f_bavail * frsize;

        Some(DiskSample { mount: path.display().to_string(), total, used, available })
    }
}

/// `disk_usage` of each path, skipping the `None`s, keeping the order.
pub fn read_disks(mounts: &[PathBuf]) -> Vec<DiskSample> {
    mounts.iter().filter_map(|m| disk_usage(m)).collect()
}
