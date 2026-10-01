// SPDX-License-Identifier: GPL-3.0-only

//! Removable drives, recognized by their filesystem UUID.
//!
//! A USB drive labeled "Backup" is mounted at `/media/alex/Backup` today and
//! at `/media/alex/Backup1` tomorrow if another drive with the same label is
//! plugged in first. A backup that remembered the mount point would then
//! write to the wrong drive or fail. Stellarshot remembers the drive's UUID
//! and a path inside it, and looks up where that drive is mounted right now.

use std::collections::HashMap;
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use crate::debug::ENGINE;
use crate::error_log;

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

/// Every removable drive that is mounted now. `Err` if the system's list
/// of mounts could not be read, which is not the same as no drive being
/// plugged in.
pub fn mounted_drives() -> Result<Vec<Drive>, String> {
    // Bytes, not a `String`: mountinfo only escapes space, tab, newline and
    // backslash, so one mount point with a non-UTF-8 name would make a
    // `read_to_string` of the whole file fail and every drive look unplugged.
    let mountinfo = std::fs::read("/proc/self/mountinfo").map_err(|err| {
        error_log!(ENGINE, "could not read /proc/self/mountinfo: {err}");
        err.to_string()
    })?;
    Ok(drives_from(
        &device_links(Path::new("/dev/disk/by-uuid")),
        &device_links(Path::new("/dev/disk/by-label")),
        &mountinfo,
        // A mapped device (an unlocked LUKS volume) is mounted from
        // `/dev/mapper/…`, a link to the `/dev/dm-N` the by-uuid link
        // resolves to.
        &|source| std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf()),
    ))
}

/// Where the drive with `uuid` is mounted now, if it is.
pub fn mount_point(uuid: &str) -> Option<PathBuf> {
    mounted_drives()
        .unwrap_or_default()
        .into_iter()
        .find(|drive| drive.uuid == uuid)
        .map(|drive| drive.mount_point)
}

/// The removable drive `path` is on, and the path relative to its root.
pub fn drive_for(path: &Path) -> Option<(Drive, PathBuf)> {
    locate(&mounted_drives().unwrap_or_default(), path)
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
            let name = unescape(entry.file_name().as_bytes())
                .to_string_lossy()
                .into_owned();
            Some((name, device))
        })
        .collect()
}

/// Build the drive list from `/dev/disk/by-uuid`, `/dev/disk/by-label` and
/// `/proc/self/mountinfo`. `resolve` turns a mount's source device into the
/// canonical path the `by_*` links were resolved to (injected so tests need
/// no real devices).
fn drives_from(
    by_uuid: &[(String, PathBuf)],
    by_label: &[(String, PathBuf)],
    mountinfo: &[u8],
    resolve: &dyn Fn(&Path) -> PathBuf,
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
    for line in mountinfo.split(|&byte| byte == b'\n') {
        let Some(separator) = line.windows(3).position(|window| window == b" - ") else {
            continue;
        };
        let (before, after) = (&line[..separator], &line[separator + 3..]);
        let mut fields = before.split(|&byte| byte == b' ').filter(|f| !f.is_empty());
        let Some(mount_point) = fields.nth(4) else {
            continue;
        };
        let Some(source) = after
            .split(|&byte| byte == b' ')
            .filter(|f| !f.is_empty())
            .nth(1)
        else {
            continue;
        };
        let mount_point = PathBuf::from(unescape(mount_point));
        let removable = REMOVABLE_ROOTS.iter().any(|root| {
            mount_point
                .as_os_str()
                .as_bytes()
                .starts_with(root.as_bytes())
        });
        if !removable {
            continue;
        }
        let device = resolve(Path::new(&unescape(source)));
        let Some(uuid) = uuids.get(device.as_path()) else {
            continue;
        };
        let label = labels
            .get(device.as_path())
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
/// `\x20` (hex) in `/dev/disk/by-label`. Returns the raw decoded bytes as
/// an `OsString`, not a lossy `String`: a mount point built from this must
/// keep its exact bytes, or a real one containing them (rare, but a real
/// filesystem label or path is not guaranteed to be UTF-8) would never be
/// recognized as the drive it actually is.
fn unescape(bytes: &[u8]) -> OsString {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            if bytes.get(i + 1) == Some(&b'x')
                && let Some(value) = bytes
                    .get(i + 2..i + 4)
                    .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok())
            {
                out.push(value);
                i += 4;
                continue;
            }
            if let Some(value) = bytes
                .get(i + 1..i + 4)
                .and_then(|octal| u8::from_str_radix(std::str::from_utf8(octal).ok()?, 8).ok())
            {
                out.push(value);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    OsString::from_vec(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTINFO: &[u8] = b"\
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

    /// No device is a link to another.
    fn same(source: &Path) -> PathBuf {
        source.to_path_buf()
    }

    #[test]
    fn only_removable_mounts_with_a_uuid_are_drives() {
        let (by_uuid, by_label) = links();
        let drives = drives_from(&by_uuid, &by_label, MOUNTINFO, &same);

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
        let moved = String::from_utf8_lossy(MOUNTINFO)
            .replace("/media/alex/Backup ", "/media/alex/Backup1 ");
        let drives = drives_from(&by_uuid, &by_label, moved.as_bytes(), &same);

        let drive = drives.iter().find(|d| d.uuid == "1111-AAAA").unwrap();
        assert_eq!(drive.mount_point, PathBuf::from("/media/alex/Backup1"));
    }

    #[test]
    fn missing_drive_is_unavailable() {
        let (by_uuid, by_label) = links();
        let unplugged: String = String::from_utf8_lossy(MOUNTINFO)
            .lines()
            .filter(|line| !line.contains("/dev/sdb1"))
            .map(|line| format!("{line}\n"))
            .collect();
        let drives = drives_from(&by_uuid, &by_label, unplugged.as_bytes(), &same);

        assert!(drives.iter().all(|d| d.uuid != "1111-AAAA"));
    }

    #[test]
    fn drive_for_finds_the_containing_mount() {
        let (by_uuid, by_label) = links();
        let drives = drives_from(&by_uuid, &by_label, MOUNTINFO, &same);

        let (drive, relative) =
            locate(&drives, Path::new("/media/alex/Backup/Stellarshot/laptop")).unwrap();
        assert_eq!(drive.uuid, "1111-AAAA");
        assert_eq!(relative, PathBuf::from("Stellarshot/laptop"));
        assert!(locate(&drives, Path::new("/home/alex/Backups")).is_none());
    }

    #[test]
    fn escapes_are_decoded() {
        assert_eq!(unescape(b"Photo\\040Disk"), "Photo Disk");
        assert_eq!(unescape(b"My\\x20Drive"), "My Drive");
        assert_eq!(unescape(b"plain"), "plain");
    }

    #[test]
    fn an_unlocked_encrypted_drive_is_found_through_its_mapper_link() {
        let (by_uuid, by_label) = links();
        // The by-uuid link of a LUKS volume resolves to `/dev/dm-0`, but
        // mountinfo names it by its `/dev/mapper` link.
        let by_uuid = [
            by_uuid,
            vec![("3333-CCCC".to_owned(), PathBuf::from("/dev/dm-0"))],
        ]
        .concat();
        let mountinfo = b"94 22 254:0 / /run/media/alex/Vault rw - ext4 /dev/mapper/luks-abc rw\n";
        let resolve = |source: &Path| {
            if source == Path::new("/dev/mapper/luks-abc") {
                PathBuf::from("/dev/dm-0")
            } else {
                source.to_path_buf()
            }
        };

        let drives = drives_from(&by_uuid, &by_label, mountinfo, &resolve);

        assert_eq!(drives.len(), 1);
        assert_eq!(drives[0].uuid, "3333-CCCC");
    }

    #[test]
    fn a_non_utf8_mount_point_elsewhere_does_not_hide_every_drive() {
        let (by_uuid, by_label) = links();
        let mut mountinfo = b"95 22 8:49 / /mnt/caf\xe9 rw - ext4 /dev/sdd1 rw\n".to_vec();
        mountinfo.extend_from_slice(MOUNTINFO);

        let drives = drives_from(&by_uuid, &by_label, &mountinfo, &same);

        assert!(
            drives.iter().any(|d| d.uuid == "1111-AAAA"),
            "the plugged-in drive is still there: {drives:?}"
        );
    }

    #[test]
    fn a_drive_mounted_at_a_non_utf8_path_keeps_its_exact_bytes() {
        let (by_uuid, by_label) = links();
        let mountinfo = b"96 22 8:17 / /media/alex/caf\xe9 rw - ext4 /dev/sdb1 rw\n";

        let drives = drives_from(&by_uuid, &by_label, mountinfo, &same);

        assert_eq!(
            drives[0].mount_point.as_os_str().as_bytes(),
            b"/media/alex/caf\xe9"
        );
    }
}
