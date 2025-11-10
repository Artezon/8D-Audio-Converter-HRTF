use rodio::{Sink, Source};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

pub struct Player {
    output_samples: Vec<(f32, f32)>,
}

impl Player {
    pub fn new(output_samples: Vec<(f32, f32)>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { output_samples })
    }

    pub fn play(self) -> Result<(), Box<dyn std::error::Error>> {
        // Get default audio output
        let stream = rodio::OutputStreamBuilder::open_default_stream()?;
        let sink = Arc::new(Sink::connect_new(stream.mixer())); // Wrap in Arc for thread-safe sharing

        // Create audio source from processed samples
        let source = StereoSampleSource::new(self.output_samples.clone(), 44100);

        // Current position tracking
        let current_sample = Arc::new(AtomicUsize::new(0));
        let total_samples = self.output_samples.len();

        // Update thread for display
        let current_sample_clone = current_sample.clone();
        let total_duration = Duration::from_secs_f64(total_samples as f64 / 44100.0);
        thread::spawn(move || loop {
            let pos = current_sample_clone.load(Ordering::Relaxed);
            if pos >= total_samples {
                break;
            }

            let current_pos_duration = Duration::from_secs_f64(pos as f64 / 44100.0);

            print!(
                "Playing... {:02}:{:02} / {:02}:{:02}. Press Ctrl+C to stop\r",
                current_pos_duration.as_secs() / 60,
                current_pos_duration.as_secs() % 60,
                total_duration.as_secs() / 60,
                total_duration.as_secs() % 60
            );
            std::io::Write::flush(&mut std::io::stdout()).unwrap();

            thread::sleep(Duration::from_millis(100));
        });

        // Create source with position tracking
        let tracked_source = source.track_position(current_sample);

        // Add to sink and play
        sink.append(tracked_source);

        // Wait for playback to complete
        sink.sleep_until_end();

        println!("\n\nPlayback finished!");

        Ok(())
    }
}

// Stereo audio source implementation
struct StereoSampleSource {
    samples: Vec<(f32, f32)>,
    position: usize,
    sample_rate: u32,
}

impl StereoSampleSource {
    fn new(samples: Vec<(f32, f32)>, sample_rate: u32) -> Self {
        Self {
            samples,
            position: 0,
            sample_rate,
        }
    }

    fn track_position(self, counter: Arc<AtomicUsize>) -> TrackedSource {
        TrackedSource {
            source: self,
            counter,
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

        let (left, right) = self.samples[sample_idx];
        Some(if channel == 0 { left } else { right })
    }
}

impl Source for StereoSampleSource {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.samples.len() * 2 - self.position)
    }

    fn channels(&self) -> u16 {
        2
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        let total_samples = self.samples.len() as u64;
        let duration_secs = total_samples as f64 / self.sample_rate as f64;
        Some(Duration::from_secs_f64(duration_secs))
    }
}

// Wrapper to track playback position
struct TrackedSource {
    source: StereoSampleSource,
    counter: Arc<AtomicUsize>,
}

impl Iterator for TrackedSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.source.next();
        if result.is_some() {
            let sample_idx = self.source.position / 2;
            self.counter.store(sample_idx, Ordering::Relaxed);
        }
        result
    }
}

impl Source for TrackedSource {
    fn current_span_len(&self) -> Option<usize> {
        self.source.current_span_len()
    }

    fn channels(&self) -> u16 {
        self.source.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.source.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }
}
