#![no_std]
#![no_main]

use guest::{display, nv, svc};
use scene::{Words, sprites::*};

core::arch::global_asm!(
    ".section .text._start, \"ax\"",
    ".global _start",
    "_start:",
    "b main",
    ".word 0, 0, 0",
    ".ascii \"NRO0\"",
    ".word 0, __bss - _start, 0",
    ".word 0, __ro - _start, __ro - _start, __ro_size, __data - _start, __data_size",
    ".word __bss_size, 0",
    ".space 0x40",
);

const FULL: [u32; 2] = [1920, 1080];
const HALF: [u32; 2] = [960, 540];
const HALF_TARGET: u64 = 0x190_0000;
const PROGRAM: u64 = 0x1b0_0000;
const CBUF: u64 = 0x1b1_0000;
const QMD: u64 = 0x1b1_1000;
const MAPPED: u32 = 0x1c0_0000;

fn qmd(base: u64) -> [u32; 64] {
    let mut qmd = [0; 64];
    [qmd[4], qmd[5]] = [0x40, 0xfc00_0000];
    [qmd[12], qmd[13], qmd[14]] = [120, 68, 1];
    [qmd[17], qmd[18], qmd[19], qmd[20]] = [1024, 32 << 16 | 3 << 4, 1 << 16 | 1, 48 << 8 | 1];
    [qmd[32], qmd[33]] = [(base + CBUF) as u32, ((base + CBUF) >> 32) as u32 | 0x17 << 19];
    [qmd[48], qmd[49]] = [(base + PROGRAM) as u32, ((base + PROGRAM) >> 32) as u32];
    qmd
}

fn draw() -> svc::Result<()> {
    let heap = svc::set_heap_size(0x300_0000)?;
    let memory = heap + 0x10_0000;
    let channel = nv::Channel::new()?;
    let (base, handle) = channel.buffer(memory, MAPPED)?;
    for (offset, program) in [
        (VERTEX_PROGRAM, &include_bytes!("../../../core/tests/fixtures/driver-shaders/sprite2d.vert.bin")[..]),
        (PIXEL_PROGRAM, &include_bytes!("../../../core/tests/fixtures/driver-shaders/sprite2d.frag.bin")[..]),
    ] {
        nv::guest_slice(memory + offset, program.len()).copy_from_slice(program);
        nv::guest_slice(memory + offset + program.len() as u64, 16).copy_from_slice(&scene::sass::bra(-16).to_le_bytes());
    }
    for (offset, words) in descriptors(base) {
        nv::guest_slice(memory + offset, words.len()).copy_from_slice(&words);
    }
    let atlas = include_bytes!(concat!(env!("OUT_DIR"), "/sprites-atlas.bin"));
    nv::guest_slice(memory + ATLAS, atlas.len()).copy_from_slice(atlas);
    let instances = include_bytes!(concat!(env!("OUT_DIR"), "/sprites-instances.bin"));
    nv::guest_slice(memory + INSTANCES, instances.len()).copy_from_slice(instances);
    let program = include_bytes!("../../../core/tests/fixtures/tensor-upscale-sm86.bin");
    nv::guest_slice(memory + PROGRAM, program.len()).copy_from_slice(program);
    let cbuf = include_bytes!("../../../core/tests/fixtures/tensor-upscale-cbuf0.bin");
    nv::guest_slice(memory + CBUF, cbuf.len()).copy_from_slice(cbuf);
    nv::guest_slice::<u64>(memory + CBUF + 0x160, 2).copy_from_slice(&[base + HALF_TARGET, base + TARGET]);
    nv::guest_slice(memory + QMD, 64).copy_from_slice(&qmd(base));
    let window = display::Window::target(&channel.nv, channel.nvmap, handle, FULL, TARGET as u32)?;
    let mut words = Words { storage: nv::guest_slice(memory + PUSHBUFFER, 0x1000), len: 0 };
    let mut tick = 0u32;
    loop {
        let scroll = tick as f32 * 2.0;
        tick += 1;
        words.len = 0;
        init_segment_at(base, base + TARGET, FULL, &mut words);
        frame(base, scroll, SUNSET, &mut words);
        channel.submit(base + PUSHBUFFER, words.len)?;
        words.len = 0;
        init_segment_at(base, base + HALF_TARGET, HALF, &mut words);
        frame(base, scroll, SUNSET, &mut words);
        channel.submit(base + PUSHBUFFER, words.len)?;
        words.len = 0;
        scene::particles::compute(scene::host::SET_OBJECT, &[0xc7c0], &mut words);
        scene::particles::compute(scene::particles::SEND_PCAS_A, &[((base + QMD) >> 8) as u32], &mut words);
        scene::particles::compute(scene::particles::SEND_SIGNALING_PCAS2_B, &[scene::particles::INVALIDATE_COPY_SCHEDULE], &mut words);
        channel.submit(base + PUSHBUFFER, words.len)?;
        window.present(FULL)?;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    if draw().is_err() { let _ = svc::output_debug_string("tensor upscale failed"); }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("tensor upscale panicked");
    svc::exit_process()
}
