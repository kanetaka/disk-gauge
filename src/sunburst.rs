use crate::tree::Tree;
use crate::treemap::{ColorMode, VizKey};
use crate::util;
use eframe::egui::{Color32, Mesh, Pos2, Rect};
use std::f32::consts::TAU;

pub const LEVELS: u16 = 7;

pub struct Seg {
    pub node: u32,
    pub a0: f32,
    pub a1: f32,
    pub r0: f32,
    pub r1: f32,
}

pub struct Sunburst {
    pub key: VizKey,
    pub center: Pos2,
    pub inner_r: f32,
    pub segs: Vec<Seg>,
    pub mesh: Mesh,
    /// (position, text)
    pub labels: Vec<(Pos2, String)>,
}

struct Builder<'a> {
    tree: &'a Tree,
    key: VizKey,
    center: Pos2,
    inner: f32,
    ring: f32,
    segs: Vec<Seg>,
    mesh: Mesh,
    labels: Vec<(Pos2, String)>,
}

impl Builder<'_> {
    fn color(&self, id: u32, depth: u16, branch: Color32) -> Color32 {
        let tree = self.tree;
        let n = tree.node(id);
        if n.is_dir {
            return util::lighten(branch, depth as f32 * 0.07);
        }
        let base = match self.key.mode {
            ColorMode::Ext => tree.file_color(n),
            ColorMode::Folder => util::lighten(branch, 0.25 + depth as f32 * 0.05),
        };
        match self.key.hl {
            Some(h) if n.ext != h => util::mix(base, Color32::from_gray(38), 0.85),
            _ => base,
        }
    }

    fn rec(&mut self, id: u32, a0: f32, a1: f32, depth: u16, branch: Color32) {
        let tree = self.tree;
        let total = tree.node(id).size;
        if total == 0 {
            return;
        }
        let r0 = self.inner + depth as f32 * self.ring;
        let r1 = r0 + self.ring;
        let mut a = a0;
        for (k, cid) in tree.children(id).enumerate() {
            let c = tree.node(cid);
            let span = ((a1 - a0) as f64 * (c.size as f64 / total as f64)) as f32;
            if span * r1 < 1.2 {
                break;
            }
            let b = if depth == 0 { util::branch_color(k) } else { branch };
            let col = self.color(cid, depth, b);
            let gap = (0.6 / r1).min(span * 0.2);
            util::add_arc(&mut self.mesh, self.center, r0 + 0.8, r1 - 0.8, a + gap, a + span - gap, util::mul(col, 0.72), col);
            self.segs.push(Seg { node: cid, a0: a, a1: a + span, r0, r1 });
            let mid_r = (r0 + r1) * 0.5;
            if span * mid_r > 70.0 && depth < 2 {
                let p = util::arc_point(self.center, mid_r, a + span * 0.5);
                self.labels.push((p, c.name.to_string()));
            }
            if c.is_dir && depth + 1 < LEVELS {
                self.rec(cid, a, a + span, depth + 1, b);
            }
            a += span;
        }
    }
}

impl Sunburst {
    pub fn build(tree: &Tree, key: VizKey, rect: Rect) -> Self {
        let center = rect.center();
        let outer = (rect.width().min(rect.height()) * 0.5 - 14.0).max(60.0);
        let inner = outer * 0.2;
        let ring = (outer - inner) / LEVELS as f32;
        let mut b = Builder {
            tree,
            key,
            center,
            inner,
            ring,
            segs: Vec::new(),
            mesh: Mesh::default(),
            labels: Vec::new(),
        };
        util::add_arc(&mut b.mesh, center, 0.0, inner - 2.0, 0.0, TAU, Color32::from_gray(44), Color32::from_gray(34));
        b.rec(key.root, 0.0, TAU, 0, util::branch_color(0));
        Sunburst { key, center, inner_r: inner, segs: b.segs, mesh: b.mesh, labels: b.labels }
    }

    pub fn hit(&self, p: Pos2) -> Option<&Seg> {
        let d = p - self.center;
        let r = d.length();
        let a = d.x.atan2(-d.y).rem_euclid(TAU);
        self.segs.iter().find(|s| r >= s.r0 && r < s.r1 && a >= s.a0 && a < s.a1)
    }

    pub fn seg_of(&self, node: u32) -> Option<&Seg> {
        self.segs.iter().find(|s| s.node == node)
    }
}
