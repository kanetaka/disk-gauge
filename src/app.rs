use crate::drives::{self, DriveInfo};
use crate::platform;
use crate::scan::{self, ScanJob};
use crate::sunburst::Sunburst;
use crate::tree::{Analysis, Tree, NONE};
use crate::treemap::{rect_key, ColorMode, Label, Treemap, VizKey};
use crate::util::{self, fmt_count, fmt_pct, fmt_size, ACCENT, ACCENT_DIM, BG_PANEL, BG_TOP, CARD, DIM, SEL, WARN};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, FontId, Id, LayerId, Layout, Mesh, Order, Painter, Rect, RichText,
    Sense, Shape, Stroke, Ui,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
enum ViewMode {
    Treemap,
    Sunburst,
}

#[derive(Clone, Copy, PartialEq)]
enum RightTab {
    Files,
    Exts,
}

enum Action {
    Scan(PathBuf),
    Select(u32),
    Toggle(u32),
    Zoom(u32),
    /// Show the folder containing a file and select the file.
    Locate(u32),
    Up,
    Reveal(u32),
    OpenDir(u32),
    CopyPath(u32),
    Highlight(u16),
    CollapseAll,
}

pub struct App {
    drives: Vec<DriveInfo>,
    path_input: String,
    scan: Option<ScanJob>,
    tree: Option<Tree>,
    generation: u64,
    view_root: u32,
    selected: u32,
    hover: u32,
    menu_node: u32,
    expanded: HashSet<u32>,
    scroll_to_selected: bool,
    view_mode: ViewMode,
    color_mode: ColorMode,
    highlight_ext: Option<u16>,
    right_tab: RightTab,
    analysis: Option<Analysis>,
    treemap: Option<Treemap>,
    sunburst: Option<Sunburst>,
}

const TREE_CHILD_LIMIT: usize = 400;

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(bytes) = platform::japanese_font() {
        fonts.font_data.insert("jp".to_owned(), egui::FontData::from_owned(bytes));
        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "jp".to_owned());
        fonts.families.entry(egui::FontFamily::Monospace).or_default().push("jp".to_owned());
    }
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG_PANEL;
    v.window_fill = Color32::from_rgb(30, 33, 40);
    v.extreme_bg_color = Color32::from_rgb(15, 16, 20);
    v.faint_bg_color = Color32::from_rgb(30, 33, 40);
    v.selection.bg_fill = ACCENT_DIM;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_gray(48));
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(40, 44, 54);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(52, 58, 72);
    ctx.set_visuals(v);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = vec2(8.0, 6.0);
    style.spacing.button_padding = vec2(10.0, 4.0);
    ctx.set_style(style);
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx);
        let mut app = Self {
            drives: drives::list(),
            path_input: String::new(),
            scan: None,
            tree: None,
            generation: 0,
            view_root: 0,
            selected: NONE,
            hover: NONE,
            menu_node: NONE,
            expanded: HashSet::new(),
            scroll_to_selected: false,
            view_mode: ViewMode::Treemap,
            color_mode: ColorMode::Ext,
            highlight_ext: None,
            right_tab: RightTab::Files,
            analysis: None,
            treemap: None,
            sunburst: None,
        };
        if let Some(arg) = std::env::args_os().nth(1) {
            app.start_scan(PathBuf::from(arg), &cc.egui_ctx);
        }
        app
    }

    fn start_scan(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.scan.is_some() {
            return;
        }
        let path = platform::normalize_input(&path.to_string_lossy());
        if !path.is_dir() {
            return;
        }
        self.drives = drives::list();
        let expected = drives::drive_root_of(&self.drives, &path).map(|d| d.used());
        self.path_input = path.display().to_string();
        if let Some(old) = self.tree.take() {
            // Freeing millions of nodes can take a moment; do it off the UI thread.
            std::thread::spawn(move || drop(old));
        }
        self.treemap = None;
        self.sunburst = None;
        self.analysis = None;
        self.scan = Some(scan::start(path, expected, ctx.clone()));
    }

    fn poll_scan(&mut self) {
        let done = self.scan.as_ref().is_some_and(|j| j.progress.done.load(Relaxed));
        if !done {
            return;
        }
        let job = self.scan.take().unwrap();
        let tree = job.result.lock().unwrap().take();
        if let Some(t) = tree {
            self.tree = Some(t);
            self.generation += 1;
            self.view_root = 0;
            self.selected = NONE;
            self.highlight_ext = None;
            self.expanded.clear();
            self.expanded.insert(0);
            self.drives = drives::list();
        }
    }

    fn select(&mut self, id: u32) {
        let Some(tree) = &self.tree else { return };
        for a in tree.ancestors(id) {
            if a != id {
                self.expanded.insert(a);
            }
        }
        self.selected = id;
        self.scroll_to_selected = true;
    }

    fn zoom(&mut self, id: u32) {
        let Some(tree) = &self.tree else { return };
        let n = tree.node(id);
        let id = if n.is_dir { id } else { n.parent };
        if id == NONE {
            return;
        }
        for a in tree.ancestors(id) {
            self.expanded.insert(a);
        }
        self.view_root = id;
    }

    fn apply(&mut self, a: Action, ctx: &egui::Context) {
        match a {
            Action::Scan(p) => self.start_scan(p, ctx),
            Action::Select(id) => self.select(id),
            Action::Toggle(id) => {
                if !self.expanded.remove(&id) {
                    self.expanded.insert(id);
                }
            }
            Action::Zoom(id) => self.zoom(id),
            Action::Locate(id) => {
                self.zoom(id);
                self.select(id);
            }
            Action::Up => {
                if let Some(tree) = &self.tree {
                    if self.view_root != 0 {
                        let old = self.view_root;
                        self.view_root = tree.node(old).parent;
                        self.select(old);
                    }
                }
            }
            Action::Reveal(id) => {
                if let Some(t) = &self.tree {
                    platform::reveal(&t.path(id));
                }
            }
            Action::OpenDir(id) => {
                if let Some(t) = &self.tree {
                    platform::open_dir(&t.path(id));
                }
            }
            Action::CopyPath(id) => {
                if let Some(t) = &self.tree {
                    let s = t.path(id).display().to_string();
                    ctx.output_mut(|o| o.copied_text = s);
                }
            }
            Action::Highlight(e) => {
                self.highlight_ext = if self.highlight_ext == Some(e) { None } else { Some(e) };
            }
            Action::CollapseAll => {
                self.expanded.clear();
                self.expanded.insert(0);
            }
        }
    }

    fn ensure_analysis(&mut self) {
        if let Some(tree) = &self.tree {
            let stale = self.analysis.as_ref().map_or(true, |a| a.root != self.view_root || a.generation != self.generation);
            if stale {
                self.analysis = Some(tree.analyze(self.view_root, self.generation, 300));
            }
        }
    }

    // ------------------------------------------------------------------ top bar

    fn ui_top(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        egui::TopBottomPanel::top("top")
            .frame(egui::Frame::none().fill(BG_TOP).inner_margin(egui::Margin::symmetric(12.0, 8.0)))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("DiskGauge").size(20.0).strong().color(ACCENT));
                    ui.add_space(6.0);
                    let scanning = self.scan.is_some();
                    for d in &self.drives {
                        if drive_chip(ui, d, !scanning).clicked() && !scanning {
                            actions.push(Action::Scan(PathBuf::from(&d.root)));
                        }
                    }
                    ui.add_space(6.0);
                    ui.add_enabled_ui(!scanning, |ui| {
                        let te = ui.add(
                            egui::TextEdit::singleline(&mut self.path_input)
                                .hint_text("解析するフォルダのパス")
                                .desired_width(260.0),
                        );
                        let enter = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.button("参照…").clicked() {
                            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                                actions.push(Action::Scan(p));
                            }
                        }
                        let go = ui.add(egui::Button::new(RichText::new("解析").strong().color(Color32::WHITE)).fill(ACCENT_DIM));
                        if (go.clicked() || enter) && !self.path_input.trim().is_empty() {
                            actions.push(Action::Scan(PathBuf::from(self.path_input.trim())));
                        }
                    });
                });
            });
    }

    fn ui_status(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::none().fill(BG_TOP).inner_margin(egui::Margin::symmetric(12.0, 4.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(t) = &self.tree {
                        let r = t.node(0);
                        ui.label(RichText::new(t.root_path.display().to_string()).strong());
                        ui.separator();
                        ui.label(format!("{} ファイル", fmt_count(r.files as u64)));
                        ui.separator();
                        ui.label(format!("{} フォルダ", fmt_count(r.dirs as u64)));
                        ui.separator();
                        ui.label(fmt_size(r.size));
                        ui.separator();
                        ui.label(format!("解析時間 {:.1} 秒", t.scan_secs));
                        if t.errors > 0 {
                            ui.separator();
                            ui.label(RichText::new(format!("アクセス不可 {}", fmt_count(t.errors))).color(WARN))
                                .on_hover_text(platform::ACCESS_HINT);
                        }
                        if t.cancelled {
                            ui.separator();
                            ui.label(RichText::new("中断された結果です").color(WARN));
                        }
                    } else if self.scan.is_some() {
                        ui.label("解析中…");
                    } else {
                        ui.label(RichText::new("ドライブまたはフォルダを選択してください").color(DIM));
                    }
                });
            });
    }

    // ------------------------------------------------------------------ welcome / scanning

    fn ui_welcome(&self, ui: &mut Ui, actions: &mut Vec<Action>) {
        ui.add_space(36.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("ストレージを解析").size(28.0).strong());
            ui.add_space(4.0);
            ui.label(RichText::new("ドライブを選ぶと、容量を圧迫しているフォルダやファイルを可視化します").color(DIM));
        });
        ui.add_space(28.0);
        let card = vec2(300.0, 124.0);
        let gap = 16.0;
        let avail = ui.available_width();
        let cols = (((avail + gap) / (card.x + gap)).floor() as usize).max(1);
        let total_w = cols as f32 * (card.x + gap) - gap;
        let pad = ((avail - total_w) * 0.5).max(0.0);
        for chunk in self.drives.chunks(cols) {
            ui.horizontal(|ui| {
                ui.add_space(pad);
                ui.spacing_mut().item_spacing.x = gap;
                for d in chunk {
                    if drive_card(ui, d, card).clicked() {
                        actions.push(Action::Scan(PathBuf::from(&d.root)));
                    }
                }
            });
            ui.add_space(gap);
        }
        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("ヒント: 上部の「参照…」から任意のフォルダだけを解析することもできます").color(DIM).size(12.0));
        });
    }

    fn ui_scanning(&self, ui: &mut Ui) {
        let Some(job) = &self.scan else { return };
        let p = &job.progress;
        let files = p.files.load(Relaxed);
        let dirs = p.dirs.load(Relaxed);
        let bytes = p.bytes.load(Relaxed);
        let errors = p.errors.load(Relaxed);
        let building = p.building.load(Relaxed);
        let cancelling = p.cancel.load(Relaxed);
        let elapsed = job.started.elapsed().as_secs_f64().max(0.001);

        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.16).max(20.0));
            egui::Frame::none().fill(CARD).rounding(14.0).inner_margin(28.0).show(ui, |ui| {
                ui.set_width(580.0);
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(24.0).color(ACCENT));
                    let title = if building {
                        "解析結果を構築中…"
                    } else if cancelling {
                        "中止しています…"
                    } else {
                        "解析中…"
                    };
                    ui.label(RichText::new(title).size(22.0).strong());
                });
                ui.label(RichText::new(job.path.display().to_string()).color(DIM));
                ui.add_space(14.0);

                if let Some(exp) = job.expected.filter(|&e| e > 0) {
                    let f = (bytes as f32 / exp as f32).min(1.0);
                    let rate = bytes as f64 / elapsed;
                    let remain = if rate > 0.0 && f < 1.0 { (exp.saturating_sub(bytes)) as f64 / rate } else { 0.0 };
                    ui.add(
                        egui::ProgressBar::new(f)
                            .fill(ACCENT)
                            .text(format!("{:.0}%   {} / {}   残り約 {:.0} 秒", f * 100.0, fmt_size(bytes), fmt_size(exp), remain)),
                    );
                    ui.add_space(14.0);
                }

                egui::Grid::new("scan_stats").num_columns(4).spacing([28.0, 10.0]).show(ui, |ui| {
                    stat(ui, "ファイル", fmt_count(files));
                    stat(ui, "フォルダ", fmt_count(dirs));
                    ui.end_row();
                    stat(ui, "サイズ", fmt_size(bytes));
                    stat(ui, "経過時間", format!("{:.1} 秒", elapsed));
                    ui.end_row();
                    stat(ui, "速度", format!("{} 件/秒", fmt_count((files as f64 / elapsed) as u64)));
                    stat(ui, "アクセス不可", fmt_count(errors));
                    ui.end_row();
                });
                ui.add_space(12.0);
                let cur = p.current.lock().map(|g| g.clone()).unwrap_or_default();
                ui.label(RichText::new(util::truncate_start(&cur, 72)).size(11.5).color(Color32::from_gray(120)));
                ui.add_space(14.0);
                if ui.add_enabled(!cancelling && !building, egui::Button::new("中止（途中結果を表示）")).clicked() {
                    p.cancel.store(true, Relaxed);
                }
            });
        });
    }

    // ------------------------------------------------------------------ folder tree (left)

    fn ui_tree(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("フォルダツリー").strong().size(15.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("すべて閉じる").clicked() {
                    actions.push(Action::CollapseAll);
                }
            });
        });
        ui.separator();
        let Some(tree) = &self.tree else { return };
        let st = TreeState {
            tree,
            expanded: &self.expanded,
            selected: self.selected,
            view_root: self.view_root,
            hover: self.hover,
        };
        let mut scroll = self.scroll_to_selected;
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            tree_rows(ui, &st, 0, 0, &mut scroll, actions);
        });
        self.scroll_to_selected = false;
    }

    // ------------------------------------------------------------------ right panel

    fn ui_right(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let Some(tree) = &self.tree else { return };
        let vr = self.view_root;
        let n = tree.node(vr);
        let total = tree.node(0).size;
        ui.add_space(8.0);
        egui::Frame::none().fill(CARD).rounding(10.0).inner_margin(14.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(tree.display_name(vr)).size(15.0).strong());
            ui.label(RichText::new(fmt_size(n.size)).size(30.0).strong().color(ACCENT));
            if vr != 0 {
                ui.label(RichText::new(format!("解析対象全体の {}", fmt_pct(n.size, total))).color(DIM));
            }
            ui.label(format!("{} ファイル ・ {} フォルダ", fmt_count(n.files as u64), fmt_count(n.dirs as u64)));
            if vr == 0 {
                if let Some(d) = drives::drive_root_of(&self.drives, &tree.root_path) {
                    ui.add_space(6.0);
                    usage_bar(ui, d, n.size);
                }
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.right_tab, RightTab::Files, RichText::new("大きいファイル TOP300").strong());
            ui.selectable_value(&mut self.right_tab, RightTab::Exts, RichText::new("拡張子別").strong());
        });
        ui.separator();
        let Some(an) = &self.analysis else { return };
        match self.right_tab {
            RightTab::Files => {
                let max = an.top_files.first().map(|&id| tree.node(id).size).unwrap_or(1).max(1);
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show_rows(ui, 36.0, an.top_files.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for i in range {
                        let id = an.top_files[i];
                        file_row(ui, tree, i, id, max, self.selected == id, actions);
                    }
                });
            }
            RightTab::Exts => {
                ext_stack_bar(ui, tree, an, n.size);
                ui.add_space(6.0);
                let hl = self.highlight_ext;
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show_rows(ui, 28.0, an.exts.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for i in range {
                        ext_row(ui, tree, an.exts[i], n.size, hl, actions);
                    }
                });
            }
        }
    }

    // ------------------------------------------------------------------ visualization (center)

    fn ui_viz(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let Some(tree) = &self.tree else { return };
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view_mode, ViewMode::Treemap, RichText::new("ツリーマップ").strong());
            ui.selectable_value(&mut self.view_mode, ViewMode::Sunburst, RichText::new("サンバースト").strong());
            ui.separator();
            ui.label(RichText::new("色分け").color(DIM));
            ui.selectable_value(&mut self.color_mode, ColorMode::Ext, "拡張子");
            ui.selectable_value(&mut self.color_mode, ColorMode::Folder, "フォルダ");
            if let Some(e) = self.highlight_ext {
                ui.separator();
                let info = &tree.exts[e as usize];
                if ui
                    .add(egui::Button::new(RichText::new(format!("強調中: {}  ×", info.label())).color(Color32::BLACK)).fill(info.color))
                    .on_hover_text("クリックで強調を解除（Esc）")
                    .clicked()
                {
                    actions.push(Action::Highlight(e));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if ui.add_enabled(self.view_root != 0, egui::Button::new("↑ 上へ")).on_hover_text("Backspace").clicked() {
                actions.push(Action::Up);
            }
            ui.add_space(6.0);
            for (i, id) in tree.ancestors(self.view_root).into_iter().enumerate() {
                if i > 0 {
                    ui.label(RichText::new("›").color(DIM));
                }
                let last = id == self.view_root;
                let text = RichText::new(tree.display_name(id)).color(if last { Color32::WHITE } else { Color32::from_gray(175) });
                if ui.add(egui::Button::new(if last { text.strong() } else { text }).frame(false)).clicked() {
                    actions.push(Action::Zoom(id));
                }
            }
            ui.add_space(8.0);
            ui.label(RichText::new(fmt_size(tree.node(self.view_root).size)).strong().color(ACCENT));
        });

        let avail = ui.available_size();
        let size = vec2(avail.x, (avail.y - 28.0).max(120.0));
        self.hover = NONE;
        match self.view_mode {
            ViewMode::Treemap => self.ui_treemap(ui, size, actions),
            ViewMode::Sunburst => self.ui_sunburst(ui, size, actions),
        }

        let Some(tree) = &self.tree else { return };
        ui.horizontal(|ui| {
            if self.hover != NONE {
                let n = tree.node(self.hover);
                ui.label(RichText::new(fmt_size(n.size)).strong().color(ACCENT));
                ui.label(RichText::new(tree.path(self.hover).display().to_string()).color(Color32::from_gray(200)));
            } else {
                ui.label(
                    RichText::new("クリック: 選択   ダブルクリック: フォルダに入る   右クリック: メニュー   Backspace: 上の階層へ")
                        .color(DIM)
                        .size(12.0),
                );
            }
        });
    }

    fn ui_treemap(&mut self, ui: &mut Ui, size: egui::Vec2, actions: &mut Vec<Action>) {
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let Some(tree) = &self.tree else { return };
        let key = VizKey { generation: self.generation, root: self.view_root, rect: rect_key(rect), mode: self.color_mode, hl: self.highlight_ext };
        if self.treemap.as_ref().map_or(true, |t| t.key != key) {
            self.treemap = Some(Treemap::build(tree, key, rect));
        }
        let tm = self.treemap.as_ref().unwrap();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(16));
        painter.add(Shape::mesh(tm.mesh.clone()));
        for l in &tm.labels {
            draw_label(&painter, l);
        }

        // Selection outline (nearest visible ancestor if the item itself is too small).
        let mut cur = self.selected;
        while cur != NONE && cur != self.view_root {
            if let Some(r) = tm.rect_of(cur) {
                painter.rect_stroke(r.shrink(1.0), 2.0, Stroke::new(2.5_f32, SEL));
                break;
            }
            cur = tree.node(cur).parent;
        }

        let hovered = resp.hover_pos().and_then(|p| tm.hit(p)).map(|it| (it.node, it.rect));
        if let Some((node, r)) = hovered {
            let top = tree.top_child(self.view_root, node);
            if top != node {
                if let Some(tr) = tm.rect_of(top) {
                    painter.rect_stroke(tr, 0.0, Stroke::new(1.5_f32, Color32::from_white_alpha(110)));
                }
            }
            painter.rect_stroke(r, 0.0, Stroke::new(1.5_f32, Color32::WHITE));
            self.hover = node;
            if resp.clicked() {
                actions.push(Action::Select(node));
            }
            if resp.double_clicked() && top != self.view_root && tree.node(top).is_dir {
                actions.push(Action::Zoom(top));
            }
            if resp.secondary_clicked() {
                self.menu_node = node;
            }
            show_tip(ui.ctx(), tree, node, self.view_root);
        }
        let menu_node = self.menu_node;
        if menu_node != NONE {
            resp.context_menu(|ui| node_menu(ui, tree, menu_node, actions));
        }
    }

    fn ui_sunburst(&mut self, ui: &mut Ui, size: egui::Vec2, actions: &mut Vec<Action>) {
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let Some(tree) = &self.tree else { return };
        let key = VizKey { generation: self.generation, root: self.view_root, rect: rect_key(rect), mode: self.color_mode, hl: self.highlight_ext };
        if self.sunburst.as_ref().map_or(true, |s| s.key != key) {
            self.sunburst = Some(Sunburst::build(tree, key, rect));
        }
        let sb = self.sunburst.as_ref().unwrap();
        let painter = ui.painter_at(rect);
        painter.add(Shape::mesh(sb.mesh.clone()));

        let overlay = |painter: &Painter, s: &crate::sunburst::Seg, c: Color32| {
            let mut m = Mesh::default();
            util::add_arc(&mut m, sb.center, s.r0 + 0.8, s.r1 - 0.8, s.a0, s.a1, c, c);
            painter.add(Shape::mesh(m));
        };
        if self.selected != NONE {
            if let Some(s) = sb.seg_of(self.selected) {
                overlay(&painter, s, util::with_alpha(SEL, 120));
            }
        }

        for (p, text) in &sb.labels {
            let t = util::truncate_end(text, 16);
            painter.text(*p + vec2(1.0, 1.0), Align2::CENTER_CENTER, &t, FontId::proportional(12.0), Color32::from_black_alpha(170));
            painter.text(*p, Align2::CENTER_CENTER, &t, FontId::proportional(12.0), Color32::WHITE);
        }

        // Center disc: current folder; click to go up.
        let root = tree.node(self.view_root);
        let c = sb.center;
        painter.text(c - vec2(0.0, 12.0), Align2::CENTER_CENTER, util::truncate_start(&tree.display_name(self.view_root), 16), FontId::proportional(13.0), Color32::from_gray(210));
        painter.text(c + vec2(0.0, 9.0), Align2::CENTER_CENTER, fmt_size(root.size), FontId::proportional(18.0), ACCENT);

        if let Some(p) = resp.hover_pos() {
            if let Some(s) = sb.hit(p) {
                overlay(&painter, s, Color32::from_white_alpha(60));
                let node = s.node;
                self.hover = node;
                if resp.clicked() {
                    actions.push(Action::Select(node));
                }
                if resp.double_clicked() {
                    actions.push(if tree.node(node).is_dir { Action::Zoom(node) } else { Action::Locate(node) });
                }
                if resp.secondary_clicked() {
                    self.menu_node = node;
                }
                show_tip(ui.ctx(), tree, node, self.view_root);
            } else if (p - c).length() < sb.inner_r && self.view_root != 0 {
                painter.circle_stroke(c, sb.inner_r - 2.0, Stroke::new(1.5_f32, Color32::from_white_alpha(120)));
                painter.text(c + vec2(0.0, 28.0), Align2::CENTER_CENTER, "クリックで上へ", FontId::proportional(11.0), DIM);
                if resp.clicked() {
                    actions.push(Action::Up);
                }
            }
        }
        let menu_node = self.menu_node;
        if menu_node != NONE {
            resp.context_menu(|ui| node_menu(ui, tree, menu_node, actions));
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_scan();
        let mut actions = Vec::new();
        self.ui_top(ctx, &mut actions);
        self.ui_status(ctx);

        if self.scan.is_some() {
            egui::CentralPanel::default().show(ctx, |ui| self.ui_scanning(ui));
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if self.tree.is_some() {
            self.ensure_analysis();
            egui::SidePanel::left("tree_panel")
                .resizable(true)
                .default_width(400.0)
                .min_width(220.0)
                .show(ctx, |ui| self.ui_tree(ui, &mut actions));
            egui::SidePanel::right("right_panel")
                .resizable(true)
                .default_width(380.0)
                .min_width(260.0)
                .show(ctx, |ui| self.ui_right(ui, &mut actions));
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(BG_PANEL).inner_margin(8.0))
                .show(ctx, |ui| self.ui_viz(ui, &mut actions));

            if ctx.memory(|m| m.focused().is_none()) {
                if ctx.input(|i| i.key_pressed(egui::Key::Backspace)) {
                    actions.push(Action::Up);
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.highlight_ext = None;
                }
            }
        } else {
            egui::CentralPanel::default().show(ctx, |ui| self.ui_welcome(ui, &mut actions));
        }

        for a in actions {
            self.apply(a, ctx);
        }
    }
}

// ====================================================================== widgets

fn stat(ui: &mut Ui, label: &str, value: String) {
    ui.label(RichText::new(label).color(DIM));
    ui.label(RichText::new(value).strong().size(17.0));
}

fn node_menu(ui: &mut Ui, tree: &Tree, id: u32, actions: &mut Vec<Action>) {
    let n = tree.node(id);
    ui.label(RichText::new(tree.display_name(id)).strong());
    ui.label(RichText::new(fmt_size(n.size)).color(ACCENT));
    ui.separator();
    if n.is_dir {
        if ui.button("このフォルダを表示").clicked() {
            actions.push(Action::Zoom(id));
            ui.close_menu();
        }
        if ui.button("フォルダを開く").clicked() {
            actions.push(Action::OpenDir(id));
            ui.close_menu();
        }
    } else if ui.button("このファイルの場所を表示").clicked() {
        actions.push(Action::Locate(id));
        ui.close_menu();
    }
    if ui.button(platform::REVEAL_LABEL).clicked() {
        actions.push(Action::Reveal(id));
        ui.close_menu();
    }
    if ui.button("フルパスをコピー").clicked() {
        actions.push(Action::CopyPath(id));
        ui.close_menu();
    }
    if !n.is_dir && ui.button(format!("{} を強調表示", tree.exts[n.ext as usize].label())).clicked() {
        actions.push(Action::Highlight(n.ext));
        ui.close_menu();
    }
}

fn draw_label(painter: &Painter, l: &Label) {
    let clip = l.rect.intersect(painter.clip_rect());
    if !clip.is_positive() {
        return;
    }
    let p = painter.with_clip_rect(clip);
    if l.header {
        p.text(l.rect.left_center() + vec2(4.0, 0.0), Align2::LEFT_CENTER, &l.text, FontId::proportional(11.0), Color32::from_gray(215));
    } else {
        let c = l.rect.left_top() + vec2(4.0, 2.0);
        let f = FontId::proportional(12.0);
        p.text(c + vec2(1.0, 1.0), Align2::LEFT_TOP, &l.text, f.clone(), Color32::from_black_alpha(190));
        p.text(c, Align2::LEFT_TOP, &l.text, f, Color32::WHITE);
    }
}

/// Rich tooltip drawn on the tooltip layer, following the pointer.
fn show_tip(ctx: &egui::Context, tree: &Tree, id: u32, view_root: u32) {
    let Some(pos) = ctx.pointer_hover_pos() else { return };
    let n = tree.node(id);
    let mut lines: Vec<(String, f32, Color32)> = Vec::new();
    lines.push((tree.display_name(id), 14.5, Color32::WHITE));
    lines.push((
        format!("{}   （表示中の {}）", fmt_size(n.size), fmt_pct(n.size, tree.node(view_root).size)),
        13.5,
        ACCENT,
    ));
    if n.is_dir {
        lines.push((format!("{} ファイル ・ {} フォルダ", fmt_count(n.files as u64), fmt_count(n.dirs as u64)), 12.0, Color32::from_gray(190)));
    } else {
        lines.push((
            format!("{}   更新 {}", tree.exts[n.ext as usize].label(), util::fmt_time(n.mtime)),
            12.0,
            Color32::from_gray(190),
        ));
    }
    if n.parent != NONE {
        lines.push((tree.path(n.parent).display().to_string(), 11.0, Color32::from_gray(135)));
    }
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("viz_tip")));
    let galleys: Vec<_> = lines
        .into_iter()
        .map(|(t, s, c)| painter.layout(t, FontId::proportional(s), c, 480.0))
        .collect();
    let w = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max) + 24.0;
    let h = galleys.iter().map(|g| g.size().y + 3.0).sum::<f32>() + 18.0;
    let screen = ctx.screen_rect();
    let mut min = pos + vec2(18.0, 18.0);
    if min.x + w > screen.right() {
        min.x = pos.x - w - 12.0;
    }
    if min.y + h > screen.bottom() {
        min.y = pos.y - h - 12.0;
    }
    let r = Rect::from_min_size(min, vec2(w, h));
    painter.rect_filled(r.translate(vec2(0.0, 4.0)).expand(2.0), 10.0, Color32::from_black_alpha(90));
    painter.rect(r, 8.0, Color32::from_rgba_unmultiplied(28, 31, 39, 248), Stroke::new(1.0_f32, Color32::from_gray(75)));
    let col = if n.is_dir { Color32::from_rgb(0xe8, 0xb9, 0x4a) } else { tree.file_color(n) };
    painter.rect_filled(Rect::from_min_size(r.min + vec2(0.0, 8.0), vec2(3.0, h - 16.0)), 1.5, col);
    let mut y = r.top() + 9.0;
    for g in galleys {
        let gh = g.size().y;
        painter.galley(pos2(r.left() + 13.0, y), g, Color32::WHITE);
        y += gh + 3.0;
    }
}

fn drive_chip(ui: &mut Ui, d: &DriveInfo, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(158.0, 40.0), if enabled { Sense::click() } else { Sense::hover() });
    let hovered = resp.hovered() && enabled;
    let p = ui.painter();
    p.rect(
        rect,
        7.0,
        if hovered { Color32::from_rgb(44, 49, 60) } else { Color32::from_rgb(32, 35, 43) },
        Stroke::new(1.0_f32, if hovered { ACCENT } else { Color32::from_gray(52) }),
    );
    let ratio = d.ratio();
    let name = p.text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, &d.name, FontId::proportional(18.0), Color32::WHITE);
    // `C:` fits in the default column; mount point names (`home`, `Data`, …) may need more.
    let tx = (rect.left() + 44.0).max(name.right() + 8.0).min(rect.right() - 60.0);
    let clip = p.with_clip_rect(rect.shrink(3.0));
    clip.text(pos2(tx, rect.top() + 10.0), Align2::LEFT_CENTER, d.display_label(), FontId::proportional(11.0), Color32::from_gray(175));
    clip.text(pos2(tx, rect.top() + 23.0), Align2::LEFT_CENTER, format!("空き {}", fmt_size(d.free)), FontId::proportional(11.0), Color32::from_gray(215));
    let bar = Rect::from_min_size(pos2(tx, rect.bottom() - 9.0), vec2(rect.right() - tx - 9.0, 4.0));
    p.rect_filled(bar, 2.0, Color32::from_gray(60));
    p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * ratio, bar.height())), 2.0, util::usage_color(ratio));
    resp.on_hover_text(format!(
        "{} {}  ({})\n使用 {} / {}  ({:.0}%)\nクリックで解析",
        d.root,
        d.display_label(),
        d.fs,
        fmt_size(d.used()),
        fmt_size(d.total),
        ratio * 100.0
    ))
}

fn drive_card(ui: &mut Ui, d: &DriveInfo, size: egui::Vec2) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let p = ui.painter();
    p.rect(
        rect,
        12.0,
        if hovered { Color32::from_rgb(42, 47, 58) } else { CARD },
        Stroke::new(1.0_f32, if hovered { ACCENT } else { Color32::from_gray(50) }),
    );
    let ratio = d.ratio();
    let col = util::usage_color(ratio);
    let c = rect.left_center() + vec2(62.0, 0.0);
    let mut mesh = Mesh::default();
    util::add_arc(&mut mesh, c, 32.0, 44.0, 0.0, std::f32::consts::TAU, Color32::from_gray(52), Color32::from_gray(62));
    util::add_arc(&mut mesh, c, 32.0, 44.0, 0.0, std::f32::consts::TAU * ratio, util::mul(col, 0.75), col);
    p.add(Shape::mesh(mesh));
    p.text(c, Align2::CENTER_CENTER, format!("{:.0}%", ratio * 100.0), FontId::proportional(17.0), Color32::WHITE);

    let x = rect.left() + 124.0;
    let clip = p.with_clip_rect(rect.shrink(4.0));
    clip.text(pos2(x, rect.top() + 26.0), Align2::LEFT_CENTER, format!("{}  {}", d.name, d.display_label()), FontId::proportional(16.0), Color32::WHITE);
    clip.text(pos2(x, rect.top() + 48.0), Align2::LEFT_CENTER, &d.fs, FontId::proportional(11.0), DIM);
    clip.text(pos2(x, rect.top() + 72.0), Align2::LEFT_CENTER, format!("使用 {} / {}", fmt_size(d.used()), fmt_size(d.total)), FontId::proportional(12.5), Color32::from_gray(210));
    clip.text(pos2(x, rect.top() + 94.0), Align2::LEFT_CENTER, format!("空き {}", fmt_size(d.free)), FontId::proportional(12.5), col);
    resp
}

fn usage_bar(ui: &mut Ui, d: &DriveInfo, scanned: u64) {
    let used = d.used();
    let other = used.saturating_sub(scanned);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, Color32::from_gray(52));
    let total = d.total.max(1) as f32;
    let w1 = rect.width() * scanned.min(used) as f32 / total;
    let w2 = rect.width() * other as f32 / total;
    p.rect_filled(Rect::from_min_size(rect.min, vec2(w1, rect.height())), 4.0, ACCENT);
    p.rect_filled(Rect::from_min_size(rect.min + vec2(w1, 0.0), vec2(w2, rect.height())), 0.0, Color32::from_rgb(0x8a, 0x6f, 0xc0));
    legend(ui, ACCENT, &format!("解析済み {}", fmt_size(scanned)));
    legend(ui, Color32::from_rgb(0x8a, 0x6f, 0xc0), &format!("未計上 {}（システム領域・アクセス不可など）", fmt_size(other)));
    legend(ui, Color32::from_gray(52), &format!("空き {} / 全体 {}", fmt_size(d.free), fmt_size(d.total)));
}

fn legend(ui: &mut Ui, c: Color32, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
        ui.painter().rect_filled(r, 2.0, c);
        ui.label(RichText::new(text).size(12.0).color(Color32::from_gray(190)));
    });
}

// ---------------------------------------------------------------- tree rows

struct TreeState<'a> {
    tree: &'a Tree,
    expanded: &'a HashSet<u32>,
    selected: u32,
    view_root: u32,
    hover: u32,
}

fn tree_rows(ui: &mut Ui, st: &TreeState, id: u32, depth: u32, scroll: &mut bool, actions: &mut Vec<Action>) {
    tree_row(ui, st, id, depth, scroll, actions);
    let n = st.tree.node(id);
    if n.is_dir && st.expanded.contains(&id) {
        let ch = st.tree.children(id);
        for cid in ch.clone().take(TREE_CHILD_LIMIT) {
            tree_rows(ui, st, cid, depth + 1, scroll, actions);
        }
        if ch.len() > TREE_CHILD_LIMIT {
            let rest: u64 = ch.clone().skip(TREE_CHILD_LIMIT).map(|c| st.tree.node(c).size).sum();
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
            ui.painter().text(
                pos2(rect.left() + 24.0 + (depth + 1) as f32 * 14.0, rect.center().y),
                Align2::LEFT_CENTER,
                format!("… 他 {} 項目（{}）", fmt_count((ch.len() - TREE_CHILD_LIMIT) as u64), fmt_size(rest)),
                FontId::proportional(12.0),
                DIM,
            );
        }
    }
}

fn tree_row(ui: &mut Ui, st: &TreeState, id: u32, depth: u32, scroll: &mut bool, actions: &mut Vec<Action>) {
    let tree = st.tree;
    let n = tree.node(id);
    let h = 22.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width().max(200.0), h), Sense::click());
    if *scroll && id == st.selected {
        resp.scroll_to_me(Some(Align::Center));
        *scroll = false;
    }
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if id == st.selected {
            p.rect_filled(rect, 3.0, util::with_alpha(ACCENT, 70));
        } else if id == st.hover || resp.hovered() {
            p.rect_filled(rect, 3.0, Color32::from_white_alpha(12));
        }
        if id == st.view_root {
            p.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, h)), 1.0, ACCENT);
        }
        let x0 = rect.left() + 6.0 + depth as f32 * 14.0;
        let cy = rect.center().y;
        let tri_col = Color32::from_gray(150);
        if n.is_dir && n.child_count > 0 {
            let pts = if st.expanded.contains(&id) {
                vec![pos2(x0, cy - 3.0), pos2(x0 + 8.0, cy - 3.0), pos2(x0 + 4.0, cy + 3.0)]
            } else {
                vec![pos2(x0 + 2.0, cy - 4.0), pos2(x0 + 7.0, cy), pos2(x0 + 2.0, cy + 4.0)]
            };
            p.add(Shape::convex_polygon(pts, tri_col, Stroke::NONE));
        }
        // icon
        let ix = x0 + 13.0;
        if n.is_dir {
            let folder = Color32::from_rgb(0xe8, 0xb9, 0x4a);
            p.rect_filled(Rect::from_min_size(pos2(ix, cy - 5.0), vec2(6.0, 3.0)), 1.0, folder);
            p.rect_filled(Rect::from_min_size(pos2(ix, cy - 3.5), vec2(13.0, 9.0)), 1.5, folder);
        } else {
            p.rect_filled(Rect::from_min_size(pos2(ix + 2.0, cy - 5.0), vec2(10.0, 10.0)), 2.0, tree.file_color(n));
        }
        // right side: bar, percentage, size
        let parent_size = if n.parent == NONE { n.size } else { tree.node(n.parent).size };
        let frac = if parent_size == 0 { 0.0 } else { n.size as f32 / parent_size as f32 };
        let size_x = rect.right() - 6.0;
        p.text(pos2(size_x, cy), Align2::RIGHT_CENTER, fmt_size(n.size), FontId::proportional(12.0), Color32::from_gray(225));
        let pct_x = size_x - 62.0;
        p.text(pos2(pct_x, cy), Align2::RIGHT_CENTER, format!("{:.1}%", frac * 100.0), FontId::proportional(11.0), DIM);
        let bar = Rect::from_min_size(pos2(pct_x - 82.0, cy - 4.0), vec2(38.0, 8.0));
        p.rect_filled(bar, 2.0, Color32::from_gray(45));
        let bar_col = if n.is_dir { util::mix(ACCENT, Color32::from_rgb(0xf2, 0x6b, 0x5b), frac) } else { tree.file_color(n) };
        p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * frac, bar.height())), 2.0, bar_col);
        // name
        let name_rect = Rect::from_min_max(pos2(ix + 19.0, rect.top()), pos2(bar.left() - 6.0, rect.bottom()));
        if name_rect.is_positive() {
            let name_col = if n.is_dir { Color32::from_gray(235) } else { Color32::from_gray(195) };
            p.with_clip_rect(name_rect).text(name_rect.left_center(), Align2::LEFT_CENTER, tree.display_name(id), FontId::proportional(13.0), name_col);
        }
    }

    if resp.clicked() {
        let on_triangle = resp.interact_pointer_pos().is_some_and(|pp| pp.x < rect.left() + 18.0 + depth as f32 * 14.0);
        if on_triangle && n.is_dir {
            actions.push(Action::Toggle(id));
        } else {
            actions.push(Action::Select(id));
            if n.is_dir && !st.expanded.contains(&id) {
                actions.push(Action::Toggle(id));
            }
        }
    }
    if resp.double_clicked() {
        actions.push(if n.is_dir { Action::Zoom(id) } else { Action::Locate(id) });
    }
    resp.context_menu(|ui| node_menu(ui, tree, id, actions));
}

// ---------------------------------------------------------------- right panel rows

fn file_row(ui: &mut Ui, tree: &Tree, rank: usize, id: u32, max: u64, selected: bool, actions: &mut Vec<Action>) {
    let n = tree.node(id);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 4.0, util::with_alpha(ACCENT, 70));
    } else if resp.hovered() {
        p.rect_filled(rect, 4.0, Color32::from_white_alpha(12));
    }
    let top = rect.top() + 11.0;
    let bottom = rect.top() + 26.0;
    p.text(pos2(rect.left() + 26.0, top), Align2::RIGHT_CENTER, format!("{}", rank + 1), FontId::proportional(11.0), DIM);
    let col = tree.file_color(n);
    p.rect_filled(Rect::from_min_size(pos2(rect.left() + 32.0, top - 5.0), vec2(10.0, 10.0)), 2.0, col);
    let right = rect.right() - 6.0;
    p.text(pos2(right, top), Align2::RIGHT_CENTER, fmt_size(n.size), FontId::proportional(12.5), Color32::WHITE);
    let bar = Rect::from_min_size(pos2(right - 72.0, bottom - 2.5), vec2(72.0, 5.0));
    p.rect_filled(bar, 2.0, Color32::from_gray(45));
    p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * n.size as f32 / max as f32, 5.0)), 2.0, col);
    let text_rect = Rect::from_min_max(pos2(rect.left() + 48.0, rect.top()), pos2(right - 80.0, rect.bottom()));
    if text_rect.is_positive() {
        let clip = p.with_clip_rect(text_rect);
        clip.text(pos2(text_rect.left(), top), Align2::LEFT_CENTER, &*n.name, FontId::proportional(13.0), Color32::from_gray(235));
        let dir = if n.parent == NONE { String::new() } else { tree.path(n.parent).display().to_string() };
        clip.text(pos2(text_rect.left(), bottom), Align2::LEFT_CENTER, dir, FontId::proportional(10.5), Color32::from_gray(125));
    }
    let resp = resp.on_hover_text(format!("{}\n{}  ・  更新 {}\nダブルクリックで場所を表示", tree.path(id).display(), fmt_size(n.size), util::fmt_time(n.mtime)));
    if resp.clicked() {
        actions.push(Action::Select(id));
    }
    if resp.double_clicked() {
        actions.push(Action::Locate(id));
    }
    resp.context_menu(|ui| node_menu(ui, tree, id, actions));
}

fn ext_stack_bar(ui: &mut Ui, tree: &Tree, an: &Analysis, total: u64) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, Color32::from_gray(52));
    let mut x = rect.left();
    let total = total.max(1) as f32;
    for &(e, size, _) in an.exts.iter().take(14) {
        let w = rect.width() * size as f32 / total;
        if w < 1.0 {
            break;
        }
        p.rect_filled(Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height())), 0.0, tree.exts[e as usize].color);
        x += w;
    }
}

fn ext_row(ui: &mut Ui, tree: &Tree, (e, size, count): (u16, u64, u64), total: u64, hl: Option<u16>, actions: &mut Vec<Action>) {
    let info = &tree.exts[e as usize];
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    let p = ui.painter();
    if hl == Some(e) {
        p.rect_filled(rect, 4.0, util::with_alpha(info.color, 60));
    } else if resp.hovered() {
        p.rect_filled(rect, 4.0, Color32::from_white_alpha(12));
    }
    let cy = rect.center().y;
    p.rect_filled(Rect::from_min_size(pos2(rect.left() + 6.0, cy - 6.0), vec2(12.0, 12.0)), 3.0, info.color);
    p.text(pos2(rect.left() + 26.0, cy), Align2::LEFT_CENTER, info.label(), FontId::proportional(13.0), Color32::from_gray(235));
    p.text(pos2(rect.left() + 140.0, cy), Align2::LEFT_CENTER, format!("{} 件", fmt_count(count)), FontId::proportional(11.0), DIM);
    let right = rect.right() - 6.0;
    p.text(pos2(right, cy), Align2::RIGHT_CENTER, fmt_size(size), FontId::proportional(12.5), Color32::WHITE);
    let frac = if total == 0 { 0.0 } else { size as f32 / total as f32 };
    let bar = Rect::from_min_max(pos2(rect.left() + 210.0, cy - 4.0), pos2(right - 110.0, cy + 4.0));
    if bar.is_positive() {
        p.rect_filled(bar, 2.0, Color32::from_gray(45));
        p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * frac, bar.height())), 2.0, info.color);
    }
    p.text(pos2(right - 70.0, cy), Align2::RIGHT_CENTER, format!("{:.1}%", frac * 100.0), FontId::proportional(11.0), DIM);
    if resp.on_hover_text("クリックでこの拡張子を強調表示").clicked() {
        actions.push(Action::Highlight(e));
    }
}
