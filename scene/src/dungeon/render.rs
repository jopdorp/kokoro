//! Dungeon graphics descriptor tables and C797 pass encoding.

use crate::{inc, particles::handle, split, threed::*};

const DRIVER_BANK: u32 = 0x1d;
const TRIANGLES: u32 = 4;
const HDR: u32 = 0xca;
const PITCH: u32 = 0x1000;
const ZF32: u32 = 0x0a;
const LEQUAL: u32 = 0x203;
const FUNC_ADD: u32 = 0x8006;
const ZERO: u32 = 0x4000;
const ONE: u32 = 0x4001;
const ONE_MINUS_SRC_ALPHA: u32 = 0x4303;

#[derive(Clone, Copy)]
pub struct Buffer {
    pub address: u64,
    pub size: u32,
}

#[derive(Clone, Copy)]
pub struct Target {
    pub colour: u64,
    pub depth: u64,
    pub size: [u32; 2],
}

#[derive(Clone, Copy)]
pub struct Programs {
    pub dungeon_vertex: u64,
    pub dungeon_fragment: u64,
    pub cones_vertex: u64,
    pub sprite_vertex: u64,
    pub sprite_fragment: u64,
}

#[derive(Clone, Copy)]
pub struct RayPrograms {
    pub dungeon_vertex: u64,
    pub dungeon_fragment: u64,
    pub cones_vertex: u64,
}

#[derive(Clone, Copy)]
pub struct Tables {
    pub descriptors: u64,
    pub cone_descriptors: u64,
    pub vertex_constants: u64,
    pub fragment_constants: u64,
    pub draw_parameters: u64,
    pub textures: u64,
    pub samplers: u64,
}

#[derive(Clone, Copy)]
pub struct Mesh {
    pub vertices: Buffer,
    pub indices: u64,
}

#[derive(Clone, Copy)]
pub struct RayGeometry {
    pub vertices: Buffer,
    pub indices: Buffer,
    pub tlas: u64,
}

#[derive(Clone, Copy)]
pub struct DungeonResources {
    pub frame: Buffer,
    pub lights: Buffer,
    pub reflection: Buffer,
    pub refraction: Buffer,
    pub geometry: Buffer,
    pub shadows: Buffer,
    pub rooms: Buffer,
    pub vertex_data: Buffer,
    pub textures: [u32; 9],
}

#[derive(Clone, Copy)]
pub struct SpriteResources {
    pub frame: Buffer,
    pub lights: Buffer,
    pub depth: Buffer,
    pub particles: Buffer,
    pub texture: u32,
}

fn ssbo(table: &mut [u32], word: usize, buffer: Buffer) {
    table[word..word + 3].copy_from_slice(&[
        buffer.address as u32,
        (buffer.address >> 32) as u32,
        buffer.size,
    ]);
}

pub fn dungeon_ray_descriptors(resources: DungeonResources, tlas: u64) -> [u32; 44] {
    let mut table = [0; 44];
    table[..9].copy_from_slice(&resources.textures);
    table[10..12].copy_from_slice(&handle(resources.frame.address, resources.frame.size));
    ssbo(&mut table, 12, resources.lights);
    table[16..18].copy_from_slice(&[tlas as u32, (tlas >> 32) as u32]);
    for (word, buffer) in [
        (20, resources.reflection),
        (24, resources.refraction),
        (28, resources.geometry),
        (32, resources.shadows),
        (36, resources.rooms),
        (40, resources.vertex_data),
    ] {
        ssbo(&mut table, word, buffer);
    }
    table
}

pub fn acceleration_structure(geometry: RayGeometry, words: &mut impl Extend<u32>) {
    crate::pbr::acceleration_structure_registry(&[crate::pbr::AccelerationStructureGeometry {
        vertices: geometry.vertices.address,
        stride: 48,
        vertex_count: geometry.vertices.size / 48,
        indices: geometry.indices.address,
        index_count: geometry.indices.size / 4,
        opaque: true,
        transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]],
    }], words);
}

pub fn dungeon_descriptors(resources: DungeonResources) -> [u32; 40] {
    let mut table = [0; 40];
    table[..9].copy_from_slice(&resources.textures);
    table[10..12].copy_from_slice(&handle(resources.frame.address, resources.frame.size));
    for (word, buffer) in [
        (12, resources.lights),
        (16, resources.reflection),
        (20, resources.refraction),
        (24, resources.geometry),
        (28, resources.shadows),
        (32, resources.rooms),
        (36, resources.vertex_data),
    ] {
        ssbo(&mut table, word, buffer);
    }
    table
}

pub fn sprite_descriptors(resources: SpriteResources) -> [u32; 16] {
    let mut table = [0; 16];
    table[0] = resources.texture;
    table[2..4].copy_from_slice(&handle(resources.frame.address, resources.frame.size));
    for (word, buffer) in [
        (4, resources.lights),
        (8, resources.depth),
        (12, resources.particles),
    ] {
        ssbo(&mut table, word, buffer);
    }
    table
}

pub const fn draw_parameters() -> [u32; 2] {
    [0; 2]
}

pub fn cone_constants() -> [u32; 12] {
    [0i32, 0, 1, 0, 0, 1, 1, 0, 1, 1, 0, 1].map(|v| v as u32)
}

pub fn sprite_constants() -> [u32; 12] {
    [
        -1.0f32, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0,
    ]
    .map(f32::to_bits)
}

pub fn fragment_constants() -> [u32; 48] {
    let mut constants = [0; 48];
    let rights = [
        [0.0f32, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
    ];
    let ups = [
        [0.0f32, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    for (base, vectors) in [(0, rights), (24, ups)] {
        for (i, vector) in vectors.into_iter().enumerate() {
            constants[base + i * 4..base + i * 4 + 3].copy_from_slice(&vector.map(f32::to_bits));
        }
    }
    constants
}

fn bind(group: usize, bank: u32, buffer: Buffer, words: &mut impl Extend<u32>) {
    let [hi, lo] = split(buffer.address);
    inc(
        SET_CONSTANT_BUFFER_SELECTOR_A,
        &[buffer.size, hi, lo],
        words,
    );
    inc(
        BIND_GROUP_CONSTANT_BUFFER + 32 * group,
        &[bank << 4 | 1],
        words,
    );
}

fn program(
    pipeline: usize,
    registers: u32,
    group: usize,
    address: u64,
    words: &mut impl Extend<u32>,
) {
    let [hi, lo] = split(address);
    inc(
        SET_PIPELINE_SHADER + pipeline * 64,
        &[if pipeline == 1 { 0x11 } else { 0x51 }],
        words,
    );
    inc(
        SET_PIPELINE_SHADER + pipeline * 64 + 0xc,
        &[registers, group as u32, hi, lo],
        words,
    );
}

fn target(target: Target, tables: Tables, words: &mut impl Extend<u32>) {
    let [colour_hi, colour_lo] = split(target.colour);
    let [depth_hi, depth_lo] = split(target.depth);
    let [texture_hi, texture_lo] = split(tables.textures);
    let [sampler_hi, sampler_lo] = split(tables.samplers);
    let [width, height] = target.size;
    let (x, y) = (width as f32 / 2.0, height as f32 / 2.0);
    inc(crate::host::SET_OBJECT, &[u32::from(CLASS_ID)], words);
    inc(
        SET_COLOR_TARGET_A,
        &[colour_hi, colour_lo, width * 8, height, HDR, PITCH],
        words,
    );
    inc(SET_ZT_A, &[depth_hi, depth_lo, ZF32], words);
    inc(SET_ZT_SELECT, &[1], words);
    inc(
        SET_VIEWPORT_SCALE_X,
        &[x, y, 1.0, x, y, 0.0].map(f32::to_bits),
        words,
    );
    inc(
        SET_VIEWPORT_CLIP_HORIZONTAL,
        &[width << 16, height << 16, 0, 1.0f32.to_bits()],
        words,
    );
    inc(SET_SCISSOR_ENABLE, &[1, width << 16, height << 16], words);
    inc(SET_CT_WRITE, &[0x1111], words);
    inc(SET_DEPTH_TEST, &[1], words);
    inc(SET_DEPTH_FUNC, &[LEQUAL], words);
    inc(SET_TEX_HEADER_POOL_A, &[texture_hi, texture_lo, 8], words);
    inc(SET_TEX_SAMPLER_POOL_A, &[sampler_hi, sampler_lo, 8], words);
}

fn descriptor_banks(tables: Tables, vertex_constants: u32, descriptor_size: u32, words: &mut impl Extend<u32>) {
    if vertex_constants != 0 {
        bind(
            0,
            0,
            Buffer {
                address: tables.draw_parameters,
                size: 8,
            },
            words,
        );
        bind(
            0,
            1,
            Buffer {
                address: tables.vertex_constants,
                size: vertex_constants,
            },
            words,
        );
    }
    bind(
        0,
        DRIVER_BANK,
        Buffer {
            address: tables.descriptors,
            size: descriptor_size,
        },
        words,
    );
    bind(
        4,
        1,
        Buffer {
            address: tables.fragment_constants,
            size: 192,
        },
        words,
    );
    bind(
        4,
        DRIVER_BANK,
        Buffer {
            address: tables.descriptors,
            size: descriptor_size,
        },
        words,
    );
}

fn mesh_state(
    target_at: Target,
    programs: Programs,
    ray: Option<RayPrograms>,
    tables: Tables,
    mesh: Mesh,
    words: &mut impl Extend<u32>,
) {
    const FLOAT: u32 = 7 << 27;
    target(target_at, tables, words);
    inc(
        SET_VERTEX_STREAM_SIZE_A,
        &split(u64::from(mesh.vertices.size)),
        words,
    );
    let [vertex_hi, vertex_lo] = split(mesh.vertices.address);
    inc(
        SET_VERTEX_STREAM_A_FORMAT,
        &[0x1000 | 48, vertex_hi, vertex_lo],
        words,
    );
    inc(
        SET_VERTEX_ATTRIBUTE_A,
        &[
            0x02 << 21 | FLOAT,
            12 << 7 | 0x02 << 21 | FLOAT,
            24 << 7 | 0x04 << 21 | FLOAT,
            32 << 7 | 0x02 << 21 | FLOAT,
            44 << 7 | 0x12 << 21 | FLOAT,
        ],
        words,
    );
    let [index_hi, index_lo] = split(mesh.indices);
    inc(SET_INDEX_BUFFER_A, &[index_hi, index_lo], words);
    inc(SET_INDEX_BUFFER_E, &[2], words);
    let (vertex, fragment, registers, descriptor_size) = ray.map_or((programs.dungeon_vertex, programs.dungeon_fragment, 46, 160), |ray| (ray.dungeon_vertex, ray.dungeon_fragment, 70, 176));
    program(1, 18, 0, vertex, words);
    program(5, registers, 4, fragment, words);
    descriptor_banks(tables, 0, descriptor_size, words);
    inc(SET_BLEND, &[0], words);
    inc(SET_DEPTH_WRITE, &[1], words);
}

fn cones_state(programs: Programs, ray: Option<RayPrograms>, tables: Tables, words: &mut impl Extend<u32>) {
    let (vertex, fragment, registers, descriptor_size) = ray.map_or((programs.cones_vertex, programs.dungeon_fragment, 46, 160), |ray| (ray.cones_vertex, ray.dungeon_fragment, 70, 176));
    program(1, 23, 0, vertex, words);
    program(5, registers, 4, fragment, words);
    descriptor_banks(Tables { descriptors: tables.cone_descriptors, ..tables }, 48, descriptor_size, words);
    inc(SET_BLEND, &[0], words);
    inc(SET_DEPTH_WRITE, &[1], words);
}

fn sprite_state(
    target_at: Target,
    programs: Programs,
    tables: Tables,
    words: &mut impl Extend<u32>,
) {
    target(target_at, tables, words);
    program(1, 30, 0, programs.sprite_vertex, words);
    program(5, 38, 4, programs.sprite_fragment, words);
    bind(
        0,
        0,
        Buffer {
            address: tables.draw_parameters,
            size: 8,
        },
        words,
    );
    bind(
        0,
        1,
        Buffer {
            address: tables.vertex_constants,
            size: 48,
        },
        words,
    );
    bind(
        0,
        DRIVER_BANK,
        Buffer {
            address: tables.descriptors,
            size: 64,
        },
        words,
    );
    bind(
        4,
        DRIVER_BANK,
        Buffer {
            address: tables.descriptors,
            size: 64,
        },
        words,
    );
    inc(
        SET_BLEND_SEPARATE_FOR_ALPHA,
        &[1, FUNC_ADD, ONE, ONE_MINUS_SRC_ALPHA, FUNC_ADD, ZERO],
        words,
    );
    inc(SET_BLEND_ALPHA_DEST_COEFF, &[ONE], words);
    inc(SET_BLEND, &[1], words);
    inc(SET_DEPTH_WRITE, &[0], words);
}

fn indexed(first: u32, count: u32, words: &mut impl Extend<u32>) {
    inc(BEGIN, &[TRIANGLES], words);
    inc(SET_INDEX_BUFFER_F, &[first], words);
    inc(DRAW_INDEX_BUFFER, &[count], words);
    inc(END, &[0], words);
}

fn instances(vertices: u32, count: u32, words: &mut impl Extend<u32>) {
    if count == 0 {
        return;
    }
    let draw = TRIANGLES << 28 | vertices << 16;
    inc(DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_FIRST, &[draw], words);
    let mut remaining = count - 1;
    while remaining != 0 {
        let packet = remaining.min(8191);
        words.extend([crate::header(
            3,
            DRAW_VERTEX_ARRAY_BEGIN_END_INSTANCE_SUBSEQUENT,
            packet,
        )]);
        words.extend(core::iter::repeat_n(draw, packet as usize));
        remaining -= packet;
    }
}

fn pair(
    target_at: Target,
    programs: Programs,
    ray: Option<RayPrograms>,
    tables: Tables,
    mesh: Mesh,
    draws: (u32, [u32; 2]),
    clear: [f32; 4],
    words: &mut impl Extend<u32>,
) {
    mesh_state(target_at, programs, ray, tables, mesh, words);
    inc(SET_COLOR_CLEAR_VALUE, &clear.map(f32::to_bits), words);
    inc(SET_Z_CLEAR_VALUE, &[1.0f32.to_bits()], words);
    inc(CLEAR_SURFACE, &[0x3d], words);
    indexed(0, draws.0, words);
    cones_state(programs, ray, tables, words);
    instances(draws.1[0], draws.1[1], words);
}

pub fn shadow(
    target: Target,
    programs: Programs,
    tables: Tables,
    mesh: Mesh,
    indices: u32,
    cones: [u32; 2],
    words: &mut impl Extend<u32>,
) {
    pair(
        target,
        programs,
        None,
        tables,
        mesh,
        (indices, cones),
        [30.0; 4],
        words,
    );
}

pub fn reflection(
    target: Target,
    programs: Programs,
    tables: Tables,
    mesh: Mesh,
    indices: u32,
    cones: [u32; 2],
    words: &mut impl Extend<u32>,
) {
    pair(
        target,
        programs,
        None,
        tables,
        mesh,
        (indices, cones),
        [0.008, 0.012, 0.018, 40.0],
        words,
    );
}

pub fn opaque(
    target: Target,
    programs: Programs,
    tables: Tables,
    mesh: Mesh,
    indices: u32,
    cones: [u32; 2],
    words: &mut impl Extend<u32>,
) {
    pair(
        target,
        programs,
        None,
        tables,
        mesh,
        (indices, cones),
        [0.008, 0.012, 0.018, 40.0],
        words,
    );
}

pub fn ray_opaque(
    target: Target,
    programs: Programs,
    ray: RayPrograms,
    tables: Tables,
    mesh: Mesh,
    indices: u32,
    cones: [u32; 2],
    words: &mut impl Extend<u32>,
) {
    pair(target, programs, Some(ray), tables, mesh, (indices, cones), [0.008, 0.012, 0.018, 40.0], words);
}

pub fn water(
    target: Target,
    programs: Programs,
    tables: Tables,
    mesh: Mesh,
    first: u32,
    count: u32,
    words: &mut impl Extend<u32>,
) {
    mesh_state(target, programs, None, tables, mesh, words);
    inc(SET_DEPTH_WRITE, &[0], words);
    indexed(first, count, words);
}

pub fn ray_water(
    target: Target,
    programs: Programs,
    ray: RayPrograms,
    tables: Tables,
    mesh: Mesh,
    first: u32,
    count: u32,
    words: &mut impl Extend<u32>,
) {
    mesh_state(target, programs, Some(ray), tables, mesh, words);
    inc(SET_DEPTH_WRITE, &[0], words);
    indexed(first, count, words);
}

pub fn sprites(
    target: Target,
    programs: Programs,
    tables: Tables,
    count: u32,
    words: &mut impl Extend<u32>,
) {
    sprite_state(target, programs, tables, words);
    instances(6, count, words);
}
