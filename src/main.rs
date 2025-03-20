use std::fs::File;
use eframe::egui::{Slider, Sense, Label};
use std::time::{Duration, Instant};
use std::path::Path;
use std::sync::{Arc, Mutex};
use eframe::egui::{self, CentralPanel, Context, SidePanel, Button};
use rfd::FileDialog;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use walkdir::WalkDir;

#[derive(Default)]
struct MyApp {
    audio_files: Vec<String>,
    directory: Option<String>,
    current_file: Option<String>,
    _stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Option<Arc<Mutex<Sink>>>,
    start_time: Option<Instant>,
    pause_duration: Duration,
    total_duration: Duration,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        SidePanel::left("left_panel").show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                if self.directory.is_none() {
                    if ui.add(Button::new("Choose Directory")).clicked() {
                        let default_path = dirs::home_dir().unwrap_or_else(|| std::env::current_dir().unwrap());
                        if let Some(folder) = FileDialog::new().set_directory(default_path).pick_folder() {
                            self.directory = Some(folder.display().to_string());
                            self.scan_audio_files();
                        }
                    }
                } else {
                    ui.label(format!("Directory: {}", self.directory.as_ref().unwrap()));
                    ui.separator();

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let audio_files = self.audio_files.clone();
                        for file in audio_files {
                            let file_name = Path::new(&file)
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Unknown or corrupted file");

                            let response = ui.add(Label::new(file_name).sense(Sense::click()));
                            if response.double_clicked() {
                                self.play_file(file.clone());
                            }
                        }
                    });

                    if ui.button("Change Directory").clicked() {
                        let default_path = dirs::home_dir().unwrap_or_else(|| std::env::current_dir().unwrap());
                        if let Some(folder) = FileDialog::new().set_directory(default_path).pick_folder() {
                            self.directory = Some(folder.display().to_string());
                            self.scan_audio_files();
                        }
                    }
                }
            });
        });

        CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Audio Controls");
                if let Some(ref current) = self.current_file {
                    ui.label(Path::new(current).file_name().unwrap().to_str().unwrap_or("Playing"));
                } else {
                    ui.label("No song playing");
                }

                let progress = self.progress().as_secs_f32();
                let total = self.total_duration.as_secs_f32();

                let ratio = if total > 0.0 { progress / total } else { 0.0 };
                ui.add(Slider::new(&mut (ratio * 100.0), 0.0..=100.0).text("Progress").show_value(true));
                ui.horizontal(|ui| {
                    ui.label(format!("{:.2} sec", progress));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("{:.2} sec", total));
                    });
                });

                // Control Buttons
                ui.horizontal(|ui| {
                    if ui.button("Play").clicked() && self.sink.is_some() {
                        self.resume_audio();
                    }
                    if ui.button("Pause").clicked() {
                        self.pause_audio();
                    }
                    if ui.button("Stop").clicked() {
                        self.stop_audio();
                    }
                });
            });

            ctx.request_repaint();
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

    fn play_file(&mut self, file_path: String) {
        // Stop previously playing audio if any
        self.stop_audio();

        let (_stream, stream_handle) = OutputStream::try_default().expect("Audio output error");
        self.stream_handle = Some(stream_handle.clone());
        self._stream = Some(_stream);

        let file = File::open(&file_path).expect("Failed to open audio file");
        let decoder = Decoder::new(std::io::BufReader::new(file)).expect("Failed to decode audio file");
        self.total_duration = decoder.total_duration().unwrap_or_default();

        let sink = Sink::try_new(&stream_handle).expect("Failed to create audio sink");
        sink.append(decoder);

        self.sink = Some(Arc::new(Mutex::new(sink)));
        self.start_time = Some(Instant::now());
        self.pause_duration = Duration::ZERO;
        self.current_file = Some(file_path);
    }

    fn pause_audio(&mut self) {
        if let Some(sink) = &self.sink {
            let sink_guard = sink.lock().unwrap();
            sink_guard.pause();
            if let Some(start) = self.start_time.take() {
                self.pause_duration += Instant::now() - start;
            }
        }
    }

    fn resume_audio(&mut self) {
        if let Some(sink) = &self.sink {
            let sink_guard = sink.lock().unwrap();
            sink_guard.play();
            self.start_time = Some(Instant::now());
        }
    }

    fn stop_audio(&mut self) {
        if let Some(sink) = &self.sink {
            let sink_guard = sink.lock().unwrap();
            sink_guard.stop();
        }
        self.sink = None;
        self.current_file = None;
        self.start_time = None;
        self.pause_duration = Duration::ZERO;
        self.total_duration = Duration::ZERO;
    }

    fn progress(&self) -> Duration {
        if let Some(ref sink) = self.sink {
            let sink_guard = sink.lock().unwrap();
            if sink_guard.is_paused() {
                self.pause_duration
            } else if let Some(start) = self.start_time {
                self.pause_duration + (Instant::now() - start)
            } else {
                Duration::ZERO
            }
        } else {
            Duration::ZERO
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