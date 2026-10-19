//! Frames drawn by translated SASS programs that write RGBA8 pixels into global memory, presented as is.
use super::compute::{ComputePipeline, HostBuffer};
use super::sm86::{CBUF_BINDING, GLOBAL_BINDING, SUPPORT_BINDING, Stage, emit_module};
use super::demo::SCENE;
use super::demo_post::POST;
use super::demo_sprites::SPRITES;
use super::engine::Gpu;
use super::{VkError, device};
use crate::tests::gpu::engine::{TARGET, triangle_frame, triangle_guest};
use crate::tests::gpu_test::inst::*;
use ash::vk::DescriptorType;
use std::path::PathBuf;
use std::sync::Mutex;

use scene::julia::*;
pub use scene::julia::julia_program;
// r = x * 17 >> 7 and g = y * 495176 >> 21 are x * 255 / 1920 and y * 255 / 1080 exactly over the frame.
// The u32 is stored little-endian, so r is byte 0 and a is byte 3 of each RGBA8 pixel.
fn gradient_program() -> Vec<u128> {
    let shr = |rd, rs, n| alu3(SHF ^ IMM_B | SHF_R | SHF_U32 | SHF_HI, rd, RZ, n, rs);
    pixel_program(&[
        alu3(IMAD ^ IMM_B, 1, 0, 17, RZ),
        shr(1, 1, 7),
        alu3(IMAD ^ IMM_B, 3, 2, 495176, RZ),
        shr(3, 3, 21),
        alu3(IMAD ^ IMM_B, 1, 3, 0x100, 1),
        alu3(LOP3 ^ IMM_B | lop3_fields(0xfc, 7, 7, true), 1, 1, 0xff80_0000, RZ),
    ])
}

struct Frame {
    pipeline: ComputePipeline<'static>,
    pixels_buffer: HostBuffer<'static>,
    cbuf0: HostBuffer<'static>,
    _support: HostBuffer<'static>,
}

impl Frame {
    fn new(program: &[u128]) -> Result<Self, VkError> {
        let state = device()?;
        let size = u64::from(WIDTH * HEIGHT * 4);
        let module = emit_module(Some(state), Stage::Compute { local_size: [GROUP, GROUP, 1] }, |decoder| decoder.translate_program(program));
        let pixels_buffer = HostBuffer::new(state, size)?;
        let cbuf0 = HostBuffer::new(state, 16)?;
        let mut support = HostBuffer::new(state, 16)?;
        support.write(&BASE.to_le_bytes())?;
        let ubo = DescriptorType::UNIFORM_BUFFER;
        let bindings = [(SUPPORT_BINDING, ubo), (CBUF_BINDING, ubo), (GLOBAL_BINDING, DescriptorType::STORAGE_BUFFER)];
        let mut pipeline = ComputePipeline::new(state, &module.words(), &bindings)?;
        pipeline.descriptors.bind(&[(SUPPORT_BINDING, &support), (CBUF_BINDING, &cbuf0), (GLOBAL_BINDING, &pixels_buffer)]);
        Ok(Self { pipeline, pixels_buffer, cbuf0, _support: support })
    }

    fn render(&mut self) -> Result<(), VkError> {
        // Round up so 1080 rows get a partial last group; the program's guard drops the overhang.
        device()?.dispatch(&self.pipeline, [WIDTH.div_ceil(GROUP), HEIGHT.div_ceil(GROUP), 1])?;
        crate::display::present_buffer(&self.pixels_buffer, WIDTH, HEIGHT)
    }
}

pub fn render_test_frame() -> Result<(), VkError> {
    Frame::new(&gradient_program())?.render()
}

pub fn render_demo_frame(seconds: f32) -> Result<(), VkError> {
    static DEMO: Mutex<Option<Frame>> = Mutex::new(None);
    let mut slot = DEMO.lock().unwrap();
    let demo = match &mut *slot {
        Some(demo) => demo,
        empty => empty.insert(Frame::new(&julia_program())?),
    };
    demo.cbuf0.write(&(seconds / SECONDS_PER_TURN).to_le_bytes())?;
    demo.render()
}

static TRIANGLE_DEMO: Mutex<Option<Gpu<'static>>> = Mutex::new(None);

/// `$XDG_CACHE_HOME/kokoromi`, or `~/.cache/kokoromi`, or the temp dir's when there is no home.
pub(crate) fn cache_root() -> PathBuf {
    let home = || std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache"));
    std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(home).unwrap_or_else(std::env::temp_dir).join("kokoromi")
}

/// Draws a triangle from a pushbuffer in guest memory, turning once per `SECONDS_PER_TURN`, and presents it.
pub fn render_triangle_demo(seconds: f32) -> Result<(), String> {
    let mut slot = TRIANGLE_DEMO.lock().unwrap();
    let gpu = match &mut *slot {
        Some(gpu) => gpu,
        empty => {
            let vertices = scene::TRIANGLE_VERTICES;
            let state = device().map_err(|e| format!("{e:?}"))?;
            empty.insert(triangle_guest(state, Some((&cache_root(), "demos")), WIDTH, HEIGHT, &vertices).map_err(|e| format!("{e:?}"))?)
        }
    };
    let (sin, cos) = (seconds / SECONDS_PER_TURN * std::f32::consts::TAU).sin_cos();
    let aspect = HEIGHT as f32 / WIDTH as f32;
    triangle_frame(gpu, WIDTH, HEIGHT, [cos * aspect, -sin * aspect, sin, cos], 0)?;
    gpu.present(TARGET, WIDTH, HEIGHT)
}

/// Saves the running demos' pipeline cache, so the next run starts warm.
pub fn flush_demo_caches() -> std::io::Result<()> {
    TRIANGLE_DEMO.lock().unwrap().as_ref().map_or(Ok(()), Gpu::flush_caches)?;
    POST.lock().unwrap().as_ref().map_or(Ok(()), |post| post.gpu.flush_caches())?;
    SCENE.lock().unwrap().as_ref().map_or(Ok(()), |(_, scene)| scene.gpu.flush_caches())?;
    SPRITES.lock().unwrap().as_ref().map_or(Ok(()), Gpu::flush_caches)
}
