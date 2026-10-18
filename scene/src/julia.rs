//! The Julia shader and launch layout shared by the guest and host oracle.
pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
pub const BASE: u64 = 0x2_0000_1000;
pub use crate::sass::PP_PT;
use crate::sass::S32;
pub const IMAD: u128 = 0x224 | S32 | 7 << 81 | PP_PT;
pub const IMAD_WIDE_U32: u128 = 0x225 | 7 << 81 | PP_PT;
pub const ISETP_GE_U32: u128 = 0x20c | 6 << 76 | 7 << 68 | 7 << 84;
pub const BOP_OR: u128 = 1 << 74;
pub const P0: u32 = 0;
pub const RZ: u32 = 255;
pub const GROUP: u32 = 16;
pub const MAX_ITERATIONS: u32 = 64;
pub const ESCAPE_RADIUS_SQUARED: f32 = 256.0;
pub const UNITS_PER_PIXEL: f32 = 2.6 / HEIGHT as f32;
pub const C_RADIUS: f32 = 0.7885;
pub const SECONDS_PER_TURN: f32 = 16.0;

#[cfg(feature = "std")]
mod code {
use std::prelude::v1::*;
use crate::sass::*;
use super::*;
// Runs `colour` with x in R0 and y in R2, then stores R1 as the pixel's RGBA8.
pub fn pixel_program(colour: &[u128]) -> Vec<u128> {
    let head = [
        s2r(0, 37),
        s2r(1, 33),
        s2r(2, 38),
        s2r(3, 34),
        alu3(IMAD ^ IMM_B, 0, 0, GROUP, 1),
        alu3(IMAD ^ IMM_B, 2, 2, GROUP, 3),
        alu(ISETP_GE_U32 ^ IMM_B | PP_PT, 0, 0, WIDTH),
        alu(ISETP_GE_U32 ^ IMM_B | BOP_OR, 0, 2, HEIGHT),
        guard(exit(), P0, false),
    ];
    let store = [
        alu3(IMAD ^ IMM_B, 0, 2, WIDTH, 0),
        mov_imm(4, BASE as u32),
        mov_imm(5, (BASE >> 32) as u32),
        alu3(IMAD_WIDE_U32 ^ IMM_B, 4, 0, 4, 4),
        stg(4, 0, 1),
        exit(),
    ];
    [&head, colour, &store].concat()
}

// Julia set z = z * z + c with c circling the origin as the time in turns at c[0x0][0x0] grows,
// coloured by i - log2(log2(|z|^2)) through a cosine palette and black where z never escapes.
pub fn julia_program() -> Vec<u128> {
    let channel = |rd, phase| [fadd_imm(rd, 14, phase), mufu(rd, MUFU_COS, rd), ffma_imm_b(rd, rd, 127.5, 15), f2i(rd, rd, cvt::S32, cvt::F32, cvt::RN)];
    let iterate = [
        fmul(12, 7, 7),
        ffma(13, 6, 6, 12),
        alu(FSETP_GT ^ IMM_B, 0, 13, ESCAPE_RADIUS_SQUARED.to_bits()),
        alu(ISETP_GE_U32 ^ IMM_B | BOP_OR, 0, 10, MAX_ITERATIONS),
        guard(bra(6 * 16), P0, false),
        fmul(14, 6, 7),
        ffma_imm_b(7, 14, 2.0, 9),
        ffma(14, 6, 6, 12) | FP_NEG_C,
        fadd(6, 14, 8),
        iadd_imm(10, 10, 1),
        bra(-11 * 16),
    ];
    let start = [
        i2f(6, 0, cvt::F32, cvt::S32),
        i2f(7, 2, cvt::F32, cvt::S32),
        fadd_imm(6, 6, -(WIDTH as f32) / 2.0),
        fmul_imm(6, 6, UNITS_PER_PIXEL),
        fadd_imm(7, 7, -(HEIGHT as f32) / 2.0),
        fmul_imm(7, 7, -UNITS_PER_PIXEL),
        mov_cbuf(11, 0, 0),
        mufu(8, MUFU_COS, 11),
        mufu(9, MUFU_SIN, 11),
        fmul_imm(8, 8, C_RADIUS),
        fmul_imm(9, 9, C_RADIUS),
        mov_imm(10, 0),
    ];
    let shade = [
        mufu(13, MUFU_LG2, 13),
        mufu(13, MUFU_LG2, 13),
        i2f(14, 10, cvt::F32, cvt::S32),
        fadd(14, 14, 13) | FP_NEG_B,
        ffma_imm_b(14, 14, 0.07, 11),
        mov_imm(15, 127.5f32.to_bits()),
    ];
    let pack = [
        alu3(IMAD ^ IMM_B, 1, 17, 0x100, 16),
        alu3(IMAD ^ IMM_B, 1, 18, 0x1_0000, 1),
        alu3(LOP3 ^ IMM_B | lop3_fields(0xfc, 7, 7, true), 1, 1, 0xff00_0000, RZ),
        alu(ISETP_GE_U32 ^ IMM_B | PP_PT, 0, 10, MAX_ITERATIONS),
        guard(mov_imm(1, 0xff00_0000), P0, false),
    ];
    pixel_program(&[&start[..], &iterate, &shade, &channel(16, 0.0), &channel(17, 0.1), &channel(18, 0.2), &pack].concat())
}

}
#[cfg(feature = "std")]
pub use code::{julia_program, pixel_program};

pub const COMMANDS: u64 = BASE - 0x1000 + 0x80_0000;
pub const PROGRAM: u64 = COMMANDS + 0x1_0000;
pub const QMD: u64 = COMMANDS + 0x2_0000;
pub const CBUF: u64 = COMMANDS + 0x3_0000;
pub const RELEASE: u64 = COMMANDS + 0x4_0000;
pub fn qmd(payload: u32) -> [u32; 64] {
    let mut qmd = [0; 64];
    [qmd[4], qmd[5]] = [0x40, 0xa000_0000];
    [qmd[12], qmd[13], qmd[14]] = [WIDTH.div_ceil(GROUP), HEIGHT.div_ceil(GROUP), 1];
    [qmd[18], qmd[19], qmd[20]] = [GROUP << 16 | 3 << 4, 1 << 16 | GROUP, 19 << 8 | 1];
    [qmd[24], qmd[25], qmd[26]] = [RELEASE as u32, 0x4088_0000 | (RELEASE >> 32) as u32, payload];
    [qmd[32], qmd[33]] = [CBUF as u32, (CBUF >> 32) as u32 | 16 >> 4 << 19];
    [qmd[48], qmd[49]] = [PROGRAM as u32, (PROGRAM >> 32) as u32];
    qmd
}
pub fn frame(words: &mut impl Extend<u32>) {
    crate::particles::compute(crate::host::SET_OBJECT, &[0xc7c0], words);
    crate::particles::compute(0x02b4, &[(QMD >> 8) as u32], words);
    crate::particles::compute(0x02c0, &[3], words);
}
