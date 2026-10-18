use super::{BLOCK, V3, asin, atan2, cross, delayed, dot, length, lerp, scale, sub, unit};
use core::f32::consts::{FRAC_PI_2, PI};

pub const TAPS: usize = 160;
const AZIMUTHS: usize = 24;
const ELEVATIONS: usize = 13;
const DATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../demos/assets/hrir/d1.bin"
));

#[derive(Clone, Copy, Default)]
pub struct Direction {
    indices: [usize; 4],
    weights: [f32; 4],
}

pub struct Filter([[f32; TAPS]; 2]);

impl Direction {
    pub const ZERO: Self = Self {
        indices: [0; 4],
        weights: [0.0; 4],
    };

    pub fn new(position: V3, listener: V3, forward: V3, up: V3) -> Self {
        let offset = sub(position, listener);
        let direction = scale(offset, 1.0 / length(offset).max(1e-9));
        let mut azimuth = -atan2(
            dot(direction, unit(cross(forward, up))),
            dot(direction, forward),
        ) * 12.0
            / PI;
        if azimuth < 0.0 {
            azimuth += AZIMUTHS as f32;
        }
        let elevation = (asin(dot(direction, up).clamp(-1.0, 1.0)) * 6.0 / FRAC_PI_2 + 6.0)
            .clamp(0.0, (ELEVATIONS - 1) as f32);
        let (az0, el0) = (azimuth as usize, elevation as usize);
        let (az1, el1) = ((az0 + 1) % AZIMUTHS, (el0 + 1).min(ELEVATIONS - 1));
        let (az, el) = (azimuth - az0 as f32, elevation - el0 as f32);
        Self {
            indices: [
                el0 * AZIMUTHS + az0,
                el0 * AZIMUTHS + az1,
                el1 * AZIMUTHS + az0,
                el1 * AZIMUTHS + az1,
            ],
            weights: [
                (1.0 - az) * (1.0 - el),
                az * (1.0 - el),
                (1.0 - az) * el,
                az * el,
            ],
        }
    }

    pub fn filter(self) -> Filter {
        let mut filter = Filter([[0.0; TAPS]; 2]);
        for ear in 0..2 {
            for tap in 0..TAPS {
                filter.0[ear][tap] = self
                    .indices
                    .into_iter()
                    .zip(self.weights)
                    .map(|(direction, weight)| coefficient(direction, ear, tap) * weight)
                    .sum();
            }
        }
        filter
    }
}

impl Filter {
    pub fn render(
        &self,
        history: &[f32],
        written: u64,
        from_delay: f32,
        delay: f32,
        out: &mut [[f32; BLOCK]; 2],
    ) {
        let mut input = [0.0; BLOCK + TAPS - 1];
        for (index, sample) in input.iter_mut().enumerate() {
            let offset = index as isize - TAPS as isize + 1;
            let at = written as isize + offset;
            if at >= 0 {
                let t = ((offset + 1) as f32 / BLOCK as f32).clamp(0.0, 1.0);
                *sample = delayed(history, at as u64, lerp(from_delay, delay, t));
            }
        }
        for (ear, output) in out.iter_mut().enumerate() {
            for (tap, &coefficient) in self.0[ear].iter().enumerate() {
                for (sample, &input) in output.iter_mut().zip(&input[TAPS - 1 - tap..]) {
                    *sample += input * coefficient;
                }
            }
        }
    }
}

fn coefficient(direction: usize, ear: usize, tap: usize) -> f32 {
    let at = ((direction * 2 + ear) * TAPS + tap) * 2;
    i16::from_le_bytes([DATA[at], DATA[at + 1]]) as f32 / 32767.0
}
