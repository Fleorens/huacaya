# T29 — Show the machine's real disks by default

| | |
|---|---|
| **Batch** | 6 — any machine |
| **Depends on** | T28 |
| **Read first** | `src/collect/disk.rs`; `src/collect/sampler.rs`; `grep -n "mount" src/config.rs README.md` |
| **Deliverable** | changes to `src/collect/disk.rs`, `src/collect/sampler.rs`, `src/config.rs`, README; `tests/auto_mounts.rs` |

## Why

llamatop is public now: `--mount` must not default to one author's disks, nor to `/` alone. Without `--mount`,
show every real filesystem of the machine (its root, its data disks, its NAS mounts), each once, and nothing
of the pseudo filesystems (`/proc`, `tmpfs`, container overlays, snaps).

## Contract

### `src/collect/disk.rs`

```rust
/// Filesystem types shown by default: on-disk and network filesystems.
pub const REAL_FS: &[&str] = &[
    "ext2", "ext3", "ext4", "xfs", "btrfs", "zfs", "f2fs", "bcachefs", "jfs", "reiserfs",
    "vfat", "exfat", "ntfs", "ntfs3", "fuseblk", "nfs", "nfs4", "cifs", "smb3",
];

/// The mount points worth showing, from the text of `/proc/self/mounts` (`<device> <mount point> <type>
/// <options> 0 0` per line), in file order: type in `REAL_FS`; mount point not under `/boot`, `/snap`,
/// `/var/lib/docker`, `/var/lib/containers`, `/run`, `/proc`, `/sys` or `/dev` (the path itself or below it);
/// one entry per device (the first mount of a device wins: btrfs subvolumes and bind mounts appear once);
/// octal escapes of the mount point decoded (`\040` is a space, `\011` a tab, `\134` a backslash); at most 8.
/// Malformed lines are skipped.
pub fn parse_mounts(text: &str) -> Vec<PathBuf>;

/// `parse_mounts` of `<sysroot>/proc/self/mounts` (empty when unreadable). The paths are the machine's real
/// mount points: `disk_usage` reads them directly, not under the sysroot.
pub fn auto_mounts(sysroot: &Path) -> Vec<PathBuf>;
```

### `src/collect/sampler.rs`

`Sampler::new(sysroot, mounts)` unchanged; when `mounts` is empty, each `sample` uses `auto_mounts(sysroot)`
(re-read every time: a disk plugged in later appears).

### `src/config.rs`

`--mount` loses its default (`#[arg(long = "mount")]`), doc comment: "A mount point to show, repeatable;
default: every real filesystem of the machine". The `defaults` test expects `mounts` empty.

### README

The `--mount` row: default `every real disk`.

## Pitfalls

- `/proc/self/mounts` escapes spaces in paths as `\040`: decode three-digit octal escapes only.
- "Under `/boot`" means the path is `/boot` or starts with `/boot/`; `/bootstrap` is not under it.
- The fixture sysroot has no `proc/self/mounts`; add none — test `parse_mounts` with strings, and
  `auto_mounts` on a temp directory you create.
- Disks are read with `statvfs` on real paths: keep `read_disks` as it is.

## Tests

`tests/auto_mounts.rs`, exactly these names.

- `real_filesystems_only`: from
  ```
  /dev/nvme0n1p2 / ext4 rw,relatime 0 0
  proc /proc proc rw 0 0
  tmpfs /run tmpfs rw 0 0
  /dev/nvme0n1p1 /boot/efi vfat rw 0 0
  /dev/sda1 /srv/models xfs rw 0 0
  overlay /var/lib/docker/overlay2/x/merged overlay rw 0 0
  /dev/loop3 /snap/core/1 squashfs ro 0 0
  nas:/export /mnt/nas nfs4 rw 0 0
  ```
  → `/`, `/srv/models`, `/mnt/nas`.
- `one_entry_per_device`: `/dev/sdb1 /data btrfs rw,subvol=/a 0 0` then `/dev/sdb1 /data/b btrfs
  rw,subvol=/b 0 0` → only `/data`.
- `escaped_mount_points`: `/dev/sdc1 /media/my\040disk ext4 rw 0 0` → `/media/my disk`.
- `boot_prefix_is_a_path_prefix`: `/dev/sdd1 /bootstrap ext4 rw 0 0` is kept; `/dev/sde1 /boot ext4 rw 0 0`
  is not.
- `at_most_eight`: ten lines of distinct devices → 8 entries, the first eight.
- `auto_mounts_reads_the_sysroot`: a temp dir with `proc/self/mounts` holding the first test's text →
  `auto_mounts` gives the same three paths; a temp dir without the file → empty.

## Acceptance criteria

- [ ] The public API above exists; without `--mount`, the sampler shows the machine's real disks.
- [ ] The 6 tests above exist under these names and pass; every earlier test passes (the `defaults` test with
      an empty `mounts`).
- [ ] `bash .devbox/check` is green.
