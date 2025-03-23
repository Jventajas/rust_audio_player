use crate::audio::player::AudioPlayer;
use crate::audio::waveform::WaveformGenerator;
use crate::utils::file_scanner::AudioFileScanner;
use eframe::egui::{self, Color32, Context, CentralPanel, Pos2, ScrollArea, SidePanel, Slider, Stroke, Vec2, Layout};
use eframe::Frame;
use std::time::Duration;
use std::path::Path;

use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use std::fs::File;


pub struct MyApp {
    audio_files: Vec<String>,
    directory: Option<String>,
    player: AudioPlayer,
    waveform: WaveformGenerator,
    total_duration: Duration,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            audio_files: Vec::new(),
            directory: None,
            player: AudioPlayer::default(),
            waveform: WaveformGenerator::default(),
            total_duration: Duration::from_secs(0),
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        ctx.request_repaint_after(Duration::from_millis(30));
        self.waveform.update_buffer();

        self.render_ui(ctx);
    }
}

impl MyApp {
    fn render_ui(&mut self, ctx: &Context) {
        self.render_sidebar(ctx);
        self.render_main_panel(ctx);
    }

    fn render_sidebar(&mut self, ctx: &Context) {
        SidePanel::left("side_panel").default_width(200.0).show(ctx, |ui| {
            ui.heading("Audio Player");
            ui.separator();

            if ui.button("Select Directory").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.directory = Some(dir.display().to_string());
                    self.scan_audio_files();
                }
            }

            ui.separator();

            let mut file_to_play: Option<String> = None;

            ScrollArea::vertical().show(ui, |ui| {
                for file in &self.audio_files {
                    let file_name = Path::new(file)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();

                    let is_current = self.player.current_file()
                        .map_or(false, |current| current == file);

                    if ui.selectable_label(is_current, &file_name).clicked() {
                        file_to_play = Some(file.clone());
                    }
                }
            });

            if let Some(file) = file_to_play {
                self.play_file(&file);
            }
        });
    }

    pub fn render_main_panel(&mut self, ctx: &Context) {
        CentralPanel::default().show(ctx, |ui| {
            let available_height = ui.available_height();
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), available_height * 0.5),
                Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), available_height * 0.25),
                        Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            if ui.button("⏸ Pause").clicked() {
                                self.player.pause();
                            }
                            if ui.button("▶ Resume").clicked() {
                                self.player.resume();
                            }
                            if ui.button("⏹ Stop").clicked() {
                                self.player.stop();
                            }
                        },
                    );

                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), available_height * 0.25),
                        Layout::top_down(egui::Align::Center),
                        |ui| {
                            let progress = self.player.progress().as_secs_f32();
                            let total = self.total_duration.as_secs_f32();
                            let mut ratio = if total > 0.0 { progress / total * 100.0 } else { 0.0 };

                            ui.add(
                                Slider::new(&mut ratio, 0.0..=100.0)
                                    .text("Progress")
                                    .show_value(true),
                            );

                            ui.horizontal(|ui| {
                                ui.label(format!("{:.2} sec", progress));
                                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(format!("{:.2} sec", total));
                                });
                            });
                        },
                    );
                },
            );

            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), available_height * 0.5),
                Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    self.render_waveform(ui);
                },
            );
        });
    }


    fn render_waveform(&self, ui: &mut egui::Ui) {
        let progress_secs = self.player.progress().as_secs_f32();
        let waveform_buffer = self.waveform.get_buffer();
        let waveform_len = waveform_buffer.len();
        let sample_rate = self.waveform.get_sample_rate();

        let samples_played = (progress_secs * sample_rate as f32) as usize;
        let visible_length_samples = (sample_rate as usize) * 2; // Show 2 seconds of audio

        let start_idx = samples_played.saturating_sub(visible_length_samples / 2);
        let end_idx = (start_idx + visible_length_samples).min(waveform_len);

        let displayed_waveform = if start_idx < end_idx && waveform_len > 0 {
            &waveform_buffer[start_idx..end_idx]
        } else {
            &[] as &[f32]
        };

        let waveform_rect = ui.available_rect_before_wrap();
        let painter = ui.painter_at(waveform_rect);

        if !displayed_waveform.is_empty() {
            let wave_height = waveform_rect.height() / 2.0;
            let wave_width = waveform_rect.width() / displayed_waveform.len().max(1) as f32;
            let center_y = waveform_rect.center().y;

            let points: Vec<Pos2> = displayed_waveform
                .iter()
                .enumerate()
                .map(|(i, sample)| {
                    let x = waveform_rect.left_top().x + (i as f32 * wave_width);
                    let y = center_y - (sample * wave_height);
                    Pos2 { x, y }
                })
                .collect();

            painter.add(egui::Shape::line(
                points,
                Stroke::new(1.5, Color32::LIGHT_BLUE),
            ));
        } else {
            painter.text(
                waveform_rect.center(),
                egui::Align2::CENTER_CENTER,
                "Loading...",
                egui::FontId::default(),
                Color32::GRAY,
            );
        }

        ui.add_space(waveform_rect.height() + 10.0);
    }

    fn scan_audio_files(&mut self) {
        if let Some(dir) = &self.directory {
            self.audio_files = AudioFileScanner::scan_directory(dir, 3);
            self.audio_files.sort();
        }
    }

    fn play_file(&mut self, file_path: &str) {
        if let Err(err) = self.player.play(file_path) {
            eprintln!("Error playing file: {}", err);
            return;
        }

        self.waveform.generate_for(file_path);

        match self.get_audio_duration(file_path) {
            Ok(duration) => self.total_duration = duration,
            Err(_) => self.total_duration = Duration::from_secs(180),
        }
    }

    fn get_audio_duration(&self, file_path: &str) -> Result<Duration, Box<dyn std::error::Error>> {
        let file = File::open(file_path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(extension) = Path::new(file_path).extension() {
            hint.with_extension(&extension.to_string_lossy());
        }

        let format_opts = FormatOptions::default();
        let metadata_opts = MetadataOptions::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)?;

        if let Some(track) = probed.format.tracks().get(0) {
            if let (Some(n_frames), Some(sample_rate)) = (
                track.codec_params.n_frames,
                track.codec_params.sample_rate,
            ) {
                let seconds = n_frames as f64 / sample_rate as f64;
                return Ok(Duration::from_secs_f64(seconds));
            }
        }

        Err("Could not determine audio duration".into())
    }
}