//! Flat AArch64 guest images: the bytes go to guest RAM unchanged and execution starts at their first byte.
use crate::cpu::GuestRam;

/// The address guest/link.ld links the image for.
pub const BASE: u64 = 0x0800_0000;

/// Places `image` at `base` in `ram`, returning its entry point, or None when it does not fit.
pub fn load_flat_image(ram: &GuestRam, image: &[u8], base: u64) -> Option<u64> {
    load_segments(ram, base, &[(0, image)]).map(|()| base)
}

/// Writes each (offset, bytes) segment at `base + offset` in `ram`, or nothing and None when one does not fit.
pub fn load_segments(ram: &GuestRam, base: u64, segments: &[(u64, &[u8])]) -> Option<()> {
    let end = |&(offset, bytes): &(u64, &[u8])| base.checked_add(offset)?.checked_add(bytes.len() as u64);
    segments.iter().all(|s| end(s).is_some_and(|end| end <= ram.size() as u64)).then(|| {
        for &(offset, bytes) in segments {
            ram.write(base + offset, bytes);
        }
    })
}

#[derive(Clone, Copy, PartialEq)]
pub enum Demo { Triangle, Sprites, Bounce, BoomBox, Helmet , Post , Particles , Julia, Mandelbrot, Music }

pub struct Machine {
    cpus: crate::cpu::cpu_manager::CpuManager,
    kernel: crate::kernel::Kernel,
    vsync: std::rc::Rc<std::cell::Cell<bool>>,
}

impl Machine {
    pub fn demo(demo: Demo) -> Result<Self, String> {
        use crate::nn::{hipc::Service, nvdrv::Nvdrv, vi};
        use std::{cell::RefCell, rc::Rc};
        let (name, image): (_, &[u8]) = match demo {
            Demo::BoomBox => ("boombox.nro", include_bytes!("../tests/fixtures/guest/boombox.nro")),
            Demo::Helmet => ("helmet.nro", include_bytes!("../tests/fixtures/guest/helmet.nro")),
            Demo::Post => ("post.nro", include_bytes!("../tests/fixtures/guest/post.nro")),
            Demo::Particles => ("particles.nro", include_bytes!("../tests/fixtures/guest/particles.nro")),
            Demo::Music => ("music.nro", include_bytes!("../tests/fixtures/guest/music.nro")),
            Demo::Julia => ("julia.nro", include_bytes!("../tests/fixtures/guest/julia.nro")),
            Demo::Mandelbrot => ("mandelbrot.nro", include_bytes!("../tests/fixtures/guest/mandelbrot.nro")),
            Demo::Bounce => ("bounce.nro", include_bytes!("../tests/fixtures/guest/bounce.nro")),
            Demo::Triangle => ("triangle.nro", include_bytes!("../tests/fixtures/guest/triangle.nro")),
            Demo::Sprites => ("sprites.nro", include_bytes!("../tests/fixtures/guest/sprites.nro")),
        };
        let cpus = crate::cpu::cpu_manager::CpuManager::new();
        let mut kernel = crate::kernel::Kernel::default();
        kernel.scheduler.virtual_time = true;
        // One virtual tick per block keeps demo updates inside the audio period; this is an execution budget, not hardware timing.
        cpus.get_core(0).unwrap().set_block_ticks(1);
        let gpu = crate::gpu::engine::Gpu::with_ram(crate::gpu::device().map_err(|e| format!("{e:?}"))?, cpus.ram.clone(), None, Default::default()).map_err(|e| format!("{e:?}"))?;
        let nvdrv = Rc::new(RefCell::new(Nvdrv::new(gpu)));
        kernel.sm.borrow_mut().0.insert("nvdrv".into(), nvdrv.clone() as Rc<RefCell<dyn Service>>);
        let (display, vsync) = vi::services(nvdrv);
        kernel.sm.borrow_mut().0.extend(display);
        let path = crate::gpu::frame::cache_root().join("guests").join(name);
        crate::loader::nro::load_bytes(&cpus.ram, cpus.get_core(0).unwrap(), &mut kernel, &path, image).map_err(|e| e.to_string())?;
        Ok(Self { cpus, kernel, vsync })
    }

    pub fn frame(&mut self) -> Result<(), String> { self.frame_with(|| {}) }

    /// Polls between CPU slices so offline audio consumers can drain before the live sink ring fills.
    pub fn frame_with(&mut self, mut poll: impl FnMut()) -> Result<(), String> {
        self.vsync.set(false);
        self.kernel.run_until(self.cpus.get_core(0).unwrap(), || { poll(); self.vsync.get() });
        if !self.vsync.get() {
            return Err(format!("guest stopped before present: {:?}", self.kernel.debug_output));
        }
        Ok(())
    }
}

// The kernel's Rc service graph stays on one thread even when Qt changes render workers.
pub fn render_demo(demo: Demo) -> Result<(), String> {
    use std::sync::{Mutex, mpsc::{self, Sender, SyncSender}};
    static RUNNER: Mutex<Option<(Demo, SyncSender<Sender<Result<(), String>>>)>> = Mutex::new(None);
    let mut runner = RUNNER.lock().unwrap();
    if runner.as_ref().is_none_or(|(current, _)| *current != demo) {
        let (tx, rx) = mpsc::sync_channel::<Sender<Result<(), String>>>(0);
        std::thread::spawn(move || {
            let mut machine = Machine::demo(demo);
            for reply in rx {
                let result = machine.as_mut().map_err(|e| e.clone()).and_then(Machine::frame);
                let _ = reply.send(result);
            }
        });
        *runner = Some((demo, tx));
    }
    let (tx, rx) = mpsc::channel();
    runner.as_ref().unwrap().1.send(tx).map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

#[cfg(test)]
pub(crate) static DISPLAY_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::cpu_manager::CpuManager;
    use crate::gpu::engine::Gpu;
    use crate::input::InputState;
    use crate::kernel::Kernel;
    use crate::nn::{hipc::Service, nvdrv::Nvdrv, vi};
    use crate::tests::gpu::harness::vk_state;
    use std::cell::RefCell;
    use std::rc::Rc;

    // guest.bin is what scripts/build-guest.sh builds from guest/. nvdrv needs a GPU, which the kernel's own sm: lacks.
    #[test]
    fn the_guest_image_initializes_services_draws_and_presents() {
        let _display = DISPLAY_TEST.lock().unwrap();
        let Some(state) = vk_state() else { return };
        let cpu = CpuManager::new();
        let image = include_bytes!("../tests/fixtures/guest/guest.bin");
        let entry = load_flat_image(&cpu.ram, image, BASE).unwrap();
        let core = cpu.get_core(0).unwrap();
        let mut kernel = Kernel::default();
        kernel.hid.borrow_mut().input = || InputState { buttons: 1, left: [0.5, -0.5], right: [0.0; 2], connected: true };
        let nvdrv = Rc::new(RefCell::new(Nvdrv::new(Gpu::with_ram(Box::leak(Box::new(state)), cpu.ram.clone(), None, Default::default()).unwrap())));
        kernel.sm.borrow_mut().0.insert("nvdrv".into(), nvdrv.clone() as Rc<RefCell<dyn Service>>);
        kernel.sm.borrow_mut().0.extend(vi::services(nvdrv).0);
        kernel.start(core, entry, image.len() as u64);
        assert!(kernel.run(core));
        assert_eq!(kernel.debug_output[0], "hello from guest");
        let time: i64 = kernel.debug_output[1].strip_prefix("focus 1 time ").unwrap().parse().unwrap();
        assert!((time - std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64).abs() < 5);
        println!("{}", kernel.debug_output[1]);
        assert_eq!(&kernel.debug_output[2..], ["buttons 0x1 left 16383 -16383", "pixel ff8000ff", "slots 0 1 0"]);
        let mut frame = crate::display::DISPLAY.lock().unwrap();
        assert_eq!((frame.width, frame.height), (128, 64));
        assert_eq!(&frame.pixels()[(20 * 128 + 20) * 4..][..4], &[40, 80, 192, 255]);
    }

    #[test]
    fn guest_triangle_end_to_end() {
        let _display = DISPLAY_TEST.lock().unwrap();
        let state = crate::gpu::device().expect("guest triangle requires Vulkan");
        crate::gpu::frame::render_triangle_demo(0.0).unwrap();
        let (generation, expected) = {
            let mut frame = crate::display::DISPLAY.lock().unwrap();
            assert_eq!((frame.width, frame.height), (1920, 1080));
            (frame.generation, frame.pixels().to_vec())
        };
        let dir = std::env::temp_dir().join(format!("oboromi-triangle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("triangle.nro");
        std::fs::write(&path, include_bytes!("../tests/fixtures/guest/triangle.nro")).unwrap();
        let cpus = CpuManager::new();
        let cpu = cpus.get_core(0).unwrap();
        let mut kernel = Kernel::default();
        let nvdrv = Rc::new(RefCell::new(Nvdrv::new(Gpu::with_ram(state, cpus.ram.clone(), None, Default::default()).unwrap())));
        kernel.sm.borrow_mut().0.insert("nvdrv".into(), nvdrv.clone() as Rc<RefCell<dyn Service>>);
        kernel.sm.borrow_mut().0.extend(vi::services(nvdrv).0);
        crate::loader::nro::load(&cpus.ram, cpu, &mut kernel, &path).unwrap();
        assert!(!kernel.run_until(cpu, || crate::display::DISPLAY.lock().unwrap().generation > generation), "{:?}", kernel.debug_output);
        {
            let mut frame = crate::display::DISPLAY.lock().unwrap();
            assert_eq!(frame.generation, generation + 1);
            assert_eq!((frame.width, frame.height), (1920, 1080));
            let digest = |bytes: &[u8]| bytes.iter().fold(0xcbf29ce484222325u64, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3));
            assert_eq!(digest(frame.pixels()), digest(&expected));
            assert!(frame.pixels() == expected);
            println!("guest and host triangle FNV-1a: {:016x}", digest(frame.pixels()));
        }
        assert!(kernel.debug_output.is_empty(), "{:?}", kernel.debug_output);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn hash_audio(hash: &mut u64, samples: &[i16]) {
        for byte in samples.iter().flat_map(|s| s.to_le_bytes()) { *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x100000001b3); }
    }

    fn guest_audio(demo: Demo, frames: usize) -> (Machine, u64, usize) {
        let mut discard = vec![0; crate::audio::queued() * 2];
        crate::audio::pull(&mut discard);
        let mut guest = Machine::demo(demo).unwrap();
        let (mut actual, mut count) = (0xcbf29ce484222325u64, 0usize);
        for _ in 0..frames {
            guest.frame_with(|| {
                let mut samples = [0; 480];
                let frames = crate::audio::pull(&mut samples);
                hash_audio(&mut actual, &samples[..frames * 2]);
                count += frames * 2;
            }).unwrap();
        }
        (guest, actual, count)
    }

    #[test]
    fn guest_music_matches_host_audio() {
        let _display = DISPLAY_TEST.lock().unwrap();
        let (mut guest, actual, count) = guest_audio(Demo::Music, 360);
        let mut host = crate::audio::demo_music::music();
        let mut expected = 0xcbf29ce484222325u64;
        let periods = (f64::from(359f32 / 60.0) * 200.0) as usize + 10;
        for _ in 0..periods {
            crate::audio::demo_music::sequence(&mut host);
            hash_audio(&mut expected, host.period());
        }
        assert_eq!(count, periods * 480);
        assert_eq!(actual, expected);
        println!("guest and host music PCM FNV-1a: {actual:016x}, {periods} periods");
        let cpu = guest.cpus.get_core(0).unwrap();
        cpu.idle_until(cpu.ticks() + crate::cpu::unicorn_interface::TICK_HZ * 3 / 200);
        let mut first = true;
        guest.kernel.run_until(cpu, || { let stop = !first; first = false; stop });
        let mut autonomous = [0; 1440];
        assert_eq!(crate::audio::pull(&mut autonomous), 720);
        let expected: Vec<_> = (0..3).flat_map(|_| host.period().to_vec()).collect();
        assert_eq!(autonomous.as_slice(), expected);
    }

    #[test]
    fn guest_bounce_matches_host_audio() {
        let _display = DISPLAY_TEST.lock().unwrap();
        let (_guest, actual, count) = guest_audio(Demo::Bounce, 61);
        let mut host = crate::gpu::demo_bounce::Bounce::new();
        let mut expected = 0xcbf29ce484222325u64;
        for _ in 0..210 { host.step(); hash_audio(&mut expected, host.sound.period()); }
        assert_eq!(count, 210 * 480);
        assert_eq!(actual, expected);
        println!("guest and host bounce PCM FNV-1a: {actual:016x}, 210 periods");
    }

    #[test]
    fn guest_helmet_matches_host_frame() {
        if !crate::gpu::demo::asset_dir("flighthelmet").exists() { return eprintln!("skipped: run demos/assets/fetch.sh for flighthelmet"); }
        let _display = DISPLAY_TEST.lock().unwrap();
        crate::gpu::demo::render_helmet_frame(1.0).unwrap();
        let expected = crate::display::DISPLAY.lock().unwrap().pixels().to_vec();
        let mut guest = Machine::demo(Demo::Helmet).unwrap();
        for _ in 0..=60 { guest.frame().unwrap(); }
        let mut frame = crate::display::DISPLAY.lock().unwrap();
        let digest = |bytes: &[u8]| bytes.iter().fold(0xcbf29ce484222325u64, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3));
        assert_eq!(digest(frame.pixels()), digest(&expected));
        println!("guest and host helmet FNV-1a: {:016x}", digest(&expected));
    }

    #[test]
    fn guest_post_matches_host_frame() {
        let _display = DISPLAY_TEST.lock().unwrap();
        crate::gpu::demo_post::render_post_frame(1.0).unwrap();
        let expected = crate::display::DISPLAY.lock().unwrap().pixels().to_vec();
        let mut guest = Machine::demo(Demo::Post).unwrap();
        for _ in 0..=60 { guest.frame().unwrap(); }
        let mut frame = crate::display::DISPLAY.lock().unwrap();
        let digest = |bytes: &[u8]| bytes.iter().fold(0xcbf29ce484222325u64, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3));
        assert_eq!(digest(frame.pixels()), digest(&expected));
        println!("guest and host post FNV-1a: {:016x}", digest(&expected));
    }

    #[test]
    fn guest_particles_matches_host_frame() {
        let _display = DISPLAY_TEST.lock().unwrap();
        for tick in 0..=60 { crate::gpu::demo_particles::render_particles_frame(tick as f32 / 60.0).unwrap(); }
        let expected = crate::display::DISPLAY.lock().unwrap().pixels().to_vec();
        let mut guest = Machine::demo(Demo::Particles).unwrap();
        for _ in 0..=60 { guest.frame().unwrap(); }
        let mut frame = crate::display::DISPLAY.lock().unwrap();
        let digest = |bytes: &[u8]| bytes.iter().fold(0xcbf29ce484222325u64, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3));
        assert_eq!(digest(frame.pixels()), digest(&expected));
        println!("guest and host particles FNV-1a: {:016x}", digest(&expected));
    }

    #[test]
    fn guest_julia_matches_host_frame() {
        let _display = DISPLAY_TEST.lock().unwrap();
        crate::gpu::frame::render_demo_frame(1.0).unwrap();
        let expected = crate::display::DISPLAY.lock().unwrap().pixels().to_vec();
        let mut guest = Machine::demo(Demo::Julia).unwrap();
        for _ in 0..=60 { guest.frame().unwrap(); }
        let mut frame = crate::display::DISPLAY.lock().unwrap();
        let digest = |bytes: &[u8]| bytes.iter().fold(0xcbf29ce484222325u64, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3));
        assert_eq!(digest(frame.pixels()), digest(&expected));
        println!("guest and host julia FNV-1a: {:016x}", digest(&expected));
    }

    #[test]
    fn guest_mandelbrot_matches_host_launch() {
        use crate::gpu::cubin::Arg;
        use crate::tests::gpu::harness::run_kernel;
        use scene::mandelbrot::*;
        let _display = DISPLAY_TEST.lock().unwrap();
        let frame_number = 10u32;
        let name = "_Z11Mandelbrot0IfEvP6uchar4iiiT_S2_S2_S2_S2_S0_iiiib";
        let output = vec![0; (WIDTH * HEIGHT * 4) as usize];
        let [width, height, crunch, frame, animation, grid_width, blocks] = [WIDTH, HEIGHT, CRUNCH, 0, 0, GRID_WIDTH, GRID[0]].map(u32::to_le_bytes);
        let [x_off, zero, scale] = [-0.5f32, 0.0, scale(frame_number)].map(f32::to_le_bytes);
        let args = [
            Arg::Buffer(&output), Arg::Value(&width), Arg::Value(&height), Arg::Value(&crunch),
            Arg::Value(&x_off), Arg::Value(&zero), Arg::Value(&zero), Arg::Value(&zero),
            Arg::Value(&scale), Arg::Value(&[3, 5, 7, 0]), Arg::Value(&frame),
            Arg::Value(&animation), Arg::Value(&grid_width), Arg::Value(&blocks), Arg::Value(&[0]),
        ];
        let expected = run_kernel(include_bytes!("../tests/fixtures/mandelbrot-sm86.cubin"), name, GRID, BLOCK, &args).unwrap().remove(0);
        let mut guest = Machine::demo(Demo::Mandelbrot).unwrap();
        for _ in 0..=frame_number { guest.frame().unwrap(); }
        let mut displayed = crate::display::DISPLAY.lock().unwrap();
        let digest = |bytes: &[u8]| bytes.iter().fold(0xcbf29ce484222325u64, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3));
        assert_eq!(digest(displayed.pixels()), digest(&expected));
        println!("guest and host Mandelbrot FNV-1a: {:016x}", digest(&expected));
    }

    #[test]
    fn guest_boombox_matches_host_frame() {
        if !crate::gpu::demo::asset_dir("boombox").exists() { return eprintln!("skipped: run demos/assets/fetch.sh for boombox"); }
        let _display = DISPLAY_TEST.lock().unwrap();
        crate::gpu::demo::render_boombox_frame(1.0).unwrap();
        let expected = crate::display::DISPLAY.lock().unwrap().pixels().to_vec();
        let mut guest = Machine::demo(Demo::BoomBox).unwrap();
        for _ in 0..=60 { guest.frame().unwrap(); }
        let mut frame = crate::display::DISPLAY.lock().unwrap();
        let digest = |bytes: &[u8]| bytes.iter().fold(0xcbf29ce484222325u64, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3));
        assert_eq!(digest(frame.pixels()), digest(&expected));
        println!("guest and host boombox FNV-1a: {:016x}", digest(&expected));
    }

    #[test]
    fn guest_bounce_matches_host_frame() {
        let _display = DISPLAY_TEST.lock().unwrap();
        for tick in 0..=60 { crate::gpu::demo_bounce::render_bounce_frame(tick as f32 / 60.0).unwrap(); }
        let expected = crate::display::DISPLAY.lock().unwrap().pixels().to_vec();
        let mut guest = Machine::demo(Demo::Bounce).unwrap();
        for _ in 0..=60 { guest.frame().unwrap(); }
        let mut frame = crate::display::DISPLAY.lock().unwrap();
        let digest = |bytes: &[u8]| bytes.iter().fold(0xcbf29ce484222325u64, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3));
        assert_eq!(digest(frame.pixels()), digest(&expected));
        println!("guest and host bounce FNV-1a: {:016x}", digest(&expected));
    }

    #[test]
    fn host_stick_moves_the_guest_sprites_at_the_second_present() {
        let _display = DISPLAY_TEST.lock().unwrap();
        let snapshot = || crate::display::DISPLAY.lock().unwrap().pixels().to_vec();
        let sun = |pixels: &[u8]| {
            let (mut x, mut y, mut count) = (0.0, 0.0, 0.0);
            for row in 60..210 {
                for col in 1500..1750 {
                    let p = &pixels[(row * 1920 + col) * 4..][..4];
                    if p[0] > 200 && p[1] > 170 && p[2] < 150 {
                        x += col as f64; y += row as f64; count += 1.0;
                    }
                }
            }
            assert!(count > 2000.0);
            [x / count, y / count]
        };
        crate::input::set_override(Some(InputState::default()));
        crate::gpu::demo_sprites::render_sprites_frame(0.0).unwrap();
        let expected = snapshot();
        let mut centered = Machine::demo(Demo::Sprites).unwrap();
        centered.frame().unwrap();
        centered.frame().unwrap();
        let baseline = snapshot();
        assert!(baseline == expected);
        drop(centered);
        let mut controlled = Machine::demo(Demo::Sprites).unwrap();
        controlled.frame().unwrap();
        crate::input::set_override(Some(InputState { left: [0.5, 0.5], connected: true, ..Default::default() }));
        controlled.frame().unwrap();
        let moved = snapshot();
        for ((before, after), expected) in sun(&baseline).into_iter().zip(sun(&moved)).zip([3.0, -3.0]) {
            assert!((after - before - expected).abs() < 0.1, "sun moved {} instead of {expected}", after - before);
        }
        crate::input::set_override(Some(InputState { buttons: crate::input::A, connected: true, ..Default::default() }));
        controlled.frame().unwrap();
        let tinted = snapshot();
        let center = (132 * 1920 + 1623) * 4;
        assert!(tinted[center] < moved[center] && tinted[center + 2] > moved[center + 2]);
        controlled.frame().unwrap();
        assert!(snapshot() == tinted);
        crate::input::set_override(None);
    }

}
