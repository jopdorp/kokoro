fn main() {
    println!("cargo:rerun-if-changed=link.ld");
    println!("cargo:rustc-link-arg-bins=-T{}/link.ld", env!("CARGO_MANIFEST_DIR"));
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("sprites-atlas.bin"), scene::sprites_assets::atlas()).unwrap();
    let instances: Vec<u8> = scene::sprites_assets::instances(0.0).iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(out.join("sprites-instances.bin"), instances).unwrap();
}
