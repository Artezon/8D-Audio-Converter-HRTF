use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use kira::{sound::PlaybackState, AudioManager, AudioManagerSettings, DefaultBackend, Frame};
use std::{io::Write, sync::Arc, thread, time::Duration};

pub struct AudioPlayer {
    output_samples: Vec<(f32, f32)>,
}

impl AudioPlayer {
    pub fn new(output_samples: Vec<(f32, f32)>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { output_samples })
    }

    pub fn play(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let sample_rate = 44100u32;
        let total_samples = self.output_samples.len();
        let total_duration = Duration::from_secs_f64(total_samples as f64 / sample_rate as f64);

        let frames: Vec<Frame> = self
            .output_samples
            .iter()
            .map(|(l, r)| Frame::new(*l, *r))
            .collect();

        let sound_data = StaticSoundData {
            sample_rate,
            frames: Arc::from(frames),
            settings: StaticSoundSettings::default(),
            slice: None,
        };

        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())?;
        let sound = manager.play(sound_data)?;

        let progress_thread = thread::spawn(move || loop {
            let current_pos_duration = Duration::from_secs_f64(sound.position());

            print!(
                "\rPlaying... {:02}:{:02} / {:02}:{:02}. Press Ctrl+C to stop",
                current_pos_duration.as_secs() / 60,
                current_pos_duration.as_secs() % 60,
                total_duration.as_secs() / 60,
                total_duration.as_secs() % 60
            );
            std::io::stdout().flush().unwrap();

            if sound.state() == PlaybackState::Stopped {
                break;
            }

            thread::sleep(Duration::from_millis(100));
        });

        progress_thread.join().unwrap();

        println!("\n\nPlayback finished!");
        Ok(())
    }
}
