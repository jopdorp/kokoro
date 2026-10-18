#![no_std]
#![no_main]

use guest::{audren, display, nv, svc};
use scene::{Words, bounce::*};

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

fn hypot(x: f32, y: f32) -> f32 {
    let mut square = f64::from(x) * f64::from(x) + f64::from(y) * f64::from(y);
    unsafe {
        core::arch::asm!("fsqrt {s:d}, {s:d}", s = inout(vreg) square, options(pure, nomem, nostack));
    }
    square as f32
}

fn draw() -> svc::Result<()> {
    let size = [WIDTH, HEIGHT];
    let heap = svc::set_heap_size(0x200_0000)?;
    let audio_memory = heap + 0x100_0000;
    let samples = [(audio_memory, 6000)];
    let mut notes = [scene::sound::Note::default(); 16];
    let mut sound = audren::Sound::new(audio_memory, include_bytes!(concat!(env!("OUT_DIR"), "/bounce-pcm.bin")), &samples, &mut notes)?;
    let biquad = include_bytes!(concat!(env!("OUT_DIR"), "/biquad.bin"));
    let channel = nv::Channel::new()?;
    let memory = heap + 0x10_0000;
    let (base, handle) = channel.buffer(memory, MAPPED as u32)?;
    for (offset, program) in [
        (VERTEX_PROGRAM, &include_bytes!("../../../core/tests/fixtures/driver-shaders/particles.vert.bin")[..]),
        (PIXEL_PROGRAM, &include_bytes!("../../../core/tests/fixtures/driver-shaders/particles.frag.bin")[..]),
    ] {
        nv::guest_slice(memory + offset, program.len()).copy_from_slice(program);
        nv::guest_slice(memory + offset + program.len() as u64, 16).copy_from_slice(&scene::sass::bra(-16).to_le_bytes());
    }
    nv::guest_slice(memory + VS_POINTERS, 2).copy_from_slice(&pointers(base));
    nv::guest_slice(memory + VS_UNIFORMS, 24).copy_from_slice(&uniforms());
    let initial = include_bytes!(concat!(env!("OUT_DIR"), "/bounce-instances.bin"));
    nv::guest_slice(memory + INSTANCES, initial.len()).copy_from_slice(initial);
    let count = initial.len() / 48;
    let instances = nv::guest_slice::<[f32; 12]>(memory + INSTANCES, count);
    let mut words = Words { storage: nv::guest_slice(memory, 0x1000), len: 0 };
    init_segment(base, count as u32, &mut words);
    channel.submit(base, words.len)?;
    let window = display::Window::target(&channel.nv, channel.nvmap, handle, size, TARGET as u32)?;
    let (mut balls, mut tick) = (balls(), 0u64);
    loop {
        while sound.periods < (f64::from(tick as f32 / 60.0) * 200.0) as u64 + 10 {
            // The host pans impacts using the period's final positions; a second pass avoids storing an unbounded impact list.
            let mut final_balls = balls;
            physics(&mut final_balls, hypot, |_, _| {});
            physics(&mut balls, hypot, |i, speed| {
                if speed > SOFT {
                    let ball = &final_balls[i];
                    sound.trigger(sound.free_voice(), 0, 0.08 / ball.radius, (speed / 8.0).min(0.6), ball.at[0] / HALF[0]);
                }
            });
            sound.period(biquad)?;
        }
        for (ball, quads) in balls.iter().zip(instances[count - 2 * BALLS..].chunks_exact_mut(2)) {
            quads[0][..2].copy_from_slice(&ball.at);
            quads[1][..2].copy_from_slice(&[ball.at[0] - 0.3 * ball.radius, ball.at[1] + 0.3 * ball.radius]);
        }
        words.len = 0;
        frame(base, count, tick as u32 + 1, &mut words);
        channel.submit(base, words.len)?;
        window.present(size)?;
        tick += 1;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    if draw().is_err() {
        let _ = svc::output_debug_string("bounce failed");
    }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("bounce panicked");
    svc::exit_process()
}
