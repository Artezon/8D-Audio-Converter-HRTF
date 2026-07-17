use rodio::{DeviceSinkBuilder, Player, Source};
use std::io::Write;
use std::num::NonZero;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

pub struct AudioPlayer {
    output_samples: Vec<(f32, f32)>,
}

impl AudioPlayer {
    pub fn new(output_samples: Vec<(f32, f32)>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { output_samples })
    }

    pub fn play(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut handle = DeviceSinkBuilder::open_default_sink()?;
        handle.log_on_drop(false);
        let player = Player::connect_new(&handle.mixer());

        let current_sample = Arc::new(AtomicUsize::new(0));
        let current_sample_clone = Arc::clone(&current_sample);
        let total_samples = self.output_samples.len();

        let source = StereoSampleSource::new(self.output_samples.clone(), 44100, current_sample);

        let total_duration = Duration::from_secs_f64(total_samples as f64 / 44100.0);

        // Playback progress thread
        thread::spawn(move || loop {
            let pos = current_sample_clone.load(Ordering::Relaxed);
            if pos >= total_samples {
                break;
            }
            let current_pos_duration = Duration::from_secs_f64(pos as f64 / 44100.0);
            print!(
                "\rPlaying... {:02}:{:02} / {:02}:{:02}. Press Ctrl+C to stop",
                current_pos_duration.as_secs() / 60,
                current_pos_duration.as_secs() % 60,
                total_duration.as_secs() / 60,
                total_duration.as_secs() % 60
            );
            std::io::stdout().flush().unwrap();
            thread::sleep(Duration::from_millis(100));
        });

        player.append(source);
        player.sleep_until_end();

        println!("\n\nPlayback finished!");
        Ok(())
    }
}

struct StereoSampleSource {
    samples: Vec<(f32, f32)>,
    position: usize,
    sample_rate: u32,
    current_sample: Arc<AtomicUsize>,
}

impl StereoSampleSource {
    fn new(samples: Vec<(f32, f32)>, sample_rate: u32, current_sample: Arc<AtomicUsize>) -> Self {
        Self {
            samples,
            position: 0,
            sample_rate,
            current_sample,
        }
    }
}

impl Iterator for StereoSampleSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.samples.len() * 2 {
            return None;
        }

        let sample_idx = self.position / 2;
        let channel = self.position % 2;
        self.position += 1;

        if channel == 1 {
            self.current_sample.store(sample_idx + 1, Ordering::Relaxed);
        }

        let (left, right) = self.samples[sample_idx];
        Some(if channel == 0 { left } else { right })
    }
}

impl Source for StereoSampleSource {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.samples.len() * 2 - self.position)
    }

    fn channels(&self) -> NonZero<u16> {
        NonZero::new(2).unwrap()
    }

    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(self.sample_rate).unwrap()
    }

    fn total_duration(&self) -> Option<Duration> {
        let duration_secs = self.samples.len() as f64 / self.sample_rate as f64;
        Some(Duration::from_secs_f64(duration_secs))
    }
}
