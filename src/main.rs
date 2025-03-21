use std::fs::File;
use std::io::BufReader;
use eframe::egui::{Slider, Sense, Label, Color32, FontId, Align2, Stroke, Frame, pos2};
use std::time::{Duration, Instant};
use std::path::Path;
use std::sync::{Arc, Mutex};
use eframe::egui::{self, CentralPanel, Context, SidePanel, Button};
use rfd::FileDialog;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sample, Sink, Source};
use walkdir::WalkDir;
use hound;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;
use symphonia::default::{get_codecs, get_probe};

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
    waveform: Vec<f32>,
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

                Frame::canvas(ui.style()).fill(Color32::BLACK).show(ui, |ui| {
                    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());

                    let rect = response.rect;
                    let mid_y = rect.center().y;
                    let waveform_width = rect.width();
                    let waveform_height = rect.height() / 2.0;

                    if !self.waveform.is_empty() {
                        let step = waveform_width / self.waveform.len() as f32;
                        for (i, sample) in self.waveform.iter().enumerate() {
                            let x = rect.left_top().x + step * i as f32;
                            painter.line_segment(
                                [
                                    pos2(x, mid_y - sample * waveform_height),
                                    pos2(x, mid_y + sample * waveform_height)
                                ],
                                Stroke::new(1.0, Color32::LIGHT_GREEN)
                            );
                        }
                    } else {
                        painter.text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "No waveform available",
                            FontId::default(),
                            Color32::GRAY
                        );
                    }
                });

            });

            ctx.request_repaint();
        });
    }
}

impl MyApp {

    fn load_waveform(&mut self, file_path: &str) {
        // Reset waveform data
        self.waveform.clear();

        let file = match File::open(file_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to open file '{}': {:?}", file_path, e);
                return;
            }
        };

        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let hint = Hint::new(); // Empty hint, detection auto
        let format_opts = FormatOptions::default();
        let probe = match get_probe().format(&hint, mss, &format_opts, &Default::default()) {
            Ok(p) => p,
            Err(err) => {
                eprintln!("Symphonia probe error: {:?}", err);
                return;
            }
        };

        let mut reader = probe.format;
        let track = match reader.default_track() {
            Some(track) => track,
            None => {
                eprintln!("No default audio track found");
                return;
            }
        };

        let decoder_opts = DecoderOptions::default();
        let mut decoder = match get_codecs().make(&track.codec_params, &decoder_opts) {
            Ok(dec) => dec,
            Err(err) => {
                eprintln!("Failed to make decoder: {:?}", err);
                return;
            }
        };

        const DOWN_SAMPLE_RESOLUTION: usize = 500; // adjustable resolution
        let mut samples = Vec::<f32>::new();

        loop {
            match reader.next_packet() {
                Ok(packet) => {
                    match decoder.decode(&packet) {
                        Ok(decoded) => match decoded {
                            AudioBufferRef::F32(buf) => samples.extend(buf.chan(0)),
                            AudioBufferRef::S16(buf) => {
                                samples.extend(buf.chan(0).iter().map(|&s| s.to_f32()))
                            },
                            AudioBufferRef::U8(buf) => {
                                samples.extend(buf.chan(0).iter().map(|&s| (s as f32 - 128.0) / 128.0))
                            },
                            _ => {},
                        },
                        Err(err) => {
                            eprintln!("Error decoding audio packet: {:?}", err);
                            continue;
                        }
                    }
                },
                Err(err) => {
                    use symphonia::core::errors::Error::*;
                    match err {
                        ResetRequired => continue,
                        IoError(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => break,
                        _ => {
                            eprintln!("Error reading packet: {:?}", err);
                            break;
                        }
                    }
                },
            }
        }

        if !samples.is_empty() {
            // Down-sample waveform data
            let chunk_size = samples.len().max(1) / DOWN_SAMPLE_RESOLUTION;
            self.waveform = samples
                .chunks(chunk_size)
                .map(|chunk| {
                    chunk.iter().copied().sum::<f32>() / chunk.len() as f32
                })
                .collect();

            // Normalize waveform amplitudes
            if let Some(max_amp) = self.waveform.iter().map(|v| v.abs()).fold(None, |max, val| {
                Some(if let Some(max) = max { val.max(max) } else { val })
            }) {
                if max_amp > 0.0 {
                    self.waveform.iter_mut().for_each(|v| *v /= max_amp);
                }
            }
        } else {
            eprintln!("No audio samples were decoded from file");
        }
    }

    fn play_file(&mut self, file_path: String) {
        self.stop_audio();
        self.load_waveform(&file_path); // generates waveform via Symphonia

        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();

        let file = File::open(&file_path).unwrap();
        let decoder = Decoder::new(BufReader::new(file)).unwrap();
        self.total_duration = decoder.total_duration().unwrap_or_default();

        sink.append(decoder);
        sink.play();

        self._stream = Some(_stream);
        self.stream_handle = Some(stream_handle);
        self.sink = Some(Arc::new(Mutex::new(sink)));

        self.current_file = Some(file_path);
        self.start_time = Some(Instant::now());
        self.pause_duration = Duration::default();
    }

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