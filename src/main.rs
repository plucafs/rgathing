mod app;
mod search;
mod types;
mod ui;

use app::RgaGuiApp;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_title("rgathing — ripgrep-all GUI"),
        ..Default::default()
    };

    eframe::run_native(
        "rgathing",
        options,
        Box::new(|cc| {
            let app = RgaGuiApp::new();
            cc.egui_ctx.set_zoom_factor(app.ui_scale);
            cc.egui_ctx.set_visuals(if app.light_mode {
                egui::Visuals::light()
            } else {
                egui::Visuals::dark()
            });
            Ok(Box::new(app))
        }),
    )
}
