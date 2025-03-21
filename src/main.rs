use std::fs::File;
use std::io::BufReader;
use eframe::egui::{Slider, Sense, Label, Color32, FontId, Align2, Stroke, Frame, pos2, Pos2, ScrollArea};
use std::time::{Duration, Instant};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Sender, Receiver, channel};
use symphonia::default::{get_probe, get_codecs};
use symphonia::core::audio::Signal;
use eframe::egui::{self, CentralPanel, Context, SidePanel, Button};
use rfd::FileDialog;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sample, Sink, Source};
use walkdir::WalkDir;
use hound;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::errors::Error;

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
    waveform_receiver: Option<Receiver<Vec<f32>>>,
    waveform_buffer: Vec<f32>,
    sample_rate: u32,

}

impl eframe::App for MyApp {

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(30));

        if let Some(receiver) = &self.waveform_receiver {
            for chunk in receiver.try_iter() {
                self.waveform_buffer.extend(chunk);
            }
        }

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
                    let file_name = std::path::Path::new(file)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string();
                    if ui
                        .selectable_label(Some(file) == self.current_file.as_ref(), &file_name)
                        .clicked()
                    {
                        file_to_play = Some(file.clone());
                    }
                }
            });

            if let Some(file) = file_to_play {
                self.play_file(file);
            }
        });

        CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("⏸️ Pause").clicked() {
                    self.pause_audio();
                }
                if ui.button("▶️ Resume").clicked() {
                    self.resume_audio();
                }
                if ui.button("⏹️ Stop").clicked() {
                    self.stop_audio();
                }
            });

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

            ui.separator();

            let progress_secs = self.progress().as_secs_f32();
            let waveform_len = self.waveform_buffer.len();
            let samples_played = (progress_secs * self.sample_rate as f32) as usize;
            let visible_length_samples = (self.sample_rate as usize) * 2;

            let start_idx = samples_played.saturating_sub(visible_length_samples / 2);
            let end_idx = (start_idx + visible_length_samples).min(waveform_len);

            let displayed_waveform = if start_idx < end_idx {
                &self.waveform_buffer[start_idx..end_idx]
            } else {
                &[]
            };

            let waveform_rect = ui.available_rect_before_wrap();
            let painter = ui.painter_at(waveform_rect);

            if !displayed_waveform.is_empty() {
                let wave_height = waveform_rect.height() / 2.0;
                let wave_width = waveform_rect.width() / displayed_waveform.len() as f32;
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

            let progress = progress_secs / self.total_duration.as_secs_f32().max(1.0);
            ui.horizontal(|ui| {
                ui.label(format!("{:.1} sec", progress_secs));
                ui.add(egui::ProgressBar::new(progress).show_percentage());
                ui.label(format!("{:.1} sec", self.total_duration.as_secs_f32()));
            });
        });
    }

}

impl MyApp {

    fn load_waveform_streaming(file_path: String, tx: Sender<Vec<f32>>) {
        std::thread::spawn(move || {
            let file = std::fs::File::open(&file_path).expect("Failed to open file");
            let mss = MediaSourceStream::new(Box::new(file), Default::default());

            let probed = get_probe()
                .format(&Default::default(), mss, &Default::default(), &Default::default())
                .expect("Probe failed");
            let mut format_reader = probed.format;
            let track = format_reader.default_track().expect("No track found");
            let codec_params = &track.codec_params;

            let mut decoder = get_codecs()
                .make(&codec_params, &Default::default())
                .expect("Decoder failed");

            loop {
                match format_reader.next_packet() {
                    Ok(packet) => match decoder.decode(&packet) {
                        Ok(audio_buffer) => {
                            let channels = audio_buffer.spec().channels.count();
                            let frames = audio_buffer.frames();
                            let mut chunk_waveform = Vec::new();

                            match audio_buffer {
                                symphonia::core::audio::AudioBufferRef::U8(buf) => {
                                    for frame in 0..frames {
                                        let mut sum = 0f32;
                                        for ch in 0..channels {
                                            sum += (buf.chan(ch)[frame] as f32 - 128.0) / 128.0;
                                        }
                                        chunk_waveform.push(sum / channels as f32);
                                    }
                                }
                                symphonia::core::audio::AudioBufferRef::U16(buf) => {
                                    for frame in 0..frames {
                                        let mut sum = 0f32;
                                        for ch in 0..channels {
                                            sum += (buf.chan(ch)[frame] as f32 - 32768.0) / 32768.0;
                                        }
                                        chunk_waveform.push(sum / channels as f32);
                                    }
                                }
                                symphonia::core::audio::AudioBufferRef::U24(buf) => {
                                    for frame in 0..frames {
                                        let mut sum = 0f32;
                                        for ch in 0..channels {
                                            let sample = buf.chan(ch)[frame].into_u32() as i32 - 8_388_608;
                                            sum += (sample as f32 - 8_388_608.0) / 8_388_608.0;
                                        }
                                        chunk_waveform.push(sum / channels as f32);
                                    }
                                }
                                symphonia::core::audio::AudioBufferRef::U32(buf) => {
                                    for frame in 0..frames {
                                        let mut sum = 0f32;
                                        for ch in 0..channels {
                                            sum += (buf.chan(ch)[frame] as f32 - 2_147_483_648.0)
                                                / 2_147_483_648.0;
                                        }
                                        chunk_waveform.push(sum / channels as f32);
                                    }
                                }
                                symphonia::core::audio::AudioBufferRef::S8(buf) => {
                                    for frame in 0..frames {
                                        let mut sum = 0f32;
                                        for ch in 0..channels {
                                            sum += buf.chan(ch)[frame] as f32 / i8::MAX as f32;
                                        }
                                        chunk_waveform.push(sum / channels as f32);
                                    }
                                }
                                symphonia::core::audio::AudioBufferRef::S16(buf) => {
                                    for frame in 0..frames {
                                        let mut sum = 0f32;
                                        for ch in 0..channels {
                                            sum += buf.chan(ch)[frame] as f32 / i16::MAX as f32;
                                        }
                                        chunk_waveform.push(sum / channels as f32);
                                    }
                                }
                                symphonia::core::audio::AudioBufferRef::S24(buf) => {
                                    for frame in 0..frames {
                                        let mut sum = 0f32;
                                        for ch in 0..channels {
                                            let sample = buf.chan(ch)[frame].into_i32();
                                            sum += sample as f32 / 8_388_608.0;
                                        }
                                        chunk_waveform.push(sum / channels as f32);
                                    }
                                }
                                symphonia::core::audio::AudioBufferRef::S32(buf) => {
                                    for frame in 0..frames {
                                        let mut sum = 0f32;
                                        for ch in 0..channels {
                                            sum += buf.chan(ch)[frame] as f32 / i32::MAX as f32;
                                        }
                                        chunk_waveform.push(sum / channels as f32);
                                    }
                                }
                                symphonia::core::audio::AudioBufferRef::F32(buf) => {
                                    for frame in 0..frames {
                                        let mut sum = 0f32;
                                        for ch in 0..channels {
                                            sum += buf.chan(ch)[frame];
                                        }
                                        chunk_waveform.push(sum / channels as f32);
                                    }
                                }
                                symphonia::core::audio::AudioBufferRef::F64(buf) => {
                                    for frame in 0..frames {
                                        let mut sum = 0f32;
                                        for ch in 0..channels {
                                            sum += buf.chan(ch)[frame] as f32;
                                        }
                                        chunk_waveform.push(sum / channels as f32);
                                    }
                                }
                                _ => {
                                    // Graceful no-op: Ignore unknown formats or log message.
                                    eprintln!("Encountered unsupported audio buffer format.");
                                    continue;
                                }
                            }
                            tx.send(chunk_waveform).expect("Waveform send error");
                        }
                        Err(Error::DecodeError(_)) => continue,
                        Err(_) => break,
                    },
                    Err(Error::IoError(_)) => break,
                    Err(_) => break,
                }
            }
        });
    }


    pub fn play_file(&mut self, file_path: String) {
        // Update current file selection
        self.current_file = Some(file_path.clone());

        // Initialize the sample rate (used for waveform visualization; common default: 44100 Hz)
        self.sample_rate = 44100;

        // Stop previous playback if there's an active sink
        if let Some(sink) = &self.sink {
            sink.lock().unwrap().stop();
        }

        // Create Rodio audio stream (output stream and handle)
        let (stream, stream_handle) =
            OutputStream::try_default().expect("Failed to create audio output stream");
        let sink = Sink::try_new(&stream_handle).expect("Failed to create Sink");

        // Open audio file for rodio playback
        let file = std::fs::File::open(&file_path).expect("Failed to open audio file");
        let source = Decoder::new(std::io::BufReader::new(file)).expect("Failed to decode audio file");

        // Attach audio source to the sink (playback)
        sink.append(source);

        // Store sink and stream resources
        self.stream_handle = Some(stream_handle);
        self._stream = Some(stream);
        self.sink = Some(Arc::new(Mutex::new(sink)));

        // Playback timing initialization
        self.start_time = Some(Instant::now());
        self.pause_duration = Duration::ZERO;

        // Channel for waveform streaming
        let (tx, rx) = channel();
        self.waveform_receiver = Some(rx);
        self.waveform_buffer.clear();

        // Launch waveform loading thread (non-blocking)
        Self::load_waveform_streaming(file_path, tx);
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