fn main() {
    println!("cargo:rerun-if-changed=link.ld");
    println!("cargo:rerun-if-changed=../demos/assets/boombox-lofi/lofi.pcm");
    println!("cargo:rustc-link-arg-bins=-T{}/link.ld", env!("CARGO_MANIFEST_DIR"));
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let dungeon_assets = scene::dungeon::data::assets();
    for (name, data) in [
        ("vertices", dungeon_assets.vertices),
        ("indices", dungeon_assets.indices),
        ("ray-geometry", dungeon_assets.ray_geometry),
        ("ray-indices", dungeon_assets.ray_indices),
        ("lights", dungeon_assets.lights),
        ("zones", dungeon_assets.zones),
        ("props", dungeon_assets.props),
        ("tour", dungeon_assets.tour),
        ("acoustic-rooms", dungeon_assets.acoustic_rooms),
    ] {
        std::fs::write(out.join(format!("dungeon-{name}.bin")), data).unwrap();
    }
    dungeon(&out);
    std::fs::write(out.join("sprites-atlas.bin"), scene::sprites_assets::atlas()).unwrap();
    std::fs::write(out.join("particles.bin"), scene::particles::assets::initial_particles()).unwrap();
    std::fs::write(out.join("particles-background.bin"), scene::particles::assets::background()).unwrap();
    std::fs::write(out.join("solar-bodies.bin"), scene::solar::assets::initial_bodies()).unwrap();
    std::fs::write(out.join("solar-stars.bin"), scene::solar::assets::stars()).unwrap();
    let julia: Vec<u8> = scene::julia::julia_program().iter().chain(&[scene::sass::bra(-16)]).flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(out.join("julia.bin"), julia).unwrap();
    let instances: Vec<u8> = scene::sprites_assets::instances(0.0).iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(out.join("sprites-instances.bin"), instances).unwrap();
    let bounce: Vec<u8> = scene::bounce::assets::instances(&scene::bounce::balls(), &scene::bounce::assets::walls()).iter().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(out.join("bounce-instances.bin"), bounce).unwrap();
    let (music, samples) = scene::sound::assets::place(&scene::sound::assets::instruments(), 0);
    std::fs::write(out.join("music-pcm.bin"), music).unwrap();
    let (knock, _) = scene::sound::assets::place(&[scene::sound::assets::knock()], 0);
    std::fs::write(out.join("bounce-pcm.bin"), knock).unwrap();
    std::fs::write(out.join("biquad.bin"), scene::sound::assets::biquad()).unwrap();
    let pitches: Vec<f32> = (-12..=15).map(|n| 2f32.powf(n as f32 / 12.0)).collect();
    std::fs::write(out.join("music.rs"), format!("const SAMPLES: [(u64,u32); 4] = {samples:?};\nconst PITCHES: [f32; 28] = {pitches:?};\nconst BIQUAD: [u8;10] = {:?};\n", scene::sound::assets::biquad())).unwrap();
    for (name, directory, room, traced) in [("helmet", "flighthelmet", false, false), ("raytraced", "boombox", true, true)] {
        println!("cargo:rerun-if-changed=../demos/assets/{directory}");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../demos/assets").join(directory);
    let asset = scene::pbr::assets::Asset::load(&|file| std::fs::read(dir.join(file)).map_err(|e| e.to_string()), 1920, 1080, room, traced, false).expect("run demos/assets/fetch.sh before building guests");
    let mut data = Vec::new();
    let writes: Vec<_> = asset.writes.iter().map(|(va, bytes)| { let offset = data.len(); data.extend(bytes); (*va, offset, bytes.len()) }).collect();
    std::fs::write(out.join(format!("{name}.bin")), data).unwrap();
    let showcase = traced.then(|| [include_bytes!("../core/tests/fixtures/driver-shaders/rt.frag.bin").as_slice(), &scene::sass::bra(-16).to_le_bytes()].concat()).unwrap_or_default();
    std::fs::write(out.join(format!("{name}-showcase.bin")), showcase).unwrap();
    let blends = asset.blends.iter().map(|blend| match blend { Some(words) => format!("Some(&{words:?})"), None => "None".into() }).collect::<Vec<_>>().join(", ");
    std::fs::write(out.join(format!("{name}.rs")), format!("const WRITES: &[(u64, usize, usize)] = &{writes:?};\nconst BLENDS: &[Option<&[u32]>] = &[{blends}];\nconst DRAWS: &[[u32; 3]] = &{:?};\nconst FIT: ([f32; 3], f32) = {:?};\nconst VIEW: scene::pbr::Matrix = {:?};\nconst TARGET: u64 = {};\nconst END: u64 = {};\nconst INIT_WORDS: usize = {};\nconst ROOM: bool = {room};\nconst TRACED: bool = {traced};\nconst GEOMETRY: scene::pbr::RayGeometry = scene::pbr::RayGeometry {{ model_vertices: {}, model_indices: {}, vertex_count: {}, index_count: {}, indices_va: {}, light_count: {} }};\nconst LIGHT_FIRST: u32 = {};\nconst MATERIAL_COUNT: usize = {};\nconst TEXTURE_COUNT: u32 = {};\nconst SHOWCASE_DATA: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{name}-showcase.bin\"));\n", asset.draws, asset.fit, scene::pbr::assets::view_projection(1920, 1080, scene::pbr::EYE), asset.target, asset.end, asset.writes.last().unwrap().1.len()/4, asset.model_vertices, asset.model_indices, asset.vertex_count, asset.index_count, asset.indices_va, asset.light_count, asset.light_first, asset.material_count, asset.texture_count)).unwrap();
}

fn dungeon(out: &std::path::Path) {
    use scene::dungeon::data::{Builder, Sfx, UP, Vertex, WATER, World, cross, dot, scale, sub, tour_pose, unit};
    use scene::dungeon::sound::Sample;
    use scene::pbr::{Matrix, multiply};

    fn floats(values: impl IntoIterator<Item = f32>) -> Vec<u8> {
        values.into_iter().flat_map(f32::to_le_bytes).collect()
    }
    fn packed(vertices: &[Vertex]) -> Vec<u8> {
        floats(vertices.iter().flat_map(|v| [v.pos, v.normal, [v.uv[0], v.uv[1], v.tint[0]], [v.tint[1], v.tint[2], v.material]].concat()))
    }
    fn matrices(world: &World, pose: scene::dungeon::data::TourPose, seconds: f32, grid: [usize; 2]) -> [f32; 128] {
        let right = [pose.yaw.cos(), 0.0, pose.yaw.sin()];
        let forward = [pose.yaw.sin() * pose.pitch.cos(), pose.pitch.sin(), -pose.yaw.cos() * pose.pitch.cos()];
        let up = cross(right, forward);
        let back = scale(forward, -1.0);
        let axes = [right, up, back];
        let view: Matrix = std::array::from_fn(|column| std::array::from_fn(|row| if row == 3 { f32::from(column == 3) } else if column == 3 { -dot(axes[row], pose.eye) } else { axes[row][column] }));
        let focal = 1.0 / (std::f32::consts::PI / 6.0).tan();
        let projection = [[focal * 1080.0 / 1920.0, 0.0, 0.0, 0.0], [0.0, -focal, 0.0, 0.0], [0.0, 0.0, -100.0 / 99.9, -1.0], [0.0, 0.0, -10.0 / 99.9, 0.0]];
        let vp = multiply(projection, view);
        let plane = if world.zones[world.cave].space.contains(pose.eye) { WATER } else { 0.0 };
        let reflect = [[1.0, 0.0, 0.0, 0.0], [0.0, -1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, plane * 2.0, 0.0, 1.0]];
        let reflected = multiply(vp, reflect);
        let reflected_eye = [pose.eye[0], plane * 2.0 - pose.eye[1], pose.eye[2]];
        let focus = sub(pose.eye, scale(cross(right, up), 4.0));
        let mut nearest: Vec<_> = (0..world.lights.len()).collect();
        nearest.sort_by(|&a, &b| {
            let score = |i: usize| world.lights[i].colour[0] / (1.0 + dot(sub(world.lights[i].pos, focus), sub(world.lights[i].pos, focus)));
            score(b).total_cmp(&score(a))
        });
        let uniform = |matrix: Matrix, eye: [f32; 3], reflection: f32| {
            let mut u = matrix.as_flattened().to_vec();
            u.extend(reflected.as_flattened());
            u.extend([eye[0], eye[1], eye[2], seconds, right[0], right[1], right[2], 0.0, up[0], up[1], up[2], 0.0, world.river.lo[0], world.river.lo[2], world.river.hi[0], world.river.hi[2]]);
            u.extend(world.grate.centre());
            u.push(pose.fade.min(0.999) * 0.5);
            u.extend([1920.0, 1080.0, reflection, plane]);
            u.extend(nearest.iter().take(4).map(|&i| i as f32));
            u.extend([grid[0] as f32, grid[1] as f32, nearest[4] as f32, nearest[5] as f32]);
            u
        };
        uniform(reflected, reflected_eye, 1.0).into_iter().chain(uniform(vp, pose.eye, 0.0)).collect::<Vec<_>>().try_into().unwrap()
    }

    let world = World::new(scene::dungeon::WORLD_SEED);
    let mut mesh = Builder::default();
    world.build(&mut mesh);
    let mut constants = format!("const INITIAL_EYE: [f32; 3] = {:?};\n", scene::dungeon::data::add(world.path[0], [0., scene::dungeon::data::EYE, 0.]));
    for (quality, grid) in [("realtime", [96, 24]), ("showcase", [256, 64])] {
        let first = mesh.vertices.len();
        let mut vertices = mesh.vertices.clone();
        for i in 0..grid[0] * grid[1] {
            let uv = [(i % grid[0]) as f32 / (grid[0] - 1) as f32, (i / grid[0]) as f32 / (grid[1] - 1) as f32];
            let x = world.river.lo[0] + uv[0] * (world.river.hi[0] - world.river.lo[0]);
            vertices.push(Vertex { pos: [x, WATER, world.river_bank(x, 0) + uv[1] * (world.river_bank(x, 1) - world.river_bank(x, 0))], normal: UP, uv, material: 4.0, ..Vertex::default() });
        }
        let mut indices = mesh.indices.clone();
        let opaque = indices.len();
        for y in 0..grid[1] - 1 { for x in 0..grid[0] - 1 {
            let a = (first + y * grid[0] + x) as u32;
            let row = grid[0] as u32;
            indices.extend([a, a + row, a + 1, a + 1, a + row, a + row + 1]);
        }}
        std::fs::write(out.join(format!("dungeon-{quality}-vertices.bin")), packed(&vertices)).unwrap();
        std::fs::write(out.join(format!("dungeon-{quality}-indices.bin")), indices.iter().flat_map(|i| i.to_le_bytes()).collect::<Vec<_>>()).unwrap();
        constants.push_str(&format!("const {}_OPAQUE_INDICES: u32 = {opaque};\nconst {}_WATER_INDICES: u32 = {};\n", quality.to_uppercase(), quality.to_uppercase(), indices.len() - opaque));
    }

    let poses: Vec<_> = (0..=72 * 60).map(|frame| tour_pose(&world, frame as f32 / 60.0)).collect();
    std::fs::write(out.join("dungeon-poses.bin"), floats(poses.iter().flat_map(|p| [p.eye[0], p.eye[1], p.eye[2], p.yaw, p.pitch, p.fade]))).unwrap();
    let frames = poses.iter().enumerate().flat_map(|(frame, &pose)| floats(matrices(&world, pose, frame as f32 / 60.0, [96, 24]))).collect::<Vec<_>>();
    std::fs::write(out.join("dungeon-frames.bin"), frames).unwrap();
    let mut shadow_lights: Vec<_> = (0..world.lights.len()).collect();
    shadow_lights.sort_by(|&a, &b| world.lights[b].colour[0].total_cmp(&world.lights[a].colour[0]));
    shadow_lights.truncate(10);
    let mut shadows = Vec::new();
    for &light in &shadow_lights {
        let eye = world.lights[light].pos;
        for dir in [[1., 0., 0.], [-1., 0., 0.], [0., 1., 0.], [0., -1., 0.], [0., 0., 1.], [0., 0., -1.]] {
            let right = unit(cross(dir, if dir[1].abs() > 0.5 { [0., 0., 1.] } else { UP }));
            let axes = [right, cross(right, dir), scale(dir, -1.)];
            let view = std::array::from_fn(|c| std::array::from_fn(|r| if r == 3 { f32::from(c == 3) } else if c == 3 { -dot(axes[r], eye) } else { axes[r][c] }));
            let projection = [[1., 0., 0., 0.], [0., -1., 0., 0.], [0., 0., -30. / 29.95, -1.], [0., 0., -1.5 / 29.95, 0.]];
            let mut uniform = [0.; 64];
            uniform[..16].copy_from_slice(multiply(projection, view).as_flattened());
            uniform[32..35].copy_from_slice(&eye);
            uniform[54] = 2.;
            shadows.extend(uniform);
        }
    }
    std::fs::write(out.join("dungeon-shadow-frames.bin"), floats(shadows)).unwrap();
    let mut lights = Vec::new();
    for frame in 0..poses.len() { for (i, light) in world.lights.iter().enumerate() {
        let seconds = frame as f32 / 60.0;
        let phase = i as f32 * 2.3;
        let flicker = 1.0 + light.flicker * (0.21 * (seconds * 11.7 + phase).sin() + 0.11 * (seconds * 23.1 + phase * 1.7).sin() + 0.08 * (seconds * 4.3 + phase).sin());
        let reveal = if world.zones[world.cave].space.contains(light.pos) { 0.12 + 0.88 * scene::dungeon::data::smoother((seconds - 36.0) / 6.0) } else { 1.0 };
        lights.extend([light.pos[0], light.pos[1], light.pos[2], light.flame * reveal, light.colour[0] * flicker * reveal, light.colour[1] * flicker * reveal, light.colour[2] * flicker * reveal, shadow_lights.iter().position(|&slot| slot == i).map_or(0.0, |slot| slot as f32 + 1.0)]);
    }}
    std::fs::write(out.join("dungeon-frame-lights.bin"), floats(lights)).unwrap();
    let cones = world.props.iter().filter_map(|prop| match *prop {
        scene::dungeon::data::Prop::Stalactite(p, h, r) => Some([p[0], p[1], p[2], -h, r, (p[0] * 3.0).sin() * 0.17, (p[2] * 2.0).cos() * 0.17, 0.0]),
        scene::dungeon::data::Prop::Stalagmite(p, h, r) => Some([p[0], p[1], p[2], h, r, (p[0] * 3.0).sin() * 0.17, (p[2] * 2.0).cos() * 0.17, 0.0]),
        _ => None,
    }).collect::<Vec<_>>();
    std::fs::write(out.join("dungeon-cones.bin"), floats(cones.iter().flatten().copied())).unwrap();

    let sample = |sfx| match sfx { Sfx::Torch => Sample::Torch, Sfx::Fire => Sample::Fire, Sfx::River => Sample::River, Sfx::Wind => Sample::Wind, Sfx::Heart => Sample::Heart, Sfx::Creak => Sample::Creak, Sfx::Bats => Sample::Bats };
    let mut voices: Vec<_> = world.sources.iter().enumerate().map(|(i, &(sfx, position, gain))| (sample(sfx), position, gain * if sfx == Sfx::Heart { 0.12 } else { 1.2 }, 0.99 + 0.02 * ((i * 7) % 11) as f32 / 10.0, sfx != Sfx::Creak && !matches!(sfx, Sfx::Torch | Sfx::Fire | Sfx::River))).collect();
    voices.extend(world.lights.iter().filter(|light| light.flame >= 0.3).enumerate().map(|(i, light)| (if light.flame > 1.0 { Sample::Fire } else { Sample::Torch }, light.pos, if light.flame > 1.0 { 2.64 } else { 0.66 }, 0.99 + 0.02 * ((i * 7) % 11) as f32 / 10.0, true)));
    voices.extend((0..4).map(|i| (Sample::River, [world.river.lo[0] + (i as f32 + 0.5) / 4.0 * (world.river.hi[0] - world.river.lo[0]), world.river.hi[1], world.river.centre()[2]], 1.56, 0.99 + i as f32 * 0.006, true)));
    voices.extend(world.acoustic_rooms().iter().enumerate().map(|(i, room)| (Sample::Room, std::array::from_fn(|axis| (room.min[axis] + room.max[axis]) * 0.5), if i == world.cave { 0.28 } else { 0.08 }, 1.0, true)));
    let cave = world.zones[world.cave].space;
    let mut positions: Vec<_> = world.drips.iter().map(|&(tip, floor)| [tip[0], floor, tip[2]]).collect();
    positions.sort_by(|a, b| dot(sub(*a, cave.centre()), sub(*a, cave.centre())).total_cmp(&dot(sub(*b, cave.centre()), sub(*b, cave.centre()))));
    voices.extend([Sample::Drip, Sample::Drip, Sample::Drip, Sample::Drip, Sample::Scream, Sample::Step, Sample::Step].into_iter().enumerate().map(|(i, sample)| (sample, positions.get(i).copied().unwrap_or(cave.centre()), 0.0, 1.0, false)));
    std::fs::write(out.join("dungeon-voices.bin"), voices.iter().flat_map(|(sample, position, gain, pitch, active)| [*sample as u32, position[0].to_bits(), position[1].to_bits(), position[2].to_bits(), gain.to_bits(), pitch.to_bits(), u32::from(*active)].into_iter().flat_map(u32::to_le_bytes)).collect::<Vec<_>>()).unwrap();
    let corridors: Vec<_> = world.zones.iter().filter(|zone| zone.kind == scene::dungeon::data::Kind::Corridor).map(|zone| [zone.space.lo, zone.space.hi]).collect();
    let drips: Vec<_> = world.drips.iter().map(|&(tip, floor)| ([tip[0], floor, tip[2]], ((tip[1] - floor) / 4.9).sqrt())).collect();
    constants.push_str(&format!("const CORRIDORS: &[[[f32; 3]; 2]] = &{corridors:?};\nconst DRIPS: &[([f32; 3], f32)] = &{drips:?};\n"));
    let rooms = world.acoustic_rooms();
    constants.push_str(&format!("const VOICE_COUNT: usize = {};\nconst LIGHT_COUNT: usize = {};\nconst CONE_COUNT: u32 = {};\nconst BRIDGE: [[f32; 3]; 2] = {:?};\nconst REVEAL: [[f32; 3]; 2] = {:?};\n", voices.len(), world.lights.len(), cones.len(), [world.bridge.lo, world.bridge.hi], [cave.lo, cave.hi]));
    constants.push_str(&format!("const ROOM_COUNT: usize = {};\nconst DOORWAY_COUNT: usize = {};\n", rooms.len(), rooms.iter().map(|room| room.doorways.len()).sum::<usize>()));
    let audio = ["torch", "fire", "river", "wind", "heart", "creak", "room", "drip0", "drip1", "drip2", "stone0", "stone1", "stone2", "stone3", "stone4", "stone5", "wet0", "wet1", "wet2", "wet3", "wet4", "wet5", "gravel0", "gravel1", "gravel2", "gravel3", "gravel4", "gravel5", "wood0", "wood1", "wood2", "wood3", "wood4", "wood5"];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../demos/assets");
    let sizes: Vec<_> = audio.iter().map(|name| std::fs::metadata(root.join("dungeon-audio").join(format!("{name}.pcm"))).unwrap().len() as usize).collect();
    let textures = ["PavingStones142-colour", "PavingStones142-normal", "PavingStones142-surface", "Rock035-colour", "Rock035-normal", "Rock035-surface", "Metal055C-colour", "Metal055C-normal", "Metal055C-surface"];
    let texture_sizes: Vec<_> = textures.iter().map(|name| std::fs::metadata(root.join("dungeon").join(format!("{name}.rgba8"))).unwrap().len() as usize).collect();
    constants.push_str(&format!("const AUDIO_BYTES: [usize; {}] = {:?};\nconst TEXTURE_BYTES: [usize; 9] = {:?};\n", sizes.len(), sizes, texture_sizes));
    std::fs::write(out.join("dungeon-runtime.rs"), constants).unwrap();
}
}
