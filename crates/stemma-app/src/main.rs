#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let path = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("."), PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Stemma")
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Stemma",
        options,
        Box::new(move |cc| {
            // Panels take the neumorphic surface color so the soft shadows work.
            egui_neo::install(&cc.egui_ctx);
            Ok(Box::new(app::StemmaApp::open(&path, cc.egui_ctx.clone())))
        }),
    )
}
