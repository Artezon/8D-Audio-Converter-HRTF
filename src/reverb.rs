use freeverb::Freeverb;

pub struct ReverbProcessor {
    freeverb: Freeverb,
    sample_rate: usize,
}

impl ReverbProcessor {
    pub fn new(sample_rate: u32, room_size: f32, dampening: f32, width: f32) -> Self {
        let mut freeverb = Freeverb::new(sample_rate as usize);

        freeverb.set_room_size(room_size.clamp(0.0, 1.0) as f64);
        freeverb.set_dampening(dampening.clamp(0.0, 1.0) as f64);
        freeverb.set_width(width.clamp(0.0, 1.0) as f64);

        Self {
            freeverb,
            sample_rate: sample_rate as usize,
        }
    }

    pub fn process(&mut self, input_left: f32, input_right: f32) -> (f32, f32) {
        let (out_left, out_right) = self.freeverb.tick((input_left as f64, input_right as f64));
        (out_left as f32, out_right as f32)
    }

    pub fn reset(&mut self) {
        self.freeverb = Freeverb::new(self.sample_rate);
    }
}
