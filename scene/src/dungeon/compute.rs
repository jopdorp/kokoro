//! Dungeon compute QMD and descriptor-bank encoders.

use crate::host::SET_OBJECT;
use crate::particles::{compute, handle};

#[derive(Clone, Copy)]
pub struct Buffer {
    pub address: u64,
    pub size: u32,
}

#[derive(Clone, Copy)]
pub struct Pass {
    pub program: u64,
    pub qmd: u64,
    pub cbuf: u64,
    pub release: u64,
}

pub struct Encoded<const N: usize> {
    pub qmd: [u32; 64],
    pub cbuf: [u32; N],
}

fn ssbo<const N: usize>(cbuf: &mut [u32; N], word: usize, buffer: Buffer) {
    cbuf[word..word + 3].copy_from_slice(&[
        buffer.address as u32,
        (buffer.address >> 32) as u32,
        buffer.size,
    ]);
}

fn encode<const N: usize>(
    pass: Pass,
    grid: [u32; 3],
    block: [u32; 3],
    registers: u32,
    payload: u32,
    cbuf: [u32; N],
) -> Encoded<N> {
    let mut qmd = [0; 64];
    [qmd[4], qmd[5]] = [0x40, 0xa000_0000];
    [qmd[12], qmd[13], qmd[14]] = grid;
    [qmd[18], qmd[19], qmd[20]] = [
        block[0] << 16 | 3 << 4,
        block[2] << 16 | block[1],
        registers << 8 | 1,
    ];
    [qmd[24], qmd[25], qmd[26]] = [
        pass.release as u32,
        0x4088_0000 | (pass.release >> 32) as u32,
        payload,
    ];
    [qmd[32], qmd[33]] = [
        pass.cbuf as u32,
        (pass.cbuf >> 32) as u32 | (N as u32 / 4) << 19,
    ];
    [qmd[48], qmd[49]] = [pass.program as u32, (pass.program >> 32) as u32];
    Encoded { qmd, cbuf }
}

pub fn linear<const N: usize>(
    pass: Pass,
    buffers: [Buffer; N],
    uniforms: Buffer,
    count: u32,
    registers: u32,
    payload: u32,
) -> Encoded<64> {
    let mut cbuf = [0; 64];
    cbuf[12..14].copy_from_slice(&handle(uniforms.address, uniforms.size));
    for (i, buffer) in buffers.into_iter().enumerate() {
        ssbo(&mut cbuf, 16 + i * 4, buffer);
    }
    encode(
        pass,
        [count.div_ceil(256), 1, 1],
        [256, 1, 1],
        registers,
        payload,
        cbuf,
    )
}

pub fn water(
    pass: Pass,
    previous: Buffer,
    next: Buffer,
    surface: Buffer,
    uniforms: Buffer,
    size: [u32; 2],
    payload: u32,
) -> Encoded<28> {
    let mut cbuf = [0; 28];
    cbuf[12..14].copy_from_slice(&handle(uniforms.address, uniforms.size));
    ssbo(&mut cbuf, 16, previous);
    ssbo(&mut cbuf, 20, next);
    ssbo(&mut cbuf, 24, surface);
    encode(
        pass,
        [size[0].div_ceil(8), size[1].div_ceil(8), 1],
        [8, 8, 1],
        24,
        payload,
        cbuf,
    )
}

pub fn particles(
    pass: Pass,
    lights: Buffer,
    particles: Buffer,
    uniforms: Buffer,
    count: u32,
    payload: u32,
) -> Encoded<24> {
    let mut cbuf = [0; 24];
    cbuf[12..14].copy_from_slice(&handle(uniforms.address, uniforms.size));
    ssbo(&mut cbuf, 16, lights);
    ssbo(&mut cbuf, 20, particles);
    encode(
        pass,
        [count.div_ceil(64), 1, 1],
        [64, 1, 1],
        62,
        payload,
        cbuf,
    )
}

pub fn fog(
    pass: Pass,
    [hdr, fog, uniforms, lights, bloom]: [Buffer; 5],
    size: [u32; 2],
    payload: u32,
) -> Encoded<32> {
    let mut cbuf = [0; 32];
    cbuf[12..14].copy_from_slice(&handle(uniforms.address, uniforms.size));
    ssbo(&mut cbuf, 16, hdr);
    ssbo(&mut cbuf, 20, fog);
    ssbo(&mut cbuf, 24, lights);
    ssbo(&mut cbuf, 28, bloom);
    encode(
        pass,
        [(size[0] / 4).div_ceil(8), (size[1] / 4).div_ceil(8), 1],
        [8, 8, 1],
        43,
        payload,
        cbuf,
    )
}

pub fn post(
    pass: Pass,
    [hdr, output, uniforms, lights, fog, bloom]: [Buffer; 6],
    size: [u32; 2],
    payload: u32,
) -> Encoded<36> {
    let mut cbuf = [0; 36];
    cbuf[12..14].copy_from_slice(&handle(uniforms.address, uniforms.size));
    ssbo(&mut cbuf, 16, hdr);
    ssbo(&mut cbuf, 20, output);
    ssbo(&mut cbuf, 24, lights);
    ssbo(&mut cbuf, 28, fog);
    ssbo(&mut cbuf, 32, bloom);
    encode(
        pass,
        [size[0].div_ceil(8), size[1].div_ceil(8), 1],
        [8, 8, 1],
        70,
        payload,
        cbuf,
    )
}

pub fn post_volume(
    pass: Pass,
    [hdr, output, uniforms, lights, fog, bloom, volume]: [Buffer; 7],
    size: [u32; 2],
    registers: u32,
    payload: u32,
) -> Encoded<40> {
    let mut cbuf = [0; 40];
    cbuf[12..14].copy_from_slice(&handle(uniforms.address, uniforms.size));
    for (i, buffer) in [hdr, output, lights, fog, bloom, volume]
        .into_iter()
        .enumerate()
    {
        ssbo(&mut cbuf, 16 + i * 4, buffer);
    }
    encode(
        pass,
        [size[0].div_ceil(8), size[1].div_ceil(8), 1],
        [8, 8, 1],
        registers,
        payload,
        cbuf,
    )
}

pub fn dispatch(pass: Pass, words: &mut impl Extend<u32>) {
    compute(SET_OBJECT, &[0xc7c0], words);
    compute(0x02b4, &[(pass.qmd >> 8) as u32], words);
    compute(0x02c0, &[3], words);
}
