mod app;
mod audio;
mod utils;

use app::MyApp;
use eframe::Error;
use eframe::NativeOptions;

fn main() -> Result<(), Error> {
    let options = NativeOptions::default();

    eframe::run_native(
        "Audio Player",
        options,
        Box::new(|_cc| Ok(Box::<MyApp>::default())),
    )
}