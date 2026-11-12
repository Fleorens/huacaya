//! Integration tests for `collect::disk` mount parsing (`parse_mounts`, `auto_mounts`).

use std::path::PathBuf;
use std::time::SystemTime;

use llamatop::collect::disk::{auto_mounts, parse_mounts};

/// The mount table used by `real_filesystems_only` and `auto_mounts_reads_the_sysroot`.
const REAL_FS_TEXT: &str = "\
/dev/nvme0n1p2 / ext4 rw,relatime 0 0
proc /proc proc rw 0 0
tmpfs /run tmpfs rw 0 0
/dev/nvme0n1p1 /boot/efi vfat rw 0 0
/dev/sda1 /srv/models xfs rw 0 0
overlay /var/lib/docker/overlay2/x/merged overlay rw 0 0
/dev/loop3 /snap/core/1 squashfs ro 0 0
nas:/export /mnt/nas nfs4 rw 0 0
";

/// Create a fresh, uniquely named empty directory under the system temp dir, and return its path.
fn temp_dir(prefix: &str) -> PathBuf {
    let unique = format!(
        "{}-{}-{}",
        prefix,
        std::process::id(),
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn real_filesystems_only() {
    assert_eq!(
        parse_mounts(REAL_FS_TEXT),
        vec![PathBuf::from("/"), PathBuf::from("/srv/models"), PathBuf::from("/mnt/nas")]
    );
}

#[test]
fn one_entry_per_device() {
    let text = "\
/dev/sdb1 /data btrfs rw,subvol=/a 0 0
/dev/sdb1 /data/b btrfs rw,subvol=/b 0 0
";
    assert_eq!(parse_mounts(text), vec![PathBuf::from("/data")]);
}

#[test]
fn escaped_mount_points() {
    let text = "/dev/sdc1 /media/my\\040disk ext4 rw 0 0\n";
    assert_eq!(parse_mounts(text), vec![PathBuf::from("/media/my disk")]);
}

#[test]
fn boot_prefix_is_a_path_prefix() {
    let text = "\
/dev/sdd1 /bootstrap ext4 rw 0 0
/dev/sde1 /boot ext4 rw 0 0
";
    assert_eq!(parse_mounts(text), vec![PathBuf::from("/bootstrap")]);
}

#[test]
fn at_most_eight() {
    let mut text = String::new();
    for i in 0..10 {
        text.push_str(&format!("/dev/sd{} /mnt/disk{} xfs rw 0 0\n", i, i));
    }
    let mounts = parse_mounts(&text);
    assert_eq!(mounts.len(), 8);
    assert_eq!(mounts[0], PathBuf::from("/mnt/disk0"));
    assert_eq!(mounts[7], PathBuf::from("/mnt/disk7"));
}

#[test]
fn auto_mounts_reads_the_sysroot() {
    let root = temp_dir("llamatop-auto-mounts-ok");
    std::fs::create_dir_all(root.join("proc/self")).unwrap();
    std::fs::write(root.join("proc/self/mounts"), REAL_FS_TEXT).unwrap();
    assert_eq!(
        auto_mounts(&root),
        vec![PathBuf::from("/"), PathBuf::from("/srv/models"), PathBuf::from("/mnt/nas")]
    );
    std::fs::remove_dir_all(&root).ok();

    let empty_root = temp_dir("llamatop-auto-mounts-missing");
    assert!(auto_mounts(&empty_root).is_empty());
    std::fs::remove_dir_all(&empty_root).ok();
}
