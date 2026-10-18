#![no_std]
#![no_main]

use guest::{display, nv, svc};
use scene::{Words, mandelbrot::*};

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

const CUBIN: &[u8] = include_bytes!("../../../core/tests/fixtures/mandelbrot-sm86.cubin");

fn draw() -> svc::Result<()> {
    let heap = svc::set_heap_size(0x100_0000)?;
    let memory = heap + 0x10_0000;
    let channel = nv::Channel::new()?;
    let (_, target_handle) = channel.buffer_at(memory, MAPPED, BASE)?;
    nv::guest_slice(memory + PROGRAM - BASE, 0x1900).copy_from_slice(&CUBIN[0x7e80..0x9780]);
    nv::guest_slice(memory + CBUF - BASE, 0x19d).copy_from_slice(&CUBIN[0x2500..0x269d]);
    nv::guest_slice::<u32>(memory + CBUF - BASE, 6).copy_from_slice(&[BLOCK[0], BLOCK[1], BLOCK[2], GRID[0], GRID[1], GRID[2]]);
    nv::guest_slice::<u32>(memory + CBUF - BASE + 0x28, 1)[0] = 0x1000;
    let mut words = Words {
        storage: nv::guest_slice(memory + COMMANDS - BASE, 16),
        len: 0,
    };
    commands(&mut words);
    let window = display::Window::target(
        &channel.nv,
        channel.nvmap,
        target_handle,
        [WIDTH, HEIGHT],
        (TARGET - BASE) as u32,
    )?;
    let mut frame = 0u32;
    loop {
        let cbuf = nv::guest_slice::<u32>(memory + CBUF - BASE, 0x1a0 / 4);
        cbuf[0x160 / 4..0x19c / 4].copy_from_slice(&[
            TARGET as u32,
            (TARGET >> 32) as u32,
            WIDTH,
            HEIGHT,
            CRUNCH,
            (-0.5f32).to_bits(),
            0,
            0,
            0,
            scale(frame).to_bits(),
            u32::from_le_bytes([3, 5, 7, 0]),
            0,
            0,
            GRID_WIDTH,
            GRID[0],
        ]);
        cbuf[0x19c / 4] = 0;
        nv::guest_slice(memory + QMD - BASE, 64).copy_from_slice(&qmd(frame + 1));
        channel.submit(COMMANDS, words.len)?;
        window.present([WIDTH, HEIGHT])?;
        frame += 1;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    if draw().is_err() {
        let _ = svc::output_debug_string("mandelbrot failed");
    }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("mandelbrot panicked");
    svc::exit_process()
}
