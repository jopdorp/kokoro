use super::{engine::Gpu, VkState};
use crate::tests::{gpu::engine::bytes, gpu_test::inst::bra};
use scene::{dungeon::render, pitch_rgba8_tic};

pub(crate) const BASE: u64 = 0x3_0000_0000;
pub(crate) const ARENA: u64 = 0x20_0000;
pub(crate) const PUSH: u64 = BASE;
pub(crate) const DUNGEON_VERTEX: u64 = BASE + 0x1_0000;
pub(crate) const DUNGEON_FRAGMENT: u64 = BASE + 0x2_0000;
pub(crate) const CONES_VERTEX: u64 = BASE + 0x4_0000;
pub(crate) const SPRITE_VERTEX: u64 = BASE + 0x5_0000;
pub(crate) const SPRITE_FRAGMENT: u64 = BASE + 0x6_0000;
pub(crate) const DESCRIPTORS: u64 = BASE + 0x8_0000;
pub(crate) const VERTEX_CONSTANTS: u64 = BASE + 0x8_1000;
pub(crate) const FRAGMENT_CONSTANTS: u64 = BASE + 0x8_2000;
pub(crate) const DRAW_PARAMETERS: u64 = BASE + 0x8_3000;
pub(crate) const TIC: u64 = BASE + 0x8_4000;
pub(crate) const TSC: u64 = BASE + 0x8_5000;
pub(crate) const TEXELS: u64 = BASE + 0x8_6000;
pub(crate) const DATA: u64 = BASE + 0x9_0000;
pub(crate) const TARGET: u64 = BASE + 0x10_0000;
pub(crate) const REFLECTION: u64 = BASE + 0x11_0000;
pub(crate) const DUNGEON_RT_VERTEX: u64 = BASE + 0x12_0000;
pub(crate) const DUNGEON_RT_FRAGMENT: u64 = BASE + 0x13_0000;
pub(crate) const CONES_RT_VERTEX: u64 = BASE + 0x16_0000;
pub(crate) const DEPTH: u64 = BASE + ARENA;

pub(crate) struct GuestDungeon {
    pub gpu: Gpu<'static>,
}

fn shader(bytes: &[u8]) -> Vec<u8> {
    [bytes, &bra(-16).to_le_bytes()].concat()
}

impl GuestDungeon {
    pub fn new(state: &'static VkState) -> Self {
        let mut gpu = Gpu::new(state, ARENA as usize, None, Default::default()).unwrap();
        gpu.space.map(BASE, 0, ARENA);
        for (address, program) in [
            (
                DUNGEON_VERTEX,
                include_bytes!("../../tests/fixtures/driver-shaders/dungeon.vert.bin").as_slice(),
            ),
            (
                DUNGEON_FRAGMENT,
                include_bytes!("../../tests/fixtures/driver-shaders/dungeon.frag.bin").as_slice(),
            ),
            (
                CONES_VERTEX,
                include_bytes!("../../tests/fixtures/driver-shaders/dungeon_cones.vert.bin")
                    .as_slice(),
            ),
            (
                SPRITE_VERTEX,
                include_bytes!("../../tests/fixtures/driver-shaders/dungeon_sprite.vert.bin")
                    .as_slice(),
            ),
            (
                SPRITE_FRAGMENT,
                include_bytes!("../../tests/fixtures/driver-shaders/dungeon_sprite.frag.bin")
                    .as_slice(),
            ),
            (
                DUNGEON_RT_VERTEX,
                include_bytes!("../../tests/fixtures/driver-shaders/dungeon_rt.vert.bin").as_slice(),
            ),
            (
                DUNGEON_RT_FRAGMENT,
                include_bytes!("../../tests/fixtures/driver-shaders/dungeon_rt.frag.bin").as_slice(),
            ),
            (
                CONES_RT_VERTEX,
                include_bytes!("../../tests/fixtures/driver-shaders/dungeon_cones_rt.vert.bin").as_slice(),
            ),
        ] {
            gpu.write(address, &shader(program)).unwrap();
        }
        gpu.write(DRAW_PARAMETERS, &bytes(&render::draw_parameters()))
            .unwrap();
        gpu.write(FRAGMENT_CONSTANTS, &bytes(&render::fragment_constants()))
            .unwrap();
        let tic = pitch_rgba8_tic(TEXELS, 1, 1, 32);
        gpu.write(TIC, &bytes(&tic).repeat(9)).unwrap();
        gpu.write(TSC, &bytes(&scene::NEAREST_CLAMP_TSC).repeat(9))
            .unwrap();
        gpu.write(TEXELS, &[255; 32]).unwrap();
        Self { gpu }
    }

    pub fn programs() -> render::Programs {
        render::Programs {
            dungeon_vertex: DUNGEON_VERTEX,
            dungeon_fragment: DUNGEON_FRAGMENT,
            cones_vertex: CONES_VERTEX,
            sprite_vertex: SPRITE_VERTEX,
            sprite_fragment: SPRITE_FRAGMENT,
        }
    }

    pub fn ray_programs() -> render::RayPrograms {
        render::RayPrograms {
            dungeon_vertex: DUNGEON_RT_VERTEX,
            dungeon_fragment: DUNGEON_RT_FRAGMENT,
            cones_vertex: CONES_RT_VERTEX,
        }
    }

    pub fn tables() -> render::Tables {
        render::Tables {
            descriptors: DESCRIPTORS,
            vertex_constants: VERTEX_CONSTANTS,
            fragment_constants: FRAGMENT_CONSTANTS,
            draw_parameters: DRAW_PARAMETERS,
            textures: TIC,
            samplers: TSC,
        }
    }

    pub fn write_words(&mut self, words: &[u32]) {
        self.gpu.write(PUSH, &bytes(words)).unwrap();
        self.gpu.submit(&[scene::entry(PUSH, words.len())]).unwrap();
    }
}
