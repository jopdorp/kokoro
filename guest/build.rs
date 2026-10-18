fn main() {
    println!("cargo:rerun-if-changed=link.ld");
    println!("cargo:rerun-if-changed=../demos/assets/boombox-lofi/lofi.pcm");
    println!("cargo:rustc-link-arg-bins=-T{}/link.ld", env!("CARGO_MANIFEST_DIR"));
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let dungeon = scene::dungeon::data::assets();
    for (name, data) in [
        ("vertices", dungeon.vertices),
        ("indices", dungeon.indices),
        ("lights", dungeon.lights),
        ("zones", dungeon.zones),
        ("props", dungeon.props),
        ("tour", dungeon.tour),
        ("acoustic-rooms", dungeon.acoustic_rooms),
    ] {
        std::fs::write(out.join(format!("dungeon-{name}.bin")), data).unwrap();
    }
    std::fs::write(out.join("sprites-atlas.bin"), scene::sprites_assets::atlas()).unwrap();
    std::fs::write(out.join("particles.bin"), scene::particles::assets::initial_particles()).unwrap();
    std::fs::write(out.join("particles-background.bin"), scene::particles::assets::background()).unwrap();
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
}
