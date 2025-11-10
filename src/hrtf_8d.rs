use crate::crossover::LinkwitzRileyCrossover;
use crate::reverb::ReverbProcessor;
use hrtf::{HrirSphere, HrtfContext, HrtfProcessor, Vec3};
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy)]
pub enum MovementPattern {
    Circular,
    Figure8,
    Spiral,
    Random,
    VerticalCircle,
}

/// Range for oscillating parameters
#[derive(Debug, Clone, Copy)]
pub struct ParamValue {
    pub min: f32,
    pub max: f32,
}

impl ParamValue {
    pub fn new_fixed(value: f32) -> Self {
        Self {
            min: value,
            max: value,
        }
    }

    pub fn new_range(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    pub fn is_oscillating(&self) -> bool {
        (self.max - self.min).abs() > 1e-6
    }

    pub fn get_value(&self, time: f32, speed: f32) -> f32 {
        if !self.is_oscillating() {
            return self.min;
        }
        let t = (time * speed * 2.0 * PI).sin() * 0.5 + 0.5;
        self.min + (self.max - self.min) * t
    }
}

fn normalize_vec3(v: Vec3) -> Vec3 {
    let length = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if length > 0.0 {
        Vec3::new(v.x / length, v.y / length, v.z / length)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    }
}

fn calculate_distance_gain(distance: f32) -> f32 {
    let reference_distance = 1.0;
    let min_distance = 0.1;
    let d = distance.max(min_distance);
    let gain = f32::powf(reference_distance / d, 1.5);
    gain.min(2.0)
}

#[derive(Clone)]
pub struct PositionCalculator {
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
    pub fn new(
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

    pub fn get_position(&mut self, dt: f32) -> (Vec3, f32) {
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
                let spiral_progress = (self.time * current_velocity * 0.1).sin() * 0.5 + 0.5;
                let spiral_radius = current_distance * (0.2 + 0.8 * spiral_progress);
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
                let circle_angle =
                    self.time * self.elevation_osc_speed * 2.0 * PI + self.start_angle;
                let plane_rotation = self.time * current_velocity * 2.0 * PI;
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
pub struct Audio8DProcessor {
    hrtf_processor: HrtfProcessor,
    sample_rate: u32,
    block_size: usize,
    crossover: LinkwitzRileyCrossover,
    reverb: ReverbProcessor,
    reverb_mix: f32,
    bass_boost_gain: f32,
}

impl Audio8DProcessor {
    pub fn new(
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

    pub fn process_audio(
        &mut self,
        input_samples: &[f32],
        mut position_calc: PositionCalculator,
        progress_callback: Option<&dyn Fn(f32)>,
    ) -> Vec<(f32, f32)> {
        let block_size = self.block_size;
        let interpolation_steps = 8;
        let chunk_size = interpolation_steps * block_size;

        let total_samples = input_samples.len();
        let mut output = vec![(0.0, 0.0); total_samples];

        self.crossover.reset();
        self.reverb.reset();

        let mut midhi_prev_left = Vec::new();
        let mut midhi_prev_right = Vec::new();

        let dt = chunk_size as f32 / self.sample_rate as f32;
        let mut prev_pos = Vec3::new(0.0, 0.0, 1.0);
        let mut prev_distance = 1.0;

        let num_chunks = (total_samples + chunk_size - 1) / chunk_size;
        let total_samples_f = total_samples as f32;

        for chunk_idx in 0..num_chunks {
            if let Some(callback) = progress_callback {
                if chunk_idx % 10 == 0 || chunk_idx == num_chunks - 1 {
                    let progress = if chunk_idx == num_chunks - 1 {
                        1.0
                    } else {
                        (chunk_idx * chunk_size) as f32 / total_samples_f
                    };
                    callback(progress);
                }
            }

            let start_idx = chunk_idx * chunk_size;
            let end_idx = (start_idx + chunk_size).min(total_samples);

            let mut source_buffer = vec![0.0; chunk_size];
            let source_len = end_idx - start_idx;
            source_buffer[..source_len].copy_from_slice(&input_samples[start_idx..end_idx]);

            let mut bass_buffer = vec![0.0; chunk_size];
            let mut mid_high_buffer = vec![0.0; chunk_size];

            for (i, &sample) in source_buffer.iter().enumerate() {
                let (bass, high) = self.crossover.process(sample);
                bass_buffer[i] = bass * self.bass_boost_gain;
                mid_high_buffer[i] = high;
            }

            let (new_pos, new_distance) = position_calc.get_position(dt);

            let prev_pos_normalized = normalize_vec3(prev_pos);
            let new_pos_normalized = normalize_vec3(new_pos);

            let new_distance_gain = calculate_distance_gain(new_distance);
            let prev_distance_gain = calculate_distance_gain(prev_distance);

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

            let bass_gain_start = f32::powf(prev_distance_gain, 0.75);
            let bass_gain_end = f32::powf(new_distance_gain, 0.75);

            for i in 0..chunk_size {
                let (mid_high_left, mid_high_right) = mid_high_output[i];

                let t = i as f32 / chunk_size as f32;
                let bass_gain = bass_gain_start + (bass_gain_end - bass_gain_start) * t;

                let bass_mono = bass_buffer[i] * bass_gain;
                let bass_left = bass_mono;
                let bass_right = bass_mono;

                let left = mid_high_left + bass_left;
                let right = mid_high_right + bass_right;

                let (reverb_left, reverb_right) = self.reverb.process(left, right);
                let output_left = left * (1.0 - self.reverb_mix) + reverb_left * self.reverb_mix;
                let output_right = right * (1.0 - self.reverb_mix) + reverb_right * self.reverb_mix;

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
