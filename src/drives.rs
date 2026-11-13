use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))] // Ram / Unknown only come from the Windows API
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Optical,
    Ram,
    Unknown,
}

#[derive(Clone)]
pub struct DriveInfo {
    /// Path to scan: `C:\` on Windows, the mount point on macOS / Linux.
    pub root: String,
    /// Short name for compact display: `C:` on Windows, e.g. `/` or `home` elsewhere.
    pub name: String,
    pub label: String,
    pub fs: String,
    pub kind: DriveKind,
    pub total: u64,
    pub free: u64,
}

impl DriveInfo {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.free)
    }
    pub fn ratio(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.used() as f32 / self.total as f32
        }
    }
    pub fn display_label(&self) -> String {
        if !self.label.is_empty() {
            return self.label.clone();
        }
        match self.kind {
            DriveKind::Removable => "リムーバブル",
            DriveKind::Fixed => "ローカル ディスク",
            DriveKind::Network => "ネットワーク",
            DriveKind::Optical => "CD/DVD",
            DriveKind::Ram => "RAM ディスク",
            DriveKind::Unknown => "ドライブ",
        }
        .to_string()
    }
}

pub use imp::list;

/// The drive whose root *is* `path` (e.g. `C:\` or `/home`), if any.
#[cfg(windows)]
pub fn drive_root_of<'a>(drives: &'a [DriveInfo], path: &Path) -> Option<&'a DriveInfo> {
    let s = path.to_string_lossy();
    let s = s.trim_end_matches('\\');
    if s.len() != 2 {
        return None;
    }
    drives.iter().find(|d| d.name.eq_ignore_ascii_case(s))
}

/// The drive whose root *is* `path` (e.g. `C:\` or `/home`), if any.
#[cfg(not(windows))]
pub fn drive_root_of<'a>(drives: &'a [DriveInfo], path: &Path) -> Option<&'a DriveInfo> {
    drives.iter().find(|d| Path::new(&d.root) == path)
}

// ====================================================================== Windows

#[cfg(windows)]
mod imp {
    use super::{DriveInfo, DriveKind};
    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn from_wide(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    pub fn list() -> Vec<DriveInfo> {
        let mut out = Vec::new();
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let name = format!("{}:", (b'A' + i as u8) as char);
            let root = format!("{name}\\");
            let w = wide(&root);
            let kind = match unsafe { GetDriveTypeW(w.as_ptr()) } {
                2 => DriveKind::Removable,
                3 => DriveKind::Fixed,
                4 => DriveKind::Network,
                5 => DriveKind::Optical,
                6 => DriveKind::Ram,
                _ => DriveKind::Unknown,
            };
            let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
            let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) };
            if ok == 0 {
                continue; // no media etc.
            }
            let mut label = [0u16; 261];
            let mut fs = [0u16; 261];
            unsafe {
                GetVolumeInformationW(
                    w.as_ptr(),
                    label.as_mut_ptr(),
                    label.len() as u32,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    fs.as_mut_ptr(),
                    fs.len() as u32,
                );
            }
            out.push(DriveInfo {
                root,
                name,
                label: from_wide(&label),
                fs: from_wide(&fs),
                kind,
                total,
                free,
            });
        }
        out
    }
}

// ====================================================================== macOS / Linux

#[cfg(not(windows))]
mod imp {
    use super::{DriveInfo, DriveKind};
    use std::collections::HashSet;
    use std::path::Path;
    use sysinfo::Disks;

    /// Virtual / container file systems that are not worth showing as a disk.
    const PSEUDO_FS: &[&str] = &[
        "autofs", "binfmt_misc", "bpf", "cgroup", "cgroup2", "configfs", "debugfs", "devfs", "devpts",
        "devtmpfs", "efivarfs", "fusectl", "hugetlbfs", "mqueue", "nsfs", "overlay", "proc", "pstore",
        "ramfs", "securityfs", "squashfs", "sysfs", "tracefs", "nullfs", "fuse.portal", "fuse.gvfsd-fuse",
    ];
    const NETWORK_FS: &[&str] = &["nfs", "nfs4", "cifs", "smbfs", "smb3", "afpfs", "webdav", "sshfs", "fuse.sshfs"];

    /// macOS splits the boot disk into a read-only system volume (`/`) and a data volume.
    /// User files live on the data volume (reachable from `/` only via firmlinks), so that is
    /// the one worth scanning; the other helper volumes under /System/Volumes are hidden.
    fn skip_mount(mount: &str) -> bool {
        if cfg!(target_os = "macos") {
            return mount.starts_with("/System/Volumes/") && mount != "/System/Volumes/Data";
        }
        ["/proc", "/sys", "/dev", "/run", "/snap", "/var/lib/docker"]
            .iter()
            .any(|p| mount == *p || mount.starts_with(&format!("{p}/")))
            && !mount.starts_with("/run/media/")
    }

    pub fn list() -> Vec<DriveInfo> {
        let disks = Disks::new_with_refreshed_list();
        let has_mac_data = disks.list().iter().any(|d| d.mount_point() == Path::new("/System/Volumes/Data"));
        let mut seen_dev = HashSet::new();
        let mut out = Vec::new();
        let mut sorted: Vec<_> = disks.list().iter().collect();
        // Shorter mount points first so bind mounts / subvolumes of one device keep the main one.
        sorted.sort_by_key(|d| d.mount_point().as_os_str().len());
        for d in sorted {
            let mount = d.mount_point().to_string_lossy().into_owned();
            let fs = d.file_system().to_string_lossy().into_owned();
            let dev = d.name().to_string_lossy().into_owned();
            if d.total_space() == 0 || PSEUDO_FS.contains(&fs.as_str()) || fs == "tmpfs" || skip_mount(&mount) {
                continue;
            }
            if cfg!(target_os = "macos") && has_mac_data && mount == "/" {
                continue;
            }
            if cfg!(target_os = "linux") && dev.starts_with("/dev/") && !seen_dev.insert(dev.clone()) {
                continue;
            }
            let kind = if NETWORK_FS.contains(&fs.as_str()) {
                DriveKind::Network
            } else if fs == "iso9660" || fs == "udf" || fs == "cd9660" {
                DriveKind::Optical
            } else if d.is_removable() {
                DriveKind::Removable
            } else {
                DriveKind::Fixed
            };
            let name = match Path::new(&mount).file_name() {
                Some(n) => n.to_string_lossy().into_owned(),
                None => mount.clone(),
            };
            // macOS reports the volume name ("Macintosh HD"); Linux reports the device ("/dev/sda1").
            let (label, fs) = if dev.starts_with('/') || dev.is_empty() {
                (String::new(), if dev.is_empty() { fs } else { format!("{fs}  {dev}") })
            } else {
                (dev, fs)
            };
            out.push(DriveInfo { root: mount, name, label, fs, kind, total: d.total_space(), free: d.available_space() });
        }
        out.sort_by(|a, b| a.root.cmp(&b.root));
        out
    }
}
