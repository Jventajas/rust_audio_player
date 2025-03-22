use std::fs::File;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::errors::Error;
use symphonia::default::{get_codecs, get_probe};

pub struct WaveformGenerator {
    // Optional channel receiver to fetch waveform chunks
    receiver: Option<Receiver<(Option<u32>, Vec<f32>)>>,

    // Buffer to store the waveform data
    buffer: Vec<f32>,
    // The audio sample rate (e.g., 44100 Hz)
    sample_rate: u32,
}

// Default implementation to initialize WaveformGenerator with default values
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
    // Starts the generation process for the waveform by clearing the buffer and spawning a thread
    pub fn generate_for(&mut self, file_path: &str) {
        self.buffer.clear();
        let (tx, rx) = channel();
        self.receiver = Some(rx);

        let file_path = file_path.to_string();
        thread::spawn(move || {
            Self::load_waveform_streaming(file_path, tx);
        });
    }

    // Updates the buffer with data received on the channel
    pub fn update_buffer(&mut self) {
        if let Some(receiver) = &mut self.receiver {
            let received_data: Vec<_> = receiver.try_iter().collect();
            for (rate_opt, chunk) in received_data {
                if let Some(rate) = rate_opt {
                    self.set_sample_rate(rate);
                }
                self.buffer.extend(chunk);
            }
        }
    }

    // Sets the sample rate of the waveform
    pub fn set_sample_rate(&mut self, rate: u32) {
        self.sample_rate = rate;
    }

    // Retrieves the sample rate
    pub fn get_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    // Retrieves the current waveform buffer
    pub fn get_buffer(&self) -> &[f32] {
        &self.buffer
    }

    // Loads the audio file in a streaming fashion and processes the waveform
    fn load_waveform_streaming(file_path: String, tx: Sender<(Option<u32>, Vec<f32>)>) {
        let file = match File::open(&file_path) {
            Ok(f) => f,
            Err(_) => return,
        };

        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let probed = match get_probe().format(
            &Default::default(),
            mss,
            &Default::default(),
            &Default::default(),
        ) {
            Ok(p) => p,
            Err(_) => return,
        };

        let mut format_reader = probed.format;

        let (track_id, sample_rate) = {
            // Limit the immutable borrow to this block to avoid conflicts
            let track = match format_reader.default_track() {
                Some(t) => t,
                None => return,
            };

            let rate = track.codec_params.sample_rate;
            (track.id, rate)
        };

        let mut decoder = match get_codecs().make(&format_reader.tracks()[track_id as usize].codec_params, &Default::default()) {
            Ok(d) => d,
            Err(_) => return,
        };

        let mut sample_rate_sent = false;

        loop {
            let packet = match format_reader.next_packet() {
                Ok(p) => p,
                Err(Error::IoError(_)) => break, // End of file or I/O error
                Err(_) => continue,
            };

            match decoder.decode(&packet) {
                Ok(audio_buffer) => {
                    if !sample_rate_sent {
                        if let Some(rate) = sample_rate {
                            let empty_buf: Vec<f32> = Vec::new();
                            if tx.send((Some(rate), empty_buf)).is_err() {
                                break;
                            }
                            sample_rate_sent = true;
                        }
                    }

                    let chunk_waveform = Self::process_audio_buffer(audio_buffer);
                    if !chunk_waveform.is_empty() {
                        if tx.send((None, chunk_waveform)).is_err() {
                            break; // Disconnected receiver
                        }
                    }
                }
                Err(Error::DecodeError(_)) => continue,
                Err(_) => break,
            }
        }
    }

    // Converts the raw audio buffer into a uniform waveform vector
    fn process_audio_buffer(audio_buffer: AudioBufferRef) -> Vec<f32> {
        let channels = audio_buffer.spec().channels.count(); // Number of audio channels
        let frames = audio_buffer.frames(); // Number of audio frames
        let mut chunk_waveform = Vec::with_capacity(frames); // Initialize a buffer for the waveform chunk

        // Match the sample format of the audio buffer and process accordingly
        match audio_buffer {
            AudioBufferRef::U8(buf) => {
                // Process unsigned 8-bit PCM samples
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += (buf.chan(ch)[frame] as f32 - 128.0) / 128.0; // Normalize to [-1, 1]
                    }
                    chunk_waveform.push(sum / channels as f32); // Average across channels
                }
            }
            AudioBufferRef::U16(buf) => {
                // Process unsigned 16-bit PCM samples
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += (buf.chan(ch)[frame] as f32 - 32768.0) / 32768.0; // Normalize to [-1, 1]
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::U24(buf) => {
                // Process unsigned 24-bit PCM samples
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        let sample = buf.chan(ch)[frame].inner() as i32 - 8_388_608;
                        sum += (sample as f32) / 8_388_608.0; // Normalize to [-1, 1]
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::U32(buf) => {
                // Process unsigned 32-bit PCM samples
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += (buf.chan(ch)[frame] as f32 - 2_147_483_648.0) / 2_147_483_648.0; // Normalize to [-1, 1]
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::S8(buf) => {
                // Process signed 8-bit PCM samples
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame] as f32 / i8::MAX as f32; // Normalize to [-1, 1]
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::S16(buf) => {
                // Process signed 16-bit PCM samples
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame] as f32 / i16::MAX as f32; // Normalize to [-1, 1]
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::S24(buf) => {
                // Process signed 24-bit PCM samples
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        let sample = buf.chan(ch)[frame].inner();
                        sum += sample as f32 / 8_388_608.0; // Normalize to [-1, 1]
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::S32(buf) => {
                // Process signed 32-bit PCM samples
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame] as f32 / i32::MAX as f32; // Normalize to [-1, 1]
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::F32(buf) => {
                // Process 32-bit floating point PCM samples
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame]; // Already normalized in the range [-1, 1]
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
            AudioBufferRef::F64(buf) => {
                // Process 64-bit floating point PCM samples
                for frame in 0..frames {
                    let mut sum = 0f32;
                    for ch in 0..channels {
                        sum += buf.chan(ch)[frame] as f32; // Already normalized in the range [-1, 1]
                    }
                    chunk_waveform.push(sum / channels as f32);
                }
            }
        }

        chunk_waveform // Return the processed waveform chunk
    }

}