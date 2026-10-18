//! Pinwheel and post-effect command encoding shared by the guest and host oracle.
use crate::{inc, split};
pub const BASE: u64 = 0x1000_0000;
pub const PUSHBUFFER: u64 = BASE;
pub const FRAME_SEGMENT: u64 = BASE + 0x8000;
pub const SCENE_VERTEX: u64 = BASE + 0x1_0000;
pub const SCENE_PIXEL: u64 = BASE + 0x1_8000;
pub const UNIFORMS: u64 = BASE + 0x2_5000;
pub const POINTERS: u64 = BASE + 0x2_6000;
pub const TIC: u64 = BASE + 0x3_0000;
pub const TSC: u64 = BASE + 0x3_8000;
pub const REPORT: u64 = BASE + 0x3_f000;
pub const VERTICES: u64 = BASE + 0x4_0000;
pub const POST_VERTEX: u64 = BASE + 0x9_0000;
pub const POST_PIXEL: u64 = BASE + 0x9_8000;
pub const SCENE: u64 = BASE + 0xa_0000;

// post.frag reads its texture handle at c[0x1d][0x0] and its uniform block through the handle at c[0x1d][0x8]
// (`TEX ... 0x1d, 0x0`, `ULDC.64 UR4, c[0x1d][0x8]`).
pub const POINTER_BANK: u32 = 0x1d;
pub const TEXTURE_HANDLE: u32 = 4 | 1 << 20;
pub const BLADES: usize = 6;

pub const SET_OBJECT: usize = 0x0000;
pub const WAIT_FOR_IDLE: usize = 0x0110;
pub const SET_VERTEX_STREAM_SIZE_A: usize = 0x0600;
pub const SET_COLOR_TARGET_A: usize = 0x0800;
pub const SET_COLOR_TARGET_FORMAT: usize = 0x0810;
pub const SET_VIEWPORT_SCALE_X: usize = 0x0a00;
pub const SET_VIEWPORT_CLIP_HORIZONTAL: usize = 0x0c00;
pub const SET_COLOR_CLEAR_VALUE: usize = 0x0d80;
pub const SET_SCISSOR_ENABLE: usize = 0x0e00;
pub const SET_VERTEX_ATTRIBUTE_A: usize = 0x1160;
pub const DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST: usize = 0x1214;
pub const INVALIDATE_TEXTURE_DATA_CACHE: usize = 0x1338;
pub const SET_TEX_SAMPLER_POOL_A: usize = 0x155c;
pub const SET_TEX_HEADER_POOL_A: usize = 0x1574;
pub const CLEAR_SURFACE: usize = 0x19d0;
pub const SET_CT_WRITE: usize = 0x1a00;
pub const SET_REPORT_SEMAPHORE_A: usize = 0x1b00;
pub const SET_VERTEX_STREAM_A_FORMAT: usize = 0x1c00;
pub const SET_PIPELINE_SHADER: usize = 0x2000;
pub const SET_CONSTANT_BUFFER_SELECTOR_A: usize = 0x2380;
pub const BIND_GROUP_CONSTANT_BUFFER: usize = 0x2410;

/// Colour target 0 at `va`, with the viewport and scissor covering it.
fn colour_target(va: u64, [width, height]: [u32; 2], words: &mut impl Extend<u32>) {
    let (x, y) = (width as f32 / 2.0, height as f32 / 2.0);
    inc(SET_COLOR_TARGET_A, &[split(va)[0], split(va)[1], width * 4, height], words);
    inc(SET_VIEWPORT_SCALE_X, &[x, y, 1.0, x, y, 0.0].map(f32::to_bits), words);
    inc(SET_VIEWPORT_CLIP_HORIZONTAL, &[width << 16, height << 16, 0, 1.0f32.to_bits()], words);
    inc(SET_SCISSOR_ENABLE, &[1, width << 16, height << 16], words);
}

fn programs(vertex: u64, pixel: u64, words: &mut impl Extend<u32>) {
    inc(SET_PIPELINE_SHADER + 64 + 0x14, &split(vertex), words);
    inc(SET_PIPELINE_SHADER + 5 * 64 + 0x14, &split(pixel), words);
}

pub fn init(words: &mut impl Extend<u32>) {
    const FLOAT: u32 = 7 << 27;
    const R32_G32_B32_A32: u32 = 0x01 << 21;
    inc(SET_OBJECT, &[0xc797], words);
    inc(SET_COLOR_TARGET_FORMAT, &[0xd5, 0x1000], words);
    inc(SET_CT_WRITE, &[0x1111], words);
    inc(SET_COLOR_CLEAR_VALUE, &[0.35f32, 0.4, 0.5, 1.0].map(f32::to_bits), words);
    inc(SET_VERTEX_STREAM_A_FORMAT, &[0x1000 | 16, split(VERTICES)[0], split(VERTICES)[1]], words);
    inc(SET_VERTEX_STREAM_SIZE_A, &split(16 * 3 * BLADES as u64), words);
    inc(SET_VERTEX_ATTRIBUTE_A, &[R32_G32_B32_A32 | FLOAT], words);
    inc(SET_PIPELINE_SHADER + 64, &[0x11], words);
    inc(SET_PIPELINE_SHADER + 5 * 64, &[0x51], words);
    inc(SET_PIPELINE_SHADER + 5 * 64 + 0x10, &[4], words);
    inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[16, split(POINTERS)[0], split(POINTERS)[1]], words);
    inc(BIND_GROUP_CONSTANT_BUFFER + 4 * 32, &[POINTER_BANK << 4 | 1], words);
    inc(SET_TEX_HEADER_POOL_A, &[split(TIC)[0], split(TIC)[1], 4], words);
    inc(SET_TEX_SAMPLER_POOL_A, &[split(TSC)[0], split(TSC)[1], 1], words);
}
pub fn frame(scene: [u32; 2], out: [u32; 2], target: u64, seconds: f32, payload: u32, words: &mut impl Extend<u32>) {
    colour_target(SCENE, scene, words);
    programs(SCENE_VERTEX, SCENE_PIXEL, words);
    inc(CLEAR_SURFACE, &[0x3c], words);
    inc(DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST, &[4 << 28 | (3 * BLADES as u32) << 16], words);
    inc(WAIT_FOR_IDLE, &[0], words);
    inc(INVALIDATE_TEXTURE_DATA_CACHE, &[0], words);
    colour_target(target, out, words);
    programs(POST_VERTEX, POST_PIXEL, words);
    inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[32, split(UNIFORMS)[0], split(UNIFORMS)[1], 0, seconds.to_bits()], words);
    inc(DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST, &[4 << 28 | 3 << 16], words);
    const ONE_WORD: u32 = 1 << 28;
    inc(SET_REPORT_SEMAPHORE_A, &[split(REPORT)[0], split(REPORT)[1], payload, ONE_WORD], words);
}
pub fn vertices(scene: [u32; 2], seconds: f32) -> [[f32; 4]; 3 * BLADES] {
    let aspect = scene[1] as f32 / scene[0] as f32;
    let corner = |angle: f32, radius: f32, [r, g]: [f32; 2]| {
        let (sin, cos) = crate::pbr::sin_cos(angle);
        // triangle.vert colours a vertex (z, w, x, y) * 0.5 + 0.25.
        [cos * radius * aspect, sin * radius, (r - 0.25) * 2.0, (g - 0.25) * 2.0]
    };
    core::array::from_fn(|i| {
        let k = i / 3;
        let angle = seconds * 0.5 + k as f32 * core::f32::consts::TAU / BLADES as f32;
        let colour = if k % 2 == 0 { [1.0, 0.2] } else { [0.2, 0.9] };
        match i % 3 {
            0 => corner(0.0, 0.0, [1.0, 1.0]),
            1 => corner(angle - 0.25, 0.85, colour),
            _ => corner(angle + 0.25, 0.85, colour),
        }
    })
}
