#![cfg(test)]

use super::engine::{bytes, entry};
use super::harness::shared_device;
use crate::gpu::compute::{ComputePipeline, HostBuffer};
use crate::gpu::engine::Gpu;
use crate::gpu::guest_dungeon::{
    GuestDungeon, DATA as GRAPHICS_DATA, DEPTH, DESCRIPTORS, REFLECTION, TARGET, VERTEX_CONSTANTS,
};
use crate::gpu::VkState;
use crate::tests::gpu_test::inst::bra;
use ash::vk::DescriptorType;
use scene::dungeon::compute::{self, Buffer, Encoded, Pass};
use scene::dungeon::render::{self, DungeonResources, Mesh, SpriteResources, Target};

const BASE: u64 = 0x3_0000_0000;
const ARENA: u64 = 0x20_0000;
const PUSH: u64 = BASE;
const PROGRAM: u64 = BASE + 0x1_0000;
const QMD: u64 = BASE + 0x2_0000;
const CBUF: u64 = BASE + 0x2_1000;
const RELEASE: u64 = BASE + 0x2_2000;
const DATA: u64 = BASE + 0x4_0000;
const SIZE: [u32; 2] = [8, 8];
const GRAPHICS_SIZE: [u32; 2] = [16, 16];
const GRAPHICS_FRAME: u64 = GRAPHICS_DATA;
const GRAPHICS_LIGHTS: u64 = GRAPHICS_DATA + 0x1000;
const GRAPHICS_REFLECTION: u64 = GRAPHICS_DATA + 0x2000;
const GRAPHICS_REFRACTION: u64 = GRAPHICS_DATA + 0x3000;
const GRAPHICS_VERTEX_DATA: u64 = GRAPHICS_DATA + 0x7000;
const GRAPHICS_VERTICES: u64 = GRAPHICS_DATA + 0x8000;
const GRAPHICS_INDICES: u64 = GRAPHICS_DATA + 0x9000;
const GRAPHICS_PARTICLES: u64 = GRAPHICS_DATA + 0xa000;

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn f32s(bytes: &[u8]) -> impl Iterator<Item = f32> + '_ {
    bytes
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
}

fn close(expected: &[u8], actual: &[u8], tolerance: f32) {
    assert_eq!(actual.len(), expected.len());
    for (i, (expected, actual)) in f32s(expected).zip(f32s(actual)).enumerate() {
        assert!(
            (actual - expected).abs() <= tolerance * (1.0 + expected.abs()),
            "word {i}: {actual} != {expected}"
        );
    }
}

fn shader(bytes: &[u8]) -> Vec<u8> {
    [bytes, &bra(-16).to_le_bytes()].concat()
}

fn spirv(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
        .collect()
}

fn buffer(address: u64, bytes: &[u8]) -> Buffer {
    Buffer {
        address,
        size: bytes.len() as u32,
    }
}

fn pass(slot: u64, program: u64) -> Pass {
    Pass {
        program,
        qmd: QMD + slot * 0x100,
        cbuf: CBUF + slot * 0x100,
        release: RELEASE + slot * 4,
    }
}

fn gpu(state: &'static VkState) -> Gpu<'static> {
    let mut gpu = Gpu::new(state, ARENA as usize, None, Default::default()).unwrap();
    gpu.space.map(BASE, 0, ARENA);
    gpu
}

fn install<const N: usize>(gpu: &mut Gpu, pass: Pass, encoded: &Encoded<N>) {
    gpu.write(pass.qmd, &bytes(&encoded.qmd)).unwrap();
    gpu.write(pass.cbuf, &bytes(&encoded.cbuf)).unwrap();
}

fn submit(gpu: &mut Gpu, passes: &[Pass]) {
    let mut words = Vec::new();
    for &pass in passes {
        compute::dispatch(pass, &mut words);
    }
    gpu.write(PUSH, &bytes(&words)).unwrap();
    gpu.submit(&[entry(PUSH, words.len())]).unwrap();
    for (slot, pass) in passes.iter().enumerate() {
        let mut release = [0; 4];
        gpu.read(pass.release, &mut release).unwrap();
        assert_eq!(u32::from_le_bytes(release), slot as u32 + 1);
    }
}

fn native(
    state: &VkState,
    code: &[u8],
    data: &[Vec<u8>],
    uniform: usize,
    groups: [u32; 3],
) -> Vec<Vec<u8>> {
    let buffers: Vec<_> = data
        .iter()
        .map(|data| {
            let mut buffer = HostBuffer::new(state, data.len() as u64).unwrap();
            buffer.write(data).unwrap();
            buffer
        })
        .collect();
    let bindings: Vec<_> = (0..buffers.len())
        .map(|i| {
            (
                i as u32,
                if i == uniform {
                    DescriptorType::UNIFORM_BUFFER
                } else {
                    DescriptorType::STORAGE_BUFFER
                },
            )
        })
        .collect();
    let mut pipeline = ComputePipeline::new(state, &spirv(code), &bindings).unwrap();
    pipeline.descriptors.bind(
        &buffers
            .iter()
            .enumerate()
            .map(|(i, buffer)| (i as u32, buffer))
            .collect::<Vec<_>>(),
    );
    state.dispatch(&pipeline, groups).unwrap();
    buffers
        .iter()
        .map(|buffer| {
            let mut bytes = vec![0; buffer.size as usize];
            buffer.read(&mut bytes).unwrap();
            bytes
        })
        .collect()
}

fn water_uniforms() -> Vec<u8> {
    let mut uniforms = vec![0; 32 + 32 * 16];
    uniforms[..48].copy_from_slice(&floats(&[
        8.0,
        8.0,
        1.0 / 60.0,
        2.0,
        8.0,
        4.0,
        1.0,
        0.75,
        4.0,
        4.0,
        0.1,
        2.0,
    ]));
    uniforms
}

fn frame_uniforms() -> Vec<u8> {
    let mut frame = [0.0f32; 60];
    for matrix in [0, 16] {
        for i in 0..4 {
            frame[matrix + i * 5] = 1.0;
        }
    }
    frame[32..36].copy_from_slice(&[0.0, 1.0, -2.0, 0.25]);
    frame[36..40].copy_from_slice(&[1.0, 0.0, 0.0, 0.0]);
    frame[40..44].copy_from_slice(&[0.0, 1.0, 0.0, 0.0]);
    frame[44..48].copy_from_slice(&[-2.0, -2.0, 2.0, 2.0]);
    frame[48..52].copy_from_slice(&[10.0, 10.0, 10.0, 0.5]);
    frame[52..56].copy_from_slice(&[8.0, 8.0, 0.0, 0.0]);
    frame[56..60].copy_from_slice(&[0.0, 1.0, 2.0, 3.0]);
    floats(&frame)
}

fn lights() -> Vec<u8> {
    floats(&[
        0.0f32, 1.0, 0.0, 0.8, 1.0, 0.2, 0.05, 1.0, 1.0, 1.5, 0.5, 0.6, 0.3, 0.6, 1.0, 1.0, -1.0,
        0.8, 1.0, 0.4, 0.2, 1.0, 0.4, 1.0, 0.5, 2.0, -0.5, 0.7, 0.8, 0.7, 0.3, 1.0,
    ])
}

fn hdr() -> Vec<u8> {
    let texel = [0x3800_3e00u32, 0x4400_4000];
    bytes(&texel).repeat((SIZE[0] * SIZE[1]) as usize)
}

fn graphics_frame(mode: f32, size: [u32; 2]) -> Vec<u8> {
    let mut frame = [0.0f32; 64];
    for matrix in [0, 16] {
        for i in 0..4 {
            frame[matrix + i * 5] = 1.0;
        }
    }
    frame[32..36].copy_from_slice(&[0.0, 0.0, 2.0, 0.0]);
    frame[36..40].copy_from_slice(&[1.0, 0.0, 0.0, 1.0]);
    frame[40..44].copy_from_slice(&[0.0, 1.0, 0.0, 0.0]);
    frame[44..48].copy_from_slice(&[-1.0, -1.0, 1.0, 1.0]);
    frame[52..56].copy_from_slice(&[size[0] as f32, size[1] as f32, mode, 0.0]);
    frame[60..64].copy_from_slice(&[1.0, 1.0, 0.0, 0.0]);
    floats(&frame)
}

fn graphics_vertices(material: f32) -> Vec<u8> {
    [[-0.8, -0.8, 0.0], [0.8, -0.8, 0.0], [0.0, 0.8, 0.0]]
        .into_iter()
        .flat_map(|position| {
            [
                position[0],
                position[1],
                position[2],
                0.0,
                0.0,
                1.0,
                0.5,
                0.5,
                0.4,
                0.6,
                0.8,
                material,
            ]
            .into_iter()
            .flat_map(f32::to_le_bytes)
        })
        .collect()
}

fn install_graphics(gpu: &mut GuestDungeon, mode: f32, target_size: [u32; 2]) -> Mesh {
    let geometry_at = GRAPHICS_DATA + 0x4000;
    let shadows_at = GRAPHICS_DATA + 0x5000;
    let rooms_at = GRAPHICS_DATA + 0x6000;
    let frame = graphics_frame(mode, target_size);
    let vertices = graphics_vertices(5.0);
    let hdr_bytes = (target_size[0] * target_size[1] * 8) as usize;
    for (address, data) in [
        (GRAPHICS_FRAME, frame.clone()),
        (GRAPHICS_LIGHTS, vec![0; 32]),
        (GRAPHICS_REFLECTION, vec![0; hdr_bytes]),
        (GRAPHICS_REFRACTION, vec![0; hdr_bytes]),
        (geometry_at, vec![0; 48]),
        (shadows_at, vec![0; 8]),
        (rooms_at, floats(&[-10.0, -10.0, 10.0, 10.0])),
        (
            GRAPHICS_VERTEX_DATA,
            floats(&[0.55, -0.4, 0.0, 1.0, 0.32, 0.0, 0.0, 0.0]),
        ),
        (GRAPHICS_VERTICES, vertices.clone()),
        (GRAPHICS_INDICES, bytes(&[0, 1, 2])),
    ] {
        gpu.gpu.write(address, &data).unwrap();
    }
    let buffer = |address, size| render::Buffer { address, size };
    let descriptors = render::dungeon_descriptors(DungeonResources {
        frame: buffer(GRAPHICS_FRAME, frame.len() as u32),
        lights: buffer(GRAPHICS_LIGHTS, 32),
        reflection: buffer(GRAPHICS_REFLECTION, hdr_bytes as u32),
        refraction: buffer(GRAPHICS_REFRACTION, hdr_bytes as u32),
        geometry: buffer(geometry_at, 48),
        shadows: buffer(shadows_at, 8),
        rooms: buffer(rooms_at, 16),
        vertex_data: buffer(GRAPHICS_VERTEX_DATA, 32),
        textures: [0, 1, 2, 3, 4, 5, 6, 7, 8],
    });
    gpu.gpu.write(DESCRIPTORS, &bytes(&descriptors)).unwrap();
    gpu.gpu
        .write(VERTEX_CONSTANTS, &bytes(&render::cone_constants()))
        .unwrap();
    Mesh {
        vertices: buffer(GRAPHICS_VERTICES, vertices.len() as u32),
        indices: GRAPHICS_INDICES,
    }
}

fn install_sprites(gpu: &mut GuestDungeon, depth: u64, size: [u32; 2]) {
    let particle = floats(&[0.0, 0.0, 0.0, 3.0, 0.6, 0.6, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5]);
    gpu.gpu.write(GRAPHICS_PARTICLES, &particle).unwrap();
    let descriptors = render::sprite_descriptors(SpriteResources {
        frame: render::Buffer {
            address: GRAPHICS_FRAME,
            size: 256,
        },
        lights: render::Buffer {
            address: GRAPHICS_LIGHTS,
            size: 32,
        },
        depth: render::Buffer {
            address: depth,
            size: size[0] * size[1] * 4,
        },
        particles: render::Buffer {
            address: GRAPHICS_PARTICLES,
            size: particle.len() as u32,
        },
        texture: 0,
    });
    gpu.gpu.write(DESCRIPTORS, &bytes(&descriptors)).unwrap();
    gpu.gpu
        .write(VERTEX_CONSTANTS, &bytes(&render::sprite_constants()))
        .unwrap();
}

fn half(value: u16) -> f32 {
    let sign = u32::from(value & 0x8000) << 16;
    let exponent = (value >> 10) & 0x1f;
    let mantissa = u32::from(value & 0x03ff);
    let bits = match exponent {
        0 if mantissa == 0 => sign,
        0 => {
            let shift = mantissa.leading_zeros() - 22;
            sign | (127 - 15 - shift) << 23 | (mantissa << (shift + 1) & 0x7f_ffff)
        }
        31 => sign | 0x7f80_0000 | mantissa << 13,
        _ => sign | (u32::from(exponent) + 112) << 23 | mantissa << 13,
    };
    f32::from_bits(bits)
}

fn hdr_pixel(bytes: &[u8], size: [u32; 2], x: u32, y: u32) -> [f32; 4] {
    let offset = ((y * size[0] + x) * 8) as usize;
    core::array::from_fn(|i| {
        half(u16::from_le_bytes(
            bytes[offset + 2 * i..][..2].try_into().unwrap(),
        ))
    })
}

#[test]
fn shadow_pass_writes_distance_to_hdr_and_consumes_its_clear() {
    let Some(state) = shared_device() else { return };
    let mut guest = GuestDungeon::new(state);
    let mesh = install_graphics(&mut guest, 2.0, GRAPHICS_SIZE);
    let mut words = Vec::new();
    render::shadow(
        Target {
            colour: TARGET,
            depth: DEPTH,
            size: GRAPHICS_SIZE,
        },
        GuestDungeon::programs(),
        GuestDungeon::tables(),
        mesh,
        3,
        [6, 0],
        &mut words,
    );
    guest.write_words(&words);
    let mut pixels = vec![0; (GRAPHICS_SIZE[0] * GRAPHICS_SIZE[1] * 8) as usize];
    guest.gpu.read(TARGET, &mut pixels).unwrap();
    assert!(hdr_pixel(&pixels, GRAPHICS_SIZE, 8, 8)
        .iter()
        .all(|v| (v - 2.0).abs() < 0.05));
    assert_eq!(hdr_pixel(&pixels, GRAPHICS_SIZE, 0, 0), [30.0; 4]);
}

#[test]
fn reflection_pass_matches_distance_oracle_at_half_and_quarter_resolution() {
    let Some(state) = shared_device() else { return };
    let mut guest = GuestDungeon::new(state);
    for size in [[16, 8], [8, 4]] {
        let mesh = install_graphics(&mut guest, 2.0, size);
        let mut words = Vec::new();
        render::reflection(
            Target {
                colour: REFLECTION,
                depth: DEPTH,
                size,
            },
            GuestDungeon::programs(),
            GuestDungeon::tables(),
            mesh,
            3,
            [6, 0],
            &mut words,
        );
        guest.write_words(&words);
        let mut pixels = vec![0; (size[0] * size[1] * 8) as usize];
        guest.gpu.read(REFLECTION, &mut pixels).unwrap();
        assert!(hdr_pixel(&pixels, size, size[0] / 2, size[1] / 2)
            .iter()
            .all(|v| (v - 2.0).abs() < 0.08));
        let clear = hdr_pixel(&pixels, size, 0, 0);
        for (actual, expected) in clear.into_iter().zip([0.008, 0.012, 0.018, 40.0]) {
            assert!((actual - expected).abs() < 0.02);
        }
    }
}

#[test]
fn opaque_pass_draws_the_dungeon_and_cone_pair() {
    let Some(state) = shared_device() else { return };
    let mut guest = GuestDungeon::new(state);
    let mesh = install_graphics(&mut guest, 2.0, GRAPHICS_SIZE);
    let mut vertices = graphics_vertices(5.0);
    for vertex in vertices.chunks_exact_mut(48) {
        vertex[8..12].copy_from_slice(&0.8f32.to_le_bytes());
    }
    guest.gpu.write(GRAPHICS_VERTICES, &vertices).unwrap();
    guest
        .gpu
        .write(
            GRAPHICS_VERTEX_DATA,
            &floats(&[0.0, -1.0, 0.5, 2.0, 1.0, 0.0, 0.0, 0.0]),
        )
        .unwrap();
    let mut mesh_words = Vec::new();
    render::opaque(
        Target {
            colour: TARGET,
            depth: DEPTH,
            size: GRAPHICS_SIZE,
        },
        GuestDungeon::programs(),
        GuestDungeon::tables(),
        mesh,
        3,
        [6, 0],
        &mut mesh_words,
    );
    guest.write_words(&mesh_words);
    let mut mesh_pixels = vec![0; (GRAPHICS_SIZE[0] * GRAPHICS_SIZE[1] * 8) as usize];
    guest.gpu.read(TARGET, &mut mesh_pixels).unwrap();

    let mut pair_words = Vec::new();
    render::opaque(
        Target {
            colour: REFLECTION,
            depth: DEPTH + 0x1000,
            size: GRAPHICS_SIZE,
        },
        GuestDungeon::programs(),
        GuestDungeon::tables(),
        mesh,
        3,
        [24 * 12 * 6, 1],
        &mut pair_words,
    );
    guest.write_words(&pair_words);
    let mut pair_pixels = vec![0; mesh_pixels.len()];
    guest.gpu.read(REFLECTION, &mut pair_pixels).unwrap();
    assert!(pair_pixels.iter().zip(&mesh_pixels).any(|(a, b)| a != b));
    assert!(hdr_pixel(&pair_pixels, GRAPHICS_SIZE, 8, 8)[0] < 3.0);
}

#[test]
fn water_pass_loads_the_opaque_target_without_reclearing_it() {
    let Some(state) = shared_device() else { return };
    let mut guest = GuestDungeon::new(state);
    let mesh = install_graphics(&mut guest, 2.0, GRAPHICS_SIZE);
    let mut opaque = Vec::new();
    render::opaque(
        Target {
            colour: TARGET,
            depth: DEPTH,
            size: GRAPHICS_SIZE,
        },
        GuestDungeon::programs(),
        GuestDungeon::tables(),
        mesh,
        3,
        [6, 0],
        &mut opaque,
    );
    guest.write_words(&opaque);
    let mut before = vec![0; (GRAPHICS_SIZE[0] * GRAPHICS_SIZE[1] * 8) as usize];
    guest.gpu.read(TARGET, &mut before).unwrap();
    let water_vertices = graphics_vertices(4.0);
    guest.gpu.write(GRAPHICS_VERTICES, &water_vertices).unwrap();
    guest
        .gpu
        .write(GRAPHICS_VERTEX_DATA, &floats(&[1.0, 0.0, 0.0, 0.0]))
        .unwrap();
    let mut words = Vec::new();
    render::water(
        Target {
            colour: TARGET,
            depth: DEPTH,
            size: GRAPHICS_SIZE,
        },
        GuestDungeon::programs(),
        GuestDungeon::tables(),
        mesh,
        0,
        3,
        &mut words,
    );
    guest.write_words(&words);
    let mut pixels = vec![0; (GRAPHICS_SIZE[0] * GRAPHICS_SIZE[1] * 8) as usize];
    guest.gpu.read(TARGET, &mut pixels).unwrap();
    assert!(pixels.iter().zip(&before).any(|(a, b)| a != b));
    let clear = hdr_pixel(&pixels, GRAPHICS_SIZE, 0, 0);
    assert!((clear[0] - 0.008).abs() < 0.002 && (clear[3] - 40.0).abs() < 0.02);
}

#[test]
fn sprite_pass_uses_premultiplied_blending_over_the_opaque_target() {
    let Some(state) = shared_device() else { return };
    let mut guest = GuestDungeon::new(state);
    let mesh = install_graphics(&mut guest, 1.0, GRAPHICS_SIZE);
    let mut opaque = Vec::new();
    render::opaque(
        Target {
            colour: TARGET,
            depth: DEPTH,
            size: GRAPHICS_SIZE,
        },
        GuestDungeon::programs(),
        GuestDungeon::tables(),
        mesh,
        3,
        [6, 0],
        &mut opaque,
    );
    guest.write_words(&opaque);
    let mut before = vec![0; (GRAPHICS_SIZE[0] * GRAPHICS_SIZE[1] * 8) as usize];
    guest.gpu.read(TARGET, &mut before).unwrap();
    install_sprites(&mut guest, DEPTH, GRAPHICS_SIZE);
    let mut words = Vec::new();
    render::sprites(
        Target {
            colour: TARGET,
            depth: DEPTH,
            size: GRAPHICS_SIZE,
        },
        GuestDungeon::programs(),
        GuestDungeon::tables(),
        1,
        &mut words,
    );
    guest.write_words(&words);
    let mut pixels = vec![0; before.len()];
    guest.gpu.read(TARGET, &mut pixels).unwrap();
    let (background, blended) = (
        hdr_pixel(&before, GRAPHICS_SIZE, 8, 8),
        hdr_pixel(&pixels, GRAPHICS_SIZE, 8, 8),
    );
    assert!(
        (blended[0] - (0.05 + background[0] * 0.5)).abs() < 0.03,
        "{background:?} -> {blended:?}"
    );
    assert!((blended[1] - background[1] * 0.5).abs() < 0.02);
    assert!((blended[3] - background[3]).abs() < 0.02);
}

#[test]
fn water_ping_pong_matches_native_and_releases_each_visible_step() {
    let Some(state) = shared_device() else { return };
    let mut initial = Vec::new();
    for y in 0..SIZE[1] {
        for x in 0..SIZE[0] {
            initial.extend_from_slice(&floats(&[
                (x + y) as f32 * 0.001,
                (x as f32 - y as f32) * 0.002,
            ]));
        }
    }
    let uniforms = water_uniforms();
    let blank = vec![0; initial.len()];
    let surface = vec![0; (SIZE[0] * SIZE[1] * 16) as usize];
    let mut expected = native(
        state,
        include_bytes!("../../../../demos/shaders/dungeon_water.spv"),
        &[
            initial.clone(),
            blank.clone(),
            surface.clone(),
            uniforms.clone(),
        ],
        3,
        [1, 1, 1],
    );
    expected = native(
        state,
        include_bytes!("../../../../demos/shaders/dungeon_water.spv"),
        &[
            expected[1].clone(),
            expected[0].clone(),
            surface.clone(),
            uniforms.clone(),
        ],
        3,
        [1, 1, 1],
    );

    let (previous, next, surface_at, uniforms_at) =
        (DATA, DATA + 0x1000, DATA + 0x2000, DATA + 0x3000);
    let passes = [pass(0, PROGRAM), pass(1, PROGRAM)];
    let mut guest = gpu(state);
    for (address, data) in [
        (
            PROGRAM,
            shader(include_bytes!(
                "../../../tests/fixtures/driver-shaders/dungeon_water.comp.bin"
            )),
        ),
        (previous, initial),
        (next, blank),
        (surface_at, surface),
        (uniforms_at, uniforms),
    ] {
        guest.write(address, &data).unwrap();
    }
    let first = compute::water(
        passes[0],
        buffer(previous, &expected[0]),
        buffer(next, &expected[1]),
        Buffer {
            address: surface_at,
            size: (SIZE[0] * SIZE[1] * 16),
        },
        buffer(uniforms_at, &water_uniforms()),
        SIZE,
        1,
    );
    let second = compute::water(
        passes[1],
        buffer(next, &expected[1]),
        buffer(previous, &expected[0]),
        Buffer {
            address: surface_at,
            size: (SIZE[0] * SIZE[1] * 16),
        },
        buffer(uniforms_at, &water_uniforms()),
        SIZE,
        2,
    );
    install(&mut guest, passes[0], &first);
    install(&mut guest, passes[1], &second);
    submit(&mut guest, &passes);
    for (address, expected) in [(previous, &expected[1]), (surface_at, &expected[2])] {
        let mut actual = vec![0; expected.len()];
        guest.read(address, &mut actual).unwrap();
        close(expected, &actual, 2e-4);
    }
}

#[test]
fn particles_match_native_emitter_order_and_48_byte_stride() {
    let Some(state) = shared_device() else { return };
    let lights = lights();
    let uniforms = floats(&[0.75, 16.0, 0.0, 0.0]);
    let output = vec![0; 2 * 16 * 48];
    let expected = native(
        state,
        include_bytes!("../../../../demos/shaders/dungeon_particles.comp.spv"),
        &[lights.clone(), output.clone(), uniforms.clone()],
        2,
        [1, 1, 1],
    );
    let (lights_at, output_at, uniforms_at) = (DATA, DATA + 0x1000, DATA + 0x3000);
    let pass = pass(0, PROGRAM);
    let mut guest = gpu(state);
    for (address, data) in [
        (
            PROGRAM,
            shader(include_bytes!(
                "../../../tests/fixtures/driver-shaders/dungeon_particles.comp.bin"
            )),
        ),
        (lights_at, lights),
        (output_at, output),
        (uniforms_at, uniforms),
    ] {
        guest.write(address, &data).unwrap();
    }
    let encoded = compute::particles(
        pass,
        buffer(lights_at, &expected[0]),
        buffer(output_at, &expected[1]),
        buffer(uniforms_at, &expected[2]),
        32,
        1,
    );
    install(&mut guest, pass, &encoded);
    submit(&mut guest, &[pass]);
    let mut actual = vec![0; expected[1].len()];
    guest.read(output_at, &mut actual).unwrap();
    assert_eq!(actual.chunks_exact(48).len(), 32);
    close(&expected[1], &actual, 3e-4);
}

#[test]
fn fog_matches_native_hdr_fog_and_bloom_outputs() {
    let Some(state) = shared_device() else { return };
    let (hdr, frame, lights) = (hdr(), frame_uniforms(), lights());
    let pixels = vec![0; 2 * 2 * 16];
    let expected = native(
        state,
        include_bytes!("../../../../demos/shaders/dungeon_fog.comp.spv"),
        &[
            hdr.clone(),
            pixels.clone(),
            frame.clone(),
            lights.clone(),
            pixels.clone(),
        ],
        2,
        [1, 1, 1],
    );
    let (hdr_at, fog_at, frame_at, lights_at, bloom_at) = (
        DATA,
        DATA + 0x1000,
        DATA + 0x2000,
        DATA + 0x3000,
        DATA + 0x4000,
    );
    let pass = pass(0, PROGRAM);
    let mut guest = gpu(state);
    for (address, data) in [
        (
            PROGRAM,
            shader(include_bytes!(
                "../../../tests/fixtures/driver-shaders/dungeon_fog.comp.bin"
            )),
        ),
        (hdr_at, hdr),
        (fog_at, pixels.clone()),
        (frame_at, frame),
        (lights_at, lights),
        (bloom_at, pixels),
    ] {
        guest.write(address, &data).unwrap();
    }
    let encoded = compute::fog(
        pass,
        [
            buffer(hdr_at, &expected[0]),
            buffer(fog_at, &expected[1]),
            buffer(frame_at, &expected[2]),
            buffer(lights_at, &expected[3]),
            buffer(bloom_at, &expected[4]),
        ],
        SIZE,
        1,
    );
    install(&mut guest, pass, &encoded);
    submit(&mut guest, &[pass]);
    for (address, expected) in [(fog_at, &expected[1]), (bloom_at, &expected[4])] {
        let mut actual = vec![0; expected.len()];
        guest.read(address, &mut actual).unwrap();
        close(expected, &actual, 5e-4);
    }
}

#[test]
fn post_matches_native_rgba8_after_visible_fog_pass() {
    let Some(state) = shared_device() else { return };
    let (hdr, frame, lights) = (hdr(), frame_uniforms(), lights());
    let quarter = vec![0; 2 * 2 * 16];
    let fogged = native(
        state,
        include_bytes!("../../../../demos/shaders/dungeon_fog.comp.spv"),
        &[
            hdr.clone(),
            quarter.clone(),
            frame.clone(),
            lights.clone(),
            quarter.clone(),
        ],
        2,
        [1, 1, 1],
    );
    let output = vec![0; (SIZE[0] * SIZE[1] * 4) as usize];
    let expected = native(
        state,
        include_bytes!("../../../../demos/shaders/dungeon_post.comp.spv"),
        &[
            hdr.clone(),
            output.clone(),
            frame.clone(),
            lights.clone(),
            fogged[1].clone(),
            fogged[4].clone(),
        ],
        2,
        [1, 1, 1],
    );
    let (hdr_at, output_at, frame_at, lights_at, fog_at, bloom_at) = (
        DATA,
        DATA + 0x1000,
        DATA + 0x2000,
        DATA + 0x3000,
        DATA + 0x4000,
        DATA + 0x5000,
    );
    let passes = [pass(0, PROGRAM), pass(1, PROGRAM + 0x4000)];
    let mut guest = gpu(state);
    for (address, data) in [
        (
            PROGRAM,
            shader(include_bytes!(
                "../../../tests/fixtures/driver-shaders/dungeon_fog.comp.bin"
            )),
        ),
        (
            PROGRAM + 0x4000,
            shader(include_bytes!(
                "../../../tests/fixtures/driver-shaders/dungeon_post.comp.bin"
            )),
        ),
        (hdr_at, hdr),
        (output_at, output),
        (frame_at, frame),
        (lights_at, lights),
        (fog_at, quarter.clone()),
        (bloom_at, quarter),
    ] {
        guest.write(address, &data).unwrap();
    }
    let fog = compute::fog(
        passes[0],
        [
            buffer(hdr_at, &expected[0]),
            buffer(fog_at, &fogged[1]),
            buffer(frame_at, &expected[2]),
            buffer(lights_at, &expected[3]),
            buffer(bloom_at, &fogged[4]),
        ],
        SIZE,
        1,
    );
    let post = compute::post(
        passes[1],
        [
            buffer(hdr_at, &expected[0]),
            buffer(output_at, &expected[1]),
            buffer(frame_at, &expected[2]),
            buffer(lights_at, &expected[3]),
            buffer(fog_at, &fogged[1]),
            buffer(bloom_at, &fogged[4]),
        ],
        SIZE,
        2,
    );
    install(&mut guest, passes[0], &fog);
    install(&mut guest, passes[1], &post);
    submit(&mut guest, &passes);
    let mut actual = vec![0; expected[1].len()];
    guest.read(output_at, &mut actual).unwrap();
    for (i, (&expected, &actual)) in expected[1].iter().zip(&actual).enumerate() {
        assert!(
            expected.abs_diff(actual) <= 2,
            "byte {i}: {actual} != {expected}"
        );
    }
}
