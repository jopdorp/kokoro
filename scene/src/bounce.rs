//! Bounce physics and command encoding shared by the host oracle and guest.
use crate::{host::SET_OBJECT, inc, threed::*};
pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
pub const BALLS: usize = 12;
pub const HALF: [f32; 2] = [1.6, 0.85];
const VIEW: [f32; 2] = [1.75, 0.984_375];
pub const GRAVITY: f32 = 3.0;
const STEP: f32 = 1.0 / 200.0;
const RESTITUTION: f32 = 0.8;
pub const SLOP: f32 = 0.001;
const CORRECTION: f32 = 0.8;
const SOLVER_ITERATIONS: usize = 16;
/// Impacts slower than this are a ball resting on another, not a knock.
pub const SOFT: f32 = 0.4;
/// The largest quad half size, which particles.vert reads from camera_right.w; smaller ones ride in position.w.
const LARGEST: f32 = 0.16;

pub const PUSHBUFFER: u64 = 0;
pub const DRAW_SEGMENT: u64 = 0x2000;
pub const VERTEX_PROGRAM: u64 = 0x1_0000;
pub const PIXEL_PROGRAM: u64 = 0x1_4000;
pub const VS_POINTERS: u64 = 0x2_0000;
pub const VS_UNIFORMS: u64 = 0x2_1000;
pub const INSTANCES: u64 = 0x10_0000;
pub const TARGET: u64 = 0x20_0000;
pub const RELEASE: u64 = 0xa0_0000;
pub const MAPPED: u64 = 0xa1_0000;
// particles.vert loads its uniform block handle with `ULDC.64 UR4, c[0x1d][0x0]`.
const POINTER_BANK: u32 = 0x1d;
const WAIT_FOR_IDLE: usize = 0x0110;
const DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_SUBSEQUENT: usize = 0x1218;
const SET_BLEND_SEPARATE_FOR_ALPHA: usize = 0x133c;
const SET_BLEND: usize = 0x1360;
const SET_VERTEX_STREAM_INSTANCE_A: usize = 0x1880;
const STRIP_OF_FOUR: u32 = 5 << 28 | 4 << 16;

#[derive(Clone, Copy)]
pub struct Ball {
    pub at: [f32; 2],
    pub velocity: [f32; 2],
    pub radius: f32,
}

pub fn balls() -> [Ball; BALLS] {
    let mut seed = 7u32;
    let mut random = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed as f32 / 4_294_967_296.0
    };
    core::array::from_fn(|i| Ball { at: [-1.35 + 0.245 * i as f32, 0.6 - 0.3 * (i % 3) as f32], velocity: [4.0 * random() - 2.0, 2.0 * random()], radius: 0.05 + 0.06 * random() })
}
pub fn physics(balls: &mut [Ball; BALLS], hypot: fn(f32, f32) -> f32, mut impact: impl FnMut(usize, f32)) {
    let fastest = balls.iter().map(|ball| hypot(ball.velocity[0], ball.velocity[1])).fold(0.0, f32::max);
    let smallest = balls.iter().map(|ball| ball.radius).fold(f32::INFINITY, f32::min);
    let travel = fastest * STEP / (0.5 * smallest);
    let substeps = (travel as usize + usize::from(travel > travel as usize as f32)).max(1);
    let dt = STEP / substeps as f32;
    for _ in 0..substeps {
        for ball in &mut *balls {
            ball.velocity[1] -= GRAVITY * dt;
            for axis in 0..2 {
                ball.at[axis] += ball.velocity[axis] * dt;
            }
        }
        for _ in 0..SOLVER_ITERATIONS {
            for i in 0..BALLS {
                for j in i + 1..BALLS {
                    let (a, b) = (&balls[i], &balls[j]);
                    let gap = [b.at[0] - a.at[0], b.at[1] - a.at[1]];
                    let distance = hypot(gap[0], gap[1]);
                    let overlap = a.radius + b.radius - distance;
                    if overlap <= 0.0 {
                        continue;
                    }
                    let normal = gap.map(|c| c / distance);
                    let closing = (b.velocity[0] - a.velocity[0]) * normal[0] + (b.velocity[1] - a.velocity[1]) * normal[1];
                    let [inv_a, inv_b] = [1.0 / (a.radius * a.radius), 1.0 / (b.radius * b.radius)];
                    let correction = CORRECTION * (overlap - SLOP).max(0.0) / (inv_a + inv_b);
                    for axis in 0..2 {
                        balls[i].at[axis] -= correction * inv_a * normal[axis];
                        balls[j].at[axis] += correction * inv_b * normal[axis];
                    }
                    if closing >= 0.0 {
                        continue;
                    }
                    let restitution = if -closing < SOFT { 0.0 } else { RESTITUTION };
                    let impulse = -(1.0 + restitution) * closing / (inv_a + inv_b);
                    for axis in 0..2 {
                        balls[i].velocity[axis] -= impulse * inv_a * normal[axis];
                        balls[j].velocity[axis] += impulse * inv_b * normal[axis];
                    }
                    impact(i, -closing);
                }
            }
            for (i, ball) in balls.iter_mut().enumerate() {
                for axis in 0..2 {
                    let limit = HALF[axis] - ball.radius;
                    if ball.at[axis].abs() > limit {
                        ball.at[axis] = limit.copysign(ball.at[axis]);
                        if ball.at[axis] * ball.velocity[axis] > 0.0 {
                            let speed = ball.velocity[axis].abs();
                            impact(i, speed);
                            ball.velocity[axis] = if speed < SOFT { 0.0 } else { -RESTITUTION * ball.velocity[axis] };
                        }
                    }
                }
            }
        }
    }
}

pub fn init_segment(base: u64, count: u32, words: &mut impl Extend<u32>) {
    const A8B8G8R8: u32 = 0xd5;
    const PITCH: u32 = 0x1000;
    const R32_G32_B32_A32_FLOAT: u32 = 0x01 << 21 | 7 << 27;
    const OGL_FUNC_ADD: u32 = 0x8006;
    const OGL_ONE: u32 = 0x4001;
    const OGL_ONE_MINUS_SRC_ALPHA: u32 = 0x4303;
    let (x, y) = (WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0);
    inc(SET_OBJECT, &[0xc797], words);
    inc(SET_COLOR_TARGET_A, &[((base + TARGET) >> 32) as u32, (base + TARGET) as u32, WIDTH * 4, HEIGHT, A8B8G8R8, PITCH], words);
    inc(SET_VIEWPORT_SCALE_X, &[x, y, 1.0, x, y, 0.0].map(f32::to_bits), words);
    inc(SET_VIEWPORT_CLIP_HORIZONTAL, &[WIDTH << 16, HEIGHT << 16, 0, 1.0f32.to_bits()], words);
    inc(SET_SCISSOR_ENABLE, &[1, WIDTH << 16, HEIGHT << 16], words);
    inc(SET_COLOR_CLEAR_VALUE, &[0.04f32, 0.05, 0.07, 1.0].map(f32::to_bits), words);
    inc(SET_CT_WRITE, &[0x1111], words);
    inc(SET_BLEND_SEPARATE_FOR_ALPHA, &[0, OGL_FUNC_ADD, OGL_ONE, OGL_ONE_MINUS_SRC_ALPHA], words);
    inc(SET_BLEND, &[1], words);
    inc(SET_VERTEX_STREAM_SIZE_A, &[0, count * 48], words);
    inc(SET_VERTEX_STREAM_A_FORMAT, &[0x1000 | 48, ((base + INSTANCES) >> 32) as u32, (base + INSTANCES) as u32, 1], words);
    inc(SET_VERTEX_STREAM_INSTANCE_A, &[1], words);
    inc(SET_VERTEX_ATTRIBUTE_A, &[R32_G32_B32_A32_FLOAT, 32 << 7 | R32_G32_B32_A32_FLOAT], words);
    // Register counts are the highest register each SASS program names plus one: R9 and R11.
    for (pipeline, enable, registers, group, program) in [(1, 0x11, 10, 0, base + VERTEX_PROGRAM), (5, 0x51, 12, 4, base + PIXEL_PROGRAM)] {
        inc(SET_PIPELINE_SHADER + pipeline * 64, &[enable], words);
        inc(SET_PIPELINE_SHADER + pipeline * 64 + 0xc, &[registers, group, (program >> 32) as u32, program as u32], words);
    }
    inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[0x100, ((base + VS_POINTERS) >> 32) as u32, (base + VS_POINTERS) as u32], words);
    inc(BIND_GROUP_CONSTANT_BUFFER, &[POINTER_BANK << 4 | 1], words);
}

pub fn frame(base: u64, count: usize, payload: u32, words: &mut impl Extend<u32>) {
    inc(WAIT_FOR_IDLE, &[0], words);
    inc(CLEAR_SURFACE, &[0x3c], words);
    inc(DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST, &[STRIP_OF_FOUR], words);
    words.extend([crate::header(3, DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_SUBSEQUENT, count as u32 - 1)]);
    words.extend(core::iter::repeat_n(STRIP_OF_FOUR, count - 1));
    inc(SET_REPORT_SEMAPHORE_A, &[((base + RELEASE) >> 32) as u32, (base + RELEASE) as u32, payload, 0x1000_f010], words);
}

pub fn uniforms() -> [f32; 24] {
    let [sx, sy] = VIEW.map(|half| 1.0 / half);
    [sx, 0.0, 0.0, 0.0, 0.0, -sy, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 0.0, 0.0, LARGEST, 0.0, 1.0, 0.0, 0.0]
}

pub fn pointers(base: u64) -> [u32; 2] {
    let va = base + VS_UNIFORMS;
    [(va >> 4) as u32, (va >> 36) as u32 | 0x100 >> 4 << 13]
}

#[cfg(feature = "std")]
pub mod assets {
    const BEAD: f32 = 0.02;
    use super::*;
    use std::{f32::consts::TAU, vec::Vec};
    /// particles.vert instances: position (x, y, 0, size as 2 - 2 size / LARGEST), unused, colour (rgb, brightness).
    pub fn instances(balls: &[Ball; BALLS], walls: &[[f32; 2]]) -> Vec<f32> {
        let quad = |[x, y]: [f32; 2], size: f32, colour: [f32; 4]| [&[x, y, 0.0, 2.0 - 2.0 * size / LARGEST, 0.0, 0.0, 0.0, 0.0][..], &colour].concat();
        let wall = walls.iter().flat_map(|&at| quad(at, BEAD, [0.07, 0.08, 0.1, 10.0]));
        // A brightness of 10 saturates the disc out to 0.83 of the quad, so its centre clamps bright and its rim keeps the colour.
        let balls = balls.iter().enumerate().flat_map(|(i, ball)| {
            let hue = i as f32 * TAU / BALLS as f32;
            let colour = [0.0, 2.1, 4.2].map(|phase| 0.09 + 0.07 * (hue + phase).cos());
            let shine = [ball.at[0] - 0.3 * ball.radius, ball.at[1] + 0.3 * ball.radius];
            [quad(ball.at, ball.radius / 0.83, [colour[0], colour[1], colour[2], 10.0]), quad(shine, ball.radius * 0.5, [0.9, 0.9, 0.9, 0.8])].concat()
        });
        wall.chain(balls).collect()
    }

    /// Bead centres along the four walls, just outside the box.
    pub fn walls() -> Vec<[f32; 2]> {
        let [w, h] = HALF.map(|half| half + 0.75 * BEAD);
        let beads = |length: f32| (length / (0.35 * BEAD)) as usize;
        let (nx, ny) = (beads(2.0 * w), beads(2.0 * h));
        let along = |n: usize, k: usize, half: f32| -half + 2.0 * half * k as f32 / n as f32;
        let horizontal = (0..=nx).flat_map(|k| [[along(nx, k, w), -h], [along(nx, k, w), h]]);
        let vertical = (1..ny).flat_map(|k| [[-w, along(ny, k, h)], [w, along(ny, k, h)]]);
        horizontal.chain(vertical).collect()
    }
}
