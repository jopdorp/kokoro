#![no_std]
#![no_main]

use core::ffi::CStr;
use guest::{display, nv, svc};
use scene::{Words, particles::handle, solar::*};

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

fn showcase(config: *const [u64; 3]) -> bool {
    for entry in (0..).map(|i| unsafe { *config.add(i) }).take_while(|entry| entry[0] as u32 != 0) {
        if entry[0] as u32 == 5 {
            let argv = unsafe { CStr::from_ptr(entry[2] as *const _) }.to_bytes();
            return argv.windows(8).any(|word| word == b"showcase");
        }
    }
    false
}

fn draw(showcase: bool) -> svc::Result<()> {
    let (count, steps, dt) = if showcase { (SHOWCASE_COUNT, 3, 0.015) } else { (REALTIME_COUNT, 1, 0.02) };
    let size = [WIDTH, HEIGHT];
    let heap = svc::set_heap_size(0x100_0000)?;
    let memory = heap + 0x10_0000;
    let channel = nv::Channel::new()?;
    let (_, target_handle) = channel.buffer_at(memory, MAPPED as u32, BASE)?;
    for (va, program) in [
        (VERTEX_PROGRAM, &include_bytes!("../../../core/tests/fixtures/driver-shaders/particles.vert.bin")[..]),
        (PIXEL_PROGRAM, include_bytes!("../../../core/tests/fixtures/driver-shaders/particles.frag.bin")),
        (STEP_PROGRAM, include_bytes!("../../../core/tests/fixtures/solar/solar-step-sm86.bin")),
    ] {
        nv::guest_slice(memory + va - BASE, program.len()).copy_from_slice(program);
        nv::guest_slice(memory + va - BASE + program.len() as u64, 16).copy_from_slice(&scene::sass::bra(-16).to_le_bytes());
    }
    nv::guest_slice(memory + VS_POINTERS - BASE, 2).copy_from_slice(&handle(VS_UNIFORMS, 0x100));
    cbuf0(nv::guest_slice(memory + CS_C0 - BASE, 0x200), count);
    nv::guest_slice(memory + BODY_QMD - BASE, 64).copy_from_slice(&qmd(count.div_ceil(THREADS)));
    nv::guest_slice(memory + CELL_QMD - BASE, 64).copy_from_slice(&qmd(CELLS / THREADS));
    let bodies = include_bytes!(concat!(env!("OUT_DIR"), "/solar-bodies.bin"));
    nv::guest_slice(memory + BODIES - BASE, count as usize * BODY_BYTES as usize).copy_from_slice(&bodies[..count as usize * BODY_BYTES as usize]);
    nv::guest_slice::<u8>(memory + BODIES - BASE + u64::from(count * BODY_BYTES), (TRAIL_COUNT * BODY_BYTES) as usize).fill(0);
    let stars = include_bytes!(concat!(env!("OUT_DIR"), "/solar-stars.bin"));
    nv::guest_slice(memory + BODIES - BASE + u64::from((count + TRAIL_COUNT) * BODY_BYTES), stars.len()).copy_from_slice(stars);
    nv::guest_slice::<u8>(memory + BODIES - BASE + u64::from((count + TRAIL_COUNT + STAR_COUNT) * BODY_BYTES), (HUD_COUNT * BODY_BYTES) as usize).fill(0);
    nv::guest_slice::<u8>(memory + SCRATCH - BASE + u64::from(count * BODY_BYTES), (TRAIL_COUNT * BODY_BYTES) as usize).fill(0);
    nv::guest_slice(memory + SCRATCH - BASE + u64::from((count + TRAIL_COUNT) * BODY_BYTES), stars.len()).copy_from_slice(stars);
    nv::guest_slice::<u8>(memory + SCRATCH - BASE + u64::from((count + TRAIL_COUNT + STAR_COUNT) * BODY_BYTES), (HUD_COUNT * BODY_BYTES) as usize).fill(0);
    let mut words = Words { storage: nv::guest_slice(memory + INIT_SEGMENT - BASE, 0x1000), len: 0 };
    init(count, &mut words);
    channel.submit(INIT_SEGMENT, words.len)?;
    let window = display::Window::target(&channel.nv, channel.nvmap, target_handle, size, (TARGET - BASE) as u32)?;
    let (mut frame, mut time) = (0u32, 0.0f32);
    let mut source = BODIES;
    loop {
        frame += 1;
        for _ in 0..steps {
            words.storage = nv::guest_slice(memory + STEP_SEGMENT - BASE, 0x2000);
            words.len = 0;
            let target = if source == BODIES { SCRATCH } else { BODIES };
            step(count, dt, time, frame % 32, frame % 8 == 0, source, target, &mut words);
            channel.submit(STEP_SEGMENT, words.len)?;
            source = target;
            time += dt;
        }
        words.storage = nv::guest_slice(memory + DRAW_SEGMENT - BASE, 0x14000);
        words.len = 0;
        let instances = nv::guest_slice::<f32>(memory + source - BASE, (count + EXTRA_COUNT) as usize * 12);
        let (bodies, extras) = instances.split_at_mut(count as usize * 12);
        scene::solar::hud(frame as f32 / 60.0, (time * 2_500.0) as u32, bodies,
            &mut extras[((TRAIL_COUNT + STAR_COUNT) * 12) as usize..], sqrt);
        let [hi, lo] = scene::split(source);
        scene::inc(scene::threed::SET_VERTEX_STREAM_A_FORMAT + 4, &[hi, lo], &mut words);
        scene::solar::draw(frame as f32 / 60.0, count, frame, sqrt, &mut words);
        channel.submit(DRAW_SEGMENT, words.len)?;
        window.present(size)?;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main(config: *const [u64; 3]) -> ! {
    if draw(showcase(config)).is_err() { let _ = svc::output_debug_string("solar failed"); }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("solar panicked");
    svc::exit_process()
}
