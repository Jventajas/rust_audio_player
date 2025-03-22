use std::fs::File;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use eframe::egui;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::errors::Error;
use symphonia::default::{get_codecs, get_probe};

pub struct WaveformGenerator {
    receiver: Option<Receiver<Vec<f32>>>,
    buffer: Vec<f32>,
    sample_rate: u32,
}

impl Default for WaveformGenerator {
    fn default() -> Self {
        Self {
            receiver: None,
            buffer: Vec::new(),
            sample_rate: 44100,
        }
    }
}

impl WaveformGenerator {
    pub fn generate_for(&mut self, file_path: &str) {
        self.buffer.clear();
        let (tx, rx) = channel();
        self.receiver = Some(rx);

        // Clone the path for the thread
        let file_path = file_path.to_string();

        thread::spawn(move || {
            Self::load_waveform_streaming(file_path, tx);
        });
    }

    pub fn update_buffer(&mut self) {
        if let Some(receiver) = &self.receiver {
            for chunk in receiver.try_iter() {
                self.buffer.extend(chunk);
            }
        }
    }

    pub fn set_sample_rate(&mut self, rate: u32) {
        self.sample_rate = rate;
    }

    pub fn get_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn get_buffer(&self) -> &[f32] {
        &self.buffer
    }

    fn load_waveform_streaming(file_path: String, tx: Sender<Vec<f32>>) {
        // Open the file
        let file = match File::open(&file_path) {
            Ok(f) => f,
            Err(_) => return, // Silently fail
        };

        // Create a media source stream
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        // Probe the media source
        let probed = match get_probe().format(
            &Default::default(),
            mss,
            &Default::default(),
            &Default::default()
        ) {
            Ok(p) => p,
            Err(_) => return,
        };

        // Get the format reader
        let mut format_reader = probed.format;

        // Get the default track
        let track = match format_reader.default_track() {
            Some(t) => t,
            None => return,
        };

        let codec_params = &track.codec_params;

        // Create a decoder for the track
        let mut decoder = match get_codecs().make(codec_params, &Default::default()) {
            Ok(d) => d,
            Err(_) => return,
        };

        // Extract and set the sample rate if available
        if let Some(rate) = codec_params.sample_rate {
            // We can't directly set sample_rate here as it's in another thread,
            // but the caller could look up the correct sample rate from the file metadata
            // tx.send(vec![rate as f32]).ok(); // A way to communicate the sample rate
        }

        // Process audio packets
        loop {
            // Get the next packet from the format reader
            let packet = match format_reader.next_packet() {
                Ok(p) => p,
                Err(Error::IoError(_)) => break, // End of file or I/O error
                Err(_) => continue,              // Skip other errors and try next packet
            };

            // Decode the packet
            match decoder.decode(&packet) {
                Ok(audio_buffer) => {
                    let chunk_waveform = Self::process_audio_buffer(audio_buffer);
                    if !chunk_waveform.is_empty() {
                        if tx.send(chunk_waveform).is_err() {
                            break; // Receiver disconnected
                        }
                    }
                }
                Err(Error::DecodeError(_)) => continue, // Skip decode errors
                Err(_) => break,                       // Stop on other errors
            }
        }
    }

    fn process_audio_buffer(audio_buffer: AudioBufferRef) -> Vec<f32> {
        let channels = audio_buffer.spec().channels.count();
        let frames = audio_buffer.frames();
        let mut chunk_waveform = Vec::with_capacity(frames);

        // Process different sample formats
        match audio_buffer {
            AudioBufferRef::U8(buf) => {
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += (buf.chan(ch)[frame] as f32 - 128.0) / 128.0;
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::U16(buf) => {
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += (buf.chan(ch)[frame] as f32 - 32768.0) / 32768.0;
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::U24(buf) => {
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        let sample = buf.chan(ch)[frame].inner() as i32 - 8_388_608;
                        sum += (sample as f32) / 8_388_608.0;
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::U32(buf) => {
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += (buf.chan(ch)[frame] as f32 - 2_147_483_648.0) / 2_147_483_648.0;
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::S8(buf) => {
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame] as f32 / i8::MAX as f32;
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::S16(buf) => {
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame] as f32 / i16::MAX as f32;
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::S24(buf) => {
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        let sample = buf.chan(ch)[frame].inner();
                        sum += sample as f32 / 8_388_608.0;
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::S32(buf) => {
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame] as f32 / i32::MAX as f32;
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::F32(buf) => {
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame];
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::F64(buf) => {
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame] as f32;
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
        }

        chunk_waveform
    }

    pub fn get_visible_waveform(&self, progress_secs: f32, window_size_secs: f32) -> &[f32] {
        let samples_played = (progress_secs * self.sample_rate as f32) as usize;
        let visible_length_samples = (window_size_secs * self.sample_rate as f32) as usize;

        // Calculate start and end indices for the visible portion
        let start_idx = samples_played.saturating_sub(visible_length_samples / 2);
        let end_idx = (start_idx + visible_length_samples).min(self.buffer.len());

        if start_idx < end_idx && !self.buffer.is_empty() {
            &self.buffer[start_idx..end_idx]
        } else {
            &[]
        }
    }
}

// Optional: Add a helper struct to visualize the waveform
pub struct WaveformVisualizer<'a> {
    waveform: &'a [f32],
    scale: f32,
    color: egui::Color32,
    stroke_width: f32,
}

impl<'a> WaveformVisualizer<'a> {
    pub fn new(waveform: &'a [f32]) -> Self {
        Self {
            waveform,
            scale: 1.0,
            color: egui::Color32::LIGHT_BLUE,
            stroke_width: 1.5,
        }
    }

    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    pub fn with_color(mut self, color: egui::Color32) -> Self {
        self.color = color;
        self
    }

    pub fn with_stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width;
        self
    }

    pub fn draw(&self, ui: &mut egui::Ui) -> egui::Response {
        // Get the available space
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::hover());

        if !self.waveform.is_empty() {
            let painter = ui.painter_at(rect);
            let height = rect.height() * self.scale;
            let center_y = rect.center().y;
            let width = rect.width();

            // Generate points for the waveform
            let points: Vec<egui::Pos2> = self.waveform.iter().enumerate()
                .map(|(i, &sample)| {
                    let x = rect.left() + (i as f32 * width / self.waveform.len() as f32);
                    let y = center_y - (sample * height / 2.0);
                    egui::Pos2::new(x, y)
                })
                .collect();

            // Draw the waveform
            if points.len() > 1 {
                painter.add(egui::Shape::line(
                    points,
                    egui::Stroke::new(self.stroke_width, self.color)
                ));
            }
        } else {
            // Draw a placeholder when no waveform data is available
            let painter = ui.painter_at(rect);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Loading...",
                egui::FontId::default(),
                egui::Color32::GRAY,
            );
        }

        response
    }
}