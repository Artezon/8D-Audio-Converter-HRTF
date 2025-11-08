use biquad::*;
use freeverb::Freeverb;
use hound::{SampleFormat, WavReader, WavWriter};
use hrtf::{HrirSphere, HrtfContext, HrtfProcessor, Vec3};
use std::env;
use std::f32::consts::PI;
use std::path::Path;

/// Different 8D movement patterns for audio positioning
#[derive(Debug)]
enum MovementPattern {
    Circular,
    Figure8,
    Spiral,
    Random,
    VerticalCircle,
}

/// Range for oscillating parameters
#[derive(Debug, Clone, Copy)]
struct ParamValue {
    min: f32,
    max: f32,
}

impl ParamValue {
    fn new_fixed(value: f32) -> Self {
        Self {
            min: value,
            max: value,
        }
    }

    fn new_range(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    fn is_oscillating(&self) -> bool {
        (self.max - self.min).abs() > 1e-6
    }

    fn get_value(&self, time: f32, speed: f32) -> f32 {
        if !self.is_oscillating() {
            return self.min;
        }
        let t = (time * speed * 2.0 * PI).sin() * 0.5 + 0.5;
        self.min + (self.max - self.min) * t
    }
}

/// Linkwitz-Riley crossover filter (4th order, -24dB/octave)
/// Creates perfectly flat frequency response when bass and mid-high are summed
struct LinkwitzRileyCrossover {
    // Two cascaded 2nd order Butterworth filters = 4th order LR
    bass_stage1: DirectForm2Transposed<f32>,
    bass_stage2: DirectForm2Transposed<f32>,
    high_stage1: DirectForm2Transposed<f32>,
    high_stage2: DirectForm2Transposed<f32>,
}

impl LinkwitzRileyCrossover {
    fn new(sample_rate: u32, crossover_freq: f32) -> Self {
        let fs = sample_rate as f32;
        let f0 = crossover_freq.hz();

        // Create Butterworth low-pass coefficients (Q = 0.707 for Butterworth)
        let lowpass_coeffs =
            Coefficients::<f32>::from_params(Type::LowPass, fs.hz(), f0, Q_BUTTERWORTH_F32)
                .unwrap();

        // Create Butterworth high-pass coefficients
        let highpass_coeffs =
            Coefficients::<f32>::from_params(Type::HighPass, fs.hz(), f0, Q_BUTTERWORTH_F32)
                .unwrap();

        Self {
            bass_stage1: DirectForm2Transposed::<f32>::new(lowpass_coeffs),
            bass_stage2: DirectForm2Transposed::<f32>::new(lowpass_coeffs),
            high_stage1: DirectForm2Transposed::<f32>::new(highpass_coeffs),
            high_stage2: DirectForm2Transposed::<f32>::new(highpass_coeffs),
        }
    }

    fn process(&mut self, input: f32) -> (f32, f32) {
        // Process through cascaded filters for 4th order response
        let bass = self.bass_stage2.run(self.bass_stage1.run(input));
        let high = self.high_stage2.run(self.high_stage1.run(input));
        (bass, high)
    }

    fn reset(&mut self) {
        // Reset filter states.
        self.bass_stage1.reset_state();
        self.bass_stage2.reset_state();
        self.high_stage1.reset_state();
        self.high_stage2.reset_state();
    }
}

struct ReverbProcessor {
    freeverb_left: Freeverb,
    freeverb_right: Freeverb,
}

impl ReverbProcessor {
    fn new(_sample_rate: u32, room_size: f32, dampening: f32, width: f32) -> Self {
        let mut freeverb_left = Freeverb::new(44100);
        let mut freeverb_right = Freeverb::new(44100);

        freeverb_left.set_room_size(room_size.clamp(0.0, 1.0) as f64);
        freeverb_right.set_room_size(room_size.clamp(0.0, 1.0) as f64);
        freeverb_left.set_dampening(dampening.clamp(0.0, 1.0) as f64);
        freeverb_right.set_dampening(dampening.clamp(0.0, 1.0) as f64);
        freeverb_left.set_width(width.clamp(0.0, 1.0) as f64);
        freeverb_right.set_width(width.clamp(0.0, 1.0) as f64);

        Self {
            freeverb_left,
            freeverb_right,
        }
    }

    fn process(&mut self, input_left: f32, input_right: f32) -> (f32, f32) {
        let (out_left, _) = self
            .freeverb_left
            .tick((input_left as f64, input_right as f64));
        let (_, out_right) = self
            .freeverb_right
            .tick((input_left as f64, input_right as f64));
        (out_left as f32, out_right as f32)
    }

    fn reset(&mut self) {
        self.freeverb_left = Freeverb::new(44100);
        self.freeverb_right = Freeverb::new(44100);
    }
}

/// Helper function to normalize a Vec3
fn normalize_vec3(v: Vec3) -> Vec3 {
    let length = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if length > 0.0 {
        Vec3::new(v.x / length, v.y / length, v.z / length)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    }
}

/// Calculate distance-based gain using inverse square law
fn calculate_distance_gain(distance: f32) -> f32 {
    // Reference distance where gain = 1.0
    let reference_distance = 1.0;
    // Minimum distance to prevent extreme gain at very close distances
    let min_distance = 0.1;

    let d = distance.max(min_distance);

    // Inverse square law: gain = (reference / distance)^2
    // let gain = f32::powi(reference_distance / (d * 0.25).max(1.0), 2);
    let gain = f32::powf(reference_distance / d, 1.5);

    gain.min(2.0)
}

/// Position calculator for different movement patterns with oscillating parameters
struct PositionCalculator {
    pattern: MovementPattern,
    time: f32,
    start_angle: f32,
    velocity: ParamValue,
    velocity_osc_speed: f32,
    elevation: ParamValue,
    elevation_osc_speed: f32,
    distance: ParamValue,
    distance_osc_speed: f32,
    prev_pos: Vec3,
    prev_distance: f32,
}

impl PositionCalculator {
    fn new(
        pattern: MovementPattern,
        start_angle: f32,
        velocity: ParamValue,
        velocity_osc_speed: f32,
        elevation: ParamValue,
        elevation_osc_speed: f32,
        distance: ParamValue,
        distance_osc_speed: f32,
    ) -> Self {
        Self {
            pattern,
            time: 0.0,
            start_angle,
            velocity,
            velocity_osc_speed,
            elevation,
            elevation_osc_speed,
            distance,
            distance_osc_speed,
            prev_pos: Vec3::new(0.0, 0.0, 1.0),
            prev_distance: distance.min,
        }
    }

    fn get_position(&mut self, dt: f32) -> (Vec3, f32) {
        self.time += dt;

        let current_velocity = self.velocity.get_value(self.time, self.velocity_osc_speed);
        let current_distance = self.distance.get_value(self.time, self.distance_osc_speed);
        let current_elevation_deg = self
            .elevation
            .get_value(self.time, self.elevation_osc_speed);
        let current_elevation_rad = current_elevation_deg * PI / 180.0;

        let current_pos = match self.pattern {
            MovementPattern::Circular => {
                let angle = self.time * current_velocity * 2.0 * PI + self.start_angle;
                let x = angle.cos() * current_distance * current_elevation_rad.cos();
                let z = angle.sin() * current_distance * current_elevation_rad.cos();
                let y = current_elevation_rad.sin() * current_distance;
                Vec3::new(x, y, z)
            }
            MovementPattern::Figure8 => {
                let angle = self.time * current_velocity * PI + self.start_angle;
                let x = angle.cos() * current_distance * current_elevation_rad.cos();
                let y_pattern =
                    (self.time * current_velocity * 2.0 * PI).sin() * current_distance * 0.5;
                let z = angle.sin() * current_distance * current_elevation_rad.cos();
                let y = y_pattern + current_elevation_rad.sin() * current_distance;
                Vec3::new(x, y, z)
            }
            MovementPattern::Spiral => {
                // Smooth spiral with many turns - oscillates radius over longer period
                let spiral_progress = (self.time * current_velocity * 0.1).sin() * 0.5 + 0.5; // Slow oscillation = more turns
                let spiral_radius = current_distance * (0.2 + 0.8 * spiral_progress); // Keep minimum radius
                let angle = self.time * current_velocity * 4.0 * PI + self.start_angle;
                let x = angle.cos() * spiral_radius * current_elevation_rad.cos();
                let z = angle.sin() * spiral_radius * current_elevation_rad.cos();
                let y = (self.time * current_velocity * 2.0 * PI).sin() * current_distance * 0.5
                    + current_elevation_rad.sin() * current_distance;
                Vec3::new(x, y, z)
            }
            MovementPattern::Random => {
                let t = self.time * current_velocity;
                let x = (t.sin() * 0.7 + (t * 3.7).sin() * 0.3)
                    * current_distance
                    * current_elevation_rad.cos();
                let y_pattern = (t * 1.3).sin() * current_distance * 0.5;
                let z = (t * 2.1).cos() * current_distance * current_elevation_rad.cos();
                let y = y_pattern + current_elevation_rad.sin() * current_distance;
                Vec3::new(x, y, z)
            }
            MovementPattern::VerticalCircle => {
                // Pure vertical circle that completes based on elevation-osc-speed
                // Velocity rotates the entire circle plane around the Y-axis
                let circle_angle =
                    self.time * self.elevation_osc_speed * 2.0 * PI + self.start_angle;

                // Rotation of the circle plane based on velocity
                let plane_rotation = self.time * current_velocity * 2.0 * PI;

                // Circle in rotated plane
                let x = circle_angle.sin() * current_distance * plane_rotation.cos();
                let y = circle_angle.cos() * current_distance;
                let z = circle_angle.sin() * current_distance * plane_rotation.sin();

                Vec3::new(x, y, z)
            }
        };

        self.prev_pos = current_pos;
        self.prev_distance = current_distance;

        (current_pos, current_distance)
    }
}

/// Audio processor for 8D conversion with crossover filtering
struct Audio8DProcessor {
    hrtf_processor: HrtfProcessor,
    sample_rate: u32,
    block_size: usize,
    crossover: LinkwitzRileyCrossover,
    reverb: ReverbProcessor,
    reverb_mix: f32,
    bass_boost_gain: f32,
}

impl Audio8DProcessor {
    fn new(
        hrir_sphere: HrirSphere,
        sample_rate: u32,
        crossover_freq: f32,
        reverb_room_size: f32,
        reverb_dampening: f32,
        reverb_width: f32,
        reverb_mix: f32,
        bass_boost_db: f32,
    ) -> Self {
        let block_size = 512;
        let interpolation_steps = 8;

        let hrtf_processor = HrtfProcessor::new(hrir_sphere, interpolation_steps, block_size);
        let crossover = LinkwitzRileyCrossover::new(sample_rate, crossover_freq);
        let reverb = ReverbProcessor::new(
            sample_rate,
            reverb_room_size,
            reverb_dampening,
            reverb_width,
        );

        Self {
            hrtf_processor,
            sample_rate,
            block_size,
            crossover,
            reverb,
            reverb_mix,
            bass_boost_gain: 10.0f32.powf(bass_boost_db / 20.0),
        }
    }

    fn process_audio(
        &mut self,
        input_samples: &[f32],
        mut position_calc: PositionCalculator,
    ) -> Vec<(f32, f32)> {
        let block_size = self.block_size;
        let interpolation_steps = 8;
        let chunk_size = interpolation_steps * block_size;

        let total_samples = input_samples.len();
        let mut output = vec![(0.0, 0.0); total_samples];

        // Reset filters
        self.crossover.reset();
        self.reverb.reset();

        // Separate HRTF state for mid/high processing only
        let mut midhi_prev_left = Vec::new();
        let mut midhi_prev_right = Vec::new();

        let dt = chunk_size as f32 / self.sample_rate as f32;
        let mut prev_pos = Vec3::new(0.0, 0.0, 1.0);
        let mut prev_distance = 1.0;

        let num_chunks = (total_samples + chunk_size - 1) / chunk_size;
        let total_samples_f = total_samples as f32;

        for chunk_idx in 0..num_chunks {
            if chunk_idx % 10 == 0 || chunk_idx == num_chunks - 1 {
                let progress = (chunk_idx * chunk_size) as f32 / total_samples_f * 100.0;
                print!("\rProcessing: {:.1}%", progress.min(100.0));
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
            }

            let start_idx = chunk_idx * chunk_size;
            let end_idx = (start_idx + chunk_size).min(total_samples);

            let mut source_buffer = vec![0.0; chunk_size];
            let source_len = end_idx - start_idx;
            source_buffer[..source_len].copy_from_slice(&input_samples[start_idx..end_idx]);

            // Split into bass and mid/high using crossover filter
            let mut bass_buffer = vec![0.0; chunk_size];
            let mut mid_high_buffer = vec![0.0; chunk_size];

            for (i, &sample) in source_buffer.iter().enumerate() {
                let (bass, high) = self.crossover.process(sample);
                bass_buffer[i] = bass * self.bass_boost_gain;
                mid_high_buffer[i] = high;
            }

            // Calculate positions and distances for this chunk
            let (new_pos, new_distance) = position_calc.get_position(dt);

            let prev_pos_normalized = normalize_vec3(prev_pos);
            let new_pos_normalized = normalize_vec3(new_pos);

            // Calculate distance gains using inverse square law
            let new_distance_gain = calculate_distance_gain(new_distance);
            let prev_distance_gain = calculate_distance_gain(prev_distance);

            // Process mid/high frequencies with HRTF and distance attenuation
            let mut mid_high_output = vec![(0.0, 0.0); chunk_size];
            let mid_high_context = HrtfContext {
                source: &mid_high_buffer,
                output: &mut mid_high_output,
                new_sample_vector: new_pos_normalized,
                prev_sample_vector: prev_pos_normalized,
                prev_left_samples: &mut midhi_prev_left,
                prev_right_samples: &mut midhi_prev_right,
                new_distance_gain,
                prev_distance_gain,
            };

            self.hrtf_processor.process_samples(mid_high_context);

            // Calculate bass distance gain (interpolate across chunk)
            let bass_gain_start = f32::powf(prev_distance_gain, 0.75);
            let bass_gain_end = f32::powf(new_distance_gain, 0.75);

            // Combine bass (with distance attenuation) and mid/high, then add reverb
            for i in 0..chunk_size {
                let (mid_high_left, mid_high_right) = mid_high_output[i];

                // Interpolate bass gain across the chunk
                let t = i as f32 / chunk_size as f32;
                let bass_gain = bass_gain_start + (bass_gain_end - bass_gain_start) * t;

                // Bass is played as-is (centered, no spatial processing) but with distance attenuation
                let bass_mono = bass_buffer[i] * bass_gain;
                let bass_left = bass_mono;
                let bass_right = bass_mono;

                // Mix frequencies together
                let left = mid_high_left + bass_left;
                let right = mid_high_right + bass_right;

                // Apply reverb
                let (reverb_left, reverb_right) = self.reverb.process(left, right);
                let output_left = left * (1.0 - self.reverb_mix) + reverb_left * self.reverb_mix;
                let output_right = right * (1.0 - self.reverb_mix) + reverb_right * self.reverb_mix;

                // Final output
                if start_idx + i < total_samples {
                    output[start_idx + i] = (output_left, output_right);
                }
            }

            prev_pos = new_pos;
            prev_distance = new_distance;
        }

        output
    }
}

fn load_mono_audio(path: &Path) -> Result<(Vec<f32>, u32), Box<dyn std::error::Error>> {
    let reader = WavReader::open(path)?;
    let spec = reader.spec();

    if spec.channels != 1 {
        return Err("Input audio must be mono".into());
    }

    let samples: Vec<f32> = reader
        .into_samples::<i16>()
        .map(|s| s.unwrap_or(0) as f32 / 32768.0)
        .collect();

    Ok((samples, spec.sample_rate))
}

fn save_stereo_audio(
    path: &Path,
    samples: &[(f32, f32)],
    sample_rate: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let mut writer = WavWriter::create(path, spec)?;

    for (left, right) in samples {
        let gain = 32767.0;
        let left_sample = (left.clamp(-1.0, 1.0) * gain) as i16;
        let right_sample = (right.clamp(-1.0, 1.0) * gain) as i16;

        writer.write_sample(left_sample)?;
        writer.write_sample(right_sample)?;
    }

    writer.finalize()?;
    Ok(())
}

fn parse_range_value(s: &str) -> Result<ParamValue, String> {
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

fn print_usage(program_name: &str) {
    println!("Binaural 8D Audio Converter");
    println!("Usage: {} <input_file> [options]", program_name);
    println!("\nRequired:");
    println!("  <input_file>              Input mono WAV file");
    println!("\nOptions:");
    println!("  -o, --output <file>       Output stereo WAV file (default: output_8d.wav)");
    println!("  -p, --pattern <pattern>   Movement pattern (circular, figure8, spiral, random, vertical)");
    println!("                            Default: circular");
    println!("  --start-angle <degrees>   Starting angle in degrees (default: 0)");
    println!("  --velocity <value|min,max> Movement velocity (default: 0.2)");
    println!("                            Single value: constant velocity");
    println!("                            Two values: oscillates between min and max");
    println!("  --velocity-osc-speed <value> Velocity oscillation speed (default: 0.1)");
    println!("  --elevation <deg|min,max> Elevation in degrees, -90 to 90 (default: 0)");
    println!("                            0 = ear level, positive = above, negative = below");
    println!("                            Single value: constant elevation");
    println!("                            Two values: oscillates between min and max");
    println!("  --elevation-osc-speed <value> Elevation oscillation speed (default: 0.1)");
    println!("  --distance <meters|min,max> Distance/radius in meters (default: 1.0)");
    println!("                            Single value: constant distance");
    println!("                            Two values: oscillates between min and max");
    println!("  --distance-osc-speed <value> Distance oscillation speed (default: 0.1)");
    println!("  --crossover <value>       Crossover frequency in Hz (50-500, default: 200.0)");
    println!("  --bass-boost <value>      Bass boost in decibels (-20.0 to +20.0, default: 0.0)");
    println!("  --help                    Show this help message");
    println!("\nReverb Options:");
    println!("  --reverb-room <value>      Room size (0.0-1.0, default: 0.5)");
    println!("                             0.3=tiny, 0.5=small, 0.75=medium, 0.9=large hall");
    println!("  --reverb-dampening <value> High-frequency dampening (0.0-1.0, default: 0.5)");
    println!("                             0.2=bright/hard, 0.5=neutral, 0.8=dark/soft");
    println!("  --reverb-width <value>     Stereo width (0.0-1.0, default: 0.9)");
    println!("                             1.0=wide stereo, 0.5=narrow, 0.0=mono");
    println!("  --reverb-mix <value>       Reverb mix amount (0.0-1.0, default: 0.3)");
    println!("                             0.0=dry, 0.5=equal mix, 1.0=fully wet");
    println!("\nExamples:");
    println!("  {} input.wav", program_name);
    println!(
        "  {} input.wav -o output.wav --pattern figure8",
        program_name
    );
    println!(
        "  {} input.wav --velocity 0.1,0.5 --elevation -30,30",
        program_name
    );
    println!(
        "  {} input.wav --distance 0.5,3 --start-angle 90",
        program_name
    );
    println!("\nNotes:");
    println!("  - Bass frequencies are NOT processed with HRTF (remain centered/omnidirectional)");
    println!("  - Mid/high frequencies receive full 8D spatial processing");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let program_name = args
        .get(0)
        .map(|s| s.as_str())
        .unwrap_or("<executable_name>");

    if args.len() < 2 || args.contains(&"--help".to_string()) {
        print_usage(program_name);
        return Ok(());
    }

    let hrir_file = "IRC_1002_C.bin";
    let input_file = args[1].clone();
    let mut output_file = "output_8d.wav".to_string();
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
                output_file = args[i + 1].clone();
                i += 2;
            }
            "-p" | "--pattern" => {
                pattern = match args[i + 1].to_lowercase().as_str() {
                    "circular" => MovementPattern::Circular,
                    "figure8" => MovementPattern::Figure8,
                    "spiral" => MovementPattern::Spiral,
                    "random" => MovementPattern::Random,
                    "vertical" => MovementPattern::VerticalCircle,
                    _ => {
                        eprintln!("Unknown pattern: {}", args[i + 1]);
                        print_usage(program_name);
                        return Err("Invalid pattern".into());
                    }
                };
                i += 2;
            }
            "--start-angle" => {
                start_angle = args[i + 1].parse::<f32>().unwrap_or(0.0) * PI / 180.0;
                i += 2;
            }
            "--velocity" => {
                velocity = parse_range_value(&args[i + 1])?;
                i += 2;
            }
            "--velocity-osc-speed" => {
                velocity_osc_speed = args[i + 1].parse::<f32>().unwrap_or(0.1);
                i += 2;
            }
            "--elevation" => {
                let elev = parse_range_value(&args[i + 1])?;
                elevation =
                    ParamValue::new_range(elev.min.clamp(-90.0, 90.0), elev.max.clamp(-90.0, 90.0));
                i += 2;
            }
            "--elevation-osc-speed" => {
                elevation_osc_speed = args[i + 1].parse::<f32>().unwrap_or(0.1);
                i += 2;
            }
            "--distance" => {
                distance = parse_range_value(&args[i + 1])?;
                i += 2;
            }
            "--distance-osc-speed" => {
                distance_osc_speed = args[i + 1].parse::<f32>().unwrap_or(0.1);
                i += 2;
            }
            "--crossover" => {
                crossover_freq = args[i + 1]
                    .parse::<f32>()
                    .unwrap_or(200.0)
                    .clamp(50.0, 500.0);
                i += 2;
            }
            "--bass-boost" => {
                bass_boost_db = args[i + 1].parse::<f32>().unwrap_or(0.0).clamp(-20.0, 20.0);
                i += 2;
            }
            "--reverb-room" => {
                reverb_room_size = args[i + 1].parse::<f32>().unwrap_or(0.5).clamp(0.0, 1.0);
                i += 2;
            }
            "--reverb-dampening" => {
                reverb_dampening = args[i + 1].parse::<f32>().unwrap_or(0.5).clamp(0.0, 1.0);
                i += 2;
            }
            "--reverb-width" => {
                reverb_width = args[i + 1].parse::<f32>().unwrap_or(0.9).clamp(0.0, 1.0);
                i += 2;
            }
            "--reverb-mix" => {
                reverb_mix = args[i + 1].parse::<f32>().unwrap_or(0.3).clamp(0.0, 1.0);
                i += 2;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_usage(program_name);
                return Err("Invalid argument".into());
            }
        }
    }

    println!("Binaural 8D Audio Converter");
    println!("===========================\n");

    println!("Loading HRIR sphere from: {}", hrir_file);
    let hrir_sphere = HrirSphere::from_file(hrir_file, 44100)
        .map_err(|e| format!("Failed to load HRIR sphere: {:?}", e))?;
    println!("✓ HRIR sphere loaded\n");

    println!("Loading input audio: {}", input_file);
    let (input_samples, sample_rate) = load_mono_audio(Path::new(&input_file))?;
    println!(
        "✓ Loaded {} samples at {} Hz\n",
        input_samples.len(),
        sample_rate
    );

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

    let mut processor = Audio8DProcessor::new(
        hrir_sphere,
        sample_rate,
        crossover_freq,
        reverb_room_size,
        reverb_dampening,
        reverb_width,
        reverb_mix,
        bass_boost_db,
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

    let output_samples = processor.process_audio(&input_samples, position_calc);

    println!("\n\nNormalizing audio...");
    let max_sample = output_samples
        .iter()
        .map(|(l, r)| l.abs().max(r.abs()))
        .fold(0.0, f32::max);

    let normalized_output: Vec<(f32, f32)> = if max_sample > 0.001 {
        let target_level = 0.945; // -0.5 dB
        let gain = (target_level / max_sample).min(1.0);
        println!("✓ Normalized to -0.5 dB (gain: {:.3}x)\n", gain);

        output_samples
            .iter()
            .map(|(l, r)| ((l * gain).clamp(-1.0, 1.0), (r * gain).clamp(-1.0, 1.0)))
            .collect()
    } else {
        output_samples
    };

    println!("Saving output to: {}", output_file);
    save_stereo_audio(Path::new(&output_file), &normalized_output, sample_rate)?;

    println!("✓ 8D audio conversion completed successfully!");
    println!("  Output: {} samples\n", normalized_output.len());

    Ok(())
}
