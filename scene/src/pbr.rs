//! PBR memory layout and command encoding shared by the guest and host oracle.
use crate::{inc, non_inc, split};
pub const BASE: u64 = 0x1000_0000;
pub const PUSHBUFFER: u64 = BASE;
pub const FRAME_SEGMENT: u64 = BASE + 0x8000;
pub const VERTEX_PROGRAM: u64 = BASE + 0x1_0000;
pub const PIXEL_PROGRAM: u64 = BASE + 0x4_0000;
pub const VERTEX_TABLE: u64 = BASE + 0x2_0000;
pub const VERTEX_UNIFORMS: u64 = VERTEX_TABLE + 0x100;
pub const MATERIALS: u64 = BASE + 0x2_1000;
pub const MATERIAL_STRIDE: u64 = 0x100;
pub const MATERIAL_UNIFORMS: u64 = 0x80;
pub const TIC: u64 = BASE + 0x3_0000;
pub const TSC: u64 = BASE + 0x3_8000;
pub const REPORT: u64 = BASE + 0x3_f000;
pub const VERTICES: u64 = PIXEL_PROGRAM + 0x2_0000;
// The depth target lives on the host only, so its VA needs no pages.
pub const DEPTH: u64 = 0x2000_0000;
pub const ALIGN: u64 = 0x1_0000;
pub const EYE: [f32; 3] = [0.0, 0.35, 3.5];
pub const TLAS: u64 = BASE + 0x2_f000;
pub const RAY_CONSTANTS: u64 = TLAS + 0x800;
pub const ACCELERATION_STRUCTURE_MAGIC: u32 = u32::from_le_bytes(*b"OAS1");
pub const GEOMETRY_WORDS: usize = 20;
#[derive(Clone, Copy)]
pub struct AccelerationStructureGeometry {
    pub vertices: u64,
    pub stride: u32,
    pub vertex_count: u32,
    pub indices: u64,
    pub index_count: u32,
    pub opaque: bool,
    pub transform: [[f32; 4]; 3],
}
#[derive(Clone, Copy)]
pub struct RayGeometry {
    pub model_vertices: u32,
    pub model_indices: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub indices_va: u64,
    pub light_count: u32,
}
pub const IDENTITY: Matrix = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
pub const ROOM_EYE: [f32; 3] = [0.0, 1.0, 5.5];
pub const ROOM_MATERIALS: [([u8; 3], f32, f32); 8] = [
    ([28, 31, 38], 0.75, 0.18), ([150, 18, 24], 0.0, 0.72), ([20, 112, 52], 0.0, 0.72),
    ([218, 205, 174], 0.0, 0.72), ([226, 215, 188], 0.0, 0.8), ([205, 218, 232], 1.0, 0.06),
    ([120, 195, 220], 0.2, 0.42),
    ([80, 190, 255], 0.0, 0.25),
];

// The driver's bank 0x1d: TEX handle k at +4k in GLSL declaration order, the binding 1 block's address at +0x18
// (pbr.vert and pbr.frag SASS: `TEX ... 0x1d, 0x4`, `ULDC.64 UR4, c[0x1d][0x18]`).
pub const POINTER_BANK: u32 = 0x1d;
pub const UNIFORM_POINTER: usize = 0x18 / 4;
pub const SLOTS: [&str; 5] = ["baseColor", "normal", "metallicRoughness", "occlusion", "emissive"];

pub const SET_OBJECT: usize = 0x0000;
pub const SET_VERTEX_STREAM_SIZE_A: usize = 0x0600;
pub const SET_COLOR_TARGET_A: usize = 0x0800;
pub const SET_VIEWPORT_SCALE_X: usize = 0x0a00;
pub const SET_VIEWPORT_CLIP_HORIZONTAL: usize = 0x0c00;
pub const SET_COLOR_CLEAR_VALUE: usize = 0x0d80;
pub const SET_SCISSOR_ENABLE: usize = 0x0e00;
pub const SET_ZT_A: usize = 0x0fe0;
pub const SET_VERTEX_ATTRIBUTE_A: usize = 0x1160;
pub const SET_DEPTH_TEST: usize = 0x12cc;
pub const SET_DEPTH_WRITE: usize = 0x12e8;
pub const SET_DEPTH_FUNC: usize = 0x130c;
pub const SET_BLEND_CONST_RED: usize = 0x131c;
pub const SET_BLEND_COLOR_OP: usize = 0x1340;
pub const SET_BLEND: usize = 0x1360;
pub const SET_ZT_SELECT: usize = 0x1538;
pub const SET_TEX_SAMPLER_POOL_A: usize = 0x155c;
pub const SET_TEX_HEADER_POOL_A: usize = 0x1574;
pub const END: usize = 0x1614;
pub const BEGIN: usize = 0x1618;
pub const SET_INDEX_BUFFER_A: usize = 0x17c8;
pub const SET_INDEX_BUFFER_E: usize = 0x17d8;
pub const SET_INDEX_BUFFER_F: usize = 0x17dc;
pub const DRAW_INDEX_BUFFER: usize = 0x17e0;
pub const CLEAR_SURFACE: usize = 0x19d0;
pub const SET_CT_WRITE: usize = 0x1a00;
pub const SET_REPORT_SEMAPHORE_A: usize = 0x1b00;
pub const SET_VERTEX_STREAM_A_FORMAT: usize = 0x1c00;
pub const SET_PIPELINE_SHADER: usize = 0x2000;
pub const SET_CONSTANT_BUFFER_SELECTOR_A: usize = 0x2380;
pub const LOAD_CONSTANT_BUFFER: usize = 0x2390;
pub const BIND_GROUP_CONSTANT_BUFFER: usize = 0x2410;

pub type Matrix = [[f32; 4]; 4];

pub fn multiply(a: Matrix, b: Matrix) -> Matrix {
    core::array::from_fn(|column| core::array::from_fn(|row| (0..4).map(|k| a[k][row] * b[column][k]).sum()))
}

pub fn view_projection(width: u32, height: u32, eye: [f32; 3], sqrt: impl Fn(f32) -> f32) -> Matrix {
    let (near, far, focal) = (0.1, 20.0, 2.414_213_7);
    let projection = [[focal * height as f32 / width as f32, 0.0, 0.0, 0.0], [0.0, -focal, 0.0, 0.0], [0.0, 0.0, far / (near - far), -1.0], [0.0, 0.0, near * far / (near - far), 0.0]];
    let length = sqrt(eye.iter().map(|v| v * v).sum());
    let back = eye.map(|v| v / length);
    let side_length = sqrt(back[0] * back[0] + back[2] * back[2]);
    let right = [back[2] / side_length, 0.0, -back[0] / side_length];
    let up = [-back[1] * right[2], back[2] * right[0] - back[0] * right[2], -back[1] * right[0]];
    let mut view = [[right[0], up[0], back[0], 0.0], [right[1], up[1], back[1], 0.0], [right[2], up[2], back[2], 0.0], [0.0; 4]];
    view[3] = [-right.iter().zip(eye).map(|(a, b)| a * b).sum::<f32>(), -up.iter().zip(eye).map(|(a, b)| a * b).sum::<f32>(), -length, 1.0];
    multiply(projection, view)
}

pub fn room_frame(seconds: f32, traced: bool, sqrt: impl Fn(f32) -> f32) -> ([f32; 3], [f32; 16], f32) {
    let (camera, _) = sin_cos(seconds * 0.18);
    let (orbit, orbit_cos) = sin_cos(seconds * 0.45);
    let (bob, _) = sin_cos(seconds * 1.4);
    let eye = [camera * 0.55, ROOM_EYE[1], ROOM_EYE[2]];
    let key = [orbit * 1.8, 2.15 + bob * 0.12, 0.4 + 1.25 * orbit_cos];
    let length = sqrt(key.iter().map(|v| v * v).sum());
    let cool = [-1.65, 0.7 + 0.25 * bob, -0.55, 0.12];
    let light = if traced { [key[0], key[1], key[2], 0.32] } else { [key[0] / length, key[1] / length, key[2] / length, 0.0] };
    let mut head = [0.0; 16];
    head[..4].copy_from_slice(&light);
    head[4..8].copy_from_slice(&[4.1, 3.5, 3.0, 0.0]);
    head[8..12].copy_from_slice(if traced { &cool } else { &[0.17, 0.18, 0.22, 0.0] });
    head[12..].copy_from_slice(&[eye[0], eye[1], eye[2], 1.0]);
    (eye, head, cool[1])
}

pub fn acceleration_structure(model: Matrix, layout: RayGeometry, words: &mut impl Extend<u32>) {
    acceleration_structure_registry(&[
        AccelerationStructureGeometry { vertices: VERTICES, stride: 48, vertex_count: layout.model_vertices, indices: layout.indices_va, index_count: layout.model_indices, opaque: true, transform: core::array::from_fn(|row| core::array::from_fn(|column| model[column][row])) },
        AccelerationStructureGeometry { vertices: VERTICES, stride: 48, vertex_count: layout.vertex_count, indices: layout.indices_va + (u64::from(layout.model_indices) + 30) * 4, index_count: layout.index_count - layout.model_indices - 30 - layout.light_count, opaque: true, transform: [IDENTITY[0], IDENTITY[1], IDENTITY[2]] },
    ], words);
}

pub fn acceleration_structure_registry(geometry: &[AccelerationStructureGeometry], words: &mut impl Extend<u32>) {
    words.extend([ACCELERATION_STRUCTURE_MAGIC, geometry.len() as u32]);
    for geometry in geometry {
        words.extend(split(geometry.vertices));
        words.extend([geometry.stride, geometry.vertex_count]);
        words.extend(split(geometry.indices));
        words.extend([geometry.index_count, u32::from(geometry.opaque)]);
        words.extend(geometry.transform.into_iter().flatten().map(f32::to_bits));
    }
}

pub fn init(width: u32, height: u32, target: u64, vertex_bytes: u64, indices_va: u64, textures: u32, words: &mut impl Extend<u32>) {
    let [vs, fs, vertex_table, ct, depth, vb, ib, tic_pool, tsc_pool] = [VERTEX_PROGRAM, PIXEL_PROGRAM, VERTEX_TABLE, target, DEPTH, VERTICES, indices_va, TIC, TSC].map(split);
    const PITCH: u32 = 0x1000;
    const ZF32: u32 = 0x0a;
    const OGL_LESS: u32 = 0x201;
    const FOUR_BYTES: u32 = 2;
    const FLOAT: u32 = 7 << 27;
    let (x, y) = (width as f32 / 2.0, height as f32 / 2.0);
    inc(SET_OBJECT, &[0xc797], words);
    inc(SET_COLOR_TARGET_A, &[ct[0], ct[1], width * 4, height, 0xd5, PITCH], words);
    inc(SET_CT_WRITE, &[0x1111], words);
    inc(SET_ZT_A, &[depth[0], depth[1], ZF32], words);
    inc(SET_ZT_SELECT, &[1], words);
    inc(SET_VIEWPORT_SCALE_X, &[x, y, 1.0, x, y, 0.0].map(f32::to_bits), words);
    inc(SET_VIEWPORT_CLIP_HORIZONTAL, &[width << 16, height << 16, 0, 1.0f32.to_bits()], words);
    inc(SET_SCISSOR_ENABLE, &[1, width << 16, height << 16], words);
    // SET_COLOR_CLEAR_VALUE is followed by SET_Z_CLEAR_VALUE.
    inc(SET_COLOR_CLEAR_VALUE, &[0.16f32, 0.17, 0.2, 1.0, 1.0].map(f32::to_bits), words);
    inc(SET_DEPTH_TEST, &[1], words);
    inc(SET_DEPTH_WRITE, &[1], words);
    inc(SET_DEPTH_FUNC, &[OGL_LESS], words);
    inc(SET_VERTEX_STREAM_A_FORMAT, &[0x1000 | 48, vb[0], vb[1]], words);
    inc(SET_VERTEX_STREAM_SIZE_A, &split(vertex_bytes), words);
    // position, normal and tangent at 0, 12 and 24 as R32_G32_B32, R32_G32_B32 and R32_G32_B32_A32, uv at 40 as R32_G32.
    inc(SET_VERTEX_ATTRIBUTE_A, &[0x02 << 21 | FLOAT, 12 << 7 | 0x02 << 21 | FLOAT, 24 << 7 | 0x01 << 21 | FLOAT, 40 << 7 | 0x04 << 21 | FLOAT], words);
    inc(SET_INDEX_BUFFER_A, &ib, words);
    inc(SET_INDEX_BUFFER_E, &[FOUR_BYTES], words);
    inc(SET_PIPELINE_SHADER + 64, &[0x11], words);
    inc(SET_PIPELINE_SHADER + 64 + 0x10, &[0, vs[0], vs[1]], words);
    inc(SET_PIPELINE_SHADER + 5 * 64, &[0x51], words);
    inc(SET_PIPELINE_SHADER + 5 * 64 + 0x10, &[4, fs[0], fs[1]], words);
    inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[48, vertex_table[0], vertex_table[1]], words);
    inc(BIND_GROUP_CONSTANT_BUFFER, &[POINTER_BANK << 4 | 1], words);
    inc(SET_TEX_HEADER_POOL_A, &[tic_pool[0], tic_pool[1], textures - 1], words);
    inc(SET_TEX_SAMPLER_POOL_A, &[tsc_pool[0], tsc_pool[1], 0], words);
}
pub fn draw<'a>(uniforms: &[u32; 32], view_proj: Matrix, draws: &[[u32; 3]], blends: impl Fn(usize) -> Option<&'a [u32]>, room: bool, traced: bool, model_indices: u32, light_first: u32, light_y: f32, payload: u32, words: &mut impl Extend<u32>) {
    const CLEAR_Z_AND_RGBA: u32 = 0x3d;
    const TRIANGLES: u32 = 4;
    const ONE_WORD: u32 = 1 << 28;
    inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[128, split(VERTEX_UNIFORMS)[0], split(VERTEX_UNIFORMS)[1], 0], words);
    non_inc(LOAD_CONSTANT_BUFFER, uniforms, words);
    inc(CLEAR_SURFACE, &[CLEAR_Z_AND_RGBA], words);
    if traced {
        inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[16, split(RAY_CONSTANTS)[0], split(RAY_CONSTANTS)[1]], words);
        inc(BIND_GROUP_CONSTANT_BUFFER + 4 * 32, &[1 << 4 | 1], words);
    }
    let mut blending = None;
    for &[first, count, material] in draws {
        let blend = blends(material as usize);
        if blending != Some(blend) {
            const OGL_LEQUAL: u32 = 0x203;
            const OGL_LESS: u32 = 0x201;
            inc(SET_BLEND, &[u32::from(blend.is_some())], words);
            inc(SET_DEPTH_WRITE, &[u32::from(blend.is_none())], words);
            inc(SET_DEPTH_FUNC, &[if blend.is_some() { OGL_LEQUAL } else { OGL_LESS }], words);
            if let Some(blend) = blend {
                inc(SET_BLEND_COLOR_OP, &blend[..3], words);
                inc(SET_BLEND_CONST_RED, &core::array::from_fn::<_, 4, _>(|i| (blend[3 + i] as f32 / 255.0).to_bits()), words);
            }
            blending = Some(blend);
        }
        if room && first >= model_indices {
            let mut object = IDENTITY;
            if first == light_first { object[3] = [-1.65, light_y, -0.55, 1.0]; }
            let uniforms = core::array::from_fn::<_, 32, _>(|i| [object, view_proj].as_flattened().as_flattened()[i].to_bits());
            inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[128, split(VERTEX_UNIFORMS)[0], split(VERTEX_UNIFORMS)[1], 0], words);
            non_inc(LOAD_CONSTANT_BUFFER, &uniforms, words);
        }
        let table = split(MATERIALS + MATERIAL_STRIDE * u64::from(material));
        inc(SET_CONSTANT_BUFFER_SELECTOR_A, &[48, table[0], table[1]], words);
        inc(BIND_GROUP_CONSTANT_BUFFER + 4 * 32, &[POINTER_BANK << 4 | 1], words);
        inc(BEGIN, &[TRIANGLES], words);
        inc(SET_INDEX_BUFFER_F, &[first], words);
        inc(DRAW_INDEX_BUFFER, &[count], words);
        inc(END, &[0], words);
    }
    let report = split(REPORT);
    inc(SET_REPORT_SEMAPHORE_A, &[report[0], report[1], payload, ONE_WORD], words);
}

#[cfg(feature = "std")]
pub mod assets;

pub fn model(([cx, cy, cz], scale): ([f32; 3], f32), (sin, cos): (f32, f32)) -> Matrix {
    let mut model = [[cos * scale, 0.0, -sin * scale, 0.0], [0.0, scale, 0.0, 0.0], [sin * scale, 0.0, cos * scale, 0.0], [0.0; 4]];
    model[3] = [-(cos * cx + sin * cz) * scale, -cy * scale, (sin * cx - cos * cz) * scale, 1.0];
    model
}

pub fn boombox_pose(seconds: f32) -> ([f32; 3], (f32, f32)) {
    let phase = sin_cos(seconds * 0.4);
    let radius = 3.5 + 0.9 * phase.0;
    let orbit = sin_cos(0.28 * phase.0);
    ([radius * orbit.0, 0.0, EYE[2] - radius * orbit.1], sin_cos(seconds * 0.8))
}

pub fn boombox_model(fit: ([f32; 3], f32), seconds: f32) -> Matrix {
    let (position, rotation) = boombox_pose(seconds);
    let mut model = model(fit, rotation);
    for i in 0..3 { model[3][i] += position[i]; }
    model
}

// Shared range reduction keeps guest and host matrix bits identical without a guest libm.
pub fn sin_cos(angle: f32) -> (f32, f32) {
    let x = f64::from(angle) % core::f64::consts::TAU;
    let x = if x > core::f64::consts::PI { x - core::f64::consts::TAU } else if x < -core::f64::consts::PI { x + core::f64::consts::TAU } else { x };
    let (mut sin, mut cos, mut s, mut c) = (x, 1.0, x, 1.0);
    for n in 1..13 {
        s *= -x * x / ((2 * n) * (2 * n + 1)) as f64;
        c *= -x * x / ((2 * n - 1) * (2 * n)) as f64;
        sin += s;
        cos += c;
    }
    (sin as f32, cos as f32)
}
