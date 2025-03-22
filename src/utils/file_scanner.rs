use walkdir::WalkDir;

pub struct AudioFileScanner;

impl AudioFileScanner {
    pub fn scan_directory(dir_path: &str, max_depth: usize) -> Vec<String> {
        let mut audio_files = Vec::new();

        for entry in WalkDir::new(dir_path)
            .min_depth(1)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let path = entry.path().display().to_string();
                if Self::is_audio_file(&path) {
                    audio_files.push(path);
                }
            }
        }

        audio_files
    }

    fn is_audio_file(path: &str) -> bool {
        let extensions = [".mp3", ".wav", ".flac", ".m4a", ".ogg"];
        extensions.iter().any(|ext| path.ends_with(ext))
    }
}