fn main() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if matches!(args.as_slice(), [help] if help == "--help")
        || matches!(args.as_slice(), [run, help] if run == "run" && help == "--help")
    {
        println!("kokoromi run <file.nro>\nkokoromi soc --kernel Image --dtb t234-min.dtb --initrd initramfs.cpio\nKOKOROMI_EXACT=1 kokoromi run <file.nro>\nKOKOROMI_CPU=unicorn kokoromi run <file.nro>\nkokoromi-gui");
        return Ok(());
    }
    if let [soc, kernel_flag, kernel, dtb_flag, dtb, initrd_flag, initrd] = args.as_slice()
        && soc == "soc" && kernel_flag == "--kernel" && dtb_flag == "--dtb" && initrd_flag == "--initrd"
    {
        return kokoromi_core::soc::boot(kernel.as_ref(), dtb.as_ref(), initrd.as_ref());
    }
    let path = match args.as_slice() {
        [command, path] if command == "run" => std::path::PathBuf::from(path),
        _ => return Err("usage: kokoromi run <file.nro>".into()),
    };
    kokoromi_core::guest::Machine::load(&path)?.frame()?;
    let (mut width, mut height) = (0, 0);
    let len = unsafe {
        kokoromi_core::ffi::kokoromi_frame_read(std::ptr::null_mut(), 0, &mut width, &mut height)
    };
    let mut pixels = vec![0; len];
    unsafe {
        kokoromi_core::ffi::kokoromi_frame_read(pixels.as_mut_ptr(), len, &mut width, &mut height)
    };
    let digest = pixels.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    println!("frame {width}x{height} fnv1a {digest:016x}");
    Ok(())
}
