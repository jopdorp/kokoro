#![cfg(test)]

use super::engine::{bytes, entry};
use super::harness::shared_device;
use crate::gpu::compute::{AccelerationStructure, ComputePipeline, HostBuffer, HostImage, Triangles};
use crate::gpu::engine::Gpu;
use crate::gpu::graphics::{DrawState, GraphicsPipeline, RasterState, RenderTarget, TargetState, VertexLayout};
use crate::gpu::guest_dungeon::{
    GuestDungeon, DATA as GRAPHICS_DATA, DEPTH, DESCRIPTORS, REFLECTION, TARGET, VERTEX_CONSTANTS,
};
use crate::gpu::VkState;
use crate::gpu::texture::TexturePool;
use crate::tests::gpu_test::inst::bra;
use ash::vk::{self, DescriptorType};
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
const GRAPHICS_RAY_GEOMETRY: u64 = GRAPHICS_DATA + 0xb000;
const GRAPHICS_RAY_INDICES: u64 = GRAPHICS_DATA + 0xc000;
const GRAPHICS_TLAS: u64 = GRAPHICS_DATA + 0xd000;

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
    let mut gpu = Gpu::new(state, ARENA as usize, None, crate::gpu::SpeedHacks::none()).unwrap();
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

fn ray_geometry() -> Vec<u8> {
    [[-0.8, -0.8, 0.5], [0.8, -0.8, 0.5], [0.0, 0.8, 0.5]]
        .into_iter()
        .flat_map(|position| floats(&[
            position[0], position[1], position[2], 5.0,
            0.0, 0.0, -1.0, 0.0,
            1.0, 1.0, 1.0, 0.0,
        ]))
        .collect()
}

fn native_ray_frame(state: &VkState, frame: &[u8], geometry: &[u8], size: [u32; 2]) -> Vec<u8> {
    let make_buffer = |data: &[u8]| {
        let mut buffer = HostBuffer::new(state, data.len() as u64).unwrap();
        buffer.write(data).unwrap();
        buffer
    };
    let vertices_data = graphics_vertices(3.0);
    let indices_data = bytes(&[0, 1, 2]);
    let vertices = make_buffer(&vertices_data);
    let indices = make_buffer(&indices_data);
    let uniforms = make_buffer(frame);
    let waves = make_buffer(&[0; 32]);
    let lights = make_buffer(&floats(&[0.0, 0.0, 1.0, 0.4, 8.0, 8.0, 8.0, 0.0]));
    let blank = make_buffer(&vec![0; (size[0] * size[1] * 8) as usize]);
    let geometry_buffer = make_buffer(geometry);
    let shadows = make_buffer(&[0; 8]);
    let rooms = make_buffer(&floats(&[-10.0, -10.0, 10.0, 10.0]));
    let positions: Vec<_> = geometry.chunks_exact(48).map(|vertex| {
        [0, 4, 8].map(|at| f32::from_le_bytes(vertex[at..at + 4].try_into().unwrap()))
    }).collect();
    let structure = AccelerationStructure::new(state, &[Triangles {
        positions: &positions,
        indices: &[0, 1, 2],
        transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]],
        opaque: true,
    }]).unwrap();
    let tic = bytes(&scene::pitch_rgba8_tic(0, 1, 1, 32));
    let tsc = bytes(&scene::NEAREST_CLAMP_TSC);
    let pool = TexturePool { tic: &tic, tsc: &tsc };
    let texture = HostImage::new(state, &pool.texture(0).unwrap(), &[255; 32], &pool.sampler(0).unwrap()).unwrap();
    let vertex_bindings = [(0, DescriptorType::UNIFORM_BUFFER), (1, DescriptorType::STORAGE_BUFFER)];
    let mut fragment_bindings = vec![(0, DescriptorType::UNIFORM_BUFFER), (1, DescriptorType::STORAGE_BUFFER)];
    fragment_bindings.extend((2..11).map(|binding| (binding, DescriptorType::COMBINED_IMAGE_SAMPLER)));
    fragment_bindings.extend((11..14).map(|binding| (binding, DescriptorType::STORAGE_BUFFER)));
    fragment_bindings.extend([(14, DescriptorType::ACCELERATION_STRUCTURE_KHR), (15, DescriptorType::STORAGE_BUFFER), (16, DescriptorType::STORAGE_BUFFER)]);
    let bindings = [vk::VertexInputBindingDescription { binding: 0, stride: 48, input_rate: vk::VertexInputRate::VERTEX }];
    let attributes = [
        (0, vk::Format::R32G32B32_SFLOAT, 0),
        (1, vk::Format::R32G32B32_SFLOAT, 12),
        (2, vk::Format::R32G32_SFLOAT, 24),
        (3, vk::Format::R32G32B32_SFLOAT, 32),
        (4, vk::Format::R32_SFLOAT, 44),
    ].map(|(location, format, offset)| vk::VertexInputAttributeDescription { location, binding: 0, format, offset });
    let blend = vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA);
    let mut pipeline = GraphicsPipeline::with_format(
        state,
        &spirv(include_bytes!("../../../../demos/shaders/dungeon.vert.spv")),
        None,
        &spirv(include_bytes!("../../../../demos/shaders/dungeon_rt.frag.spv")),
        [&vertex_bindings, &fragment_bindings],
        &VertexLayout { bindings: &bindings, attributes: &attributes, topology: vk::PrimitiveTopology::TRIANGLE_LIST },
        &TargetState { blend, depth: Some(vk::Format::D32_SFLOAT) },
        vk::Format::R16G16B16A16_SFLOAT,
    ).unwrap();
    pipeline.descriptors[0].bind(&[(0, &uniforms), (1, &waves)]);
    pipeline.descriptors[1].bind(&[(0, &uniforms), (1, &lights), (11, &blank), (12, &blank), (13, &geometry_buffer), (15, &shadows), (16, &rooms)]);
    pipeline.descriptors[1].bind_images(&(2..11).map(|binding| (binding, &texture)).collect::<Vec<_>>());
    pipeline.descriptors[1].bind_acceleration_structure(14, &structure);
    let target = RenderTarget::with_format(state, size[0], size[1], Some(vk::Format::D32_SFLOAT), None, vk::Format::R16G16B16A16_SFLOAT).unwrap();
    let raster = RasterState {
        viewport: vk::Viewport { x: 0.0, y: 0.0, width: size[0] as f32, height: size[1] as f32, min_depth: 0.0, max_depth: 1.0 },
        scissor: target.extent.into(), cull_face: vk::CullModeFlags::NONE, front_face: vk::FrontFace::COUNTER_CLOCKWISE,
        topology: vk::PrimitiveTopology::TRIANGLE_LIST, depth_test: true, depth_write: true, depth_func: vk::CompareOp::LESS_OR_EQUAL,
        blend_const: [0.0; 4], blend,
    };
    state.draw(&pipeline, &target, &DrawState {
        vertex_buffers: &[(&vertices, 48)], index_buffer: Some((&indices, vk::IndexType::UINT32)), vertex_count: 3,
        first_vertex: 0, first_instance: 0, instance_count: 1, clear: Some([0.008, 0.012, 0.018, 40.0]), clear_depth: Some(1.0), load: false, raster,
    }).unwrap();
    let mut pixels = vec![0; (size[0] * size[1] * 8) as usize];
    target.read(&mut pixels).unwrap();
    pixels
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
fn ray_pass_hits_guest_geometry_and_rebuilds_after_it_moves() {
    let Some(state) = shared_device() else { return };
    if !crate::gpu::demo::supports_raytraced(state) { return eprintln!("skipping: device does not support the ray-traced demo"); }
    let mut guest = GuestDungeon::new(state);
    let size = [4, 4];
    let mesh = install_graphics(&mut guest, 0.0, size);
    guest.gpu.write(GRAPHICS_VERTICES, &graphics_vertices(3.0)).unwrap();
    let mut frame = graphics_frame(0.0, size);
    frame[43 * 4..44 * 4].copy_from_slice(&1.0f32.to_le_bytes());
    let geometry = ray_geometry();
    let indices = bytes(&[0, 1, 2]);
    guest.gpu.write(GRAPHICS_FRAME, &frame).unwrap();
    guest.gpu.write(GRAPHICS_LIGHTS, &floats(&[0.0, 0.0, 1.0, 0.4, 8.0, 8.0, 8.0, 0.0])).unwrap();
    guest.gpu.write(GRAPHICS_RAY_GEOMETRY, &geometry).unwrap();
    guest.gpu.write(GRAPHICS_RAY_INDICES, &indices).unwrap();
    let ray = render::RayGeometry {
        vertices: render::Buffer { address: GRAPHICS_RAY_GEOMETRY, size: geometry.len() as u32 },
        indices: render::Buffer { address: GRAPHICS_RAY_INDICES, size: indices.len() as u32 },
        tlas: GRAPHICS_TLAS,
    };
    let mut registry = Vec::new();
    render::acceleration_structure(ray, &mut registry);
    guest.gpu.write(GRAPHICS_TLAS, &bytes(&registry)).unwrap();
    let resources = DungeonResources {
        frame: render::Buffer { address: GRAPHICS_FRAME, size: frame.len() as u32 },
        lights: render::Buffer { address: GRAPHICS_LIGHTS, size: 32 },
        reflection: render::Buffer { address: GRAPHICS_REFLECTION, size: size[0] * size[1] * 8 },
        refraction: render::Buffer { address: GRAPHICS_REFRACTION, size: size[0] * size[1] * 8 },
        geometry: ray.vertices,
        shadows: render::Buffer { address: GRAPHICS_DATA + 0x5000, size: 8 },
        rooms: render::Buffer { address: GRAPHICS_DATA + 0x6000, size: 16 },
        vertex_data: render::Buffer { address: GRAPHICS_VERTEX_DATA, size: 32 },
        textures: [0, 1, 2, 3, 4, 5, 6, 7, 8],
    };
    guest.gpu.write(DESCRIPTORS, &bytes(&render::dungeon_ray_descriptors(resources, ray.tlas))).unwrap();
    let draw = |target, depth, words: &mut Vec<_>| render::ray_opaque(
        Target { colour: target, depth, size },
        GuestDungeon::programs(), GuestDungeon::ray_programs(), GuestDungeon::tables(), mesh, 3, [6, 0], words,
    );
    let mut words = Vec::new();
    draw(TARGET, DEPTH, &mut words);
    guest.write_words(&words);
    let mut occluded = vec![0; (size[0] * size[1] * 8) as usize];
    guest.gpu.read(TARGET, &mut occluded).unwrap();
    frame[43 * 4..44 * 4].copy_from_slice(&0.0f32.to_le_bytes());
    guest.gpu.write(GRAPHICS_FRAME, &frame).unwrap();
    words.clear();
    draw(REFLECTION, DEPTH + 0x1000, &mut words);
    guest.write_words(&words);
    let mut ray_off = vec![0; occluded.len()];
    guest.gpu.read(REFLECTION, &mut ray_off).unwrap();
    frame[43 * 4..44 * 4].copy_from_slice(&1.0f32.to_le_bytes());
    guest.gpu.write(GRAPHICS_FRAME, &frame).unwrap();
    let mut moved = geometry.clone();
    for vertex in moved.chunks_exact_mut(48) { vertex[8..12].copy_from_slice(&4.0f32.to_le_bytes()); }
    guest.gpu.write(GRAPHICS_RAY_GEOMETRY, &moved).unwrap();
    words.clear();
    draw(REFLECTION, DEPTH + 0x1000, &mut words);
    guest.write_words(&words);
    let mut visible = vec![0; occluded.len()];
    guest.gpu.read(REFLECTION, &mut visible).unwrap();
    let (hit, off, miss) = (hdr_pixel(&occluded, size, 2, 2), hdr_pixel(&ray_off, size, 2, 2), hdr_pixel(&visible, size, 2, 2));
    let native_miss = hdr_pixel(&native_ray_frame(state, &frame, &moved, size), size, 2, 2);
    frame[43 * 4..44 * 4].copy_from_slice(&0.0f32.to_le_bytes());
    let native_off = hdr_pixel(&native_ray_frame(state, &frame, &geometry, size), size, 2, 2);
    frame[43 * 4..44 * 4].copy_from_slice(&1.0f32.to_le_bytes());
    let native_hit = hdr_pixel(&native_ray_frame(state, &frame, &geometry, size), size, 2, 2);
    assert!(hit[0] + 0.1 < miss[0], "{hit:?} !< {miss:?}");
    assert!(off.into_iter().zip(miss).all(|(a, b)| (a - b).abs() < 0.08), "{off:?} != {miss:?}");
    assert!((hit[3] - miss[3]).abs() < 0.05, "{hit:?} != {miss:?}");
    for (guest, native) in [(hit, native_hit), (off, native_off), (miss, native_miss)] {
        assert!(guest.into_iter().zip(native).all(|(a, b)| (a - b).abs() <= 0.08 * (1.0 + b.abs())), "{guest:?} != native {native:?}");
    }
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
fn fluid_fire_step_matches_native_packed_fields() {
    let Some(state) = shared_device() else { return };
    let cells = 4 * 8 * 4;
    let volumes = bytes(&[0u32, 4, 8, 4]);
    let sources = lights()[..32].to_vec();
    let mut expected_atlas = vec![0; cells * 13 * 4];
    let mut expected_lights = sources.clone();
    let mut expected_emission = vec![0; 4];
    let step = |phase: u32| bytes(&[cells as u32, 1, phase, 0, (1.0f32 / 60.0).to_bits(), 0.5f32.to_bits(), 0, 0]);
    let force = native(state, include_bytes!("../../../../demos/shaders/fire_force.comp.spv"), &[expected_atlas, volumes.clone(), sources.clone(), expected_lights, expected_emission, step(0)], 5, [1, 1, 1]);
    expected_atlas = force[0].clone(); expected_lights = force[3].clone(); expected_emission = force[4].clone();
    for code in [include_bytes!("../../../../demos/shaders/fire_curl.comp.spv").as_slice(), include_bytes!("../../../../demos/shaders/fire_confine.comp.spv"), include_bytes!("../../../../demos/shaders/fire_div.comp.spv")] { expected_atlas = native(state, code, &[expected_atlas, volumes.clone(), step(0)], 2, [1, 1, 1])[0].clone(); }
    for phase in 0..8 { expected_atlas = native(state, include_bytes!("../../../../demos/shaders/fire_jacobi.comp.spv"), &[expected_atlas, volumes.clone(), step(phase)], 2, [1, 1, 1])[0].clone(); }
    expected_atlas = native(state, include_bytes!("../../../../demos/shaders/fire_project.comp.spv"), &[expected_atlas, volumes.clone(), step(0)], 2, [1, 1, 1])[0].clone();
    let advected = native(state, include_bytes!("../../../../demos/shaders/fire_advect.comp.spv"), &[expected_atlas, volumes.clone(), expected_emission, step(0)], 3, [1, 1, 1]);
    expected_atlas = advected[0].clone(); expected_emission = advected[2].clone();
    let program = BASE + 0x8_0000; let data = BASE + 0x10_0000;
    let (atlas_at, volumes_at, sources_at, lights_at, emission_at, uniforms_at) = (data, data + 0x20000, data + 0x21000, data + 0x22000, data + 0x23000, data + 0x24000);
    let shaders = [include_bytes!("../../../tests/fixtures/driver-shaders/fire_force.comp.bin").as_slice(), include_bytes!("../../../tests/fixtures/driver-shaders/fire_curl.comp.bin"), include_bytes!("../../../tests/fixtures/driver-shaders/fire_confine.comp.bin"), include_bytes!("../../../tests/fixtures/driver-shaders/fire_div.comp.bin"), include_bytes!("../../../tests/fixtures/driver-shaders/fire_jacobi.comp.bin"), include_bytes!("../../../tests/fixtures/driver-shaders/fire_project.comp.bin"), include_bytes!("../../../tests/fixtures/driver-shaders/fire_advect.comp.bin")];
    let addresses: [u64; 7] = core::array::from_fn(|i| program + i as u64 * 0x8000);
    let programs = [0, 1, 2, 3, 4, 4, 4, 4, 4, 4, 4, 4, 5, 6];
    let passes: [Pass; 14] = core::array::from_fn(|i| pass(i as u64, addresses[programs[i]]));
    let mut guest = gpu(state);
    for (address, code) in addresses.into_iter().zip(shaders) { guest.write(address, &shader(code)).unwrap(); }
    for (address, data) in [(atlas_at, vec![0; cells * 13 * 4]), (volumes_at, volumes.clone()), (sources_at, sources.clone()), (lights_at, sources), (emission_at, vec![0; 4])] { guest.write(address, &data).unwrap(); }
    for slot in 0..14 { guest.write(uniforms_at + slot as u64 * 0x100, &step(if (4..12).contains(&slot) { slot as u32 - 4 } else { 0 })).unwrap(); }
    let uniform = |slot| Buffer { address: uniforms_at + slot as u64 * 0x100, size: 32 };
    let atlas = Buffer { address: atlas_at, size: (cells * 13 * 4) as u32 }; let volume = Buffer { address: volumes_at, size: 16 };
    install(&mut guest, passes[0], &compute::linear(passes[0], [atlas, volume, Buffer { address: sources_at, size: 32 }, Buffer { address: lights_at, size: 32 }, Buffer { address: emission_at, size: 4 }], uniform(0), cells as u32, 33, 1));
    for (slot, registers) in [(1, 20), (2, 24), (3, 19)] { install(&mut guest, passes[slot], &compute::linear(passes[slot], [atlas, volume], uniform(slot), cells as u32, registers, slot as u32 + 1)); }
    for slot in 4..12 { install(&mut guest, passes[slot], &compute::linear(passes[slot], [atlas, volume], uniform(slot), cells as u32, 20, slot as u32 + 1)); }
    install(&mut guest, passes[12], &compute::linear(passes[12], [atlas, volume], uniform(12), cells as u32, 20, 13));
    install(&mut guest, passes[13], &compute::linear(passes[13], [atlas, volume, Buffer { address: emission_at, size: 4 }], uniform(13), cells as u32, 46, 14));
    submit(&mut guest, &passes);
    let mut actual = vec![0; expected_atlas.len()]; guest.read(atlas_at, &mut actual).unwrap();
    for (i, (a, b)) in actual.chunks_exact(2).map(|v| u16::from_le_bytes(v.try_into().unwrap())).zip(expected_atlas.chunks_exact(2).map(|v| u16::from_le_bytes(v.try_into().unwrap()))).enumerate() { assert!(a.abs_diff(b) <= 2, "half {i}: {a} != {b}"); }
    for (address, expected) in [(lights_at, expected_lights), (emission_at, expected_emission)] { let mut actual = vec![0; expected.len()]; guest.read(address, &mut actual).unwrap(); assert_eq!(actual, expected); }
}

#[test]
fn smoke_volume_matches_native_temporal_resolve() {
    let Some(state) = shared_device() else { return };
    let cells = 11usize; let volumes = bytes(&(0..11).flat_map(|i| [i, 1, 1, 1]).collect::<Vec<u32>>());
    let mut source_values = Vec::new();
    for i in 0..11 { let p = if i == 10 { [0.0, 0.25, -4.0] } else { [100.0 + i as f32, 100.0, 100.0] }; source_values.extend([p[0], p[1], p[2], 0.58, 6.0, 2.5, 0.7, 0.0]); }
    let sources = floats(&source_values);
    let mut atlas_words = vec![0u32; cells * 13]; atlas_words[4 * cells + 10 * 2] = 0x4400; atlas_words[4 * cells + 10 * 2 + 1] = 0x3400;
    let atlas = bytes(&atlas_words); let step = bytes(&[cells as u32, 11, 0, 0, (1.0f32 / 60.0).to_bits(), 0.25f32.to_bits(), 0, 0]);
    let mut frame_values = [0.0f32; 80]; for base in [0, 16, 64] { for i in 0..4 { frame_values[base + i * 5] = 1.0; } } frame_values[32..36].copy_from_slice(&[0.0, 0.5, -2.0, 0.25]); frame_values[36] = 1.0; frame_values[41] = 1.0; frame_values[52..56].copy_from_slice(&[8.0, 8.0, 0.0, 0.0]); let frame = floats(&frame_values);
    let hdr = bytes(&[0u32, 0x4900_0000]).repeat(64); let blackbody = floats(&(0..256).flat_map(|_| [1.0, 0.3, 0.02, 1.0]).collect::<Vec<_>>()); let current = vec![0; 4 * 4 * 16]; let history = current.clone(); let output = vec![0; 8 * 8 * 8];
    let lit = native(state, include_bytes!("../../../../demos/shaders/smoke_light.comp.spv"), &[atlas, volumes.clone(), sources.clone(), step.clone()], 3, [1, 1, 1]);
    let marched = native(state, include_bytes!("../../../../demos/shaders/volume_march.comp.spv"), &[lit[0].clone(), volumes.clone(), sources.clone(), blackbody.clone(), hdr.clone(), current, frame.clone()], 6, [1, 1, 1]);
    let resolved = native(state, include_bytes!("../../../../demos/shaders/volume_resolve.comp.spv"), &[marched[5].clone(), history.clone(), history, hdr.clone(), output, frame.clone()], 5, [1, 1, 1]);
    let program = BASE + 0x8_0000; let data = BASE + 0x10_0000; let addresses = [program, program + 0x8000, program + 0x10000]; let passes = [pass(0, addresses[0]), pass(1, addresses[1]), pass(2, addresses[2])];
    let buffers = [data, data + 0x1000, data + 0x2000, data + 0x3000, data + 0x4000, data + 0x8000, data + 0x9000, data + 0xa000, data + 0xb000, data + 0xc000];
    let mut guest = gpu(state); for (address, code) in addresses.into_iter().zip([include_bytes!("../../../tests/fixtures/driver-shaders/smoke_light.comp.bin").as_slice(), include_bytes!("../../../tests/fixtures/driver-shaders/volume_march.comp.bin"), include_bytes!("../../../tests/fixtures/driver-shaders/volume_resolve.comp.bin")]) { guest.write(address, &shader(code)).unwrap(); }
    for (address, bytes) in buffers.into_iter().zip([lit[0].clone(), volumes, sources, blackbody, hdr, vec![0; 4 * 4 * 16], vec![0; 4 * 4 * 16], vec![0; 4 * 4 * 16], vec![0; 8 * 8 * 8], frame]) { guest.write(address, &bytes).unwrap(); } guest.write(data + 0xd000, &step).unwrap();
    let b = |i| Buffer { address: buffers[i], size: [lit[0].len(), lit[1].len(), lit[2].len(), 256 * 16, 64 * 8, 4 * 4 * 16, 4 * 4 * 16, 4 * 4 * 16, 8 * 8 * 8, 320][i] as u32 };
    install(&mut guest, passes[0], &compute::linear(passes[0], [b(0), b(1), b(2)], Buffer { address: data + 0xd000, size: 32 }, cells as u32, 35, 1));
    install(&mut guest, passes[1], &compute::linear(passes[1], [b(0), b(1), b(2), b(3), b(4), b(5)], b(9), 16, 62, 2));
    install(&mut guest, passes[2], &compute::linear(passes[2], [b(5), b(6), b(7), b(4), b(8)], b(9), 64, 38, 3)); submit(&mut guest, &passes);
    for (output, (address, expected)) in [(buffers[5], &marched[5]), (buffers[7], &resolved[2]), (buffers[8], &resolved[4])].into_iter().enumerate() { let mut actual = vec![0; expected.len()]; guest.read(address, &mut actual).unwrap(); for (i, (a, b)) in actual.chunks_exact(2).map(|v| u16::from_le_bytes(v.try_into().unwrap())).zip(expected.chunks_exact(2).map(|v| u16::from_le_bytes(v.try_into().unwrap()))).enumerate() { assert!(a.abs_diff(b) <= 2, "output {output} half {i}: {a} != {b}"); } }
}

#[test]
fn particles_match_native_emitter_order_and_48_byte_stride() {
    let Some(state) = shared_device() else { return };
    let lights = lights();
    let uniforms = floats(&[0.75, 16.0, 0.0, 0.0]);
    let output = vec![0; 2 * 16 * 48];
    let volumes = bytes(&[0u32, 4, 4, 4, 64, 8, 8, 8]);
    let atlas = vec![0; 576 * 13 * 4];
    let expected = native(
        state,
        include_bytes!("../../../../demos/shaders/dungeon_particles_f9.comp.spv"),
        &[lights.clone(), output.clone(), atlas.clone(), volumes.clone(), uniforms.clone()],
        4,
        [1, 1, 1],
    );
    let (lights_at, output_at, atlas_at, volumes_at, uniforms_at) = (DATA, DATA + 0x1000, DATA + 0x3000, DATA + 0xc000, DATA + 0xd000);
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
        (atlas_at, atlas),
        (volumes_at, volumes),
        (uniforms_at, uniforms),
    ] {
        guest.write(address, &data).unwrap();
    }
    let encoded = compute::linear(
        pass,
        [buffer(lights_at, &expected[0]), buffer(output_at, &expected[1]), buffer(atlas_at, &expected[2]), buffer(volumes_at, &expected[3])],
        buffer(uniforms_at, &expected[4]),
        32,
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
    let volume = bytes(&[0u32, 0x3c00_0000]).repeat((SIZE[0] * SIZE[1]) as usize);
    let expected = native(
        state,
        include_bytes!("../../../../demos/shaders/dungeon_post_f9.comp.spv"),
        &[
            hdr.clone(),
            output.clone(),
            frame.clone(),
            lights.clone(),
            fogged[1].clone(),
            fogged[4].clone(),
            volume.clone(),
        ],
        2,
        [1, 1, 1],
    );
    let (hdr_at, output_at, frame_at, lights_at, fog_at, bloom_at, volume_at) = (
        DATA,
        DATA + 0x1000,
        DATA + 0x2000,
        DATA + 0x3000,
        DATA + 0x4000,
        DATA + 0x5000,
        DATA + 0x6000,
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
        (volume_at, volume),
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
    let post = compute::post_volume(
        passes[1],
        [
            buffer(hdr_at, &expected[0]),
            buffer(output_at, &expected[1]),
            buffer(frame_at, &expected[2]),
            buffer(lights_at, &expected[3]),
            buffer(fog_at, &fogged[1]),
            buffer(bloom_at, &fogged[4]),
            buffer(volume_at, &expected[6]),
        ],
        SIZE,
        70,
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
