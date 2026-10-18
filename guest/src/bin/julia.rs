#![no_std]
#![no_main]

use guest::{display, nv, svc};
use scene::{Words, julia::*};

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
    let heap = svc::set_heap_size(0x100_0000)?;
    let memory = heap + 0x10_0000;
    let channel = nv::Channel::new()?;
    let (_, target_handle) = channel.buffer_at(memory, 0x85_0000, BASE - 0x1000)?;
    let memory = memory + 0x80_0000;
    let program = include_bytes!(concat!(env!("OUT_DIR"), "/julia.bin"));
    nv::guest_slice(memory + PROGRAM - COMMANDS, program.len()).copy_from_slice(program);
    let mut words = Words { storage: nv::guest_slice(memory, 0x1000), len: 0 };
    frame(&mut words);
    let window = display::Window::target(&channel.nv, channel.nvmap, target_handle, size, 0x1000)?;
    let mut tick = 0u32;
    loop {
        nv::guest_slice(memory + CBUF - COMMANDS, 1)[0] = tick as f32 / 60.0 / SECONDS_PER_TURN;
        nv::guest_slice(memory + QMD - COMMANDS, 64).copy_from_slice(&qmd(tick + 1));
        channel.submit(COMMANDS, words.len)?;
        window.present(size)?;
        tick += 1;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    if draw().is_err() { let _ = svc::output_debug_string("julia failed"); }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("julia panicked");
    svc::exit_process()
}
