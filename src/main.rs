use clap::Parser;
use hrtf::HrirSphere;
use std::io::{stdin, stdout, Write};
use std::{error::Error, f32::consts::PI, fmt::Display, path::PathBuf, str::FromStr};

const HRIR_DATA: &[u8] = include_bytes!("../IRC_1002_C.bin");

mod audio_io;
mod crossover;
mod hrtf_8d;
mod player;
mod reverb;

use audio_io::{load_audio, save_audio};
use hrtf_8d::{Audio8DProcessor, MovementPattern, PositionCalculator};
use player::AudioPlayer;

#[derive(Parser, Debug)]
#[command(
    name = "Binaural 8D Audio Generator",
    about = "Binaural 8D Audio Generator\n\nGenerate 8D audio with spatial movement and effects",
    long_about = None,
    version,
    author = "Artezon",
    arg_required_else_help = true
)]
struct Args {
    /// Input audio file path
    #[arg(value_name = "INPUT_FILE")]
    input_path: PathBuf,

    /// Output file path (WAV, FLAC, OGG, MP3). If not specified, plays audio directly
    #[arg(short = 'o', long = "output", value_name = "OUTPUT_FILE")]
    output_path: Option<PathBuf>,

    /// Movement pattern
    #[arg(
        short,
        long,
        value_enum,
        default_value = "circular",
        help_heading = "Spatial options"
    )]
    pattern: MovementPattern,

    /// Starting angle in degrees, 0 - 359
    #[arg(
        short = 'a',
        long = "start-angle",
        value_name = "DEGREES",
        default_value = "0",
        help_heading = "Spatial options"
    )]
    start_angle: f32,

    /// Rotation velocity, -100 - 100 RPM, positive - clockwise, negative - counterclockwise (single value or from,to range)
    #[arg(
        short = 'v',
        long = "velocity",
        value_name = "RPM|FROM,TO",
        default_value = "10",
        allow_hyphen_values = true,
        help_heading = "Spatial options"
    )]
    velocity: ValueOrRange,

    /// Velocity oscillation speed, 0 - 10
    #[arg(
        long,
        value_name = "SPEED",
        default_value = "5",
        help_heading = "Spatial options"
    )]
    velocity_osc_speed: f32,

    /// Elevation in degrees, -90 - 90 (single value or from,to range)
    #[arg(
        short = 'e',
        long = "elevation",
        value_name = "DEG|FROM,TO",
        default_value = "0",
        allow_hyphen_values = true,
        help_heading = "Spatial options"
    )]
    elevation: ValueOrRange,

    /// Elevation oscillation speed, 0 - 10
    #[arg(
        long,
        value_name = "SPEED",
        default_value = "5",
        help_heading = "Spatial options"
    )]
    elevation_osc_speed: f32,

    /// Distance/radius in meters, 0.1 - 100 (single value or from,to range)
    #[arg(
        short = 'd',
        long = "distance",
        value_name = "METERS|FROM,TO",
        default_value = "1",
        help_heading = "Spatial options"
    )]
    distance: ValueOrRange,

    /// Distance oscillation speed, 0 - 10
    #[arg(
        long,
        value_name = "SPEED",
        default_value = "0.1",
        help_heading = "Spatial options"
    )]
    distance_osc_speed: f32,

    /// Bass boost in dB, -20 - 20
    #[arg(
        short = 'b',
        long = "bass-boost",
        value_name = "DB",
        default_value = "0",
        help_heading = "Bass options"
    )]
    bass_boost: f32,

    /// Reverb mix amount, 0.0 - 1.0
    #[arg(
        short = 'r',
        long = "reverb-mix",
        value_name = "VALUE",
        default_value = "0.3",
        help_heading = "Reverb options"
    )]
    reverb_mix: f32,

    /// Reverb room size, 0.0 - 1.0
    #[arg(long, default_value = "0.5", help_heading = "Reverb options")]
    reverb_room: f32,

    /// Reverb high-frequency dampening, 0.0 - 1.0
    #[arg(long, default_value = "0.5", help_heading = "Reverb options")]
    reverb_dampening: f32,

    /// Reverb stereo width, 0.0 - 1.0
    #[arg(long, default_value = "0.9", help_heading = "Reverb options")]
    reverb_width: f32,
}

/// Range for oscillating parameters
#[derive(Debug, Clone, Copy)]
struct ValueOrRange {
    from: f32,
    to: f32,
}

impl ValueOrRange {
    fn new_fixed(value: f32) -> Self {
        Self {
            from: value,
            to: value,
        }
    }

    fn new_range(mut from: f32, mut to: f32) -> Self {
        if from > to {
            (from, to) = (to, from);
        }
        Self { from, to }
    }

    fn is_oscillating(&self) -> bool {
        (self.to - self.from).abs() > 1e-6
    }

    fn get_value(&self, time: f32, speed: f32) -> f32 {
        if !self.is_oscillating() {
            return self.from;
        }

        let range = self.to - self.from;
        let period = range / speed;

        let phase = (time / period) % 2.0;
        let t = if phase < 1.0 { phase } else { 2.0 - phase };

        self.from + range * t
    }
}

impl FromStr for ValueOrRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains(',') {
            let parts: Vec<&str> = s.split(',').collect();
            if parts.len() != 2 {
                return Err(format!("Invalid range format: {}", s));
            }
            let from = parts[0]
                .trim()
                .parse::<f32>()
                .map_err(|_| format!("Invalid number: {}", parts[0]))?;
            let to = parts[1]
                .trim()
                .parse::<f32>()
                .map_err(|_| format!("Invalid number: {}", parts[1]))?;
            Ok(ValueOrRange::new_range(from, to))
        } else {
            let val = s
                .trim()
                .parse::<f32>()
                .map_err(|_| format!("Invalid number: {}", s))?;
            Ok(ValueOrRange::new_fixed(val))
        }
    }
}

trait IsInRangeCheck<T> {
    fn check_range(&self, min: T, max: T, value_name: Option<&str>) -> Result<(), String>;
}

impl<T: PartialOrd + Copy + Display> IsInRangeCheck<T> for T {
    fn check_range(&self, min: T, max: T, value_name: Option<&str>) -> Result<(), String> {
        let name = value_name.unwrap_or("Value");

        if *self >= min && *self <= max {
            Ok(())
        } else {
            Err(format!(
                "{} must be in range {} to {}, got {}",
                name, min, max, *self
            ))
        }
    }
}

impl IsInRangeCheck<f32> for ValueOrRange {
    fn check_range(&self, min: f32, max: f32, value_name: Option<&str>) -> Result<(), String> {
        let name = value_name.unwrap_or("Value");

        if self.from >= min && self.from <= max && self.to >= min && self.to <= max {
            Ok(())
        } else {
            Err(format!(
                "{} range ({} to {}) must be within {} to {}",
                name, self.from, self.to, min, max
            ))
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    args.start_angle
        .check_range(0.0, 359.0, Some("Start angle"))?;
    args.velocity
        .check_range(-100.0, 100.0, Some("Rotation velocity (RPM)"))?;
    args.velocity_osc_speed
        .check_range(0.0, 10.0, Some("Velocity oscillation speed"))?;
    args.elevation
        .check_range(-90.0, 90.0, Some("Elevation angle"))?;
    args.elevation_osc_speed
        .check_range(0.0, 10.0, Some("Elevation oscillation speed"))?;
    args.distance
        .check_range(0.1, 100.0, Some("Distance in meters"))?;
    args.distance_osc_speed
        .check_range(0.0, 10.0, Some("Distance oscillation speed"))?;
    args.bass_boost
        .check_range(-20.0, 20.0, Some("Bass boost"))?;
    args.reverb_mix.check_range(0.0, 1.0, Some("Reverb mix"))?;
    args.reverb_room
        .check_range(0.0, 1.0, Some("Reverb room size"))?;
    args.reverb_dampening
        .check_range(0.0, 1.0, Some("Reverb dampening"))?;
    args.reverb_width
        .check_range(0.0, 1.0, Some("Reverb width"))?;

    println!("Binaural 8D Audio Generator\n");

    // Check output path if specified
    if let Some(ref output_path) = args.output_path {
        // Check if extension is valid by calling from_path
        audio_io::OutputFormat::from_path(output_path)?;

        // Check if file already exists
        if output_path.exists() {
            println!(
                "Warning: Output file '{}' already exists.",
                output_path.display()
            );
            print!("Do you want to overwrite it? (y/n): ");
            stdout().flush()?;

            let mut response = String::new();
            stdin().read_line(&mut response)?;

            if !response.trim().eq_ignore_ascii_case("y") {
                println!("Operation cancelled.");
                return Ok(());
            }
        }

        // Check if parent directory exists
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                return Err(format!("Output directory does not exist: {:?}", parent).into());
            }
        }
    }

    println!("Loading audio: {}", args.input_path.display());
    let input_samples = load_audio(&args.input_path)?;

    let hrir_sphere = HrirSphere::new(HRIR_DATA, 44100)
        .map_err(|e| format!("Failed to load HRIR sphere: {:?}", e))?;

    let pattern: MovementPattern = args.pattern.into();
    let start_angle = args.start_angle * PI / 180.0;

    println!("Configuration:");
    println!("  Pattern: {:?}", pattern);
    println!("  Start angle: {:.1}°", args.start_angle);

    if args.velocity.is_oscillating() {
        println!(
            "  Velocity: {:.2} to {:.2} (osc speed: {:.2})",
            args.velocity.from, args.velocity.to, args.velocity_osc_speed
        );
    } else {
        println!("  Velocity: {:.2}", args.velocity.from);
    }

    if args.elevation.is_oscillating() {
        println!(
            "  Elevation: {:.1}° to {:.1}° (osc speed: {:.2})",
            args.elevation.from, args.elevation.to, args.elevation_osc_speed
        );
    } else {
        println!("  Elevation: {:.1}°", args.elevation.from);
    }

    if args.distance.is_oscillating() {
        println!(
            "  Distance: {:.2}m to {:.2}m (osc speed: {:.2})",
            args.distance.from, args.distance.to, args.distance_osc_speed
        );
    } else {
        println!("  Distance: {:.2}m", args.distance.from);
    }

    println!("  Bass boost: {:.1} dB", args.bass_boost);
    println!(
        "  Reverb mix: {:.2}, room size: {:.2}, dampening: {:.2}, stereo width: {:.2}\n",
        args.reverb_mix, args.reverb_room, args.reverb_dampening, args.reverb_width
    );

    let position_calc = PositionCalculator::new(
        pattern,
        start_angle,
        args.velocity,
        args.velocity_osc_speed,
        args.elevation,
        args.elevation_osc_speed,
        args.distance,
        args.distance_osc_speed,
    );

    let mut processor = Audio8DProcessor::new(
        hrir_sphere,
        44100,
        args.reverb_room,
        args.reverb_dampening,
        args.reverb_width,
        args.reverb_mix,
        args.bass_boost,
    );

    let output_samples = processor.process_audio(
        &input_samples,
        position_calc,
        Some(&|progress| {
            print!("\rProcessing: {}%", (progress * 100.0) as u32);
            let _ = stdout().flush();
        }),
    );

    println!("\n\nNormalizing audio...");
    let max_sample = output_samples
        .iter()
        .map(|(l, r)| l.abs().max(r.abs()))
        .fold(0.0, f32::max);

    let normalized_output: Vec<(f32, f32)> = if max_sample > 0.001 {
        let target_level = 0.945;
        let gain = target_level / max_sample;
        println!("Normalized to -0.5 dB (gain: {:.3}x)\n", gain);

        output_samples
            .iter()
            .map(|(l, r)| ((l * gain).clamp(-1.0, 1.0), (r * gain).clamp(-1.0, 1.0)))
            .collect()
    } else {
        output_samples
    };

    if let Some(output_path) = args.output_path {
        // File output mode
        println!("Saving output to: {}", output_path.display());
        save_audio(&output_path, &normalized_output, 44100)?;
        println!("File saved successfully!");
        println!("\nNOTE: You need headphones for the true 8D listening experience.");
    } else {
        // Real-time playback mode
        println!("NOTE: You need headphones for the true 8D listening experience.\n");
        let mut player = AudioPlayer::new(normalized_output)?;
        player.play()?;
    }

    Ok(())
}
