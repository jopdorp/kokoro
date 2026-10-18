#![no_std]
#![no_main]

use guest::{display, nv, svc};

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
    let size = [1920, 1080];
    let heap = svc::set_heap_size(0x100_0000)?;
    let triangle = nv::Triangle::new(heap, size, &scene::TRIANGLE_VERTICES)?;
    let window = display::Window::target(&triangle.channel.nv, triangle.channel.nvmap, triangle.target_handle, size, 0)?;
    let (mut sin, mut cos) = (0.0, 1.0);
    loop {
        let aspect = size[1] as f32 / size[0] as f32;
        triangle.draw([cos * aspect, -sin * aspect, sin, cos])?;
        window.present(size)?;
        // One 60 Hz step around the same sixteen-second turn as the host oracle.
        (sin, cos) = (sin * 0.999_978_6 + cos * 0.006_544_938, cos * 0.999_978_6 - sin * 0.006_544_938);
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    if draw().is_err() {
        let _ = svc::output_debug_string("triangle failed");
    }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("triangle panicked");
    svc::exit_process()
}
