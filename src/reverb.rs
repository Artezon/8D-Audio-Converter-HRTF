use freeverb::Freeverb;

pub struct ReverbProcessor {
    freeverb_left: Freeverb,
    freeverb_right: Freeverb,
}

impl ReverbProcessor {
    pub fn new(_sample_rate: u32, room_size: f32, dampening: f32, width: f32) -> Self {
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

    pub fn process(&mut self, input_left: f32, input_right: f32) -> (f32, f32) {
        let (out_left, _) = self
            .freeverb_left
            .tick((input_left as f64, input_right as f64));
        let (_, out_right) = self
            .freeverb_right
            .tick((input_left as f64, input_right as f64));
        (out_left as f32, out_right as f32)
    }

    pub fn reset(&mut self) {
        self.freeverb_left = Freeverb::new(44100);
        self.freeverb_right = Freeverb::new(44100);
    }
}