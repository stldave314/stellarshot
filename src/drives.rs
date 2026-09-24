// SPDX-License-Identifier: GPL-3.0-only

//! Removable drives, recognised by their filesystem UUID.
//!
//! A USB drive labelled "Backup" is mounted at `/media/alex/Backup` today and
//! at `/media/alex/Backup1` tomorrow if another drive with the same label is
//! plugged in first. A backup that remembered the mount point would then
//! write to the wrong drive or fail. Stellarshot remembers the drive's UUID
//! and a path inside it, and looks up where that drive is mounted right now.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A mounted removable drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    pub uuid: String,
    /// The filesystem label, or the mount point's name when there is none.
    pub label: String,
    pub mount_point: PathBuf,
}

/// Where removable media is mounted on desktop Linux.
const REMOVABLE_ROOTS: &[&str] = &["/media/", "/run/media/"];

/// Every removable drive that is mounted now.
pub fn mounted_drives() -> Vec<Drive> {
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    drives_from(
        &device_links(Path::new("/dev/disk/by-uuid")),
        &device_links(Path::new("/dev/disk/by-label")),
        &mountinfo,
    )
}

/// Where the drive with `uuid` is mounted now, if it is.
pub fn mount_point(uuid: &str) -> Option<PathBuf> {
    mounted_drives()
        .into_iter()
        .find(|drive| drive.uuid == uuid)
        .map(|drive| drive.mount_point)
}

/// The removable drive `path` is on, and the path relative to its root.
pub fn drive_for(path: &Path) -> Option<(Drive, PathBuf)> {
    locate(&mounted_drives(), path)
}

fn locate(drives: &[Drive], path: &Path) -> Option<(Drive, PathBuf)> {
    drives
        .iter()
        .filter(|drive| path.starts_with(&drive.mount_point))
        .max_by_key(|drive| drive.mount_point.as_os_str().len())
        .map(|drive| {
            let relative = path
                .strip_prefix(&drive.mount_point)
                .map(Path::to_path_buf)
                .unwrap_or_default();
            (drive.clone(), relative)
        })
}

/// `name → device` for every symlink in a `/dev/disk/by-*` directory.
fn device_links(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let device = std::fs::canonicalize(entry.path()).ok()?;
            let name = unescape(&entry.file_name().to_string_lossy());
            Some((name, device))
        })
        .collect()
}

/// Build the drive list from `/dev/disk/by-uuid`, `/dev/disk/by-label` and
/// `/proc/self/mountinfo`.
fn drives_from(
    by_uuid: &[(String, PathBuf)],
    by_label: &[(String, PathBuf)],
    mountinfo: &str,
) -> Vec<Drive> {
    let uuids: HashMap<&Path, &str> = by_uuid
        .iter()
        .map(|(uuid, device)| (device.as_path(), uuid.as_str()))
        .collect();
    let labels: HashMap<&Path, &str> = by_label
        .iter()
        .map(|(label, device)| (device.as_path(), label.as_str()))
        .collect();

    let mut drives = Vec::new();
    for line in mountinfo.lines() {
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        let Some(mount_point) = before.split_whitespace().nth(4) else {
            continue;
        };
        let Some(source) = after.split_whitespace().nth(1) else {
            continue;
        };
        let mount_point = PathBuf::from(unescape(mount_point));
        let removable = REMOVABLE_ROOTS
            .iter()
            .any(|root| mount_point.to_string_lossy().starts_with(root));
        let device = Path::new(source);
        let (true, Some(uuid)) = (removable, uuids.get(device)) else {
            continue;
        };
        let label = labels
            .get(device)
            .map(|label| (*label).to_owned())
            .or_else(|| {
                mount_point
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| (*uuid).to_owned());
        drives.push(Drive {
            uuid: (*uuid).to_owned(),
            label,
            mount_point,
        });
    }
    drives
}

/// Undo the escaping the kernel and udev use: `\040` (octal) in mountinfo,
/// `\x20` (hex) in `/dev/disk/by-label`.
fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            if bytes.get(i + 1) == Some(&b'x')
                && let Some(value) = text
                    .get(i + 2..i + 4)
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                out.push(value);
                i += 4;
                continue;
            }
            if let Some(value) = text
                .get(i + 1..i + 4)
                .and_then(|octal| u8::from_str_radix(octal, 8).ok())
            {
                out.push(value);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTINFO: &str = "\
22 1 259:2 / / rw,relatime shared:1 - ext4 /dev/nvme0n1p2 rw
91 22 8:17 / /media/alex/Backup rw,nosuid,nodev,relatime shared:50 - ext4 /dev/sdb1 rw
92 22 8:33 / /run/media/alex/Photo\\040Disk rw,nosuid shared:51 - exfat /dev/sdc1 rw
93 22 0:45 / /home/alex/nas rw shared:52 - cifs //nas/share rw
";

    /// `name → device` links, as read from a `/dev/disk/by-*` directory.
    type Links = Vec<(String, PathBuf)>;

    fn links() -> (Links, Links) {
        let by_uuid = vec![
            ("root-uuid".to_owned(), PathBuf::from("/dev/nvme0n1p2")),
            ("1111-AAAA".to_owned(), PathBuf::from("/dev/sdb1")),
            ("2222-BBBB".to_owned(), PathBuf::from("/dev/sdc1")),
        ];
        let by_label = vec![("Backup".to_owned(), PathBuf::from("/dev/sdb1"))];
        (by_uuid, by_label)
    }

    #[test]
    fn only_removable_mounts_with_a_uuid_are_drives() {
        let (by_uuid, by_label) = links();
        let drives = drives_from(&by_uuid, &by_label, MOUNTINFO);

        assert_eq!(
            drives.len(),
            2,
            "the root filesystem and the NAS are not drives"
        );
        assert_eq!(drives[0].uuid, "1111-AAAA");
        assert_eq!(drives[0].label, "Backup");
        assert_eq!(
            drives[1].mount_point,
            PathBuf::from("/run/media/alex/Photo Disk")
        );
        assert_eq!(
            drives[1].label, "Photo Disk",
            "without a label, the mount point's name"
        );
    }

    #[test]
    fn uuid_resolves_to_its_current_mount_point() {
        let (by_uuid, by_label) = links();
        // The same drive, mounted somewhere else today.
        let moved = MOUNTINFO.replace("/media/alex/Backup ", "/media/alex/Backup1 ");
        let drives = drives_from(&by_uuid, &by_label, &moved);

        let drive = drives.iter().find(|d| d.uuid == "1111-AAAA").unwrap();
        assert_eq!(drive.mount_point, PathBuf::from("/media/alex/Backup1"));
    }

    #[test]
    fn missing_drive_is_unavailable() {
        let (by_uuid, by_label) = links();
        let unplugged: String = MOUNTINFO
            .lines()
            .filter(|line| !line.contains("/dev/sdb1"))
            .map(|line| format!("{line}\n"))
            .collect();
        let drives = drives_from(&by_uuid, &by_label, &unplugged);

        assert!(drives.iter().all(|d| d.uuid != "1111-AAAA"));
    }

    #[test]
    fn drive_for_finds_the_containing_mount() {
        let (by_uuid, by_label) = links();
        let drives = drives_from(&by_uuid, &by_label, MOUNTINFO);

        let (drive, relative) =
            locate(&drives, Path::new("/media/alex/Backup/Stellarshot/laptop")).unwrap();
        assert_eq!(drive.uuid, "1111-AAAA");
        assert_eq!(relative, PathBuf::from("Stellarshot/laptop"));
        assert!(locate(&drives, Path::new("/home/alex/Backups")).is_none());
    }

    #[test]
    fn escapes_are_decoded() {
        assert_eq!(unescape("Photo\\040Disk"), "Photo Disk");
        assert_eq!(unescape("My\\x20Drive"), "My Drive");
        assert_eq!(unescape("plain"), "plain");
    }
}
