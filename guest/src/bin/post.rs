#![no_std]
#![no_main]

use guest::{display, nv, svc};
use scene::{Words, post::*};

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
    let (scene_size, out) = ([1280, 720], [1920, 1080]);
    let target = (SCENE + u64::from(scene_size[0] * scene_size[1] * 4)).next_multiple_of(0x1_0000);
    let mapped = (target + u64::from(out[0] * out[1] * 4) - BASE).next_multiple_of(0x1_0000);
    let heap = svc::set_heap_size((mapped + 0x10_0000).next_multiple_of(0x20_0000))?;
    let memory = heap + 0x10_0000;
    let channel = nv::Channel::new()?;
    let (_, handle) = channel.buffer_at(memory, mapped as u32, BASE)?;
    for (va, program) in [
        (SCENE_VERTEX, &include_bytes!("../../../core/tests/fixtures/driver-shaders/triangle.vert.bin")[..]),
        (SCENE_PIXEL, include_bytes!("../../../core/tests/fixtures/driver-shaders/triangle.frag.bin")),
        (POST_VERTEX, include_bytes!("../../../core/tests/fixtures/driver-shaders/post.vert.bin")),
        (POST_PIXEL, include_bytes!("../../../core/tests/fixtures/driver-shaders/post.frag.bin")),
    ] {
        nv::guest_slice(memory + va - BASE, program.len()).copy_from_slice(program);
        nv::guest_slice(memory + va - BASE + program.len() as u64, 16).copy_from_slice(&scene::sass::bra(-16).to_le_bytes());
    }
    let [width, height] = scene_size.map(|v| v as f32);
    nv::guest_slice(memory + UNIFORMS - BASE, 8).copy_from_slice(&[0.0, 6.0, 0.35, 0.55, width, height, 1.0 / width, 1.0 / height]);
    nv::guest_slice(memory + POINTERS - BASE, 4).copy_from_slice(&[TEXTURE_HANDLE, 0, (UNIFORMS >> 4) as u32, (UNIFORMS >> 36) as u32 | 32 >> 4 << 13]);
    nv::guest_slice(memory + TIC - BASE + 4 * 32, 8).copy_from_slice(&scene::pitch_rgba8_tic(SCENE, scene_size[0], scene_size[1], scene_size[0] * 4));
    nv::guest_slice(memory + TSC - BASE + 32, 2).copy_from_slice(&[2u32 | 2 << 3 | 2 << 6, 2 | 2 << 4 | 1 << 6]);
    let mut words = Words { storage: nv::guest_slice(memory, 0x1000), len: 0 };
    init(&mut words);
    channel.submit(PUSHBUFFER, words.len)?;
    let window = display::Window::target(&channel.nv, channel.nvmap, handle, out, (target - BASE) as u32)?;
    let mut tick = 0u32;
    loop {
        let seconds = tick as f32 / 60.0;
        nv::guest_slice(memory + VERTICES - BASE, 3 * BLADES).copy_from_slice(&vertices(scene_size, seconds));
        words.len = 0;
        frame(scene_size, out, target, seconds, tick + 1, &mut words);
        channel.submit(PUSHBUFFER, words.len)?;
        window.present(out)?;
        tick += 1;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    if draw().is_err() { let _ = svc::output_debug_string("post failed"); }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("post panicked");
    svc::exit_process()
}
