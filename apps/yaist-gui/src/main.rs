//! yAIst desktop client: modules on the left, the module's panel in the
//! middle, the conversation on the right.

mod app;
mod art;
mod theme;
mod widgets;

fn main() -> eframe::Result {
    let viewport = egui::ViewportBuilder::default()
        .with_title("yAIst")
        .with_app_id("org.opensuse.yaist")
        .with_inner_size([1360.0, 860.0])
        .with_min_inner_size([980.0, 600.0])
        // The header is the title bar, as in the ArtCraft launcher.
        .with_decorations(false);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "yAIst",
        options,
        Box::new(|cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            let ctx = cc.egui_ctx.clone();
            let session = yaist_ui::Session::start(move || ctx.request_repaint());
            Ok(Box::new(app::App::new(session)))
        }),
    )
}
