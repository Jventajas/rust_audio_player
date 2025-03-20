use std::path::Path;
use eframe::egui::{self, CentralPanel, Context, SidePanel, Button};
use rfd::FileDialog;
use walkdir::WalkDir;

#[derive(Default)]
struct MyApp {
    audio_files: Vec<String>,
    directory: Option<String>,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        SidePanel::left("left_panel").show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                if self.directory.is_none() {
                    if ui.add(Button::new("Choose Directory")).clicked() {
                        if let Some(folder) = FileDialog::new()
                            .set_directory("/")
                            .pick_folder()
                        {
                            self.directory = Some(folder.display().to_string());
                            self.scan_audio_files();
                        }
                    }
                } else {
                    ui.label(format!("Directory: {}", self.directory.as_ref().unwrap()));
                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for file in &self.audio_files {
                            let file_name = Path::new(&file)
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Unknown or corrupted file.");
                            if ui.button(file_name).clicked() {
                                // Here, you could later implement playing audio functionality.
                            }
                        }
                    });

                    if ui.button("Change Directory").clicked() {
                        if let Some(folder) = FileDialog::new()
                            .set_directory("/")
                            .pick_folder()
                        {
                            self.directory = Some(folder.display().to_string());
                            self.scan_audio_files();
                        }
                    }
                }
            });
        });

        CentralPanel::default().show(ctx, |ui| {
            ui.heading("Audio Player Placeholder");
        });
    }
}

impl MyApp {
    fn scan_audio_files(&mut self) {
        self.audio_files.clear();
        if let Some(ref dir) = self.directory {
            for entry in WalkDir::new(dir).min_depth(1).max_depth(3) {
                if let Ok(entry) = entry {
                    if entry.file_type().is_file() {
                        let path = entry.path().display().to_string();
                        if path.ends_with(".mp3")
                            || path.ends_with(".wav")
                            || path.ends_with(".flac")
                            || path.ends_with(".m4a")
                            || path.ends_with(".ogg")
                        {
                            self.audio_files.push(path);
                        }
                    }
                }
            }
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Audio Player",
        options,
        Box::new(|_cc| Ok(Box::<MyApp>::default())),
    )
}