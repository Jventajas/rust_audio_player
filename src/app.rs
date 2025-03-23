use crate::audio::player::AudioPlayer;
use crate::audio::waveform::WaveformGenerator;
use crate::utils::file_scanner::AudioFileScanner;
use eframe::egui::{self, Color32, Context, CentralPanel, Pos2, ScrollArea, SidePanel, Stroke, Vec2, Layout, Rect};
use eframe::Frame;
use std::time::Duration;
use std::path::Path;

use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use std::fs::File;


const ACCENT_COLOR: Color32 = Color32::from_rgb(0x03, 0x45, 0xfc);
const LIGHTER_ACCENT_COLOR: Color32 = Color32::from_rgb(0x66, 0x99, 0xFF);


pub struct AudioPlayerApp {
    audio_files: Vec<String>,
    directory: Option<String>,
    player: AudioPlayer,
    waveform: WaveformGenerator,
    total_duration: Duration,
}

impl Default for AudioPlayerApp {
    fn default() -> Self {
        let mut app = Self {
            audio_files: Vec::new(),
            directory: dirs::audio_dir().map(|p| p.to_string_lossy().to_string()),
            player: AudioPlayer::default(),
            waveform: WaveformGenerator::default(),
            total_duration: Duration::ZERO,
        };

        app.scan_audio_files(); // Scan files immediately on startup
        app
    }
}


impl eframe::App for AudioPlayerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        ctx.request_repaint_after(Duration::from_millis(30));
        self.waveform.update_buffer();

        self.render_ui(ctx);
    }
}

impl AudioPlayerApp {
    fn render_ui(&mut self, ctx: &Context) {
        self.render_sidebar(ctx);
        self.render_main_panel(ctx);
    }

    fn render_sidebar(&mut self, ctx: &Context) {
        SidePanel::left("side_panel").default_width(300.0).show(ctx, |ui| {

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.with_layout(Layout::left_to_right(egui::Align::Center).with_main_justify(true), |ui| {
                    ui.colored_label(Color32::WHITE, egui::RichText::new("Select file to play").heading());
                });
            });

            ui.add_space(15.0);

            ui.horizontal(|ui| {
                let available_width = ui.available_width();
                let button_size = egui::vec2(100.0, 20.0);

                let indent = (available_width) / 2.0 - button_size.x / 3.0;
                ui.add_space(indent);

                let button_response = ui.scope(|ui| {
                    ui.spacing_mut().button_padding = Vec2::new(14.0, 8.0);

                    ui.add(
                        egui::Button::new(
                            egui::RichText::new("Browse")
                                .color(Color32::WHITE)
                                .size(14.0),
                        )
                            .fill(ACCENT_COLOR)
                            .rounding(egui::Rounding::same(4)),
                    )

                });

                if button_response.inner.clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.directory = Some(dir.display().to_string());
                        self.scan_audio_files();
                    }
                }
            });

            ui.add_space(10.0);

            ui.separator();

            let mut file_to_play: Option<String> = None;

            egui::Frame::default()
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
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
                });
            if let Some(file) = file_to_play {
                self.play_file(&file);
            }
        });
    }

    pub fn render_main_panel(&mut self, ctx: &Context) {
        CentralPanel::default().show(ctx, |ui| {
            let available_width = ui.available_width();
            let available_height = ui.available_height();

            let waveform_height = available_height * 0.5;
            let play_bar_height = available_height * 0.2;

            ui.with_layout(Layout::top_down(egui::Align::Min), |ui| {

                ui.allocate_ui(Vec2::new(available_width, waveform_height), |ui| {
                    self.render_waveform(ui);
                });

                ui.add_space(10.0);

                ui.allocate_ui(Vec2::new(available_width, play_bar_height), |ui| {
                    let progress_secs = self.player.progress().as_secs();
                    let total_secs = self.total_duration.as_secs();

                    let ratio = if total_secs > 0 {
                        (progress_secs as f32 / total_secs as f32).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };

                    let bar_height = 6.0;
                    let horizontal_padding = 12.0;

                    ui.add_space(5.0); // vertical margin (top)

                    let (outer_rect, _) = ui.allocate_exact_size(
                        Vec2::new(available_width, bar_height),
                        egui::Sense::hover(),
                    );

                    let bar_rect = Rect {
                        min: outer_rect.min + Vec2::new(horizontal_padding, 0.0),
                        max: outer_rect.max - Vec2::new(horizontal_padding, 0.0),
                    };

                    ui.painter().rect_filled(bar_rect, 3.0, LIGHTER_ACCENT_COLOR);
                    let played_rect = Rect {
                        min: bar_rect.min,
                        max: egui::pos2(bar_rect.min.x + bar_rect.width() * ratio, bar_rect.max.y),
                    };
                    ui.painter().rect_filled(played_rect, 3.0, ACCENT_COLOR);

                    ui.add_space(5.0);

                    ui.horizontal(|ui| {
                        ui.add_space(horizontal_padding);
                        ui.label(format!("{:02}:{:02}", progress_secs / 60, progress_secs % 60));

                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(horizontal_padding);
                            ui.label(format!("{:02}:{:02}", total_secs / 60, total_secs % 60));
                        });
                    });
                });

                ui.with_layout(Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                    ui.horizontal(|ui| {
                        let total_button_width = 3.0 * 40.0;
                        let available_width = ui.available_width();
                        let spacing = (available_width - total_button_width) / 3.0;

                        if spacing > 0.0 {
                            ui.add_space(spacing);
                        }

                        let play_response = AudioPlayerApp::styled_icon_button(ui, "Play", "▶");
                        if play_response.clicked() {
                            let file_path = self.player.current_file().map(ToOwned::to_owned);
                            if let Some(file_path) = file_path {
                                // If a file was paused, resume it
                                if self.player.is_paused() {
                                    if let Err(e) = self.player.resume() {
                                        eprintln!("Failed to resume playback: {}", e);
                                    }

                                } else {
                                    // Otherwise play the current file again
                                    self.play_file(&file_path);
                                }
                            }
                        }

                        let pause_response = AudioPlayerApp::styled_icon_button(ui, "Pause", "⏸");
                        if pause_response.clicked() {
                            self.player.pause();
                        }

                        let stop_response = AudioPlayerApp::styled_icon_button(ui, "Stop", "⏹");
                        if stop_response.clicked() {
                            self.player.stop();
                        }

                    });
                });

            });
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

        painter.rect_filled(waveform_rect, 0.0, Color32::BLACK);

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
                "Select audio file to play...",
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

    fn styled_icon_button(ui: &mut egui::Ui, label: &str, icon: &str) -> egui::Response {
        ui.add_sized(
            egui::vec2(90.0, 30.0),
            egui::Button::new(
                egui::RichText::new(format!("{} {}", icon, label))
                    .color(egui::Color32::WHITE)
                    .size(14.0),
            )
                .fill(ACCENT_COLOR)
                .rounding(egui::Rounding::same(4))
                .frame(true),
        )
    }
}