use crate::crossover::LinkwitzRileyCrossover;
use crate::reverb::ReverbProcessor;
use crate::ValueOrRange;
use biquad::*;
use clap::ValueEnum;
use hrtf::{HrirSphere, HrtfContext, HrtfProcessor, Vec3};
use std::f32::consts::TAU;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MovementPattern {
    Circular,
    Figure8,
    VerticalCircle,
    Helix,
    Random,
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
    velocity: ValueOrRange,
    velocity_osc_speed: f32,
    elevation: ValueOrRange,
    elevation_osc_speed: f32,
    distance: ValueOrRange,
    distance_osc_speed: f32,
    angle: f32,
    prev_pos: Vec3,
    prev_distance: f32,
}

impl PositionCalculator {
    pub fn new(
        pattern: MovementPattern,
        start_angle: f32,
        velocity: ValueOrRange,
        velocity_osc_speed: f32,
        elevation: ValueOrRange,
        elevation_osc_speed: f32,
        distance: ValueOrRange,
        distance_osc_speed: f32,
    ) -> Self {
        Self {
            pattern,
            time: 0.0,
            velocity,
            velocity_osc_speed,
            elevation,
            elevation_osc_speed,
            distance,
            distance_osc_speed,
            angle: start_angle.to_radians(),
            prev_pos: Vec3::new(0.0, 0.0, 1.0),
            prev_distance: distance.from,
        }
    }

    pub fn get_position(&mut self, dt: f32) -> (Vec3, f32) {
        self.time += dt;

        // Oscillating values
        let velocity = self.velocity.get_value(self.time, self.velocity_osc_speed) / 60.0;
        let distance = self.distance.get_value(self.time, self.distance_osc_speed);
        let elevation = self
            .elevation
            .get_value(self.time, self.elevation_osc_speed)
            .to_radians();

        self.angle += velocity * dt * TAU;

        let pos = match self.pattern {
            MovementPattern::Circular => {
                let x = self.angle.cos();
                let y = elevation.sin();
                let z = self.angle.sin();
                Vec3::new(x, y, z)
            }
            MovementPattern::Figure8 => {
                let x = self.angle.cos();
                let y = (self.angle * 2.0).sin() * 0.5;
                let z = self.angle.sin();
                Vec3::new(x, y, z)
            }
            MovementPattern::VerticalCircle => {
                let plane_rotation = elevation;
                let x = self.angle.sin() * plane_rotation.cos();
                let y = self.angle.cos();
                let z = self.angle.sin() * plane_rotation.sin();
                Vec3::new(x, y, z)
            }
            MovementPattern::Helix => {
                let y_progress = (self.angle / 5.0 / TAU) % 1.0; // 0 to 1 for each rotation

                // Oscillate back and forth using triangle wave
                let (y, direction) = if (y_progress * 2.0) as i32 % 2 == 0 {
                    (-1.0 + (y_progress % 0.5) * 4.0, 1.0) // Going up: -1 to 1
                } else {
                    (1.0 - ((y_progress - 0.5) % 0.5) * 4.0, -1.0) // Going down: 1 to -1
                };

                let angle = self.angle * direction;
                let x = angle.cos();
                let z = angle.sin();
                Vec3::new(x, y, z)
            }
            MovementPattern::Random => {
                let t = self.time * velocity;
                let x = t.sin() * 0.7 + (t * 3.7).sin() * 0.3;
                let y = (t * 1.3).sin() * 0.5;
                let z = (t * 2.1).cos();
                Vec3::new(x, y, z)
            }
        };

        self.prev_pos = pos;
        self.prev_distance = distance;

        (pos, distance)
    }
}

/// Audio processor for 8D conversion with crossover filtering
pub struct Audio8DProcessor {
    hrtf_processor: HrtfProcessor,
    sample_rate: u32,
    block_size: usize,
    crossover_left: LinkwitzRileyCrossover,
    crossover_right: LinkwitzRileyCrossover,
    reverb: ReverbProcessor,
    reverb_mix: f32,
    low_shelf: DirectForm2Transposed<f32>,
}

impl Audio8DProcessor {
    pub fn new(
        hrir_sphere: HrirSphere,
        sample_rate: u32,
        reverb_room_size: f32,
        reverb_dampening: f32,
        reverb_width: f32,
        reverb_mix: f32,
        bass_boost_db: f32,
    ) -> Self {
        let block_size = 512;
        let interpolation_steps = 8;

        let hrtf_processor = HrtfProcessor::new(hrir_sphere, interpolation_steps, block_size);
        let crossover_left = LinkwitzRileyCrossover::new(sample_rate, 80.0);
        let crossover_right = LinkwitzRileyCrossover::new(sample_rate, 80.0);
        let reverb = ReverbProcessor::new(
            sample_rate,
            reverb_room_size,
            reverb_dampening,
            reverb_width,
        );

        let shelf_coeffs = Coefficients::<f32>::from_params(
            Type::LowShelf(bass_boost_db),
            sample_rate.hz(),
            150.0.hz(),
            Q_BUTTERWORTH_F32,
        )
        .expect("Invalid low-shelf parameters");
        let low_shelf = DirectForm2Transposed::<f32>::new(shelf_coeffs);

        Self {
            hrtf_processor,
            sample_rate,
            block_size,
            crossover_left,
            crossover_right,
            reverb,
            reverb_mix,
            low_shelf,
        }
    }

    pub fn process_audio(
        &mut self,
        input_samples: &[(f32, f32)],
        mut position_calc: PositionCalculator,
        progress_callback: Option<&dyn Fn(f32)>,
    ) -> Vec<(f32, f32)> {
        let block_size = self.block_size;
        let interpolation_steps = 8;
        let chunk_size = interpolation_steps * block_size;

        let total_samples = input_samples.len();
        let mut output = vec![(0.0, 0.0); total_samples];

        self.crossover_left.reset_state();
        self.crossover_right.reset_state();
        self.reverb.reset_state();
        self.low_shelf.reset_state();

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
            let len = end_idx - start_idx;

            let mut bass_left = vec![0.0; chunk_size];
            let mut bass_right = vec![0.0; chunk_size];
            let mut mid_high = vec![0.0; chunk_size];
            for i in 0..len {
                let (l, r) = input_samples[start_idx + i];
                let (bass_l, mid_l) = self.crossover_left.process(l);
                let (bass_r, mid_r) = self.crossover_right.process(r);
                bass_left[i] = bass_l;
                bass_right[i] = bass_r;
                mid_high[i] = (mid_l + mid_r) * 0.5;
            }

            let (new_pos, new_distance) = position_calc.get_position(dt);

            let prev_pos_normalized = normalize_vec3(prev_pos);
            let new_pos_normalized = normalize_vec3(new_pos);

            let new_distance_gain = calculate_distance_gain(new_distance);
            let prev_distance_gain = calculate_distance_gain(prev_distance);

            let mut mid_high_output = vec![(0.0, 0.0); chunk_size];
            let mid_high_context = HrtfContext {
                source: &mid_high,
                output: &mut mid_high_output,
                new_sample_vector: new_pos_normalized,
                prev_sample_vector: prev_pos_normalized,
                prev_left_samples: &mut midhi_prev_left,
                prev_right_samples: &mut midhi_prev_right,
                new_distance_gain,
                prev_distance_gain,
            };

            self.hrtf_processor.process_samples(mid_high_context);

            for i in 0..len {
                let (mid_high_left, mid_high_right) = mid_high_output[i];

                let t = i as f32 / len as f32;
                let bass_gain = prev_distance_gain + (new_distance_gain - prev_distance_gain) * t;

                let left = mid_high_left + bass_left[i] * bass_gain;
                let right = mid_high_right + bass_right[i] * bass_gain;

                let (reverb_left, reverb_right) = self.reverb.process(left, right);
                let output_left = left * (1.0 - self.reverb_mix) + reverb_left * self.reverb_mix;
                let output_right = right * (1.0 - self.reverb_mix) + reverb_right * self.reverb_mix;

                output[start_idx + i] = (output_left, output_right);
            }

            prev_pos = new_pos;
            prev_distance = new_distance;
        }

        for (l, r) in output.iter_mut() {
            *l = self.low_shelf.run(*l);
            *r = self.low_shelf.run(*r);
        }

        output
    }
}
