use guest::{audren, display, nv, svc};
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

fn draw() -> svc::Result<()> {
    let size = [1920, 1080];
    let mapped = (END - BASE).next_multiple_of(0x1_0000);
    let audio_offset = 0x10_0000 + mapped;
    let heap = svc::set_heap_size((audio_offset + AUDIO.len() as u64 + 0x20_000).next_multiple_of(0x20_0000))?;
    let channel = nv::Channel::new()?;
    let memory = heap + 0x10_0000;
    let (_, handle) = channel.buffer_at(memory, mapped as u32, BASE)?;
    for &(va, offset, len) in WRITES {
        nv::guest_slice(memory + va - BASE, len).copy_from_slice(&DATA[offset..offset + len]);
    }
    channel.submit(PUSHBUFFER, INIT_WORDS)?;
    let window = display::Window::target(&channel.nv, channel.nvmap, handle, size, (TARGET - BASE) as u32)?;
    let mut words = Words { storage: nv::guest_slice(memory + FRAME_SEGMENT - BASE, 0x2000), len: 0 };
    let mut sound = if BOOMBOX { Some(audren::Loop::new(heap + audio_offset, AUDIO)?) } else { None };
    let mut tick = 0u32;
    loop {
        let seconds = tick as f32 / 60.0;
        if let Some(sound) = &mut sound {
            while sound.periods < (f64::from(seconds) * 200.0) as u64 + 10 { sound.period(scene::sound::Spatial::boombox(sound.periods as f32 / 200.0))?; }
        }
        let model = if BOOMBOX { pbr::boombox_model(FIT, seconds) } else { pbr::model(FIT, pbr::sin_cos(seconds * 0.8)) };
        let uniforms = core::array::from_fn(|i| [model, VIEW].as_flattened().as_flattened()[i].to_bits());
        words.len = 0;
        pbr::draw(&uniforms, VIEW, DRAWS, |i| BLENDS[i], false, false, 0, 0, 0.0, tick + 1, &mut words);
        channel.submit(FRAME_SEGMENT, words.len)?;
        window.present(size)?;
        tick += 1;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    if draw().is_err() { let _ = svc::output_debug_string("pbr failed"); }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("pbr panicked");
    svc::exit_process()
}
