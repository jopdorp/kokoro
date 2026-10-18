#![cfg(test)]

use super::engine::{bytes, entry};
use super::harness::shared_device;
use crate::gpu::compute::{ComputePipeline, HostBuffer};
use crate::gpu::engine::Gpu;
use crate::gpu::VkState;
use crate::tests::gpu_test::inst::bra;
use ash::vk::DescriptorType;
use scene::dungeon::compute::{self, Buffer, Encoded, Pass};

const BASE: u64 = 0x3_0000_0000;
const ARENA: u64 = 0x20_0000;
const PUSH: u64 = BASE;
const PROGRAM: u64 = BASE + 0x1_0000;
const QMD: u64 = BASE + 0x2_0000;
const CBUF: u64 = BASE + 0x2_1000;
const RELEASE: u64 = BASE + 0x2_2000;
const DATA: u64 = BASE + 0x4_0000;
const SIZE: [u32; 2] = [8, 8];

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
