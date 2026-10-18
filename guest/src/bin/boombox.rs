#![no_std]
#![no_main]

use guest::{display, nv, svc};
use scene::{Words, pbr::{self, BASE, PUSHBUFFER, FRAME_SEGMENT}};

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

include!(concat!(env!("OUT_DIR"), "/boombox.rs"));

fn draw() -> svc::Result<()> {
    let size = [1920, 1080];
    let heap = svc::set_heap_size((END - BASE + 0x10_0000).next_multiple_of(0x20_0000))?;
    let channel = nv::Channel::new()?;
    let memory = heap + 0x10_0000;
    let (_, handle) = channel.buffer_at(memory, (END - BASE).next_multiple_of(0x1_0000) as u32, BASE)?;
    let data = include_bytes!(concat!(env!("OUT_DIR"), "/boombox.bin"));
    for &(va, offset, len) in WRITES {
        nv::guest_slice(memory + va - BASE, len).copy_from_slice(&data[offset..offset + len]);
    }
    channel.submit(PUSHBUFFER, INIT_WORDS)?;
    let window = display::Window::target(&channel.nv, channel.nvmap, handle, size, (TARGET - BASE) as u32)?;
    let mut words = Words { storage: nv::guest_slice(memory + FRAME_SEGMENT - BASE, 0x2000), len: 0 };
    let mut tick = 0u32;
    loop {
        let model = pbr::model(FIT, pbr::sin_cos(tick as f32 / 60.0 * 0.8));
        let uniforms = core::array::from_fn(|i| [model, VIEW].as_flattened().as_flattened()[i].to_bits());
        words.len = 0;
        pbr::draw(&uniforms, VIEW, DRAWS, |_| None, false, false, 0, 0, 0.0, tick + 1, &mut words);
        channel.submit(FRAME_SEGMENT, words.len)?;
        window.present(size)?;
        tick += 1;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    if draw().is_err() { let _ = svc::output_debug_string("boombox failed"); }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("boombox panicked");
    svc::exit_process()
}
