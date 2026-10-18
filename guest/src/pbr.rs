use guest::{audren, display, nv, svc};
use scene::{Words, pbr::{self, BASE, PUSHBUFFER, FRAME_SEGMENT, MATERIALS, MATERIAL_STRIDE, MATERIAL_UNIFORMS, PIXEL_PROGRAM, TLAS}};

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

fn options(config: *const [u64; 3]) -> ([u32; 2], bool, u32) {
    let argv = (0..).map(|i| unsafe { *config.add(i) }).take_while(|entry| entry[0] as u32 != 0).find(|entry| entry[0] as u32 == 5).map(|entry| unsafe { core::ffi::CStr::from_ptr(entry[2] as *const _) }.to_bytes()).unwrap_or_default();
    (if argv.windows(5).any(|word| word == b"-test") { [320, 180] } else { [1920, 1080] }, argv.windows(9).any(|word| word == b"-showcase"), if argv.windows(6).any(|word| word == b"-audio") { 60 } else { 1 })
}

fn write_floats(address: u64, values: &[f32]) {
    for (out, value) in nv::guest_slice::<u32>(address, values.len()).iter_mut().zip(values) { *out = value.to_bits(); }
}

fn draw(config: *const [u64; 3]) -> svc::Result<()> {
    let (size, showcase, step) = options(config);
    let mapped = (END - BASE).next_multiple_of(0x1_0000);
    let audio_offset = 0x10_0000 + mapped;
    let heap = svc::set_heap_size((audio_offset + AUDIO.len() as u64 + 0x20_000).next_multiple_of(0x20_0000))?;
    let channel = nv::Channel::new()?;
    let memory = heap + 0x10_0000;
    let (_, handle) = channel.buffer_at(memory, mapped as u32, BASE)?;
    for &(va, offset, len) in WRITES {
        nv::guest_slice(memory + va - BASE, len).copy_from_slice(&DATA[offset..offset + len]);
    }
    if showcase { nv::guest_slice(memory + PIXEL_PROGRAM - BASE, SHOWCASE_DATA.len()).copy_from_slice(SHOWCASE_DATA); }
    if TRACED {
        let mut init = Words { storage: nv::guest_slice(memory + PUSHBUFFER - BASE, 0x2000), len: 0 };
        pbr::init(size[0], size[1], TARGET, u64::from(GEOMETRY.vertex_count) * 48, GEOMETRY.indices_va, TEXTURE_COUNT, &mut init);
        channel.submit(PUSHBUFFER, init.len)?;
    } else {
        channel.submit(PUSHBUFFER, INIT_WORDS)?;
    }
    let window = display::Window::target(&channel.nv, channel.nvmap, handle, size, (TARGET - BASE) as u32)?;
    let mut words = Words { storage: nv::guest_slice(memory + FRAME_SEGMENT - BASE, 0x2000), len: 0 };
    let mut sound = if ROOM { Some(audren::Loop::new(heap + audio_offset, AUDIO)?) } else { None };
    let mut tick = if step == 1 { 0 } else { step };
    loop {
        let seconds = tick as f32 / 60.0;
        if let Some(sound) = &mut sound {
            while sound.periods < (f64::from(seconds) * 200.0) as u64 + 10 {
                let time = sound.periods as f32 / 200.0;
                let (listener, velocity) = pbr::room_listener();
                sound.period(scene::sound::Spatial::boombox(time, listener, velocity))?;
            }
        }
        let model = if ROOM { pbr::room_model(FIT, seconds) } else { pbr::model(FIT, pbr::sin_cos(seconds * 0.8)) };
        let (eye, head, light_position) = if ROOM { pbr::room_frame(seconds, TRACED, sqrt) } else { (pbr::EYE, [0.0; 16], [0.0; 3]) };
        if ROOM { for material in 0..MATERIAL_COUNT { write_floats(memory + MATERIALS + MATERIAL_STRIDE * material as u64 + MATERIAL_UNIFORMS - BASE, &head); } }
        let view = if ROOM { pbr::view_projection(size[0], size[1], eye, sqrt) } else { VIEW };
        let uniforms = core::array::from_fn(|i| [model, view].as_flattened().as_flattened()[i].to_bits());
        if TRACED {
            let mut registry = Words { storage: nv::guest_slice(memory + TLAS - BASE, 2 + 2 * pbr::GEOMETRY_WORDS), len: 0 };
            pbr::acceleration_structure(model, GEOMETRY, &mut registry);
        }
        words.len = 0;
        pbr::draw(&uniforms, view, DRAWS, |i| BLENDS[i], ROOM, TRACED, GEOMETRY.model_indices, LIGHT_FIRST, light_position, tick + 1, &mut words);
        channel.submit(FRAME_SEGMENT, words.len)?;
        window.present(size)?;
        tick += step;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main(config: *const [u64; 3]) -> ! {
    if draw(config).is_err() { let _ = svc::output_debug_string("pbr failed"); }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("pbr panicked");
    svc::exit_process()
}
