//! The SM86 instruction encoders the scene programs need; the host tests build the rest on `alu` and `control`.

pub const PT_GUARD: u128 = 7 << 12;
pub const RC_RZ: u128 = 0xff << 64;
pub const PIXMASK_ALL: u128 = 0xf << 72;
pub const PP_PT: u128 = 7 << 87;
pub const NO_SCOREBOARD: u128 = 0x3f << 110;
pub const NO_WRITE_SCOREBOARD: u128 = 7 << 110;
pub const FMUL_NOSCALE: u128 = 4 << 84;
const MOV_RI: u128 = 0x802;
const MOV_RC: u128 = 0xa02;
const FMUL_RR: u128 = 0x220;
const FFMA_RRR: u128 = 0x223;
const ALD: u128 = 0x321 | 0xff << 24 | 0xff << 32;
const AST: u128 = 0x322 | 0xff << 24 | RC_RZ | NO_WRITE_SCOREBOARD;
const IPA: u128 = 0x326 | 0xff << 32 | 7 << 81;
const BRA: u128 = 0x947;
const EXIT: u128 = 0x94d;

pub const fn mov_imm(rd: u32, imm32: u32) -> u128 {
    MOV_RI | PT_GUARD | PIXMASK_ALL | ((rd as u128) & 0xff) << 16 | (imm32 as u128) << 32
}

pub const fn mov_cbuf(rd: u32, bank: u32, off: u32) -> u128 {
    MOV_RC | PT_GUARD | PIXMASK_ALL | ((rd as u128) & 0xff) << 16 | ((off as u128 >> 2) & 0x3fff) << 40 | ((bank as u128) & 0x1f) << 54
}

pub const fn control(op: u128, ra: u32, offset: i64) -> u128 {
    op | PT_GUARD | PP_PT | NO_SCOREBOARD | (ra as u128 & 0xff) << 24 | ((offset as u128 >> 2) & 0xffff_ffff_ffff) << 34
}

pub const fn bra(offset: i64) -> u128 {
    control(BRA, 0, offset)
}

pub const fn exit() -> u128 {
    control(EXIT, 0, 0)
}

pub const fn alu(op: u128, rd: u32, ra: u32, b: u32) -> u128 {
    op | PT_GUARD | (rd as u128) << 16 | (ra as u128) << 24 | (b as u128) << 32
}

pub const fn fmul(rd: u32, ra: u32, rb: u32) -> u128 {
    alu(FMUL_RR, rd, ra, rb) | FMUL_NOSCALE
}

pub const fn ffma(rd: u32, ra: u32, rb: u32, rc: u32) -> u128 {
    alu(FFMA_RRR, rd, ra, rb) | (rc as u128) << 64
}

pub const fn ald(rd: u32, attr: u32, words: u32) -> u128 {
    ALD | PT_GUARD | (rd as u128) << 16 | (attr as u128) << 40 | (words as u128 - 1) << 74
}

pub const fn ast(attr: u32, rb: u32, words: u32) -> u128 {
    AST | PT_GUARD | (rb as u128) << 32 | (attr as u128) << 40 | (words as u128 - 1) << 74
}

pub const fn ipa(rd: u32, attr: u32, mode: u128) -> u128 {
    IPA | PT_GUARD | mode | (rd as u128) << 16 | (attr as u128 >> 2) << 64
}

pub const IADD3_IMM: u128 = 0x810;

pub const GUARD: u128 = 0xf << 12;

pub const S2R: u128 = 0x919;

pub const IADD3_NO_CARRY: u128 = 0x3fff << 77;

pub const FADD_RR: u128 = 0x221;

pub const FADD_RI: u128 = 0x421;

pub const FMUL_RI: u128 = 0x820;

pub const FFMA_RIR: u128 = 0x823;

pub const MUFU_R: u128 = 0x308;

pub const FSETP_GT: u128 = 0x20b | 4 << 76 | 7 << 84 | PP_PT;

pub const FP_NEG_B: u128 = 1 << 63;

pub const FP_NEG_C: u128 = 1 << 75;

pub fn pp(pred: u32, invert: bool) -> u128 {
    (pred as u128) << 87 | (invert as u128) << 90
}

pub const MUFU_COS: u32 = 0;

pub const MUFU_SIN: u32 = 1;

pub const MUFU_LG2: u32 = 3;

pub const STG: u128 = 0x386;

pub const ADDR_64: u128 = 1 << 72;

pub const SZ_32: u128 = 4 << 73;

pub const COP_EN: u128 = 1 << 84;

pub const F2I: u128 = 0x305;

pub const F2I_64: u128 = 0x311;

pub const I2F: u128 = 0x306;

pub const I2F_64: u128 = 0x312;

pub fn iadd_imm(rd: u32, ra: u32, imm32: u32) -> u128 {
    let mut inst = IADD3_IMM | PT_GUARD | IADD3_NO_CARRY | RC_RZ;
    inst |= ((rd as u128) & 0xff) << 16;
    inst |= ((ra as u128) & 0xff) << 24;
    inst |= ((imm32 as u128) & 0xffffffff) << 32;
    inst
}

pub fn stg(ra: u32, off: u32, rb: u32) -> u128 {
    let mut inst = STG | PT_GUARD | ADDR_64 | SZ_32 | COP_EN | NO_WRITE_SCOREBOARD;
    inst |= ((ra as u128) & 0xff) << 24;
    inst |= ((rb as u128) & 0xff) << 32;
    inst |= ((off as u128) & 0xff_ffff) << 40;
    inst
}

pub fn s2r(rd: u32, sr: u32) -> u128 {
    S2R | PT_GUARD | ((rd as u128) & 0xff) << 16 | ((sr as u128) & 0xff) << 72
}

pub fn fadd(rd: u32, ra: u32, rb: u32) -> u128 {
    alu(FADD_RR, rd, ra, rb)
}

pub fn fadd_imm(rd: u32, ra: u32, imm: f32) -> u128 {
    alu(FADD_RI, rd, ra, imm.to_bits()) | RC_RZ
}

pub fn fmul_imm(rd: u32, ra: u32, imm: f32) -> u128 {
    alu(FMUL_RI, rd, ra, imm.to_bits()) | FMUL_NOSCALE
}

pub fn ffma_imm_b(rd: u32, ra: u32, imm: f32, rc: u32) -> u128 {
    alu(FFMA_RIR, rd, ra, imm.to_bits()) | (rc as u128) << 64
}

pub fn mufu(rd: u32, op: u32, rb: u32) -> u128 {
    alu(MUFU_R, rd, 0, rb) | (op as u128) << 74
}

pub mod cvt {
    pub const F16: u32 = 1;
    pub const F32: u32 = 2;
    pub const F64: u32 = 3;
    pub const U8: u32 = 0;
    pub const S8: u32 = 1;
    pub const U32: u32 = 4;
    pub const S32: u32 = 5;
    pub const U64: u32 = 6;
    pub const S64: u32 = 7;
    pub const RN: u32 = 0;
    pub const RM: u32 = 1;
    pub const RP: u32 = 2;
    pub const RZ: u32 = 3;
}

pub fn f2i(rd: u32, rb: u32, dst: u32, src: u32, rnd: u32) -> u128 {
    let op = if src == cvt::F64 { F2I_64 } else { F2I };
    alu(op, rd, 0, rb) | (dst as u128 & 1) << 72 | (dst as u128 >> 1) << 75 | (src as u128) << 84 | (rnd as u128) << 78
}

pub fn i2f(rd: u32, rb: u32, dst: u32, src: u32) -> u128 {
    let op = if dst == cvt::F64 || src >= cvt::U64 { I2F_64 } else { I2F };
    alu(op, rd, 0, rb) | (dst as u128) << 75 | (src as u128 & 1) << 74 | (src as u128 >> 1) << 84
}

pub fn guard(inst: u128, pred: u32, invert: bool) -> u128 {
    inst & !GUARD | (pred as u128 & 0x7) << 12 | (invert as u128) << 15
}

pub const LOP3: u128 = 0x212;

pub const IMM_B: u128 = 5 << 9;

pub const S32: u128 = 1 << 73;

pub fn alu3(op: u128, rd: u32, ra: u32, b: u32, rc: u32) -> u128 {
    alu(op, rd, ra, b) | (rc as u128) << 64
}

pub fn lop3_fields(lut: u8, pu: u32, pp: u32, pp_not: bool) -> u128 {
    (lut as u128) << 72 | (pu as u128) << 81 | (pp as u128) << 87 | (pp_not as u128) << 90
}
