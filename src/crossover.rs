use biquad::*;

/// Linkwitz-Riley crossover filter (4th order, -24dB/octave)
pub struct LinkwitzRileyCrossover {
    bass_stage1: DirectForm2Transposed<f32>,
    bass_stage2: DirectForm2Transposed<f32>,
    high_stage1: DirectForm2Transposed<f32>,
    high_stage2: DirectForm2Transposed<f32>,
}

impl LinkwitzRileyCrossover {
    pub fn new(sample_rate: u32, crossover_freq: f32) -> Self {
        let fs = sample_rate as f32;
        let f0 = crossover_freq.hz();

        let lowpass_coeffs =
            Coefficients::<f32>::from_params(Type::LowPass, fs.hz(), f0, Q_BUTTERWORTH_F32)
                .unwrap();

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

    pub fn process(&mut self, input: f32) -> (f32, f32) {
        let bass = self.bass_stage2.run(self.bass_stage1.run(input));
        let high = self.high_stage2.run(self.high_stage1.run(input));
        (bass, high)
    }

    pub fn reset(&mut self) {
        self.bass_stage1.reset_state();
        self.bass_stage2.reset_state();
        self.high_stage1.reset_state();
        self.high_stage2.reset_state();
    }
}