use crossterm::event::{self, poll, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use kira::{sound::PlaybackState, AudioManager, AudioManagerSettings, DefaultBackend, Frame};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{error::Error, io::stdout, io::Write, sync::Arc, thread, time::Duration};

pub struct AudioPlayer {
    output_samples: Vec<(f32, f32)>,
}

impl AudioPlayer {
    pub fn new(output_samples: Vec<(f32, f32)>) -> Result<Self, Box<dyn Error>> {
        Ok(Self { output_samples })
    }

    pub fn play(&mut self) -> Result<(), Box<dyn Error>> {
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
        let mut sound = manager.play(sound_data)?;

        let paused = Arc::new(AtomicBool::new(false));

        enable_raw_mode()?;

        let progress_thread = thread::spawn(move || loop {
            if poll(Duration::from_millis(500)).unwrap_or(false) {
                if let Ok(Event::Key(key_event)) = event::read() {
                    if key_event.kind == KeyEventKind::Press {
                        match key_event.code {
                            KeyCode::Char(' ') => {
                                let was_paused = paused.fetch_xor(true, Ordering::SeqCst);
                                if was_paused {
                                    sound.resume(Default::default());
                                } else {
                                    sound.pause(Default::default());
                                }
                            }
                            KeyCode::Esc => {
                                sound.stop(Default::default());
                                println!("\r{}Playback stopped!", Clear(ClearType::CurrentLine));
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            if sound.state() == PlaybackState::Stopped {
                println!("\r{}Playback finished!", Clear(ClearType::CurrentLine));
                break;
            }

            let current_pos_duration = Duration::from_secs_f64(sound.position());

            print!(
                "\r{}Playing... {:02}:{:02} / {:02}:{:02}. Press space to {}, esc to stop",
                Clear(ClearType::CurrentLine),
                current_pos_duration.as_secs() / 60,
                current_pos_duration.as_secs() % 60,
                total_duration.as_secs() / 60,
                total_duration.as_secs() % 60,
                if paused.load(Ordering::SeqCst) {
                    "resume"
                } else {
                    "pause"
                }
            );
            let _ = stdout().flush();
        });

        let _ = progress_thread.join();

        disable_raw_mode()?;

        Ok(())
    }
}
