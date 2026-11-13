use crate::tree::Tree;
use eframe::egui;
use rayon::prelude::*;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Intermediate tree produced by the parallel scanner.
pub struct RawNode {
    pub name: String,
    pub size: u64,
    pub mtime: i64,
    pub is_dir: bool,
    pub children: Vec<RawNode>,
}

#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    pub cancel: AtomicBool,
    pub building: AtomicBool,
    pub done: AtomicBool,
    pub current: Mutex<String>,
}

pub struct ScanJob {
    pub path: PathBuf,
    pub progress: Arc<Progress>,
    pub result: Arc<Mutex<Option<Tree>>>,
    pub started: Instant,
    /// Used bytes of the drive, when a whole drive is scanned (for a progress estimate).
    pub expected: Option<u64>,
}

// Cloud placeholders (OneDrive "online only" etc.) do not occupy local storage.
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
const CLOUD_ATTRS: u32 =
    FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;

fn to_secs(t: std::io::Result<SystemTime>) -> i64 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn scan_dir(path: &Path, name: String, mtime: i64, p: &Progress) -> RawNode {
    let mut files = Vec::new();
    let mut subdirs = Vec::new();
    if let Ok(mut g) = p.current.try_lock() {
        g.clear();
        g.push_str(&path.to_string_lossy());
    }
    match fs::read_dir(path) {
        Ok(rd) => {
            for entry in rd {
                if p.cancel.load(Relaxed) {
                    break;
                }
                let Ok(entry) = entry else {
                    p.errors.fetch_add(1, Relaxed);
                    continue;
                };
                let Ok(ft) = entry.file_type() else { continue };
                // Junctions / symlinks: skip to avoid double counting and loops.
                if ft.is_symlink() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                let md = entry.metadata().ok();
                let mtime = md.as_ref().map(|m| to_secs(m.modified())).unwrap_or(0);
                if ft.is_dir() {
                    subdirs.push((entry.path(), name, mtime));
                } else {
                    let size = md
                        .as_ref()
                        .map(|m| if m.file_attributes() & CLOUD_ATTRS != 0 { 0 } else { m.len() })
                        .unwrap_or(0);
                    p.files.fetch_add(1, Relaxed);
                    p.bytes.fetch_add(size, Relaxed);
                    files.push(RawNode { name, size, mtime, is_dir: false, children: Vec::new() });
                }
            }
        }
        Err(_) => {
            p.errors.fetch_add(1, Relaxed);
        }
    }
    p.dirs.fetch_add(1, Relaxed);

    let mut children: Vec<RawNode> = if p.cancel.load(Relaxed) {
        Vec::new()
    } else {
        subdirs
            .into_par_iter()
            .map(|(pp, n, m)| scan_dir(&pp, n, m, p))
            .collect()
    };
    children.extend(files);
    let size = children.iter().map(|c| c.size).sum();
    RawNode { name, size, mtime, is_dir: true, children }
}

pub fn start(path: PathBuf, expected: Option<u64>, ctx: egui::Context) -> ScanJob {
    let progress = Arc::new(Progress::default());
    let result = Arc::new(Mutex::new(None));
    {
        let p = progress.clone();
        let r = result.clone();
        let path = path.clone();
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let mtime = fs::metadata(&path).map(|m| to_secs(m.modified())).unwrap_or(0);
            let raw = scan_dir(&path, path.to_string_lossy().into_owned(), mtime, &p);
            let secs = t0.elapsed().as_secs_f64();
            p.building.store(true, Relaxed);
            ctx.request_repaint();
            let tree = Tree::build(raw, path, secs, p.errors.load(Relaxed), p.cancel.load(Relaxed));
            *r.lock().unwrap() = Some(tree);
            p.done.store(true, Relaxed);
            ctx.request_repaint();
        });
    }
    ScanJob { path, progress, result, started: Instant::now(), expected }
}
