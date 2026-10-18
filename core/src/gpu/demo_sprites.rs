//! The sprites2d scene (demos/scenes/sprites2d.md): a sun, three scrolling hill layers and the sprites drifting between
//! them, drawn back to front as one instanced triangle strip a frame by the NVIDIA driver's SM86 build of
//! demos/shaders/sprite2d.vert and sprite2d.frag from a host-written pushbuffer.
use super::engine::Gpu;
use super::frame::cache_root;
use super::memory::PAGE;
use super::{VkState, device};
use crate::tests::gpu::engine::{bytes, entry};
use crate::tests::gpu_test::inst::bra;
pub use scene::sprites_assets::{atlas, instances, SHAPES};
pub use scene::sprites::{SKY, SUNSET};
use scene::sprites::{self, *};
const BASE: u64 = 0x1000_0000;
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
pub const TARGET: u64 = BASE + sprites::TARGET;
use std::path::Path;
use std::sync::Mutex;

/// Guest memory holding the scene with its init segment run, drawing into a `width` x `height` target at `TARGET`.
pub fn sprites_guest(state: &'static VkState, cache: Option<(&Path, &str)>, width: u32, height: u32) -> Result<Gpu<'static>, String> {
    sprites_guest_with_programs(state, cache, width, height, [
        include_bytes!("../../tests/fixtures/driver-shaders/sprite2d.vert.bin"),
        include_bytes!("../../tests/fixtures/driver-shaders/sprite2d.frag.bin"),
    ])
}

pub(crate) fn sprites_guest_with_programs(state: &'static VkState, cache: Option<(&Path, &str)>, width: u32, height: u32, programs: [&[u8]; 2]) -> Result<Gpu<'static>, String> {
    let named = |e| format!("{e:?}");
    let len = (TARGET - BASE + u64::from(width * height) * 4).next_multiple_of(PAGE);
    let mut gpu = Gpu::new(state, len as usize, cache, *super::SPEED_HACKS.lock().unwrap()).map_err(|e| format!("{e:?}"))?;
    gpu.space.map(BASE, 0, len);
    // The rig's programs stop at their last EXIT; the self-branch a compiled program ends with marks the end.
    for (offset, program) in [VERTEX_PROGRAM, PIXEL_PROGRAM].into_iter().zip(programs) {
        gpu.write(BASE + offset, &[program, &bra(-16).to_le_bytes()].concat()).map_err(named)?;
    }
    for (offset, words) in sprites::descriptors(BASE) {
        gpu.write(BASE + offset, &bytes(&words)).map_err(named)?;
    }
    gpu.write(BASE + ATLAS, &atlas()).map_err(named)?;
    let mut words = Vec::new();
    sprites::init_segment(BASE, [width, height], &mut words);
    gpu.write(BASE + PUSHBUFFER, &bytes(&words)).map_err(named)?;
    gpu.submit(&[entry(BASE + PUSHBUFFER, words.len())])?;
    Ok(gpu)
}

/// Writes the instances for `seconds` and runs the frame segment: scroll the camera, clear to the sky, draw.
pub fn sprites_frame(gpu: &mut Gpu, seconds: f32) -> Result<(), String> {
    let named = |e| format!("{e:?}");
    gpu.write(BASE + INSTANCES, &instances(seconds).iter().flatten().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>()).map_err(named)?;
    let mut words = Vec::new();
    sprites::frame(BASE, seconds * scene::sprites_assets::SCROLL_PER_SECOND, SUNSET, &mut words);
    gpu.write(BASE + FRAME_SEGMENT, &bytes(&words)).map_err(named)?;
    gpu.submit(&[entry(BASE + FRAME_SEGMENT, words.len())])
}

pub(super) static SPRITES: Mutex<Option<Gpu<'static>>> = Mutex::new(None);

/// Draws the sprites scene at `seconds` and presents it.
pub fn render_sprites_frame(seconds: f32) -> Result<(), String> {
    let mut slot = SPRITES.lock().unwrap();
    let gpu = match &mut *slot {
        Some(gpu) => gpu,
        empty => empty.insert(sprites_guest(device().map_err(|e| format!("{e:?}"))?, Some((&cache_root(), "demos")), WIDTH, HEIGHT)?),
    };
    sprites_frame(gpu, seconds)?;
    gpu.present(TARGET, WIDTH, HEIGHT).map_err(|e| format!("{e:?}"))
}
