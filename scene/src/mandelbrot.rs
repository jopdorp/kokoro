//! NVIDIA's CUDA Mandelbrot sample launch layout shared by the guest and host oracle.
use crate::particles::compute;

pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
pub const BLOCK: [u32; 3] = [16, 16, 1];
pub const GRID: [u32; 3] = [GRID_WIDTH * HEIGHT.div_ceil(BLOCK[1]), 1, 1];
pub const GRID_WIDTH: u32 = WIDTH.div_ceil(BLOCK[0]);
pub const BASE: u64 = 0x3000_0000;
pub const COMMANDS: u64 = BASE;
pub const PROGRAM: u64 = BASE + 0x1_0000;
pub const CBUF: u64 = BASE + 0x2_0000;
pub const QMD: u64 = BASE + 0x2_1000;
pub const TARGET: u64 = BASE + 0x10_0000;
pub const RELEASE: u64 = BASE + 0x9f_0000;
pub const MAPPED: u32 = 0xa0_0000;

pub fn scale(frame: u32) -> f64 {
    (0..frame.min(599)).fold(1.0e-4, |scale, _| scale * 0.9697156560467839)
}

pub fn view(frame: u32) -> (f64, f64, f64, u32) {
    let scale = scale(frame);
    (
        -0.743643887037151 - f64::from(WIDTH) * scale * 0.5,
        0.131825904205330 - f64::from(HEIGHT) * scale * 0.5,
        scale,
        512 + frame.min(599) * 2,
    )
}

pub fn qmd(payload: u32) -> [u32; 64] {
    let mut qmd = [0; 64];
    [qmd[4], qmd[5]] = [0x40, 0xa000_0000];
    [qmd[12], qmd[13], qmd[14]] = GRID;
    [qmd[18], qmd[19], qmd[20]] = [
        BLOCK[0] << 16 | 3 << 4,
        BLOCK[2] << 16 | BLOCK[1],
        31 << 8 | 1,
    ];
    [qmd[24], qmd[25], qmd[26]] = [
        RELEASE as u32,
        0x4088_0000 | (RELEASE >> 32) as u32,
        payload,
    ];
    [qmd[32], qmd[33]] = [CBUF as u32, (CBUF >> 32) as u32 | (0x1c0 >> 4) << 19];
    [qmd[48], qmd[49]] = [PROGRAM as u32, (PROGRAM >> 32) as u32];
    qmd
}

pub fn commands(words: &mut impl Extend<u32>) {
    compute(crate::host::SET_OBJECT, &[0xc7c0], words);
    compute(0x02b4, &[(QMD >> 8) as u32], words);
    compute(0x02c0, &[3], words);
}
