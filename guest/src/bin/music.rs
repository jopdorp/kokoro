#![no_std]
#![no_main]

use guest::{audren, display, nv, svc};
include!(concat!(env!("OUT_DIR"), "/music.rs"));
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

fn draw() -> svc::Result<()> {
    let size = [WIDTH, HEIGHT];
    let heap = svc::set_heap_size(0x200_0000)?;
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
    let count = 64;
    let instances = nv::guest_slice::<[f32; 12]>(memory + INSTANCES, count);
    let pcm = include_bytes!(concat!(env!("OUT_DIR"), "/music-pcm.bin"));
    let audio_memory = heap + 0x100_0000;
    let samples = SAMPLES.map(|(offset, size)| (audio_memory + offset, size));
    let mut notes = [scene::sound::Note::default(); 8];
    let mut sound = audren::Sound::new(audio_memory, pcm, &samples, &mut notes)?;
    let mut words = Words { storage: nv::guest_slice(memory, 0x1000), len: 0 };
    init_segment(base, count as u32, &mut words);
    channel.submit(base, words.len)?;
    let window = display::Window::target(&channel.nv, channel.nvmap, handle, size, TARGET as u32)?;
    let mut tick = 0u64;
    loop {
        let mut peaks = [0.0f32; 2];
        while sound.periods < (f64::from(tick as f32 / 60.0) * 200.0) as u64 + 10 {
            scene::sound::sequence(sound.periods, |n| PITCHES[(n + 12) as usize], |voice, sample, pitch, volume, pan| sound.trigger(voice, sample, pitch, volume, pan));
            let period = sound.period(&BIQUAD)?;
            for channel in 0..2 { peaks[channel] = peaks[channel].max(period[channel]); }
        }
        for (i, quad) in instances.iter_mut().enumerate() {
            let (channel, row) = (i / 32, i % 32);
            let active = (row as f32 / 32.0) < peaks[channel];
            *quad = [-0.58 + channel as f32 * 1.16, -0.8 + row as f32 * 0.052, 0.0, 1.68, 0.0, 0.0, 0.0, 0.0, 0.04, if active { 0.6 } else { 0.08 }, if active { 0.4 } else { 0.1 }, if active { 8.0 } else { 1.0 }];
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
        let _ = svc::output_debug_string("music failed");
    }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("music panicked");
    svc::exit_process()
}
