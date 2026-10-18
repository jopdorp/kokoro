#![no_std]
#![no_main]

use core::{arch::asm, ptr::{addr_of_mut, read_volatile}};
use scene::dungeon::sound::{BLOCK, Box3, Engine, Environment, FDN_SAMPLES, HISTORY, Material, Mode, Room, Sample, Samples, Storage, Voice};

core::arch::global_asm!(".section .text._start, \"ax\"", ".global _start", "_start:", "b main");

const VOICES: usize = 11;
static PCM: [i16; 521] = [512; 521];
static mut HISTORY_MEMORY: [f32; HISTORY * VOICES] = [0.0; HISTORY * VOICES];
static mut FDN_MEMORY: [f32; FDN_SAMPLES] = [0.0; FDN_SAMPLES];

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    let stone = Material { absorption: 0.18, scattering: 0.32 };
    let rooms = [Room { min: [-5.0, -2.0, -4.0], max: [6.0, 3.0, 4.0], walls: [stone; 6], first_doorway: 0, doorway_count: 0 }];
    let mut voices = [Voice::new(Sample::Torch, [2.5, 0.4, -1.2], 0.7, 1.0, true); VOICES];
    for (voice, sample) in voices[4..].iter_mut().zip([Sample::Drip, Sample::Drip, Sample::Drip, Sample::Drip, Sample::Scream, Sample::Step, Sample::Step]) { *voice = Voice::new(sample, [1.0, 0.0, -1.0], 0.0, 1.0, false); }
    let samples = Samples { recordings: [&PCM; 13], drips: [&PCM; 3], steps: [[&PCM; 6]; 4] };
    let history = unsafe { core::slice::from_raw_parts_mut(addr_of_mut!(HISTORY_MEMORY).cast::<f32>(), HISTORY * VOICES) };
    let fdn = unsafe { core::slice::from_raw_parts_mut(addr_of_mut!(FDN_MEMORY).cast::<f32>(), FDN_SAMPLES) };
    let outside = Box3 { min: [20.0; 3], max: [21.0; 3] };
    let storage = Storage { voices: &mut voices, histories: history, fdn };
    let environment = Environment { rooms: &rooms, doorways: &[], bridge: outside, reveal: outside, ambient: false };
    let mut engine = Engine::new(samples, storage, environment, Mode::Headphones).unwrap();
    engine.play(Sample::Step, [1.2, 0.0, -0.7], 0.04, 1.0);
    let mut out = [[0; 2]; BLOCK];
    for block in 0..20 {
        engine.listen([-1.0 + block as f32 * 0.02, 0.2, 0.1], 0.7, 0.005, Mode::Headphones);
        out = engine.render();
    }
    unsafe { read_volatile(out.as_ptr()); asm!("brk 0", options(noreturn)) }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { unsafe { asm!("brk 1", options(noreturn)) } }
