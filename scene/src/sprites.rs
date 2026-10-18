//! Sprite scene layout and pushbuffer shared by the host oracle and AArch64 guest.
use crate::{inc, non_inc, host::SET_OBJECT, threed::*};

pub const PUSHBUFFER: u64 = 0;
pub const FRAME_SEGMENT: u64 = 0x8000;
pub const VERTEX_PROGRAM: u64 = 0x1_0000;
pub const PIXEL_PROGRAM: u64 = 0x1_8000;
pub const VERTEX_TABLE: u64 = 0x2_0000;
pub const CAMERA: u64 = 0x2_1000;
pub const PIXEL_TABLE: u64 = 0x2_2000;
pub const TINT: u64 = 0x2_3000;
pub const TIC: u64 = 0x3_0000;
pub const TSC: u64 = 0x3_8000;
pub const INSTANCES: u64 = 0x4_0000;
pub const ATLAS: u64 = 0x5_0000;
pub const TARGET: u64 = 0x105_0000;

pub const SET_BLEND_SEPARATE_FOR_ALPHA: usize = 0x133c;
pub const SET_BLEND_ALPHA_DEST_COEFF: usize = 0x1358;
pub const SET_BLEND: usize = 0x1360;
pub const SET_VERTEX_STREAM_INSTANCE_A: usize = 0x1880;
pub const DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_SUBSEQUENT: usize = 0x1218;
pub const FUNC_ADD: u32 = 0x8006;
pub const ONE: u32 = 0x4001;
pub const SRC_ALPHA: u32 = 0x4302;
pub const ONE_MINUS_SRC_ALPHA: u32 = 0x4303;
pub const TRIANGLE_STRIP_OF_FOUR: u32 = 5 << 28 | 4 << 16;

// Both programs read their binding 1 block through the bindless cbuf handle at c[0x1d][0x8] and the pixel program its
// atlas through the texture handle at c[0x1d][0x0] (sprite2d vert.sass 0x30, frag.sass 0x60 and 0x80).
pub const DRIVER_BANK: u32 = 0x1d;

/// The scene is laid out in 1280x720 pixels whatever the target's size.
pub const SCENE: [f32; 2] = [1280.0, 720.0];
pub const SKY: [f32; 4] = [0.55, 0.7, 0.9, 1.0];
pub const SUNSET: [f32; 4] = [1.0, 0.9, 0.8, 1.0];
pub const INSTANCE_COUNT: usize = 199;
pub const ATLAS_SIZE: usize = 2048;
fn load<const N: usize>(buffer: u64, values: &[f32; N], words: &mut impl Extend<u32>) {
    inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[16, (buffer >> 32) as u32, buffer as u32, 0], words);
    non_inc(LOAD_CONSTANT_BUFFER, &values.map(f32::to_bits), words);
}

pub fn init_segment(base: u64, [width, height]: [u32; 2], words: &mut impl Extend<u32>) {
    const R32_G32_B32_A32_FLOAT: u32 = 0x01 << 21 | 7 << 27;
    const A8B8G8R8: u32 = 0xd5;
    const PITCH: u32 = 0x1000;
    let (x, y) = (width as f32 / 2.0, height as f32 / 2.0);
    inc(SET_OBJECT, &[0xc797], words);
    inc(SET_COLOR_TARGET_A, &[((base + TARGET) >> 32) as u32, (base + TARGET) as u32, width * 4, height, A8B8G8R8, PITCH], words);
    inc(SET_VIEWPORT_SCALE_X, &[x, y, 1.0, x, y, 0.0].map(f32::to_bits), words);
    inc(SET_VIEWPORT_CLIP_HORIZONTAL, &[width << 16, height << 16, 0, 1.0f32.to_bits()], words);
    inc(SET_SCISSOR_ENABLE, &[1, width << 16, height << 16], words);
    inc(SET_COLOR_CLEAR_VALUE, &SKY.map(f32::to_bits), words);
    inc(SET_CT_WRITE, &[0x1111], words);
    inc(SET_BLEND_SEPARATE_FOR_ALPHA, &[1, FUNC_ADD, SRC_ALPHA, ONE_MINUS_SRC_ALPHA, FUNC_ADD, ONE], words);
    inc(SET_BLEND_ALPHA_DEST_COEFF, &[ONE_MINUS_SRC_ALPHA], words);
    inc(SET_BLEND, &[1], words);
    inc(SET_VERTEX_STREAM_SIZE_A, &[0, (INSTANCE_COUNT * 48) as u32], words);
    inc(SET_VERTEX_STREAM_A_FORMAT, &[0x1000 | 48, ((base + INSTANCES) >> 32) as u32, (base + INSTANCES) as u32, 1], words);
    inc(SET_VERTEX_STREAM_INSTANCE_A, &[1], words);
    inc(SET_VERTEX_ATTRIBUTE_A, &[0, 16, 32].map(|offset| offset << 7 | R32_G32_B32_A32_FLOAT), words);
    inc(SET_PIPELINE_SHADER + 64, &[0x11], words);
    inc(SET_PIPELINE_SHADER + 64 + 0x10, &[0, ((base + VERTEX_PROGRAM) >> 32) as u32, (base + VERTEX_PROGRAM) as u32], words);
    inc(SET_PIPELINE_SHADER + 5 * 64, &[0x51], words);
    inc(SET_PIPELINE_SHADER + 5 * 64 + 0x10, &[4, ((base + PIXEL_PROGRAM) >> 32) as u32, (base + PIXEL_PROGRAM) as u32], words);
    for (group, table) in [(0, base + VERTEX_TABLE), (4, base + PIXEL_TABLE)] {
        inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[16, (table >> 32) as u32, table as u32], words);
        inc(BIND_GROUP_CONSTANT_BUFFER + 32 * group, &[DRIVER_BANK << 4 | 1], words);
    }
    inc(SET_TEX_HEADER_POOL_A, &[((base + TIC) >> 32) as u32, (base + TIC) as u32, 0], words);
    inc(SET_TEX_SAMPLER_POOL_A, &[((base + TSC) >> 32) as u32, (base + TSC) as u32, 0], words);
    load(base + CAMERA, &[0.0, 0.0, 2.0 / SCENE[0], 2.0 / SCENE[1]], words);
    load(base + TINT, &SUNSET, words);
}

pub fn frame(base: u64, scroll: f32, tint: [f32; 4], words: &mut impl Extend<u32>) {
    load(base + CAMERA, &[scroll], words);
    load(base + TINT, &tint, words);
    inc(CLEAR_SURFACE, &[0x3c], words);
    inc(DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST, &[TRIANGLE_STRIP_OF_FOUR], words);
    non_inc(DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_SUBSEQUENT, &[TRIANGLE_STRIP_OF_FOUR; INSTANCE_COUNT - 1], words);
}

pub fn descriptors(base: u64) -> [(u64, [u32; 8]); 4] {
    let handle = |offset| [0, 0, ((base + offset) >> 4) as u32, ((base + offset) >> 36) as u32 | 1 << 13, 0, 0, 0, 0];
    [(VERTEX_TABLE, handle(CAMERA)), (PIXEL_TABLE, handle(TINT)),
     (TIC, crate::pitch_rgba8_tic(base + ATLAS, ATLAS_SIZE as u32, ATLAS_SIZE as u32, ATLAS_SIZE as u32 * 4)),
     (TSC, [0, 0x62, 0, 0, 0, 0, 0, 0])]
}
