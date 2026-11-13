use crate::scan::RawNode;
use crate::util;
use eframe::egui::Color32;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::ops::Range;
use std::path::PathBuf;

pub const NONE: u32 = u32::MAX;

/// Flattened node. Children of a node are stored contiguously and sorted by size (desc).
pub struct Node {
    pub name: Box<str>,
    pub size: u64,
    pub mtime: i64,
    pub parent: u32,
    pub child_start: u32,
    pub child_count: u32,
    /// Number of files below (1 for a file).
    pub files: u32,
    /// Number of folders below.
    pub dirs: u32,
    pub ext: u16,
    pub is_dir: bool,
}

pub struct ExtInfo {
    pub name: String,
    pub color: Color32,
}

impl ExtInfo {
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            "(拡張子なし)".into()
        } else {
            format!(".{}", self.name)
        }
    }
}

pub struct Tree {
    pub root_path: PathBuf,
    pub nodes: Vec<Node>,
    pub exts: Vec<ExtInfo>,
    pub scan_secs: f64,
    pub errors: u64,
    pub cancelled: bool,
}

/// Statistics for a subtree (the folder currently shown).
pub struct Analysis {
    pub root: u32,
    pub generation: u64,
    /// (ext id, bytes, count), sorted by bytes desc.
    pub exts: Vec<(u16, u64, u64)>,
    pub top_files: Vec<u32>,
}

fn ext_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() && name.len() - i - 1 <= 12 => &name[i + 1..],
        _ => "",
    }
}

struct Builder {
    nodes: Vec<Node>,
    ext_map: HashMap<String, u16>,
    ext_names: Vec<String>,
}

impl Builder {
    fn ext_id(&mut self, name: &str) -> u16 {
        let e = ext_of(name).to_lowercase();
        if let Some(&id) = self.ext_map.get(&e) {
            return id;
        }
        if self.ext_names.len() >= 60_000 {
            return 0;
        }
        let id = self.ext_names.len() as u16;
        self.ext_names.push(e.clone());
        self.ext_map.insert(e, id);
        id
    }

    fn make(&mut self, raw: &mut RawNode, parent: u32) -> Node {
        let name = std::mem::take(&mut raw.name).into_boxed_str();
        let ext = if raw.is_dir { 0 } else { self.ext_id(&name) };
        Node {
            name,
            size: raw.size,
            mtime: raw.mtime,
            parent,
            child_start: 0,
            child_count: 0,
            files: if raw.is_dir { 0 } else { 1 },
            dirs: 0,
            ext,
            is_dir: raw.is_dir,
        }
    }

    fn fill(&mut self, id: u32, raw: RawNode) -> (u32, u32) {
        let mut children = raw.children;
        children.sort_unstable_by(|a, b| b.size.cmp(&a.size));
        let start = self.nodes.len() as u32;
        let count = children.len() as u32;
        for c in children.iter_mut() {
            let n = self.make(c, id);
            self.nodes.push(n);
        }
        let (mut files, mut dirs) = (0u32, 0u32);
        for (i, c) in children.into_iter().enumerate() {
            if c.is_dir {
                let (f, d) = self.fill(start + i as u32, c);
                files += f;
                dirs += d + 1;
            } else {
                files += 1;
            }
        }
        let n = &mut self.nodes[id as usize];
        n.child_start = start;
        n.child_count = count;
        n.files = files;
        n.dirs = dirs;
        (files, dirs)
    }
}

impl Tree {
    pub fn build(mut raw: RawNode, root_path: PathBuf, scan_secs: f64, errors: u64, cancelled: bool) -> Tree {
        let mut b = Builder {
            nodes: Vec::new(),
            ext_map: HashMap::from([(String::new(), 0)]),
            ext_names: vec![String::new()],
        };
        let root = b.make(&mut raw, NONE);
        b.nodes.push(root);
        b.fill(0, raw);

        // Extension statistics; re-number extensions by total size.
        let mut acc = vec![(0u64, 0u64); b.ext_names.len()];
        for n in b.nodes.iter().filter(|n| !n.is_dir) {
            let a = &mut acc[n.ext as usize];
            a.0 += n.size;
            a.1 += 1;
        }
        let mut order: Vec<usize> = (0..acc.len()).collect();
        order.sort_by(|&x, &y| acc[y].0.cmp(&acc[x].0));
        let mut remap = vec![0u16; acc.len()];
        for (new, &old) in order.iter().enumerate() {
            remap[old] = new as u16;
        }
        for n in b.nodes.iter_mut().filter(|n| !n.is_dir) {
            n.ext = remap[n.ext as usize];
        }
        let exts = order
            .iter()
            .enumerate()
            .map(|(i, &old)| ExtInfo {
                name: b.ext_names[old].clone(),
                color: util::ext_color(i, &b.ext_names[old]),
            })
            .collect();

        Tree { root_path, nodes: b.nodes, exts, scan_secs, errors, cancelled }
    }

    #[inline]
    pub fn node(&self, id: u32) -> &Node {
        &self.nodes[id as usize]
    }

    #[inline]
    pub fn children(&self, id: u32) -> Range<u32> {
        let n = self.node(id);
        n.child_start..n.child_start + n.child_count
    }

    pub fn display_name(&self, id: u32) -> String {
        if id == 0 {
            self.root_path.display().to_string()
        } else {
            self.node(id).name.to_string()
        }
    }

    pub fn path(&self, id: u32) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != 0 && cur != NONE {
            parts.push(&*self.node(cur).name);
            cur = self.node(cur).parent;
        }
        let mut p = self.root_path.clone();
        for s in parts.iter().rev() {
            p.push(s);
        }
        p
    }

    /// Root … id (inclusive).
    pub fn ancestors(&self, id: u32) -> Vec<u32> {
        let mut v = Vec::new();
        let mut cur = id;
        while cur != NONE {
            v.push(cur);
            cur = self.node(cur).parent;
        }
        v.reverse();
        v
    }

    /// The ancestor of `id` that is a direct child of `root` (or `root` itself).
    pub fn top_child(&self, root: u32, id: u32) -> u32 {
        let mut cur = id;
        while cur != root && cur != NONE {
            let p = self.node(cur).parent;
            if p == root {
                return cur;
            }
            cur = p;
        }
        root
    }

    pub fn file_color(&self, n: &Node) -> Color32 {
        self.exts[n.ext as usize].color
    }

    pub fn analyze(&self, root: u32, generation: u64, top_n: usize) -> Analysis {
        let mut acc = vec![(0u64, 0u64); self.exts.len()];
        let mut heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::with_capacity(top_n + 1);
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let n = self.node(id);
            if n.is_dir {
                stack.extend(self.children(id));
            } else {
                let a = &mut acc[n.ext as usize];
                a.0 += n.size;
                a.1 += 1;
                if heap.len() < top_n {
                    heap.push(Reverse((n.size, id)));
                } else if n.size > heap.peek().map(|r| r.0 .0).unwrap_or(0) {
                    heap.pop();
                    heap.push(Reverse((n.size, id)));
                }
            }
        }
        let mut top: Vec<(u64, u32)> = heap.into_iter().map(|Reverse(x)| x).collect();
        top.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        let mut exts: Vec<(u16, u64, u64)> = acc
            .iter()
            .enumerate()
            .filter(|(_, a)| a.1 > 0)
            .map(|(i, a)| (i as u16, a.0, a.1))
            .collect();
        exts.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        Analysis { root, generation, exts, top_files: top.into_iter().map(|x| x.1).collect() }
    }
}
