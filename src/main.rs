#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod drives;
mod platform;
mod scan;
mod sunburst;
mod tree;
mod treemap;
mod util;

use std::sync::Arc;

use eframe::egui;

/// ウィンドウ／タスクバー／Dock に表示するアプリアイコン。
/// macOS は Apple のアイコングリッドに合わせて余白を取ったものを使う。
fn app_icon() -> egui::IconData {
    #[cfg(target_os = "macos")]
    const PNG: &[u8] = include_bytes!("../assets/icon/icon-macos-512.png");
    #[cfg(not(target_os = "macos"))]
    const PNG: &[u8] = include_bytes!("../assets/icon/icon-256.png");
    eframe::icon_data::from_png_bytes(PNG).unwrap_or_default()
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DiskGauge")
            .with_app_id("diskgauge") // Linux: diskgauge.desktop と対応（Wayland のアイコン解決に使用）
            .with_icon(Arc::new(app_icon()))
            .with_inner_size([1480.0, 920.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "DiskGauge",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
