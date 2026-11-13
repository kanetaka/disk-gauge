use std::path::Path;
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};

#[derive(Clone)]
pub struct DriveInfo {
    /// e.g. `C:\`
    pub root: String,
    pub label: String,
    pub fs: String,
    pub kind: u32,
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
            2 => "リムーバブル",
            3 => "ローカル ディスク",
            4 => "ネットワーク",
            5 => "CD/DVD",
            6 => "RAM ディスク",
            _ => "ドライブ",
        }
        .to_string()
    }
}

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
        let root = format!("{}:\\", (b'A' + i as u8) as char);
        let w = wide(&root);
        let kind = unsafe { GetDriveTypeW(w.as_ptr()) };
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
            label: from_wide(&label),
            fs: from_wide(&fs),
            kind,
            total,
            free,
        });
    }
    out
}

/// The drive whose root *is* `path` (e.g. `C:\`), if any.
pub fn drive_root_of<'a>(drives: &'a [DriveInfo], path: &Path) -> Option<&'a DriveInfo> {
    let s = path.to_string_lossy();
    let s = s.trim_end_matches('\\');
    if s.len() != 2 {
        return None;
    }
    drives
        .iter()
        .find(|d| d.root[..2].eq_ignore_ascii_case(s))
}
