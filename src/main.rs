use hrtf::HrirSphere;
use std::env;
use std::f32::consts::PI;
use std::path::Path;

const HRIR_DATA: &[u8] = include_bytes!("../IRC_1002_C.bin");

mod audio_io;
mod crossover;
mod hrtf_8d;
mod player;
mod reverb;

use audio_io::{load_audio, save_audio};
use hrtf_8d::{Audio8DProcessor, MovementPattern, ParamValue, PositionCalculator};
use player::Player;

fn print_usage(program_name: &str) {
    println!("Usage: {} <input_file> [options]", program_name);
    println!("\nRequired:");
    println!("  <input_file>                    Input audio file path");
    println!("\nOptions:");
    println!("  -o, --output <file>             Output file path (WAV, FLAC, OGG, MP3)");
    println!("                                  If not specified, plays audio directly");
    println!("  -p, --pattern <pattern>         Movement pattern (circular, figure8, spiral, random, vertical)");
    println!("                                  Default: circular");
    println!("  -a, --start-angle <degrees>         Starting angle in degrees (default: 0)");
    println!("  -v, --velocity <value|min,max>      Movement velocity (default: 0.2)");
    println!("  --velocity-osc-speed <value>    Velocity oscillation speed (default: 0.1)");
    println!("  -e, --elevation <deg|min,max>       Elevation in degrees, -90 to 90 (default: 0)");
    println!("  --elevation-osc-speed <value>   Elevation oscillation speed (default: 0.1)");
    println!("  -d, --distance <meters|min,max>     Distance/radius in meters (default: 1.0)");
    println!("  --distance-osc-speed <value>    Distance oscillation speed (default: 0.1)");
    println!(
        "  --crossover <value>             Crossover frequency in Hz (50-500, default: 200.0)"
    );
    println!(
        "  -b, --bass-boost <value>            Bass boost (-20 dB to +20 dB, default: 0.0 dB)"
    );
    println!("\nReverb Options:");
    println!("  --reverb-room <value>           Room size (0.0-1.0, default: 0.5)");
    println!("  --reverb-dampening <value>      High-frequency dampening (0.0-1.0, default: 0.5)");
    println!("  --reverb-width <value>          Stereo width (0.0-1.0, default: 0.9)");
    println!("  -r, --reverb-mix <value>            Reverb mix amount (0.0-1.0, default: 0.3)");
}

pub fn parse_range_value(s: &str) -> Result<ParamValue, String> {
    if s.contains(',') {
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid range format: {}", s));
        }
        let min = parts[0]
            .trim()
            .parse::<f32>()
            .map_err(|_| format!("Invalid number: {}", parts[0]))?;
        let max = parts[1]
            .trim()
            .parse::<f32>()
            .map_err(|_| format!("Invalid number: {}", parts[1]))?;
        Ok(ParamValue::new_range(min, max))
    } else {
        let val = s
            .trim()
            .parse::<f32>()
            .map_err(|_| format!("Invalid number: {}", s))?;
        Ok(ParamValue::new_fixed(val))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Binaural 8D Audio Generator\n");

    let args: Vec<String> = env::args().collect();
    let program_name = args.get(0).map(|s| s.as_str()).unwrap_or("<executable>");

    if args.len() < 2 || args.contains(&"--help".to_string()) {
        print_usage(program_name);
        return Ok(());
    }

    let input_file = args[1].clone();
    let mut output_file: Option<String> = None;
    let mut pattern = MovementPattern::Circular;
    let mut start_angle = 0.0;
    let mut velocity = ParamValue::new_fixed(0.2);
    let mut velocity_osc_speed = 0.1;
    let mut elevation = ParamValue::new_fixed(0.0);
    let mut elevation_osc_speed = 0.1;
    let mut distance = ParamValue::new_fixed(1.0);
    let mut distance_osc_speed = 0.1;
    let mut crossover_freq = 200.0;
    let mut bass_boost_db = 0.0;
    let mut reverb_room_size = 0.5;
    let mut reverb_dampening = 0.5;
    let mut reverb_width = 0.9;
    let mut reverb_mix = 0.3;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                if i + 1 >= args.len() {
                    return Err(format!(
                        "Flag '{}' requires a value. Usage: {} <input_file> -o <output_file>",
                        args[i], program_name
                    )
                    .into());
                }
                output_file = Some(args[i + 1].clone());
                i += 2;
            }
            "-p" | "--pattern" => {
                if i + 1 >= args.len() {
                    return Err(format!("Flag '{}' requires a value. Available patterns: circular, figure8, spiral, random, vertical", args[i]).into());
                }
                pattern = match args[i + 1].to_lowercase().as_str() {
                    "circular" => MovementPattern::Circular,
                    "figure8" => MovementPattern::Figure8,
                    "spiral" => MovementPattern::Spiral,
                    "random" => MovementPattern::Random,
                    "vertical" => MovementPattern::VerticalCircle,
                    _ => {
                        eprintln!("Unknown pattern: {}", args[i + 1]);
                        return Err("Invalid pattern".into());
                    }
                };
                i += 2;
            }
            "--start-angle" => {
                if i + 1 >= args.len() {
                    return Err(format!("Flag '{}' requires a value. Usage: {} <input_file> --start-angle <degrees>", args[i], program_name).into());
                }
                start_angle = args[i + 1].parse::<f32>().unwrap_or(0.0) * PI / 180.0;
                i += 2;
            }
            "--velocity" => {
                if i + 1 >= args.len() {
                    return Err(format!("Flag '{}' requires a value. Usage: {} <input_file> --velocity <value|min,max>", args[i], program_name).into());
                }
                velocity = parse_range_value(&args[i + 1])?;
                i += 2;
            }
            "--velocity-osc-speed" => {
                if i + 1 >= args.len() {
                    return Err(format!("Flag '{}' requires a value. Usage: {} <input_file> --velocity-osc-speed <value>", args[i], program_name).into());
                }
                velocity_osc_speed = args[i + 1].parse::<f32>().unwrap_or(0.1);
                i += 2;
            }
            "--elevation" => {
                if i + 1 >= args.len() {
                    return Err(format!("Flag '{}' requires a value. Usage: {} <input_file> --elevation <deg|min,max>", args[i], program_name).into());
                }
                let elev = parse_range_value(&args[i + 1])?;
                elevation =
                    ParamValue::new_range(elev.min.clamp(-90.0, 90.0), elev.max.clamp(-90.0, 90.0));
                i += 2;
            }
            "--elevation-osc-speed" => {
                if i + 1 >= args.len() {
                    return Err(format!("Flag '{}' requires a value. Usage: {} <input_file> --elevation-osc-speed <value>", args[i], program_name).into());
                }
                elevation_osc_speed = args[i + 1].parse::<f32>().unwrap_or(0.1);
                i += 2;
            }
            "--distance" => {
                if i + 1 >= args.len() {
                    return Err(format!("Flag '{}' requires a value. Usage: {} <input_file> --distance <meters|min,max>", args[i], program_name).into());
                }
                distance = parse_range_value(&args[i + 1])?;
                i += 2;
            }
            "--distance-osc-speed" => {
                if i + 1 >= args.len() {
                    return Err(format!("Flag '{}' requires a value. Usage: {} <input_file> --distance-osc-speed <value>", args[i], program_name).into());
                }
                distance_osc_speed = args[i + 1].parse::<f32>().unwrap_or(0.1);
                i += 2;
            }
            "--crossover" => {
                if i + 1 >= args.len() {
                    return Err(format!(
                        "Flag '{}' requires a value. Usage: {} <input_file> --crossover <Hz>",
                        args[i], program_name
                    )
                    .into());
                }
                crossover_freq = args[i + 1]
                    .parse::<f32>()
                    .unwrap_or(200.0)
                    .clamp(50.0, 500.0);
                i += 2;
            }
            "--bass-boost" => {
                if i + 1 >= args.len() {
                    return Err(format!(
                        "Flag '{}' requires a value. Usage: {} <input_file> --bass-boost <dB>",
                        args[i], program_name
                    )
                    .into());
                }
                bass_boost_db = args[i + 1].parse::<f32>().unwrap_or(0.0).clamp(-20.0, 20.0);
                i += 2;
            }
            "--reverb-room" => {
                if i + 1 >= args.len() {
                    return Err(format!("Flag '{}' requires a value. Usage: {} <input_file> --reverb-room <0.0-1.0>", args[i], program_name).into());
                }
                reverb_room_size = args[i + 1].parse::<f32>().unwrap_or(0.5).clamp(0.0, 1.0);
                i += 2;
            }
            "--reverb-dampening" => {
                if i + 1 >= args.len() {
                    return Err(format!("Flag '{}' requires a value. Usage: {} <input_file> --reverb-dampening <0.0-1.0>", args[i], program_name).into());
                }
                reverb_dampening = args[i + 1].parse::<f32>().unwrap_or(0.5).clamp(0.0, 1.0);
                i += 2;
            }
            "--reverb-width" => {
                if i + 1 >= args.len() {
                    return Err(format!("Flag '{}' requires a value. Usage: {} <input_file> --reverb-width <0.0-1.0>", args[i], program_name).into());
                }
                reverb_width = args[i + 1].parse::<f32>().unwrap_or(0.9).clamp(0.0, 1.0);
                i += 2;
            }
            "--reverb-mix" => {
                if i + 1 >= args.len() {
                    return Err(format!(
                        "Flag '{}' requires a value. Usage: {} <input_file> --reverb-mix <0.0-1.0>",
                        args[i], program_name
                    )
                    .into());
                }
                reverb_mix = args[i + 1].parse::<f32>().unwrap_or(0.3).clamp(0.0, 1.0);
                i += 2;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                return Err("Invalid argument".into());
            }
        }
    }

    // Check output path if specified
    if let Some(ref output_path) = output_file {
        // Check if extension is valid by calling from_path
        audio_io::OutputFormat::from_path(Path::new(output_path))?;

        // Check if file already exists
        if Path::new(output_path).exists() {
            println!("Warning: Output file '{}' already exists.", output_path);
            print!("Do you want to overwrite it? (y/n): ");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();

            let mut response = String::new();
            std::io::stdin().read_line(&mut response)?;

            if !response.trim().eq_ignore_ascii_case("y") {
                println!("Operation cancelled.");
                return Ok(());
            }
        }

        // Check if parent directory exists
        if let Some(parent) = Path::new(output_path).parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                return Err(format!("Output directory does not exist: {:?}", parent).into());
            }
        }
    }

    println!("Loading input audio: {}", input_file);
    let input_samples = load_audio(Path::new(&input_file))?;

    let hrir_sphere = HrirSphere::new(HRIR_DATA, 44100)
        .map_err(|e| format!("Failed to load HRIR sphere: {:?}", e))?;

    println!("Configuration:");
    println!("  Pattern: {:?}", pattern);
    println!("  Start angle: {:.1}°", start_angle * 180.0 / PI);

    if velocity.is_oscillating() {
        println!(
            "  Velocity: {:.2} to {:.2} (osc speed: {:.2})",
            velocity.min, velocity.max, velocity_osc_speed
        );
    } else {
        println!("  Velocity: {:.2}", velocity.min);
    }

    if elevation.is_oscillating() {
        println!(
            "  Elevation: {:.1}° to {:.1}° (osc speed: {:.2})",
            elevation.min, elevation.max, elevation_osc_speed
        );
    } else {
        println!("  Elevation: {:.1}°", elevation.min);
    }

    if distance.is_oscillating() {
        println!(
            "  Distance: {:.2}m to {:.2}m (osc speed: {:.2})",
            distance.min, distance.max, distance_osc_speed
        );
    } else {
        println!("  Distance: {:.2}m", distance.min);
    }

    println!("  Crossover frequency: {:.1} Hz", crossover_freq);
    println!("  Bass boost: {:.1} dB", bass_boost_db);
    println!(
        "  Reverb room: {:.2}, mix: {:.2}\n",
        reverb_room_size, reverb_mix
    );

    let position_calc = PositionCalculator::new(
        pattern,
        start_angle,
        velocity,
        velocity_osc_speed,
        elevation,
        elevation_osc_speed,
        distance,
        distance_osc_speed,
    );

    let mut processor = Audio8DProcessor::new(
        hrir_sphere,
        44100,
        crossover_freq,
        reverb_room_size,
        reverb_dampening,
        reverb_width,
        reverb_mix,
        bass_boost_db,
    );

    let output_samples = processor.process_audio(
        &input_samples,
        position_calc,
        Some(&|progress| {
            print!("\rProcessing: {}%", (progress * 100.0) as u32);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }),
    );

    println!("\n\nNormalizing audio...");
    let max_sample = output_samples
        .iter()
        .map(|(l, r)| l.abs().max(r.abs()))
        .fold(0.0, f32::max);

    let normalized_output: Vec<(f32, f32)> = if max_sample > 0.001 {
        let target_level = 0.945;
        let gain = (target_level / max_sample).min(1.0);
        println!("Normalized to -0.5 dB (gain: {:.3}x)\n", gain);

        output_samples
            .iter()
            .map(|(l, r)| ((l * gain).clamp(-1.0, 1.0), (r * gain).clamp(-1.0, 1.0)))
            .collect()
    } else {
        output_samples
    };

    if let Some(output_path) = output_file {
        // File output mode
        println!("Saving output to: {}", output_path);
        save_audio(Path::new(&output_path), &normalized_output, 44100)?;
        println!("File saved successfully!");
    } else {
        // Real-time playback mode
        let player = Player::new(normalized_output)?;
        player.play()?;
    }

    Ok(())
}
