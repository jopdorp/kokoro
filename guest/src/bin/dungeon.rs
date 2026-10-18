#![no_std]
#![no_main]

use guest::{audren, cmif::{Buffer as IpcBuffer, Request}, display, hid, nv, svc};
use scene::{Words, dungeon::{compute, render, sound}};

include!(concat!(env!("OUT_DIR"), "/dungeon-runtime.rs"));

core::arch::global_asm!(
    ".section .text._start, \"ax\"",
    ".global _start",
    "_start:",
    "b main",
    ".word 0, 0, 0",
    ".ascii \"NRO0\"",
    ".word 0, __bss - _start, 0",
    ".word 0, __ro - _start, __ro - _start, __ro_size, __data - _start, __data_size",
    ".word __bss_size, 0",
    ".space 0x40",
);

const BASE: u64 = 0x3_0000_0000;
const PUSH: u64 = BASE;
const DUNGEON_VERTEX: u64 = BASE + 0x1_0000;
const DUNGEON_FRAGMENT: u64 = BASE + 0x2_0000;
const CONES_VERTEX: u64 = BASE + 0x5_0000;
const SPRITE_VERTEX: u64 = BASE + 0x6_0000;
const SPRITE_FRAGMENT: u64 = BASE + 0x7_0000;
const WATER_PROGRAM: u64 = BASE + 0x9_0000;
const PARTICLES_PROGRAM: u64 = BASE + 0xa_0000;
const FOG_PROGRAM: u64 = BASE + 0xc_0000;
const POST_PROGRAM: u64 = BASE + 0xd_0000;
const QMD: u64 = BASE + 0xe_0000;
const CBUF: u64 = BASE + 0xe_1000;
const RELEASE: u64 = BASE + 0xe_2000;
const DESCRIPTORS: u64 = BASE + 0xe_4000;
const VERTEX_CONSTANTS: u64 = BASE + 0xe_5000;
const FRAGMENT_CONSTANTS: u64 = BASE + 0xe_6000;
const DRAW_PARAMETERS: u64 = BASE + 0xe_7000;
const TIC: u64 = BASE + 0xe_8000;
const TSC: u64 = BASE + 0xe_9000;
const TLAS: u64 = BASE + 0xe_a000;
const DATA: u64 = BASE + 0x10_0000;
const MEMORY: u64 = 0x4010_0000;
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const ZERO_PCM: [i16; 1] = [0];
const AUDIO_NAMES: [&str; 34] = ["torch", "fire", "river", "wind", "heart", "creak", "room", "drip0", "drip1", "drip2", "stone0", "stone1", "stone2", "stone3", "stone4", "stone5", "wet0", "wet1", "wet2", "wet3", "wet4", "wet5", "gravel0", "gravel1", "gravel2", "gravel3", "gravel4", "gravel5", "wood0", "wood1", "wood2", "wood3", "wood4", "wood5"];
const TEXTURE_NAMES: [&str; 9] = ["PavingStones142-colour", "PavingStones142-normal", "PavingStones142-surface", "Rock035-colour", "Rock035-normal", "Rock035-surface", "Metal055C-colour", "Metal055C-normal", "Metal055C-surface"];

const fn align(value: u64) -> u64 { (value + 0xffff) & !0xffff }

fn has_option(config: *const [u64; 3], option: &[u8]) -> bool {
    let argv = (0..).map(|i| unsafe { *config.add(i) }).take_while(|entry| entry[0] as u32 != 0).find(|entry| entry[0] as u32 == 5).map(|entry| unsafe { core::ffi::CStr::from_ptr(entry[2] as *const _) }.to_bytes()).unwrap_or_default();
    argv.windows(option.len()).any(|word| word == option)
}

fn service(name: [u8; 8]) -> svc::Result<u32> {
    let sm = svc::connect_to_named_port(c"sm:")?;
    svc::request(sm, &Request { command: 0, send_pid: true, data: &[0, 0], ..Default::default() })?;
    let words = [u32::from_le_bytes(name[..4].try_into().unwrap()), u32::from_le_bytes(name[4..].try_into().unwrap())];
    Ok(svc::request(sm, &Request { command: 1, data: &words, ..Default::default() })?.move_handles[0])
}

fn filesystem() -> svc::Result<u32> {
    let fsp = service(*b"fsp-srv\0")?;
    svc::request(fsp, &Request { command: 1, send_pid: true, data: &[0, 0], ..Default::default() })?;
    Ok(svc::request(fsp, &Request { command: 18, ..Default::default() })?.move_handles[0])
}

fn load_file(fs: u32, directory: &str, name: &str, extension: &str, address: u64, bytes: usize) -> svc::Result<()> {
    let mut path = [0u8; 0x301];
    let mut at = 0;
    for part in ["/", directory, "/", name, extension] {
        path[at..at + part.len()].copy_from_slice(part.as_bytes());
        at += part.len();
    }
    let file = svc::request(fs, &Request { command: 8, pointer: &[IpcBuffer { address: path.as_ptr() as u64, size: path.len() as u64 }], data: &[1, 0], ..Default::default() })?.move_handles[0];
    let reply = svc::request(file, &Request { command: 0, receive: &[IpcBuffer { address, size: bytes as u64 }], data: &[0, 0, 0, 0, bytes as u32, (bytes as u64 >> 32) as u32], ..Default::default() })?;
    let read = u64::from(reply.data[0]) | u64::from(reply.data[1]) << 32;
    svc::close_handle(file)?;
    if read == bytes as u64 { Ok(()) } else { Err(u32::MAX) }
}

fn cpu(address: u64) -> u64 { if address >= BASE { MEMORY + address - BASE } else { address } }

fn copy<T: Copy + 'static>(address: u64, values: &[T]) {
    nv::guest_slice(cpu(address), values.len()).copy_from_slice(values);
}

fn shader(address: u64, bytes: &[u8]) {
    nv::guest_slice(cpu(address), bytes.len()).copy_from_slice(bytes);
    copy(address + bytes.len() as u64, &scene::sass::bra(-16).to_le_bytes());
}

fn buffer(address: u64, size: usize) -> render::Buffer { render::Buffer { address, size: size as u32 } }
fn compute_buffer(address: u64, size: usize) -> compute::Buffer { compute::Buffer { address, size: size as u32 } }

fn compute_pass(slot: u64, program: u64) -> compute::Pass {
    compute::Pass { program, qmd: QMD + slot * 0x100, cbuf: CBUF + slot * 0x100, release: RELEASE + slot * 4 }
}

fn install<const N: usize>(encoded: &compute::Encoded<N>, pass: compute::Pass) {
    copy(pass.qmd, &encoded.qmd);
    copy(pass.cbuf, &encoded.cbuf);
}

struct Layout {
    vertices: u64,
    vertex_bytes: usize,
    indices: u64,
    opaque_indices: u32,
    water_indices: u32,
    lights: u64,
    rooms: u64,
    cones: u64,
    geometry: u64,
    textures: [u64; 9],
    target: u64,
    target_depth: u64,
    reflection: u64,
    reflection_depth: u64,
    shadows: u64,
    water: [u64; 2],
    water_surface: u64,
    water_uniforms: u64,
    frame: u64,
    particle_time: u64,
    particles: u64,
    fog: u64,
    bloom: u64,
    output: u64,
    end: u64,
    grid: [u32; 2],
    reflection_size: [u32; 2],
}

impl Layout {
    fn new(showcase: bool) -> Self {
        let vertex_bytes = if showcase { include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-showcase-vertices.bin")).len() } else { include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-realtime-vertices.bin")).len() };
        let index_bytes = if showcase { include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-showcase-indices.bin")).len() } else { include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-realtime-indices.bin")).len() };
        let (opaque_indices, water_indices, grid, reflection_size) = if showcase { (SHOWCASE_OPAQUE_INDICES, SHOWCASE_WATER_INDICES, [256, 64], [960, 540]) } else { (REALTIME_OPAQUE_INDICES, REALTIME_WATER_INDICES, [96, 24], [480, 270]) };
        let vertices = DATA;
        let indices = align(vertices + vertex_bytes as u64);
        let lights = align(indices + index_bytes as u64);
        let rooms = align(lights + (LIGHT_COUNT * 32) as u64);
        let cones = align(rooms + include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-zones.bin")).len() as u64);
        let geometry = align(cones + include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-cones.bin")).len() as u64);
        let mut at = align(geometry + include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-geometry.bin")).len() as u64);
        let textures = core::array::from_fn(|i| { let address = at; at = align(at + TEXTURE_BYTES[i] as u64); address });
        let target = at;
        let target_depth = target + u64::from(WIDTH * HEIGHT * 8);
        let reflection = align(target_depth + u64::from(WIDTH * HEIGHT * 4));
        let reflection_depth = reflection + u64::from(reflection_size[0] * reflection_size[1] * 8);
        let shadows = align(reflection_depth + u64::from(reflection_size[0] * reflection_size[1] * 4));
        let water = [align(shadows + 10 * 6 * 256 * 256 * 8), 0];
        let water = [water[0], water[0] + u64::from(grid[0] * grid[1] * 8)];
        let water_surface = water[1] + u64::from(grid[0] * grid[1] * 8);
        let water_uniforms = align(water_surface + u64::from(grid[0] * grid[1] * 16));
        let frame = align(water_uniforms + 0x1000);
        let particle_time = frame + 0x200;
        let particles = align(particle_time + 0x10);
        let fog = align(particles + (LIGHT_COUNT * 96 * 48) as u64);
        let bloom = align(fog + u64::from(WIDTH / 4 * HEIGHT / 4 * 16));
        let output = align(bloom + u64::from(WIDTH / 4 * HEIGHT / 4 * 16));
        let end = align(output + u64::from(WIDTH * HEIGHT * 4));
        Self { vertices, vertex_bytes, indices, opaque_indices, water_indices, lights, rooms, cones, geometry, textures, target, target_depth, reflection, reflection_depth, shadows, water, water_surface, water_uniforms, frame, particle_time, particles, fog, bloom, output, end, grid, reflection_size }
    }
}

fn pose(frame: usize) -> ([f32; 3], f32, f32, f32) {
    let data = include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-poses.bin"));
    let at = frame.min(72 * 60) * 24;
    let value = |i: usize| f32::from_le_bytes(data[at + i * 4..at + i * 4 + 4].try_into().unwrap());
    ([value(0), value(1), value(2)], value(3), value(4), value(5))
}

fn frame_data(frame: usize, showcase: bool, rays: bool, layout: &Layout) {
    let data = include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-frames.bin"));
    let at = frame.min(72 * 60) * 512;
    nv::guest_slice(cpu(layout.frame), 512).copy_from_slice(&data[at..at + 512]);
    for offset in [0, 256] {
        let uniforms = nv::guest_slice::<u32>(cpu(layout.frame + offset), 64);
        uniforms[39] = (u8::from(showcase) as f32).to_bits();
        uniforms[43] = (u8::from(rays && offset != 0) as f32).to_bits();
        uniforms[60] = (layout.grid[0] as f32).to_bits();
        uniforms[61] = (layout.grid[1] as f32).to_bits();
    }
    let light_data = include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-frame-lights.bin"));
    let bytes = LIGHT_COUNT * 32;
    let at = frame.min(72 * 60) * bytes;
    nv::guest_slice(cpu(layout.lights), bytes).copy_from_slice(&light_data[at..at + bytes]);
}

fn setup_graphics(fs: u32, layout: &Layout, showcase: bool, rays: bool) -> svc::Result<()> {
    let (vertex, fragment, cones) = if rays {
        (include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon_rt.vert.bin").as_slice(), include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon_rt.frag.bin").as_slice(), include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon_cones_rt.vert.bin").as_slice())
    } else {
        (include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon.vert.bin").as_slice(), include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon.frag.bin").as_slice(), include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon_cones.vert.bin").as_slice())
    };
    for (address, bytes) in [(DUNGEON_VERTEX, vertex), (DUNGEON_FRAGMENT, fragment), (CONES_VERTEX, cones), (SPRITE_VERTEX, include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon_sprite.vert.bin")), (SPRITE_FRAGMENT, include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon_sprite.frag.bin")), (WATER_PROGRAM, include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon_water.comp.bin")), (PARTICLES_PROGRAM, include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon_particles.comp.bin")), (FOG_PROGRAM, include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon_fog.comp.bin")), (POST_PROGRAM, include_bytes!("../../../core/tests/fixtures/driver-shaders/dungeon_post.comp.bin"))] { shader(address, bytes); }
    let vertices = if showcase { include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-showcase-vertices.bin")).as_slice() } else { include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-realtime-vertices.bin")).as_slice() };
    let indices = if showcase { include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-showcase-indices.bin")).as_slice() } else { include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-realtime-indices.bin")).as_slice() };
    nv::guest_slice(cpu(layout.vertices), vertices.len()).copy_from_slice(vertices);
    nv::guest_slice(cpu(layout.indices), indices.len()).copy_from_slice(indices);
    for (address, bytes) in [(layout.rooms, include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-zones.bin")).as_slice()), (layout.cones, include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-cones.bin")).as_slice()), (layout.geometry, include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-geometry.bin")).as_slice())] { nv::guest_slice(cpu(address), bytes.len()).copy_from_slice(bytes); }
    let mut tic = [0u32; 72];
    for (i, (&address, &bytes)) in layout.textures.iter().zip(&TEXTURE_BYTES).enumerate() {
        load_file(fs, "dungeon", TEXTURE_NAMES[i], ".rgba8", cpu(address), bytes)?;
        let [hi, lo] = scene::split(address);
        tic[i * 8..][..8].copy_from_slice(&[8 | 0o2222 << 7 | 0o5432 << 19, lo, hi | 3 << 21, 1 << 3 | 11 << 28, 2047 | 1 << 23, 2047 | 1 << 31, 0, 0]);
    }
    copy(TIC, &tic);
    copy(TSC, &[7 << 20, 2 | 2 << 4 | 3 << 6, 0xf00 << 12, 0, 0, 0, 0, 0]);
    copy(DRAW_PARAMETERS, &render::draw_parameters());
    copy(FRAGMENT_CONSTANTS, &render::fragment_constants());
    if rays {
        let [vertex_hi, vertex_lo] = scene::split(layout.vertices);
        let [index_hi, index_lo] = scene::split(layout.indices);
        let mut registry = [0u32; 22];
        registry[..10].copy_from_slice(&[
            scene::pbr::ACCELERATION_STRUCTURE_MAGIC, 1, vertex_hi, vertex_lo, 48,
            (layout.vertex_bytes / 48) as u32, index_hi, index_lo, layout.opaque_indices, 1,
        ]);
        registry[10..].copy_from_slice(&[1.0f32.to_bits(), 0, 0, 0, 0, 1.0f32.to_bits(), 0, 0, 0, 0, 1.0f32.to_bits(), 0]);
        copy(TLAS, &registry);
    }
    Ok(())
}

fn render_frame(channel: &nv::Channel, layout: &Layout, showcase: bool, rays: bool, tick: u32) -> svc::Result<()> {
    frame_data(tick as usize, showcase, rays, layout);
    let programs = render::Programs { dungeon_vertex: DUNGEON_VERTEX, dungeon_fragment: DUNGEON_FRAGMENT, dungeon_fragment_registers: if rays { 70 } else { 46 }, cones_vertex: CONES_VERTEX, sprite_vertex: SPRITE_VERTEX, sprite_fragment: SPRITE_FRAGMENT };
    let mesh = render::Mesh { vertices: buffer(layout.vertices, layout.vertex_bytes), indices: layout.indices };
    let target = render::Target { colour: layout.target, depth: layout.target_depth, size: [WIDTH, HEIGHT] };
    let reflection = render::Target { colour: layout.reflection, depth: layout.reflection_depth, size: layout.reflection_size };
    let tables = |descriptors, constants| render::Tables { descriptors, vertex_constants: constants, fragment_constants: FRAGMENT_CONSTANTS, draw_parameters: DRAW_PARAMETERS, textures: TIC, samplers: TSC };
    let resources = |frame| render::DungeonResources { frame: buffer(frame, 256), lights: buffer(layout.lights, LIGHT_COUNT * 32), reflection: if rays { buffer(TLAS, 88) } else { buffer(layout.reflection, (layout.reflection_size[0] * layout.reflection_size[1] * 8) as usize) }, refraction: buffer(layout.target, (WIDTH * HEIGHT * 8) as usize), geometry: buffer(layout.geometry, include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-geometry.bin")).len()), shadows: buffer(layout.shadows, 10 * 6 * 256 * 256 * 8), rooms: buffer(layout.rooms, include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-zones.bin")).len()), vertex_data: buffer(layout.water_surface, (layout.grid[0] * layout.grid[1] * 16) as usize), textures: [0, 1, 2, 3, 4, 5, 6, 7, 8] };
    let submit = |words: &mut Words<'_>| channel.submit(PUSH, words.len);
    let mut words = Words { storage: nv::guest_slice(cpu(PUSH), 0x10000), len: 0 };
    if !rays {
        copy(DESCRIPTORS, &render::dungeon_descriptors(resources(layout.frame)));
        copy(VERTEX_CONSTANTS, &render::cone_constants());
        render::shadow(render::Target { colour: layout.shadows, depth: layout.target_depth, size: [if showcase { 256 } else { 128 }; 2] }, programs, tables(DESCRIPTORS, VERTEX_CONSTANTS), mesh, layout.opaque_indices, [24 * 12 * 6, CONE_COUNT], &mut words);
        submit(&mut words).map_err(|_| 20u32)?;
        words.len = 0;
        copy(DESCRIPTORS, &render::dungeon_descriptors(resources(layout.frame)));
        render::reflection(reflection, programs, tables(DESCRIPTORS, VERTEX_CONSTANTS), mesh, layout.opaque_indices, [24 * 12 * 6, CONE_COUNT], &mut words);
        submit(&mut words).map_err(|_| 21u32)?;
    }
    words.len = 0;
    copy(DESCRIPTORS, &render::dungeon_descriptors(resources(layout.frame + 256)));
    copy(VERTEX_CONSTANTS, &render::cone_constants());
    render::opaque(target, programs, tables(DESCRIPTORS, VERTEX_CONSTANTS), mesh, layout.opaque_indices, [24 * 12 * 6, CONE_COUNT], &mut words);
    submit(&mut words).map_err(|_| 22u32)?;
    words.len = 0;
    render::water(target, programs, tables(DESCRIPTORS, VERTEX_CONSTANTS), mesh, layout.opaque_indices, layout.water_indices, &mut words);
    submit(&mut words).map_err(|_| 24u32)?;
    words.len = 0;
    copy(DESCRIPTORS, &render::sprite_descriptors(render::SpriteResources { frame: buffer(layout.frame + 256, 256), lights: buffer(layout.lights, LIGHT_COUNT * 32), depth: buffer(layout.target, (WIDTH * HEIGHT * 8) as usize), particles: buffer(layout.particles, LIGHT_COUNT * 96 * 48), texture: 0 }));
    copy(VERTEX_CONSTANTS, &render::sprite_constants());
    render::sprites(target, programs, tables(DESCRIPTORS, VERTEX_CONSTANTS), (LIGHT_COUNT * if showcase { 96 } else { 24 }) as u32, &mut words);
    submit(&mut words).map_err(|_| 25u32)?;

    let seconds = tick as f32 / 60.0;
    copy(layout.water_uniforms, &[layout.grid[0] as f32, layout.grid[1] as f32, 1.0 / 60.0, 2.0, 1.0, 1.0, 0.0, seconds]);
    copy(layout.particle_time, &[seconds, if showcase { 96.0 } else { 24.0 }, 0.0, 0.0]);
    let passes = [compute_pass(0, WATER_PROGRAM), compute_pass(1, PARTICLES_PROGRAM), compute_pass(2, FOG_PROGRAM), compute_pass(3, POST_PROGRAM)];
    let ping = tick as usize & 1;
    install(&compute::water(passes[0], compute_buffer(layout.water[ping], (layout.grid[0] * layout.grid[1] * 8) as usize), compute_buffer(layout.water[1 - ping], (layout.grid[0] * layout.grid[1] * 8) as usize), compute_buffer(layout.water_surface, (layout.grid[0] * layout.grid[1] * 16) as usize), compute_buffer(layout.water_uniforms, 0x1000), layout.grid, tick + 1), passes[0]);
    install(&compute::particles(passes[1], compute_buffer(layout.lights, LIGHT_COUNT * 32), compute_buffer(layout.particles, LIGHT_COUNT * 96 * 48), compute_buffer(layout.particle_time, 16), (LIGHT_COUNT * if showcase { 96 } else { 24 }) as u32, tick + 1), passes[1]);
    install(&compute::fog(passes[2], [compute_buffer(layout.target, (WIDTH * HEIGHT * 8) as usize), compute_buffer(layout.fog, (WIDTH / 4 * HEIGHT / 4 * 16) as usize), compute_buffer(layout.frame + 256, 256), compute_buffer(layout.lights, LIGHT_COUNT * 32), compute_buffer(layout.bloom, (WIDTH / 4 * HEIGHT / 4 * 16) as usize)], [WIDTH, HEIGHT], tick + 1), passes[2]);
    install(&compute::post(passes[3], [compute_buffer(layout.target, (WIDTH * HEIGHT * 8) as usize), compute_buffer(layout.output, (WIDTH * HEIGHT * 4) as usize), compute_buffer(layout.frame + 256, 256), compute_buffer(layout.lights, LIGHT_COUNT * 32), compute_buffer(layout.fog, (WIDTH / 4 * HEIGHT / 4 * 16) as usize), compute_buffer(layout.bloom, (WIDTH / 4 * HEIGHT / 4 * 16) as usize)], [WIDTH, HEIGHT], tick + 1), passes[3]);
    words.len = 0;
    for pass in passes { compute::dispatch(pass, &mut words); }
    submit(&mut words).map_err(|_| 23u32)
}

fn audio(fs: u32, memory: u64) -> svc::Result<(sound::Engine<'static>, audren::Pcm<'static>)> {
    let mut files = [(0u64, 0usize); 34];
    let mut at = memory;
    for i in 0..files.len() {
        at = (at + 1) & !1;
        load_file(fs, "dungeon-audio", AUDIO_NAMES[i], ".pcm", at, AUDIO_BYTES[i])?;
        files[i] = (at, AUDIO_BYTES[i] / 2);
        at += AUDIO_BYTES[i] as u64;
    }
    let sample = |i: usize| &*nv::guest_slice::<i16>(files[i].0, files[i].1);
    let recordings = [sample(0), sample(1), sample(2), sample(3), sample(4), sample(5), &ZERO_PCM, sample(6), sample(7), &ZERO_PCM, sample(10), sample(10), sample(10)];
    let samples = sound::Samples { recordings, drips: [sample(7), sample(8), sample(9)], steps: [core::array::from_fn(|i| sample(10 + i)), core::array::from_fn(|i| sample(16 + i)), core::array::from_fn(|i| sample(22 + i)), core::array::from_fn(|i| sample(28 + i))] };
    let config = include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-voices.bin"));
    let voices = nv::guest_slice::<sound::Voice>(align(at), VOICE_COUNT);
    for (i, voice) in voices.iter_mut().enumerate() {
        let words = core::array::from_fn::<_, 7, _>(|word| u32::from_le_bytes(config[(i * 7 + word) * 4..][..4].try_into().unwrap()));
        let sample = unsafe { core::mem::transmute::<u8, sound::Sample>(words[0] as u8) };
        *voice = sound::Voice::new(sample, [words[1], words[2], words[3]].map(f32::from_bits), f32::from_bits(words[4]), f32::from_bits(words[5]), words[6] != 0);
    }
    at = align(align(at) + (VOICE_COUNT * core::mem::size_of::<sound::Voice>()) as u64);
    let histories = nv::guest_slice::<f32>(at, sound::HISTORY * VOICE_COUNT);
    histories.fill(0.0);
    at = align(at + (histories.len() * 4) as u64);
    let fdn = nv::guest_slice::<f32>(at, sound::FDN_SAMPLES * ROOM_COUNT);
    fdn.fill(0.0);
    at = align(at + (fdn.len() * 4) as u64);
    let mut host_rooms = [sound::Room::default(); ROOM_COUNT];
    let mut doorways = [sound::Doorway::default(); DOORWAY_COUNT];
    sound::decode_rooms(include_bytes!(concat!(env!("OUT_DIR"), "/dungeon-acoustic-rooms.bin")), &mut host_rooms, &mut doorways).ok_or(u32::MAX)?;
    let room_at = at;
    nv::guest_slice::<sound::Room>(room_at, ROOM_COUNT).copy_from_slice(&host_rooms);
    at = align(room_at + (ROOM_COUNT * core::mem::size_of::<sound::Room>()) as u64);
    let guest_doors = nv::guest_slice::<sound::Doorway>(at, DOORWAY_COUNT);
    guest_doors.copy_from_slice(&doorways);
    at = align(at + (DOORWAY_COUNT * core::mem::size_of::<sound::Doorway>()) as u64);
    let engine = sound::Engine::new(samples, sound::Storage { voices, histories, fdn }, sound::Environment { rooms: nv::guest_slice(room_at, ROOM_COUNT), doorways: guest_doors, bridge: sound::Box3 { min: BRIDGE[0], max: BRIDGE[1] }, reveal: sound::Box3 { min: REVEAL[0], max: REVEAL[1] }, ambient: true }, sound::Mode::Headphones).ok_or(u32::MAX)?;
    Ok((engine, audren::Pcm::new(at, true)?))
}

fn run(config: *const [u64; 3]) -> svc::Result<()> {
    let showcase = has_option(config, b"-showcase");
    let rays = has_option(config, b"-rays");
    let layout = Layout::new(showcase);
    let heap_bytes = (layout.end - BASE + 0x2000_0000).next_multiple_of(0x20_0000);
    let heap = svc::set_heap_size(heap_bytes).map_err(|_| 7u32)?;
    let memory = heap + 0x10_0000;
    let channel = nv::Channel::new().map_err(|_| 8u32)?;
    let (_, handle) = channel.buffer_at(memory, (layout.end - BASE) as u32, BASE).map_err(|_| 9u32)?;
    let fs = filesystem().map_err(|_| 10u32)?;
    setup_graphics(fs, &layout, showcase, rays).map_err(|_| 1u32)?;
    let (mut sound, mut pcm) = audio(fs, memory + layout.end - BASE + 0x10_0000).map_err(|_| 2u32)?;
    hid::initialize().map_err(|_| 3u32)?;
    let window = display::Window::target(&channel.nv, channel.nvmap, handle, [WIDTH, HEIGHT], (layout.output - BASE) as u32).map_err(|_| 11u32)?;
    let (mut tick, mut tour, mut idle, mut blocks) = (0u32, 0usize, 180u32, 0usize);
    let (mut eye, mut yaw, _, _) = pose(0);
    loop {
        let input = hid::read();
        let moving = input.left.into_iter().chain(input.right).any(|axis| axis.unsigned_abs() > 3276);
        idle = if moving { 0 } else { idle.saturating_add(1) };
        if idle >= 180 {
            tour = (tour + 1).min(72 * 60);
            (eye, yaw, _, _) = pose(tour);
        } else {
            yaw += input.right[0] as f32 / 32767.0 / 30.0;
            eye[0] += input.left[0] as f32 / 32767.0 / 30.0;
            eye[2] -= input.left[1] as f32 / 32767.0 / 30.0;
        }
        render_frame(&channel, &layout, showcase, rays, tick + 1)?;
        let target = (tick as usize + 1) * 10 / 3;
        while blocks < target {
            sound.listen(eye, yaw, 0.005, sound::Mode::Headphones);
            pcm.period(&sound.render()).map_err(|_| 5u32)?;
            blocks += 1;
        }
        window.present([WIDTH, HEIGHT]).map_err(|_| 6u32)?;
        tick += 1;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main(config: *const [u64; 3]) -> ! {
    if let Err(stage) = run(config) {
        let message = match stage { 20 => "dungeon shadow failed", 21 => "dungeon reflection failed", 22 => "dungeon opaque failed", 23 => "dungeon compute failed", 24 => "dungeon water failed", 25 => "dungeon sprites failed", _ => ["dungeon failed", "dungeon graphics setup failed", "dungeon audio setup failed", "dungeon hid failed", "dungeon gpu frame failed", "dungeon pcm failed", "dungeon present failed", "dungeon heap failed", "dungeon channel failed", "dungeon mapping failed", "dungeon filesystem failed", "dungeon window failed"].get(stage as usize).copied().unwrap_or("dungeon failed") };
        let _ = svc::output_debug_string(message);
    }
    svc::exit_process()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    let _ = svc::output_debug_string("dungeon panicked");
    svc::exit_process()
}
