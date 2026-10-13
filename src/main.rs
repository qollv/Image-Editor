#![windows_subsystem = "windows"]

mod app;
mod convert;
mod convert_view;
mod draw;
mod editor;
mod editor_view;
mod fonts;
mod options;
mod theme;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1080.0, 760.0])
            .with_min_inner_size([880.0, 560.0])
            // 无原生边框：标题栏由应用自绘（暗色一体，避免亮色系统标题栏割裂）
            .with_decorations(false)
            .with_icon(theme::app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "图片工作台",
        options,
        Box::new(|cc| {
            // 暗房主题：启动时一次性应用
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::ImageApp::new(cc)))
        }),
    )
}
