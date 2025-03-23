use crate::audio::player::AudioPlayer;
use crate::audio::waveform::WaveformGenerator;
use crate::utils::file_scanner::AudioFileScanner;
use eframe::egui::{self, Color32, Context, CentralPanel, Pos2, ScrollArea, SidePanel, Slider, Stroke, Vec2, Layout, Rect};
use eframe::Frame;
use std::time::Duration;
use std::path::Path;

use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use std::fs::File;


const ACCENT_COLOR: egui::Color32 = egui::Color32::from_rgb(0x03, 0x45, 0xfc);
const LIGHTER_ACCENT_COLOR: egui::Color32 = egui::Color32::from_rgb(0x66, 0x99, 0xFF);


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

    pub fn render_main_panel(&mut self, ctx: &egui::Context) {
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
                            let progress_secs = self.player.progress().as_secs();
                            let total_secs = self.total_duration.as_secs();

                            let ratio = if total_secs > 0 {
                                (progress_secs as f32 / total_secs as f32).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };

                            // Formatting minutes and seconds
                            let progress_minutes = progress_secs / 60;
                            let progress_remaining_secs = progress_secs % 60;
                            let total_minutes = total_secs / 60;
                            let total_remaining_secs = total_secs % 60;

                            // Slightly reduced vertical height
                            let bar_height = 6.0;

                            // Padding from sides
                            let horizontal_padding = 12.0;
                            let total_bar_width = ui.available_width() - (horizontal_padding * 2.0);

                            ui.add_space(5.0); // vertical spacing above bar

                            // Allocate full width first
                            let (outer_rect, _) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), bar_height),
                                egui::Sense::hover(),
                            );

                            // Create modified rectangle applying padding at BOTH SIDES
                            let bar_rect = Rect {
                                min: outer_rect.min + Vec2::new(horizontal_padding, 0.0),
                                max: outer_rect.max - Vec2::new(horizontal_padding, 0.0),
                            };

                            // Draw background (unplayed section of bar)
                            ui.painter().rect_filled(bar_rect, 3.0, LIGHTER_ACCENT_COLOR);

                            // Draw foreground (played section of bar)
                            let played_rect = Rect {
                                min: bar_rect.min,
                                max: egui::pos2(bar_rect.min.x + bar_rect.width() * ratio, bar_rect.max.y),
                            };
                            ui.painter().rect_filled(played_rect, 3.0, ACCENT_COLOR);

                            ui.add_space(5.0); // vertical spacing below bar

                            // Time text with the SAME horizontal padding applied for alignment
                            ui.allocate_ui_with_layout(
                                Vec2::new(ui.available_width(), 20.0),
                                Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add_space(horizontal_padding);  // left padding
                                    ui.label(format!("{}:{:02}", progress_minutes, progress_remaining_secs));

                                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.add_space(horizontal_padding);  // right padding
                                        ui.label(format!("{}:{:02}", total_minutes, total_remaining_secs));
                                    });
                                },
                            );
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