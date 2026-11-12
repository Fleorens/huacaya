//! Disk usage, read from the host with `statvfs`.
//!
//! Unlike the other collectors this one is not relative to the sysroot: it describes a real mount
//! point. It holds the crate's single `unsafe` block, for the `libc::statvfs` syscall.

use std::ffi::{CString, OsStr};
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// One mount point's usage. `used` = (blocks − bfree) × frsize; `available` = bavail × frsize.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
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

    // SAFETY: `c_path` is a NUL-terminated string valid for the whole call, and `stat` is a
    // correctly aligned `libc::statvfs` buffer; libc fills every field when it returns 0, so
    // `assume_init` only runs on success.
    let stat = unsafe {
        if libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) != 0 {
            return None;
        }
        stat.assume_init()
    };
    if stat.f_blocks == 0 {
        return None;
    }

    let frsize = stat.f_frsize;
    let total = stat.f_blocks.saturating_mul(frsize);
    let used = stat.f_blocks.saturating_sub(stat.f_bfree).saturating_mul(frsize);
    let available = stat.f_bavail.saturating_mul(frsize);

    Some(DiskSample { mount: path.display().to_string(), total, used, available })
}

/// `disk_usage` of each path, skipping the `None`s, keeping the order.
pub fn read_disks(mounts: &[PathBuf]) -> Vec<DiskSample> {
    mounts.iter().filter_map(|m| disk_usage(m)).collect()
}

/// Filesystem types shown by default: on-disk and network filesystems.
pub const REAL_FS: &[&str] = &[
    "ext2", "ext3", "ext4", "xfs", "btrfs", "zfs", "f2fs", "bcachefs", "jfs", "reiserfs", "vfat", "exfat",
    "ntfs", "ntfs3", "fuseblk", "nfs", "nfs4", "cifs", "smb3",
];

/// Mount points worth excluding: the path itself or anything below it.
const EXCLUDED_PREFIXES: &[&str] =
    &["/boot", "/snap", "/var/lib/docker", "/var/lib/containers", "/run", "/proc", "/sys", "/dev"];

/// Parse `<device> <mount point> <type> <options> 0 0` lines from `/proc/self/mounts` text, in file
/// order: type in `REAL_FS`; mount point not under an excluded prefix; one entry per device (the first
/// mount of a device wins); mount point octal-escapes decoded; at most 8. Malformed lines are skipped.
pub fn parse_mounts(text: &str) -> Vec<PathBuf> {
    let mut mounts: Vec<PathBuf> = Vec::new();
    let mut seen_devices: Vec<&str> = Vec::new();
    for line in text.lines() {
        let mut tokens = line.split_whitespace();
        let device = match tokens.next() {
            Some(d) => d,
            None => continue,
        };
        let mount_raw = match tokens.next() {
            Some(m) => m,
            None => continue,
        };
        let fs_type = match tokens.next() {
            Some(t) => t,
            None => continue,
        };
        if !REAL_FS.contains(&fs_type) {
            continue;
        }
        let mount = decode_mount_point(mount_raw);
        if !is_real_mount(&mount) || seen_devices.contains(&device) {
            continue;
        }
        seen_devices.push(device);
        mounts.push(mount);
        if mounts.len() >= 8 {
            break;
        }
    }
    mounts
}

/// `parse_mounts` of `<sysroot>/proc/self/mounts` (empty when unreadable). The paths are the machine's
/// real mount points: `disk_usage` reads them directly, not under the sysroot.
pub fn auto_mounts(sysroot: &Path) -> Vec<PathBuf> {
    match std::fs::read_to_string(sysroot.join("proc/self/mounts")) {
        Ok(text) => parse_mounts(&text),
        Err(_) => Vec::new(),
    }
}

/// True when `mount` is itself an excluded prefix or lies below one (`/boot` matches, `/bootstrap` does not).
fn is_real_mount(mount: &Path) -> bool {
    let path = mount.as_os_str().to_string_lossy();
    !EXCLUDED_PREFIXES.iter().any(|prefix| is_under(&path, prefix))
}

/// True when `path` is `prefix` or starts with `prefix + "/"`.
fn is_under(path: &str, prefix: &str) -> bool {
    path == prefix || path.starts_with(&format!("{}/", prefix))
}

/// Decode three-digit octal escapes (`\040` space, `\011` tab, `\134` backslash) in a mount point.
fn decode_mount_point(raw: &str) -> PathBuf {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && is_octal(bytes[i + 1])
            && is_octal(bytes[i + 2])
            && is_octal(bytes[i + 3])
        {
            let value = (bytes[i + 1] - b'0') * 64 + (bytes[i + 2] - b'0') * 8 + (bytes[i + 3] - b'0');
            out.push(value);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    PathBuf::from(OsStr::from_bytes(&out))
}

/// True when `b` is one of `0`–`7`.
fn is_octal(b: u8) -> bool {
    matches!(b, b'0'..=b'7')
}
