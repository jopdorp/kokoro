//! The particles scene (demos/scenes/particles.md): each frame a QMD launch runs the NVIDIA driver's SM86 build of
//! demos/shaders/particles_update.comp over the particle buffer in guest memory, then one instanced draw of
//! particles.vert and particles.frag reads the same bytes as its vertex stream and adds a glow quad per particle.
use super::engine::Gpu;
use super::{VkState, device};
use crate::tests::gpu::engine::{bytes, entry};
use crate::tests::gpu_test::inst::bra;
use std::sync::Mutex;

pub use scene::particles::{WIDTH, HEIGHT, COUNT, PARTICLES, TARGET, RELEASE, update_uniforms};
use scene::particles::*;
use scene::particles::assets::{background, initial_particles};
pub fn camera(seconds: f32) -> ([f32; 16], [f32; 3], [f32; 3]) { scene::particles::camera(seconds, f32::sqrt) }
fn named(e: impl std::fmt::Debug) -> String { format!("{e:?}") }

pub struct Particles<'a> {
    pub gpu: Gpu<'a>,
    frame: u32,
    background: Vec<u8>,
}

impl<'a> Particles<'a> {
    pub fn new(state: &'static VkState) -> Result<Self, String> {
        let mut gpu = Gpu::new(state, MAPPED as usize, None, *super::SPEED_HACKS.lock().unwrap()).map_err(named)?;
        gpu.space.map(BASE, 0, MAPPED);
        let program = |code: &[u8]| [code, &bra(-16).to_le_bytes()].concat();
        let mut cs_c0 = [0; 19];
        [cs_c0[12], cs_c0[13]] = handle(CS_UNIFORMS, 0x100);
        [cs_c0[16], cs_c0[17], cs_c0[18]] = [PARTICLES as u32, 0, PARTICLE_BYTES];
        let mut init = Vec::new();
        scene::particles::init(&mut init);
        for (va, data) in [
            (VERTEX_PROGRAM, program(include_bytes!("../../tests/fixtures/driver-shaders/particles.vert.bin"))),
            (PIXEL_PROGRAM, program(include_bytes!("../../tests/fixtures/driver-shaders/particles.frag.bin"))),
            (UPDATE_PROGRAM, program(include_bytes!("../../tests/fixtures/driver-shaders/particles_update.comp.bin"))),
            (VS_POINTERS, bytes(&handle(VS_UNIFORMS, 0x100))),
            (CS_C0, bytes(&cs_c0)),
            (CS_UNIFORMS + 48, bytes(&[COUNT])),
            (PARTICLES, initial_particles()),
            (PUSHBUFFER, bytes(&init)),
        ] {
            gpu.write(va, &data).map_err(named)?;
        }
        gpu.submit(&[entry(PUSHBUFFER, init.len())])?;
        Ok(Self { gpu, frame: 0, background: background() })
    }

    /// Integrates every particle one step with the attractors at `seconds` and releases the frame number.
    pub fn update(&mut self, seconds: f32) -> Result<(), String> {
        self.frame += 1;
        self.gpu.write(QMD, &bytes(&qmd(self.frame))).map_err(named)?;
        let mut words = Vec::new();
        scene::particles::update(seconds, &mut words);
        self.run(UPDATE_SEGMENT, &words)
    }

    /// Writes the background into the target and adds one glow quad per particle, seen from the camera at `seconds`.
    pub fn draw(&mut self, seconds: f32) -> Result<(), String> {
        self.reset_target()?;
        self.draw_particles(seconds)
    }

    fn reset_target(&mut self) -> Result<(), String> {
        self.gpu.write(TARGET, &self.background).map_err(named)
    }

    fn draw_particles(&mut self, seconds: f32) -> Result<(), String> {
        let mut words = Vec::new();
        scene::particles::draw(seconds, self.frame, f32::sqrt, &mut words);
        self.run(DRAW_SEGMENT, &words)
    }

    fn run(&mut self, va: u64, words: &[u32]) -> Result<(), String> {
        self.gpu.write(va, &bytes(words)).map_err(named)?;
        self.gpu.submit(&[entry(va, words.len())])
    }
}

pub fn render_particles_frame(seconds: f32) -> Result<(), String> {
    static SCENE: Mutex<Option<Particles<'static>>> = Mutex::new(None);
    let mut slot = SCENE.lock().unwrap();
    let scene = match &mut *slot {
        Some(scene) => scene,
        empty => {
            let scene = empty.insert(Particles::new(device().map_err(named)?)?);
            for step in 0..WARM_UP_STEPS {
                scene.update((step as f32 - WARM_UP_STEPS as f32) / 60.0)?;
            }
            scene
        }
    };
    scene.reset_target()?;
    scene.update(seconds)?;
    scene.draw_particles(seconds)?;
    scene.gpu.present(TARGET, WIDTH, HEIGHT).map_err(named)
}
