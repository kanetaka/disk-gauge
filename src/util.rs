use eframe::egui::{pos2, Color32, Mesh, Pos2, Rect};

pub const ACCENT: Color32 = Color32::from_rgb(0x4e, 0x9a, 0xf1);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x2a, 0x5a, 0x9a);
pub const SEL: Color32 = Color32::from_rgb(0xff, 0xd5, 0x4a);
pub const WARN: Color32 = Color32::from_rgb(0xf5, 0xa6, 0x23);
pub const BG_TOP: Color32 = Color32::from_rgb(19, 21, 26);
pub const BG_PANEL: Color32 = Color32::from_rgb(25, 27, 33);
pub const CARD: Color32 = Color32::from_rgb(33, 36, 44);
pub const DIM: Color32 = Color32::from_gray(150);

/// Distinct colors for the largest extensions.
pub const PALETTE: [Color32; 16] = [
    Color32::from_rgb(0x4e, 0x9a, 0xf1),
    Color32::from_rgb(0xf2, 0x6b, 0x5b),
    Color32::from_rgb(0xf5, 0xb7, 0x2e),
    Color32::from_rgb(0x45, 0xc4, 0x86),
    Color32::from_rgb(0xa8, 0x7b, 0xf0),
    Color32::from_rgb(0x2e, 0xc4, 0xd6),
    Color32::from_rgb(0xff, 0x8a, 0x3d),
    Color32::from_rgb(0xe8, 0x6a, 0xb8),
    Color32::from_rgb(0x9c, 0xc9, 0x3f),
    Color32::from_rgb(0x6b, 0x7f, 0xf0),
    Color32::from_rgb(0x3d, 0xb3, 0xa0),
    Color32::from_rgb(0xd9, 0x5c, 0x5c),
    Color32::from_rgb(0xc9, 0xa2, 0x6b),
    Color32::from_rgb(0x8f, 0xb8, 0xde),
    Color32::from_rgb(0xb5, 0x8a, 0xc9),
    Color32::from_rgb(0x7d, 0xc9, 0x7d),
];

pub fn fmt_size(b: u64) -> String {
    const U: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if b < 1024 {
        return format!("{} B", b);
    }
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 {
        format!("{:.0} {}", v, U[i])
    } else if v >= 10.0 {
        format!("{:.1} {}", v, U[i])
    } else {
        format!("{:.2} {}", v, U[i])
    }
}

pub fn fmt_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn fmt_pct(part: u64, whole: u64) -> String {
    if whole == 0 {
        "-".into()
    } else {
        format!("{:.1}%", part as f64 * 100.0 / whole as f64)
    }
}

pub fn fmt_time(secs: i64) -> String {
    use chrono::{Local, TimeZone};
    if secs <= 0 {
        return "-".into();
    }
    match Local.timestamp_opt(secs, 0) {
        chrono::LocalResult::Single(t) => t.format("%Y/%m/%d %H:%M").to_string(),
        _ => "-".into(),
    }
}

/// Keep the end of a long path visible.
pub fn truncate_start(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    if n <= max_chars {
        s.to_string()
    } else {
        let tail: String = s.chars().skip(n - max_chars).collect();
        format!("…{}", tail)
    }
}

pub fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

pub fn mul(c: Color32, f: f32) -> Color32 {
    let m = |x: u8| (x as f32 * f).clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(m(c.r()), m(c.g()), m(c.b()), c.a())
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn lighten(c: Color32, t: f32) -> Color32 {
    mix(c, Color32::WHITE, t)
}

pub fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Color of the n-th top-level folder ("folder" color mode).
pub fn branch_color(i: usize) -> Color32 {
    let h = (i as f32 * 0.618_034 + 0.58).fract();
    hsv(h, 0.55, 0.88)
}

pub fn ext_color(rank: usize, name: &str) -> Color32 {
    if rank < PALETTE.len() {
        return PALETTE[rank];
    }
    let mut h: u32 = 2166136261;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    hsv((h % 1000) as f32 / 1000.0, 0.40, 0.72)
}

pub fn usage_color(ratio: f32) -> Color32 {
    if ratio < 0.75 {
        Color32::from_rgb(0x45, 0xc4, 0x86)
    } else if ratio < 0.9 {
        Color32::from_rgb(0xf5, 0xb7, 0x2e)
    } else {
        Color32::from_rgb(0xf2, 0x5b, 0x4b)
    }
}

// ---------------------------------------------------------------- mesh helpers

pub fn add_rect(mesh: &mut Mesh, r: Rect, c: Color32) {
    let i = mesh.vertices.len() as u32;
    mesh.colored_vertex(r.left_top(), c);
    mesh.colored_vertex(r.right_top(), c);
    mesh.colored_vertex(r.right_bottom(), c);
    mesh.colored_vertex(r.left_bottom(), c);
    mesh.add_triangle(i, i + 1, i + 2);
    mesh.add_triangle(i, i + 2, i + 3);
}

/// A rectangle shaded like a cushion: bright highlight near the upper-left,
/// darker towards the lower-right edge.
pub fn add_cushion(mesh: &mut Mesh, r: Rect, c: Color32) {
    if r.width() < 5.0 || r.height() < 5.0 {
        add_rect(mesh, r, mul(c, 0.85));
        return;
    }
    let i = mesh.vertices.len() as u32;
    mesh.colored_vertex(r.left_top(), lighten(c, 0.10));
    mesh.colored_vertex(r.right_top(), mul(c, 0.82));
    mesh.colored_vertex(r.right_bottom(), mul(c, 0.48));
    mesh.colored_vertex(r.left_bottom(), mul(c, 0.72));
    let hl = pos2(r.left() + r.width() * 0.38, r.top() + r.height() * 0.35);
    mesh.colored_vertex(hl, lighten(c, 0.32));
    for k in 0..4 {
        mesh.add_triangle(i + 4, i + k, i + (k + 1) % 4);
    }
}

/// Point on a circle; angle measured clockwise from 12 o'clock.
pub fn arc_point(c: Pos2, r: f32, a: f32) -> Pos2 {
    pos2(c.x + r * a.sin(), c.y - r * a.cos())
}

pub fn add_arc(mesh: &mut Mesh, c: Pos2, r0: f32, r1: f32, a0: f32, a1: f32, col_in: Color32, col_out: Color32) {
    let span = a1 - a0;
    if span <= 0.0 || r1 <= r0 {
        return;
    }
    let n = ((span * r1 / 4.0).ceil() as usize).clamp(1, 360);
    let base = mesh.vertices.len() as u32;
    for k in 0..=n {
        let a = a0 + span * k as f32 / n as f32;
        mesh.colored_vertex(arc_point(c, r0, a), col_in);
        mesh.colored_vertex(arc_point(c, r1, a), col_out);
    }
    for k in 0..n as u32 {
        let i = base + 2 * k;
        mesh.add_triangle(i, i + 1, i + 3);
        mesh.add_triangle(i, i + 3, i + 2);
    }
}

/// Keep the beginning of a long name visible.
pub fn truncate_end(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let head: String = s.chars().take(max_chars).collect();
        format!("{}…", head)
    }
}
