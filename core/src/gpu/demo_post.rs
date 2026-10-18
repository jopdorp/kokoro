//! The post-effect scene (demos/scenes/post-effect.md): a spinning pinwheel drawn into one colour target, then that
//! target's VA sampled as TIC 4 by the NVIDIA driver's SM86 build of demos/shaders/post.vert and post.frag into a second.
use super::engine::Gpu;
use super::frame::cache_root;
use super::memory::PAGE;
use super::{VkState, device};
use crate::tests::gpu::engine::{bytes, entry};
use crate::tests::gpu_test::inst::bra;
use std::path::Path;
use std::sync::Mutex;

use scene::post::*;
pub use scene::post::SCENE;
use scene::split;
fn named(e: impl std::fmt::Debug) -> String { format!("{e:?}") }

pub struct Post<'a> {
    pub gpu: Gpu<'a>,
    pub target: u64,
    scene: [u32; 2],
    out: [u32; 2],
    frame: u32,
}

impl<'a> Post<'a> {
    /// Draws the pinwheel at `scene` size and the post pass at `out` size, running the init segment.
    pub fn new(state: &'static VkState, cache: Option<(&Path, &str)>, scene: [u32; 2], out: [u32; 2]) -> Result<Self, String> {
        let target = (SCENE + u64::from(scene[0] * scene[1] * 4)).next_multiple_of(PAGE);
        let end = (target + u64::from(out[0] * out[1] * 4)).next_multiple_of(PAGE);
        let mut gpu = Gpu::new(state, (end - BASE) as usize, cache, *super::SPEED_HACKS.lock().unwrap()).map_err(named)?;
        gpu.space.map(BASE, 0, end - BASE);
        // The rig's programs stop at their last EXIT; the self-branch a compiled program ends with marks the end.
        for (va, program) in [
            (SCENE_VERTEX, &include_bytes!("../../tests/fixtures/driver-shaders/triangle.vert.bin")[..]),
            (SCENE_PIXEL, include_bytes!("../../tests/fixtures/driver-shaders/triangle.frag.bin")),
            (POST_VERTEX, include_bytes!("../../tests/fixtures/driver-shaders/post.vert.bin")),
            (POST_PIXEL, include_bytes!("../../tests/fixtures/driver-shaders/post.frag.bin")),
        ] {
            gpu.write(va, &[program, &bra(-16).to_le_bytes()].concat()).map_err(named)?;
        }
        let [width, height] = scene.map(|n| n as f32);
        let effect = [0.0, 6.0, 0.35, 0.55, width, height, 1.0 / width, 1.0 / height];
        gpu.write(UNIFORMS, &bytes(&effect.map(f32::to_bits))).map_err(named)?;
        // A bindless cbuf handle: the address >> 4 in bits 0..45 and the size >> 4 above (NVK nvk_bindless_cbuf).
        gpu.write(POINTERS, &bytes(&[TEXTURE_HANDLE, 0, (UNIFORMS >> 4) as u32, (UNIFORMS >> 36) as u32 | 32 >> 4 << 13])).map_err(named)?;
        const A8B8G8R8_UNORM: u32 = 8 | 0o2222 << 7 | 0o5432 << 19;
        const PITCH: u32 = 2 << 21;
        const TWO_D: u32 = 1 << 23;
        const NORMALIZED: u32 = 1 << 31;
        let tic = [A8B8G8R8_UNORM, split(SCENE)[1], split(SCENE)[0] | PITCH, scene[0] * 4 >> 5, (scene[0] - 1) | TWO_D, (scene[1] - 1) | NORMALIZED, 0, 0];
        gpu.write(TIC + 4 * 32, &bytes(&tic)).map_err(named)?;
        const CLAMP_TO_EDGE: u32 = 2 | 2 << 3 | 2 << 6;
        const LINEAR_NO_MIP: u32 = 2 | 2 << 4 | 1 << 6;
        gpu.write(TSC + 32, &bytes(&[CLAMP_TO_EDGE, LINEAR_NO_MIP])).map_err(named)?;

        let mut words = Vec::new();
        scene::post::init(&mut words);
        gpu.write(PUSHBUFFER, &bytes(&words)).map_err(named)?;
        gpu.submit(&[entry(PUSHBUFFER, words.len())])?;
        Ok(Self { gpu, target, scene, out, frame: 0 })
    }

    /// Turns the pinwheel and the effect to `seconds`, runs the frame segment and waits for its report.
    pub fn frame(&mut self, seconds: f32) -> Result<(), String> {
        let vertices = scene::post::vertices(self.scene, seconds).as_flattened().iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        self.frame += 1;
        self.gpu.write(VERTICES, &bytes(&vertices)).map_err(named)?;
        let mut words = Vec::new();
        scene::post::frame(self.scene, self.out, self.target, seconds, self.frame, &mut words);
        self.gpu.write(FRAME_SEGMENT, &bytes(&words)).map_err(named)?;
        self.gpu.submit(&[entry(FRAME_SEGMENT, words.len())])?;
        let mut released = [0; 4];
        self.gpu.read(REPORT, &mut released).map_err(named)?;
        match u32::from_le_bytes(released) == self.frame {
            true => Ok(()),
            false => Err(format!("frame {} did not release its report", self.frame)),
        }
    }
}

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;

pub(super) static POST: Mutex<Option<Post<'static>>> = Mutex::new(None);

pub fn render_post_frame(seconds: f32) -> Result<(), String> {
    let mut slot = POST.lock().unwrap();
    let post = match &mut *slot {
        Some(post) => post,
        empty => empty.insert(Post::new(device().map_err(named)?, Some((&cache_root(), "demos")), [1280, 720], [WIDTH, HEIGHT])?),
    };
    post.frame(seconds)?;
    post.gpu.present(post.target, WIDTH, HEIGHT).map_err(named)
}
