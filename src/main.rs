mod app;
mod audio;
mod utils;

use eframe::egui::ViewportBuilder;
use app::MyApp;
use eframe::Error;
use eframe::NativeOptions;

fn main() -> Result<(), Error> {
    let options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Audio Player",
        options,
        Box::new(|_cc| Ok(Box::<MyApp>::default())),
    )
}