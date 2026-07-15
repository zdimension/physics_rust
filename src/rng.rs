use bevy::prelude::Component;

pub trait DelegatedRng {
    fn f32(&mut self) -> f32;
}

#[derive(Component)]
pub struct RngComponent {
    state: u64,
}

impl Default for RngComponent {
    fn default() -> Self {
        Self { state: 0x4d59_5df4_d0f3_3173 }
    }
}

impl RngComponent {
    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        ((x.wrapping_mul(0x2545_f491_4f6c_dd1d)) >> 32) as u32
    }
}

impl DelegatedRng for RngComponent {
    fn f32(&mut self) -> f32 {
        self.next_u32() as f32 / (u32::MAX as f32 + 1.0)
    }
}