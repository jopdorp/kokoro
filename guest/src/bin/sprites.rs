#![no_std]
#![no_main]

use guest::{display, hid, nv, svc};
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

fn draw() -> svc::Result<()> {
    let size = [1920, 1080];
    let heap = svc::set_heap_size(0x200_0000)?;
    let channel = nv::Channel::new(heap)?;
    let memory = heap + 0x10_0000;
    let len = (TARGET as u32 + size[0] * size[1] * 4).next_multiple_of(0x1_0000);
    let (base, handle) = channel.buffer(memory, len)?;
    let vertex = include_bytes!("../../../core/tests/fixtures/driver-shaders/sprite2d.vert.bin");
    let pixel = include_bytes!("../../../core/tests/fixtures/driver-shaders/sprite2d.frag.bin");
    for (offset, program) in [(VERTEX_PROGRAM, &vertex[..]), (PIXEL_PROGRAM, &pixel[..])] {
        nv::guest_slice(memory + offset, program.len()).copy_from_slice(program);
        nv::guest_slice(memory + offset + program.len() as u64, 16).copy_from_slice(&scene::sass::bra(-16).to_le_bytes());
    }
    for (offset, words) in descriptors(base) {
        nv::guest_slice(memory + offset, words.len()).copy_from_slice(&words);
    }
    let atlas = include_bytes!(concat!(env!("OUT_DIR"), "/sprites-atlas.bin"));
    nv::guest_slice(memory + ATLAS, atlas.len()).copy_from_slice(atlas);
    let initial = include_bytes!(concat!(env!("OUT_DIR"), "/sprites-instances.bin"));
    nv::guest_slice(memory + INSTANCES, initial.len()).copy_from_slice(initial);
    let instances = nv::guest_slice::<[f32; 12]>(memory + INSTANCES, INSTANCE_COUNT);
    let mut words = Words { storage: nv::guest_slice(memory, 0x1000), len: 0 };
    init_segment(base, size, &mut words);
    channel.submit(base, words.len)?;
    let window = display::Window::target(&channel.nv, channel.nvmap, handle, size, TARGET as u32)?;
    hid::initialize()?;
    loop {
        let input = hid::read();
        for (i, instance) in instances.iter_mut().enumerate() {
            if i > 0 && (i - 1) % (SPRITES_PER_LAYER + 1) == 0 { continue; }
            instance[0] += input.left[0] as f32 / 32767.0 * 4.0;
            instance[1] -= input.left[1] as f32 / 32767.0 * 4.0;
        }
        words.len = 0;
        frame(base, 0.0, if input.buttons & 1 != 0 { [0.5, 0.8, 1.0, 1.0] } else { SUNSET }, &mut words);
        channel.submit(base, words.len)?;
        window.present(size)?;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    if draw().is_err() { let _ = svc::output_debug_string("sprites failed"); }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("sprites panicked");
    svc::exit_process()
}
