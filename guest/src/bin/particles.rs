#![no_std]
#![no_main]

use guest::{display, nv, svc};
use scene::{Words, particles::*};

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

fn sqrt(mut value: f32) -> f32 {
    unsafe { core::arch::asm!("fsqrt {v:s}, {v:s}", v = inout(vreg) value, options(pure, nomem, nostack)); }
    value
}

fn draw() -> svc::Result<()> {
    let size = [WIDTH, HEIGHT];
    let heap = svc::set_heap_size(0x100_0000)?;
    let memory = heap + 0x10_0000;
    let channel = nv::Channel::new()?;
    let (_, target_handle) = channel.buffer_at(memory, MAPPED as u32, BASE)?;
    for (va, program) in [
        (VERTEX_PROGRAM, &include_bytes!("../../../core/tests/fixtures/driver-shaders/particles.vert.bin")[..]),
        (PIXEL_PROGRAM, include_bytes!("../../../core/tests/fixtures/driver-shaders/particles.frag.bin")),
        (UPDATE_PROGRAM, include_bytes!("../../../core/tests/fixtures/driver-shaders/particles_update.comp.bin")),
    ] {
        nv::guest_slice(memory + va - BASE, program.len()).copy_from_slice(program);
        nv::guest_slice(memory + va - BASE + program.len() as u64, 16).copy_from_slice(&scene::sass::bra(-16).to_le_bytes());
    }
    nv::guest_slice(memory + VS_POINTERS - BASE, 2).copy_from_slice(&handle(VS_UNIFORMS, 0x100));
    let mut cs_c0 = [0; 19];
    [cs_c0[12], cs_c0[13]] = handle(CS_UNIFORMS, 0x100);
    [cs_c0[16], cs_c0[17], cs_c0[18]] = [PARTICLES as u32, 0, PARTICLE_BYTES];
    nv::guest_slice(memory + CS_C0 - BASE, cs_c0.len()).copy_from_slice(&cs_c0);
    nv::guest_slice(memory + CS_UNIFORMS - BASE + 48, 1)[0] = COUNT;
    let particles = include_bytes!(concat!(env!("OUT_DIR"), "/particles.bin"));
    nv::guest_slice(memory + PARTICLES - BASE, particles.len()).copy_from_slice(particles);
    let background = include_bytes!(concat!(env!("OUT_DIR"), "/particles-background.bin"));
    let mut words = Words { storage: nv::guest_slice(memory + DRAW_SEGMENT - BASE, 0x14000), len: 0 };
    init(&mut words);
    channel.submit(DRAW_SEGMENT, words.len)?;
    let window = display::Window::target(&channel.nv, channel.nvmap, target_handle, size, (TARGET - BASE) as u32)?;
    let mut step = 0u32;
    loop {
        let seconds = (step as f32 - WARM_UP_STEPS as f32) / 60.0;
        step += 1;
        nv::guest_slice(memory + QMD - BASE, 64).copy_from_slice(&qmd(step));
        words.len = 0;
        update(seconds, &mut words);
        channel.submit(DRAW_SEGMENT, words.len)?;
        if step <= WARM_UP_STEPS { continue; }
        nv::guest_slice(memory + TARGET - BASE, background.len()).copy_from_slice(background);
        words.len = 0;
        scene::particles::draw(seconds, step, sqrt, &mut words);
        channel.submit(DRAW_SEGMENT, words.len)?;
        window.present(size)?;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    if draw().is_err() { let _ = svc::output_debug_string("particles failed"); }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("particles panicked");
    svc::exit_process()
}
