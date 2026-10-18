fn main() {
    println!("cargo:rerun-if-changed=link.ld");
    println!("cargo:rustc-link-arg-bins=-T{}/link.ld", env!("CARGO_MANIFEST_DIR"));
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
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
    for (name, directory) in [("boombox", "boombox"), ("helmet", "flighthelmet")] {
        println!("cargo:rerun-if-changed=../demos/assets/{directory}");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../demos/assets").join(directory);
    let asset = scene::pbr::assets::Asset::load(&|file| std::fs::read(dir.join(file)).map_err(|e| e.to_string()), 1920, 1080, false, false).expect("run demos/assets/fetch.sh before building guests");
    let mut data = Vec::new();
    let writes: Vec<_> = asset.writes.iter().map(|(va, bytes)| { let offset = data.len(); data.extend(bytes); (*va, offset, bytes.len()) }).collect();
    std::fs::write(out.join(format!("{name}.bin")), data).unwrap();
    let blends = asset.blends.iter().map(|blend| match blend { Some(words) => format!("Some(&{words:?})"), None => "None".into() }).collect::<Vec<_>>().join(", ");
    std::fs::write(out.join(format!("{name}.rs")), format!("const WRITES: &[(u64, usize, usize)] = &{writes:?};\nconst BLENDS: &[Option<&[u32]>] = &[{blends}];\nconst DRAWS: &[[u32; 3]] = &{:?};\nconst FIT: ([f32; 3], f32) = {:?};\nconst VIEW: scene::pbr::Matrix = {:?};\nconst TARGET: u64 = {};\nconst END: u64 = {};\nconst INIT_WORDS: usize = {};\n", asset.draws, asset.fit, scene::pbr::assets::view_projection(1920, 1080, scene::pbr::EYE), asset.target, asset.end, asset.writes.last().unwrap().1.len()/4)).unwrap();
}
}
