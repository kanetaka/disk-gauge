use crate::tree::Tree;
use crate::util;
use eframe::egui::{pos2, vec2, Color32, Mesh, Pos2, Rect};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Ext,
    Folder,
}

/// Everything a cached visualization depends on.
#[derive(Clone, Copy, PartialEq)]
pub struct VizKey {
    pub generation: u64,
    pub root: u32,
    pub rect: [i32; 4],
    pub mode: ColorMode,
    pub hl: Option<u16>,
}

pub fn rect_key(r: Rect) -> [i32; 4] {
    [r.min.x as i32, r.min.y as i32, r.max.x as i32, r.max.y as i32]
}

pub struct Item {
    pub rect: Rect,
    pub node: u32,
}

pub struct Label {
    pub rect: Rect,
    pub text: String,
    pub header: bool,
}

pub struct Treemap {
    pub key: VizKey,
    pub items: Vec<Item>,
    pub mesh: Mesh,
    pub labels: Vec<Label>,
}

const MAX_ITEMS: usize = 90_000;
const HEADER_H: f32 = 15.0;

/// Squarified treemap layout (Bruls et al.). `values` must be sorted descending.
pub fn squarify(values: &[f64], rect: Rect) -> Vec<Rect> {
    let n = values.len();
    let mut out = Vec::with_capacity(n);
    let total: f64 = values.iter().sum();
    let (mut x, mut y) = (rect.min.x as f64, rect.min.y as f64);
    let (mut w, mut h) = (rect.width() as f64, rect.height() as f64);
    if total <= 0.0 || w <= 0.0 || h <= 0.0 {
        out.resize(n, Rect::NOTHING);
        return out;
    }
    let scale = w * h / total;
    let mut i = 0;
    while i < n && values[i] > 0.0 {
        let short = w.min(h).max(1e-9);
        let maxv = values[i] * scale;
        let (mut j, mut sum, mut best) = (i, 0.0, f64::INFINITY);
        while j < n && values[j] > 0.0 {
            let v = values[j] * scale;
            let s = sum + v;
            let worst = (short * short * maxv / (s * s)).max(s * s / (short * short * v));
            if worst > best {
                break;
            }
            best = worst;
            sum = s;
            j += 1;
        }
        if w >= h {
            let cw = (sum / h.max(1e-9)).min(w);
            let mut cy = y;
            for v in &values[i..j] {
                let hh = v * scale / cw.max(1e-9);
                out.push(Rect::from_min_size(pos2(x as f32, cy as f32), vec2(cw as f32, hh as f32)));
                cy += hh;
            }
            x += cw;
            w = (w - cw).max(0.0);
        } else {
            let rh = (sum / w.max(1e-9)).min(h);
            let mut cx = x;
            for v in &values[i..j] {
                let ww = v * scale / rh.max(1e-9);
                out.push(Rect::from_min_size(pos2(cx as f32, y as f32), vec2(ww as f32, rh as f32)));
                cx += ww;
            }
            y += rh;
            h = (h - rh).max(0.0);
        }
        i = j;
    }
    out.resize(n, Rect::NOTHING);
    out
}

struct Builder<'a> {
    tree: &'a Tree,
    mode: ColorMode,
    hl: Option<u16>,
    items: Vec<Item>,
    mesh: Mesh,
    labels: Vec<Label>,
}

impl Builder<'_> {
    fn file_color(&self, id: u32, depth: u16, branch: Color32) -> Color32 {
        let tree = self.tree;
        let n = tree.node(id);
        let base = match self.mode {
            ColorMode::Ext => tree.file_color(n),
            ColorMode::Folder => util::lighten(branch, (depth as f32 * 0.04).min(0.3)),
        };
        match self.hl {
            Some(h) if n.ext != h => util::mix(base, Color32::from_gray(38), 0.85),
            Some(_) => util::lighten(base, 0.12),
            None => base,
        }
    }

    fn lay(&mut self, id: u32, rect: Rect, depth: u16, branch: Color32) {
        let tree = self.tree;
        let n = tree.node(id);
        self.items.push(Item { rect, node: id });

        if !n.is_dir {
            let col = self.file_color(id, depth, branch);
            let r = if rect.width() > 4.0 && rect.height() > 4.0 { rect.shrink(0.5) } else { rect };
            util::add_cushion(&mut self.mesh, r, col);
            if rect.width() > 64.0 && rect.height() > 20.0 {
                let text = if rect.height() > 38.0 {
                    format!("{}\n{}", n.name, util::fmt_size(n.size))
                } else {
                    n.name.to_string()
                };
                self.labels.push(Label { rect: rect.shrink(2.0), text, header: false });
            }
            return;
        }

        // Folder: dark frame whose shade depends on depth, children inside.
        let g = (24 + depth.min(10) as u32 * 6) as u8;
        util::add_rect(&mut self.mesh, rect, Color32::from_gray(g));
        if n.child_count == 0 || rect.width() < 4.0 || rect.height() < 4.0 || self.items.len() > MAX_ITEMS {
            return;
        }
        let mut inner = if depth == 0 { rect } else { rect.shrink(1.5) };
        if depth > 0 && rect.height() > 44.0 && rect.width() > 60.0 {
            let header = Rect::from_min_max(inner.min, pos2(inner.max.x, inner.min.y + HEADER_H));
            self.labels.push(Label {
                rect: header,
                text: format!("{}   {}", n.name, util::fmt_size(n.size)),
                header: true,
            });
            inner.min.y += HEADER_H;
        }
        let range = tree.children(id);
        let values: Vec<f64> = range.clone().map(|c| tree.node(c).size as f64).collect();
        let rects = squarify(&values, inner);
        for (k, cid) in range.enumerate() {
            let r = rects[k];
            if !r.is_positive() || r.area() < 3.0 {
                break; // sizes are sorted, the rest is even smaller
            }
            let b = if depth == 0 { util::branch_color(k) } else { branch };
            self.lay(cid, r, depth + 1, b);
        }
    }
}

impl Treemap {
    pub fn build(tree: &Tree, key: VizKey, rect: Rect) -> Self {
        let mut b = Builder {
            tree,
            mode: key.mode,
            hl: key.hl,
            items: Vec::new(),
            mesh: Mesh::default(),
            labels: Vec::new(),
        };
        b.lay(key.root, rect, 0, util::branch_color(0));
        Treemap { key, items: b.items, mesh: b.mesh, labels: b.labels }
    }

    /// Deepest item under `p`.
    pub fn hit(&self, p: Pos2) -> Option<&Item> {
        self.items.iter().rev().find(|it| it.rect.contains(p))
    }

    pub fn rect_of(&self, node: u32) -> Option<Rect> {
        self.items.iter().find(|it| it.node == node).map(|it| it.rect)
    }
}
