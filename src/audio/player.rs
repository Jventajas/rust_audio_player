use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct AudioPlayer {
    _stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Option<Arc<Mutex<Sink>>>,
    start_time: Option<Instant>,
    pause_duration: Duration,
    playing_file: Option<String>,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self {
            _stream: None,
            stream_handle: None,
            sink: None,
            start_time: None,
            pause_duration: Duration::ZERO,
            playing_file: None,
        }
    }
}

impl AudioPlayer {
    pub fn play(&mut self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.stop();

        let (stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;

        let file = File::open(file_path)?;
        let source = Decoder::new(BufReader::new(file))?;

        sink.append(source);

        self._stream = Some(stream);
        self.stream_handle = Some(stream_handle);
        self.sink = Some(Arc::new(Mutex::new(sink)));
        self.start_time = Some(Instant::now());
        self.pause_duration = Duration::ZERO;
        self.playing_file = Some(file_path.to_string());

        Ok(())
    }

    pub fn pause(&mut self) {
        if let Some(sink) = &self.sink {
            let sink_guard = sink.lock().unwrap();
            sink_guard.pause();
            if let Some(start) = self.start_time.take() {
                self.pause_duration += Instant::now() - start;
            }
        }
    }

    pub fn resume(&mut self) {
        if let Some(sink) = &self.sink {
            let sink_guard = sink.lock().unwrap();
            sink_guard.play();
            self.start_time = Some(Instant::now());
        }
    }

    pub fn stop(&mut self) {
        if let Some(sink) = &self.sink {
            sink.lock().unwrap().stop();
        }
        self._stream = None;
        self.stream_handle = None;
        self.sink = None;
        self.start_time = None;
        self.pause_duration = Duration::ZERO;
        self.playing_file = None;
    }

    pub fn is_paused(&self) -> bool {
        if let Some(sink) = &self.sink {
            let sink_guard = sink.lock().unwrap();
            sink_guard.is_paused()
        } else {
            false
        }
    }


    pub fn progress(&self) -> Duration {
        if let Some(sink) = &self.sink {
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

    pub fn current_file(&self) -> Option<&str> {
        self.playing_file.as_deref()
    }
}