use crate::gpu::cubin::{Arg, Kernel};
use crate::tests::gpu::harness::{GLOBAL_BASE, Grid, sha256, shared_device};

const HALF: [usize; 2] = [960, 540];
const FULL: [usize; 2] = [1920, 1080];
const CUBIN: &[u8] = include_bytes!("../../../tests/fixtures/tensor-upscale-sm86.cubin");

fn reference(source: &[u8], mut target: Vec<u8>) -> Vec<u8> {
    let fp16 = |value: f32| {
        let bits = value.to_bits();
        if bits << 1 == 0 { return value; }
        let sign = bits & 0x8000_0000;
        let mut exponent = ((bits >> 23 & 0xff) as i32 - 112) as u32;
        let rounded = (bits & 0x7f_ffff) + 0xfff + (bits >> 13 & 1);
        let mut mantissa = rounded >> 13;
        if mantissa == 0x400 { exponent += 1; mantissa = 0; }
        f32::from_bits(sign | (exponent + 112) << 23 | mantissa << 13)
    };
    for y in 0..FULL[1] {
        for x in FULL[0] / 2 - 2..FULL[0] {
            let pixel = 4 * (y * FULL[0] + x);
            if x < FULL[0] / 2 + 2 {
                target[pixel..pixel + 4].fill(255);
                continue;
            }
            let (sx, sy, px, py) = (x / 2, y / 2, x & 1, y & 1);
            let weight = |tap: usize, phase: usize| match (tap, phase) {
                (1, 0) => 1.0,
                (1 | 2, 1) => 0.5625,
                (0 | 3, 1) => -0.0625,
                _ => 0.0,
            };
            for channel in 0..4 {
                let mut sum = 0.0;
                for ty in 0..4 {
                    let iy = (sy + ty).saturating_sub(1).min(HALF[1] - 1);
                    let horizontal = (0..4).map(|tx| {
                        let ix = (sx + tx).saturating_sub(1).min(HALF[0] - 1);
                        f32::from(source[4 * (iy * HALF[0] + ix) + channel]) * weight(tx, px)
                    }).sum();
                    sum += fp16(horizontal) * weight(ty, py);
                }
                target[pixel + channel] = sum.clamp(0.0, 255.0).round_ties_even() as u8;
            }
        }
    }
    target
}

#[test]
fn tensor_upscale_matches_the_catmull_rom_reference_in_both_modes() {
    let Some(state) = shared_device() else { return };
    let source: Vec<_> = (0..HALF[0] * HALF[1]).flat_map(|i| {
        let (x, y) = (i % HALF[0], i / HALF[0]);
        [(x ^ y) as u8, (3 * x + y) as u8, (x + 5 * y) as u8, 255]
    }).collect();
    let target: Vec<_> = (0..FULL[0] * FULL[1]).flat_map(|i| {
        let (x, y) = (i % FULL[0], i / FULL[0]);
        [x as u8, y as u8, (x + y) as u8, 255]
    }).collect();
    let expected = reference(&source, target.clone());
    let kernel = Kernel::load(CUBIN, "tensor_upscale").unwrap();
    let args = [Arg::Buffer(&source), Arg::Buffer(&target)];
    let (banks, regions) = kernel.launch([120, 68, 1], [32, 1, 1], &args, GLOBAL_BASE);
    for fast in [false, true] {
        crate::gpu::SPEED_HACKS.lock().unwrap().fast_tensor = fast;
        let mut grid = Grid::new(state, &kernel.words, [32, 1, 1], &banks, &regions);
        let actual = grid.run([120, 68, 1]).pop().unwrap();
        let maximum = actual.iter().zip(&expected).map(|(&a, &b)| a.abs_diff(b)).max().unwrap();
        assert!(maximum <= 1, "fast={fast}: maximum channel error {maximum}");
        assert_eq!(sha256(&actual), sha256(&expected), "fast={fast}: digest differs despite {maximum} channel error");
        println!("tensor upscale fast={fast}: {:.3} ms, sha256 {}", state.last_gpu_time_ns() as f64 / 1e6, sha256(&actual));
    }
    crate::gpu::SPEED_HACKS.lock().unwrap().fast_tensor = true;
    let mut guest = crate::guest::Machine::demo(crate::guest::Demo::TensorUpscale).unwrap();
    guest.frame().unwrap();
    let mut frame = crate::display::DISPLAY.lock().unwrap();
    assert_eq!((frame.width, frame.height), (FULL[0] as u32, FULL[1] as u32));
    let pixels = frame.pixels();
    assert!((0..FULL[1]).all(|y| pixels[4 * (y * FULL[0] + FULL[0] / 2 - 2)..][..16] == [255; 16]));
    println!("guest tensor upscale sha256 {}", sha256(pixels));
    crate::gpu::SPEED_HACKS.lock().unwrap().fast_tensor = false;
}
