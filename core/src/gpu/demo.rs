//! The pbr scenes (demos/scenes/pbr-boombox.md, pbr-flighthelmet.md): a converted glTF asset in guest memory, drawn
//! from a host-written pushbuffer by the NVIDIA driver's SM86 build of demos/shaders/pbr.vert and pbr.frag, one draw per
//! primitive with its material's pointer table.
use super::engine::{Geometry, Gpu};
use super::frame::cache_root;
use super::{VkState, device};
use crate::tests::gpu::engine::{bytes, entry};
use std::path::Path;
use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};

pub use scene::pbr::{Matrix, multiply};
use scene::pbr::*;
fn floats(values: &[f32]) -> Vec<u8> { values.iter().flat_map(|v| v.to_le_bytes()).collect() }

pub fn asset_dir(asset: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../demos/assets").join(asset)
}

pub fn supports_raytraced(state: &VkState) -> bool {
    use ash::vk;
    let mut subgroup = vk::PhysicalDeviceSubgroupProperties::default();
    let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut subgroup);
    unsafe { state.instance.get_physical_device_properties2(state.physical_device, &mut properties) };
    state.acceleration_structure.is_some()
        && subgroup.supported_stages.contains(vk::ShaderStageFlags::FRAGMENT)
        && subgroup.supported_operations.contains(vk::SubgroupFeatureFlags::ARITHMETIC | vk::SubgroupFeatureFlags::BALLOT | vk::SubgroupFeatureFlags::SHUFFLE)
}

pub struct Scene<'a> {
    pub gpu: Gpu<'a>,
    pub target: u64,
    width: u32,
    height: u32,
    /// first index, index count and material of each primitive.
    draws: Vec<[u32; 3]>,
    /// each material's blend op, source and destination factors and constant colour, drawn over the depth of
    /// what is already there without writing it.
    blends: Vec<Option<Vec<u32>>>,
    /// guest address of each texture, for scenes that rewrite theirs.
    pub textures: Vec<u64>,
    /// Model space centre and the scale that makes the asset two units across.
    fit: ([f32; 3], f32),
    geometry: Vec<Geometry>,
    traced: bool,
    room: bool,
    boombox: bool,
    material_count: usize,
    model_indices: u32,
    light_first: u32,
    light_y: f32,
    frame: u32,
}

impl<'a> Scene<'a> {
    /// Lays the asset in demos/assets/`asset` out in guest memory and runs the init segment.
    pub fn new(state: &'static VkState, cache: Option<(&Path, &str)>, asset: &str, width: u32, height: u32) -> Result<Self, String> {
        let room = asset.starts_with("raytraced");
        let traced = room && asset != "raytraced-off";
        if traced && !supports_raytraced(state) {
            return Err("ray-traced demo needs ray queries and fragment subgroup arithmetic, ballot and shuffle".into());
        }
        let dir = asset_dir(if room { "boombox" } else { asset });
        Self::with_scene_files(state, cache, &|file| std::fs::read(dir.join(file)).map_err(|e| format!("{}: {e}", dir.join(file).display())), width, height, room, traced, asset == "raytraced-showcase", asset == "boombox")
    }

    /// Like `new`, with the asset's files from `read`.
    pub fn with_files(state: &'static VkState, cache: Option<(&Path, &str)>, read: &dyn Fn(&str) -> Result<Vec<u8>, String>, width: u32, height: u32, raytraced: bool) -> Result<Self, String> {
        Self::with_scene_files(state, cache, read, width, height, raytraced, raytraced, false, false)
    }

    fn with_scene_files(state: &'static VkState, cache: Option<(&Path, &str)>, read: &dyn Fn(&str) -> Result<Vec<u8>, String>, width: u32, height: u32, room: bool, traced: bool, showcase: bool, boombox: bool) -> Result<Self, String> {
        let asset = scene::pbr::assets::Asset::load(read, width, height, room, traced, showcase)?;
        let mut gpu = Gpu::new(state, (asset.end - BASE) as usize, cache, *super::SPEED_HACKS.lock().unwrap()).map_err(|e| format!("{e:?}"))?;
        gpu.space.map(BASE, 0, asset.end - BASE);
        for (va, data) in &asset.writes { gpu.write(*va, data).map_err(|e| format!("{e:?}"))?; }
        let init = &asset.writes.last().unwrap().1;
        gpu.submit(&[entry(PUSHBUFFER, init.len() / 4)])?;
        let geometry = if traced {
            vec![Geometry { vertices: VERTICES, stride: 48, vertex_count: asset.model_vertices as u32, indices: asset.indices_va, index_count: asset.model_indices, transform: [[0.0; 4]; 3], opaque: true },
                Geometry { vertices: VERTICES, stride: 48, vertex_count: asset.vertex_count, indices: asset.indices_va + (u64::from(asset.model_indices) + 30) * 4, index_count: asset.index_count - asset.model_indices - 30 - asset.light_count, transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]], opaque: true }]
        } else { Vec::new() };
        Ok(Self { gpu, target: asset.target, width, height, draws: asset.draws, blends: asset.blends, textures: asset.textures, fit: asset.fit, geometry, traced, room, boombox, material_count: asset.material_count, model_indices: asset.model_indices, light_first: asset.light_first, light_y: 0.7, frame: 0 })
    }

    /// Turns the model to `seconds` at 0.8 rad/s, draws every primitive and waits for the frame's report.
    pub fn frame(&mut self, seconds: f32) -> Result<(), String> {
        let model = if self.boombox { scene::pbr::boombox_model(self.fit, seconds) } else { scene::pbr::model(self.fit, scene::pbr::sin_cos(seconds * 0.8 + if self.room { std::f32::consts::PI } else { 0.0 })) };
        if self.traced {
            self.geometry[0].transform = std::array::from_fn(|row| std::array::from_fn(|col| model[col][row]));
            self.gpu.register_acceleration_structure(TLAS, self.geometry.clone()).map_err(|e| format!("{e:?}"))?;
        }
        let (eye, head, light_y) = if self.room { scene::pbr::room_frame(seconds, self.traced, f32::sqrt) } else { (EYE, [0.0; 16], 0.0) };
        let view_proj = scene::pbr::assets::view_projection(self.width, self.height, eye);
        if self.room {
            for material in 0..self.material_count { self.gpu.write(MATERIALS + MATERIAL_STRIDE * material as u64 + MATERIAL_UNIFORMS, &floats(&head)).map_err(|e| format!("{e:?}"))?; }
            self.light_y = light_y;
        }
        self.draw(model, view_proj, eye)
    }

    /// The guest address of `material`'s uniform block.
    pub fn uniforms(material: u32) -> u64 {
        MATERIALS + MATERIAL_STRIDE * u64::from(material) + MATERIAL_UNIFORMS
    }

    /// Draws every primitive with `model` and `view_proj`, lit as seen from `eye`, and waits for the frame's report.
    pub fn draw(&mut self, model: Matrix, view_proj: Matrix, eye: [f32; 3]) -> Result<(), String> {
        let named = |e| format!("{e:?}");
        for material in 0..self.blends.len() as u32 {
            self.gpu.write(Self::uniforms(material) + 48, &floats(&eye)).map_err(named)?;
        }
        let uniforms = std::array::from_fn(|i| [model, view_proj].as_flattened().as_flattened()[i].to_bits());

        self.frame += 1;
        let mut words = Vec::new();
        scene::pbr::draw(&uniforms, view_proj, &self.draws, |i| self.blends[i].as_deref(), self.room, self.traced, self.model_indices, self.light_first, self.light_y, self.frame, &mut words);
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

pub(super) static SCENE: Mutex<Option<(&str, Scene<'static>)>> = Mutex::new(None);
static RAY_TRACING: AtomicBool = AtomicBool::new(true);
pub static SHOWCASE: AtomicBool = AtomicBool::new(false);

fn render(asset: &'static str, seconds: f32) -> Result<(), String> {
    let mut slot = SCENE.lock().unwrap();
    if slot.as_ref().is_none_or(|(name, _)| *name != asset) {
        *slot = None;
        *slot = Some((asset, Scene::new(device().map_err(|e| format!("{e:?}"))?, Some((&cache_root(), "demos")), asset, WIDTH, HEIGHT)?));
    }
    let (_, scene) = slot.as_mut().unwrap();
    scene.frame(seconds)?;
    scene.gpu.present(scene.target, WIDTH, HEIGHT).map_err(|e| format!("{e:?}"))
}

pub fn render_boombox_frame(seconds: f32) -> Result<(), String> {
    render("boombox", seconds)
}

pub fn render_raytraced_frame(seconds: f32) -> Result<(), String> {
    render(if !RAY_TRACING.load(Ordering::Relaxed) { "raytraced-off" } else if SHOWCASE.load(Ordering::Relaxed) { "raytraced-showcase" } else { "raytraced" }, seconds)
}

pub fn set_raytracing_enabled(enabled: bool) {
    RAY_TRACING.store(enabled, Ordering::Relaxed);
}

pub fn reset_scene() {
    *SCENE.lock().unwrap() = None;
}

/// Needs demos/assets/fetch.sh run first.
pub fn render_helmet_frame(seconds: f32) -> Result<(), String> {
    render("flighthelmet", seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::gpu::harness::vk_state;

    #[test]
    fn raytraced_frame_darkens_occluded_surfaces() {
        let Some(state) = vk_state() else { return };
        if !supports_raytraced(&state) { return eprintln!("skipped: device does not support the ray-traced demo"); }
        if !asset_dir("boombox").exists() { return eprintln!("skipped: run demos/assets/fetch.sh for boombox"); }
        let mut scene = Scene::new(Box::leak(Box::new(state)), None, "raytraced", 320, 180).unwrap();
        scene.frame(0.0).unwrap();
        let mut traced = vec![0; 320 * 180 * 4];
        scene.gpu.read(scene.target, &mut traced).unwrap();
        for geometry in &mut scene.geometry { geometry.transform[0][3] += 1e6; }
        scene.gpu.register_acceleration_structure(TLAS, scene.geometry.clone()).unwrap();
        // Replay the same draw commands with geometry outside every ray's reach.
        let mut commands = vec![0; 0x8000];
        scene.gpu.read(FRAME_SEGMENT, &mut commands).unwrap();
        let report = ((SET_REPORT_SEMAPHORE_A as u32 / 4) | (4 << 16) | (1 << 29)).to_le_bytes();
        let words = commands.chunks_exact(4).position(|word| word == report).unwrap() + 5;
        scene.gpu.submit(&[entry(FRAME_SEGMENT, words)]).unwrap();
        let mut unoccluded = vec![0; traced.len()];
        scene.gpu.read(scene.target, &mut unoccluded).unwrap();
        let darker = traced.chunks_exact(4).zip(unoccluded.chunks_exact(4)).filter(|(a, b)| u32::from(a[0]) + 10 < u32::from(b[0])).count();
        assert!(darker > 100, "only {darker} pixels darkened by ray intersections");
    }
}
