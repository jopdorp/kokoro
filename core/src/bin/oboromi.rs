fn main() -> Result<(), String> {
    let path = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [command, path] if command == "run" => std::path::PathBuf::from(path),
        _ => return Err("usage: oboromi run <file.nro>".into()),
    };
    oboromi_core::guest::Machine::load(&path)?.frame()?;
    let (mut width, mut height) = (0, 0);
    let len = unsafe {
        oboromi_core::ffi::oboromi_frame_read(std::ptr::null_mut(), 0, &mut width, &mut height)
    };
    let mut pixels = vec![0; len];
    unsafe {
        oboromi_core::ffi::oboromi_frame_read(pixels.as_mut_ptr(), len, &mut width, &mut height)
    };
    let digest = pixels.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    println!("frame {width}x{height} fnv1a {digest:016x}");
    Ok(())
}
