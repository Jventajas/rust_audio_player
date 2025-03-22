mod app;
mod audio;
mod utils;

use app::MyApp;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Audio Player",
        options,
        Box::new(|_cc| Ok(Box::<MyApp>::default())),
    )
}