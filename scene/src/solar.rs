//! A deterministic protoplanetary disk in star-mass, AU and year / 2pi units. CUDA applies a democratic-heliocentric
//! Kepler split with tiled mutual kicks, sorted-cell accretion, a clearing gas disk and trails.
use crate::{header, host::SET_OBJECT, inc, non_inc, particles, split, threed::*};

pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
pub const REALTIME_COUNT: u32 = 512;
pub const SHOWCASE_COUNT: u32 = 2048;
pub const TRAIL_COUNT: u32 = 6 * 32;
pub const STAR_COUNT: u32 = 512;
pub const EXTRA_COUNT: u32 = TRAIL_COUNT + STAR_COUNT;
pub const BODY_BYTES: u32 = 48;
pub const DUST_MASS: f32 = 0.000_018 / SHOWCASE_COUNT as f32;
pub const CELLS: u32 = 64 * 64;
pub const CELL_CAPACITY: u32 = 8;
pub const THREADS: u32 = 64;

pub const BASE: u64 = 0x2000_0000;
pub const INIT_SEGMENT: u64 = BASE;
pub const STEP_SEGMENT: u64 = BASE + 0x1000;
pub const DRAW_SEGMENT: u64 = BASE + 0x4_0000;
pub const VERTEX_PROGRAM: u64 = BASE + 0x1_0000;
pub const PIXEL_PROGRAM: u64 = BASE + 0x1_4000;
pub const STEP_PROGRAM: u64 = BASE + 0x4000;
pub const VS_POINTERS: u64 = BASE + 0x2_0000;
pub const VS_UNIFORMS: u64 = BASE + 0x2_1000;
pub const CS_C0: u64 = BASE + 0x2_3000;
pub const BODY_QMD: u64 = BASE + 0x3_0000;
pub const CELL_QMD: u64 = BODY_QMD + 0x100;
pub const BODIES: u64 = BASE + 0x10_0000;
pub const SCRATCH: u64 = BASE + 0x20_0000;
pub const CELL_COUNTS: u64 = BASE + 0x30_0000;
pub const CELL_SLOTS: u64 = BASE + 0x31_0000;
pub const TARGET: u64 = BASE + 0x40_0000;
pub const RELEASE: u64 = BASE + 0xc0_0000;
pub const MAPPED: u64 = 0xc1_0000;

pub fn cbuf0(out: &mut [u8], count: u32) {
    out.fill(0);
    for (offset, value) in [(0, THREADS), (4, 1), (8, 1), (12, 64), (16, 1), (20, 1), (0x28, 0x1000)] {
        out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (offset, value) in [(0x160, BODIES), (0x168, SCRATCH), (0x170, CELL_COUNTS), (0x178, CELL_SLOTS)] {
        out[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    out[0x180..0x184].copy_from_slice(&count.to_le_bytes());
    out[0x188..0x18c].copy_from_slice(&0.01f32.to_le_bytes());
    out[0x190..0x194].copy_from_slice(&DUST_MASS.to_le_bytes());
}

pub fn qmd(grid: u32) -> [u32; 64] {
    let mut qmd = [0; 64];
    [qmd[4], qmd[5]] = [0x40, 0xfc00_0000];
    [qmd[12], qmd[13], qmd[14]] = [grid, 1, 1];
    [qmd[17], qmd[18], qmd[19], qmd[20], qmd[23]] = [THREADS * 16, THREADS << 16 | 3 << 4, 1 << 16 | 1, 40 << 8 | 1, 1 << 27];
    [qmd[32], qmd[33]] = [CS_C0 as u32, 0x200 >> 4 << 19];
    qmd[48] = STEP_PROGRAM as u32;
    qmd
}

pub fn init(count: u32, words: &mut impl Extend<u32>) {
    const A8B8G8R8: u32 = 0xd5;
    const PITCH: u32 = 0x1000;
    const RGBA32F: u32 = 0x01 << 21 | 7 << 27;
    const OGL_FUNC_ADD: u32 = 0x8006;
    const OGL_ONE: u32 = 0x4001;
    let (x, y) = (WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0);
    inc(SET_OBJECT, &[0xc797], words);
    particles::compute(SET_OBJECT, &[0xc7c0], words);
    inc(SET_COLOR_TARGET_A, &[0, TARGET as u32, WIDTH * 4, HEIGHT, A8B8G8R8, PITCH], words);
    inc(SET_VIEWPORT_SCALE_X, &[x, y, 1.0, x, y, 0.0].map(f32::to_bits), words);
    inc(SET_VIEWPORT_CLIP_HORIZONTAL, &[WIDTH << 16, HEIGHT << 16, 0, 1.0f32.to_bits()], words);
    inc(SET_SCISSOR_ENABLE, &[1, WIDTH << 16, HEIGHT << 16], words);
    inc(SET_CT_WRITE, &[0x1111], words);
    inc(particles::SET_BLEND_SEPARATE_FOR_ALPHA, &[0, OGL_FUNC_ADD, OGL_ONE, OGL_ONE], words);
    inc(particles::SET_BLEND, &[1], words);
    inc(SET_VERTEX_STREAM_SIZE_A, &[0, (count + EXTRA_COUNT) * BODY_BYTES], words);
    inc(SET_VERTEX_STREAM_A_FORMAT, &[0x1000 | BODY_BYTES, 0, BODIES as u32, 1], words);
    inc(particles::SET_VERTEX_STREAM_INSTANCE_A, &[1], words);
    inc(SET_VERTEX_ATTRIBUTE_A, &[RGBA32F, 32 << 7 | RGBA32F], words);
    for (pipeline, enable, registers, group, program) in [(1, 0x11, 10, 0, VERTEX_PROGRAM), (5, 0x51, 12, 4, PIXEL_PROGRAM)] {
        inc(SET_PIPELINE_SHADER + pipeline * 64, &[enable], words);
        inc(SET_PIPELINE_SHADER + pipeline * 64 + 0xc, &[registers, group, 0, program as u32], words);
    }
    inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[0x100, 0, VS_POINTERS as u32], words);
    inc(BIND_GROUP_CONSTANT_BUFFER, &[particles::POINTER_BANK << 4 | 1], words);
}

pub fn step(count: u32, dt: f32, time: f32, trail_slot: u32, collide: bool, src: u64, dst: u64, words: &mut impl Extend<u32>) {
    let launch = |phase, bodies, scratch, words: &mut _| {
        let [bodies_hi, bodies_lo] = split(bodies);
        let [scratch_hi, scratch_lo] = split(scratch);
        let [counts_hi, counts_lo] = split(CELL_COUNTS);
        let [slots_hi, slots_lo] = split(CELL_SLOTS);
        inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[0x40, 0, CS_C0 as u32, 0x160], words);
        non_inc(LOAD_CONSTANT_BUFFER, &[bodies_lo, bodies_hi, scratch_lo, scratch_hi, counts_lo, counts_hi, slots_lo, slots_hi,
            count, phase, dt.to_bits(), time.to_bits(), DUST_MASS.to_bits(), trail_slot], words);
        inc(particles::WAIT_FOR_IDLE, &[0], words);
        let qmd = if matches!(phase, 1..=3) { CELL_QMD } else { BODY_QMD };
        particles::compute(particles::SEND_PCAS_A, &[(qmd >> 8) as u32], words);
        particles::compute(particles::SEND_SIGNALING_PCAS2_B, &[particles::INVALIDATE_COPY_SCHEDULE], words);
    };
    launch(0, src, dst, words);
    if collide {
        for phase in 1..=3 { launch(phase, src, dst, words); }
    }
}

fn camera(seconds: f32, sqrt: impl Fn(f32) -> f32) -> ([f32; 16], [f32; 3], [f32; 3]) {
    let t = (seconds / 20.0).clamp(0.0, 1.0);
    let tilt = t * t * (3.0 - 2.0 * t);
    let (sin, cos) = crate::pbr::sin_cos(0.16 * seconds);
    let eye = [(0.6 + 4.8 * tilt) * sin, 7.0 - 3.6 * tilt, (0.6 + 4.8 * tilt) * cos];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let unit = |a: [f32; 3]| a.map(|c| c / sqrt(dot(a, a)));
    let forward = unit(eye.map(|c: f32| -c));
    let right = unit(cross(forward, [0.0, 1.0, 0.0]));
    let up = cross(right, forward);
    let [r, u, back] = [right, up, forward.map(|c| -c)].map(|axis| [axis[0], axis[1], axis[2], -dot(axis, eye)]);
    let (focal, near, far) = (1.9, 0.1, 20.0);
    let depth = far / (near - far);
    let rows = [r.map(|c| c * focal * HEIGHT as f32 / WIDTH as f32), u.map(|c| -c * focal),
        [back[0] * depth, back[1] * depth, back[2] * depth, back[3] * depth + near * depth], back.map(|c| -c)];
    (core::array::from_fn(|i| rows[i % 4][i / 4]), right, up)
}

pub fn draw(seconds: f32, count: u32, payload: u32, sqrt: impl Fn(f32) -> f32, words: &mut impl Extend<u32>) {
    const STRIP: u32 = 5 << 28 | 4 << 16;
    const PACKET: u32 = 8191;
    const RELEASE_ONE_WORD: u32 = 0x1000_f010;
    let (view_proj, right, up) = camera(seconds, sqrt);
    let mut uniforms = [0.0; 24];
    uniforms[..16].copy_from_slice(&view_proj);
    uniforms[16..19].copy_from_slice(&right);
    uniforms[19] = 0.006;
    uniforms[20..23].copy_from_slice(&up);
    inc(SET_COLOR_CLEAR_VALUE, &[0.005f32.to_bits(), 0.007f32.to_bits(), 0.018f32.to_bits(), 1.0f32.to_bits()], words);
    inc(CLEAR_SURFACE, &[0x3c], words);
    inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[0x100, 0, VS_UNIFORMS as u32, 0], words);
    non_inc(LOAD_CONSTANT_BUFFER, &uniforms.map(f32::to_bits), words);
    inc(particles::WAIT_FOR_IDLE, &[0], words);
    inc(particles::DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_SUBSEQUENT - 4, &[STRIP], words);
    let instances = count + EXTRA_COUNT;
    for first in (1..instances).step_by(PACKET as usize) {
        let chunk = PACKET.min(instances - first);
        words.extend([header(3, particles::DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_SUBSEQUENT, chunk)]);
        words.extend(core::iter::repeat_n(STRIP, chunk as usize));
    }
    let [hi, lo] = split(RELEASE);
    inc(SET_REPORT_SEMAPHORE_A, &[hi, lo, payload, RELEASE_ONE_WORD], words);
}

#[cfg(feature = "std")]
pub mod assets {
    use super::*;
    use std::prelude::v1::*;

    fn random(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *seed as f32 / 4_294_967_296.0
    }

    fn body(position: [f32; 3], velocity: [f32; 3], mass: f32, scale: f32, colour: [f32; 4]) -> [f32; 12] {
        [position[0], position[1], position[2], 2.0 * (1.0 - scale), velocity[0], velocity[1], velocity[2], mass,
         colour[0], colour[1], colour[2], colour[3]]
    }

    pub fn initial_bodies() -> Vec<u8> {
        let mut bodies = Vec::with_capacity(SHOWCASE_COUNT as usize);
        bodies.push(body([0.0; 3], [0.0; 3], 1.0, 18.0, [1.0, 0.55, 0.12, 1.0]));
        let mut momentum = [0.0; 3];
        let mut seed = 0x6d2b_79f5;
        for i in 0..SHOWCASE_COUNT - 1 {
            let root_radius = 0.3f32.sqrt() + random(&mut seed) * (4.0f32.sqrt() - 0.3f32.sqrt());
            let semimajor = root_radius * root_radius;
            let angle = random(&mut seed) * core::f32::consts::TAU;
            let node = random(&mut seed) * core::f32::consts::TAU;
            let anomaly = random(&mut seed) * core::f32::consts::TAU;
            let rayleigh = |seed: &mut u32, sigma: f32| sigma * (-2.0 * (1.0 - random(seed)).ln()).sqrt();
            let eccentricity = rayleigh(&mut seed, 0.01);
            let inclination = rayleigh(&mut seed, 0.005);
            let (sin, cos) = crate::pbr::sin_cos(angle);
            let (node_sin, node_cos) = crate::pbr::sin_cos(node);
            let (anomaly_sin, anomaly_cos) = crate::pbr::sin_cos(anomaly);
            let (inclination_sin, inclination_cos) = crate::pbr::sin_cos(inclination);
            let radial = semimajor * (1.0 - eccentricity * anomaly_cos);
            let seed_body = i < 6;
            let mass = if seed_body { DUST_MASS * (24.0 + 5.0 * i as f32) } else { DUST_MASS * (0.7 + 0.6 * random(&mut seed)) };
            let plane_x = [node_cos, 0.0, node_sin];
            let plane_y = [-node_sin * inclination_sin, inclination_cos, node_cos * inclination_sin];
            let orbit: [f32; 3] = core::array::from_fn(|axis| cos * plane_x[axis] + sin * plane_y[axis]);
            let tangent: [f32; 3] = core::array::from_fn(|axis| -sin * plane_x[axis] + cos * plane_y[axis]);
            let radial_speed = eccentricity * anomaly_sin / semimajor.sqrt();
            let tangential_speed = (1.0 + eccentricity * anomaly_cos) / semimajor.sqrt();
            let position = orbit.map(|value| value * radial);
            let velocity = core::array::from_fn(|axis| orbit[axis] * radial_speed + tangent[axis] * tangential_speed);
            for axis in 0..3 { momentum[axis] += velocity[axis] * mass; }
            let scale = if seed_body { 4.0 + i as f32 * 0.3 } else { 1.0 + 2.5 * (mass / DUST_MASS).sqrt() };
            let colour = if seed_body { [0.45, 0.7, 1.0, 0.9] } else { [0.72, 0.46, 0.24, 0.35] };
            bodies.push(body(position, velocity, mass, scale, colour));
        }
        bodies[0][4..7].copy_from_slice(&momentum.map(|value| -value));
        bodies.into_iter().flatten().flat_map(f32::to_le_bytes).collect()
    }

    pub fn stars() -> Vec<u8> {
        let mut seed = 0x51a7_2026u32;
        (0..STAR_COUNT).map(|_| {
            let radius = 8.0 + random(&mut seed) * 8.0;
            let y = random(&mut seed) * 2.0 - 1.0;
            let angle = random(&mut seed) * core::f32::consts::TAU;
            let (sin, cos) = crate::pbr::sin_cos(angle);
            let ring = (1.0 - y * y).sqrt() * radius;
            body([ring * cos, y * radius, ring * sin], [0.0; 3], 0.0, 0.35 + random(&mut seed) * 0.4, [0.7, 0.8, 1.0, 0.12])
        }).flatten().flat_map(f32::to_le_bytes).collect()
    }
}
