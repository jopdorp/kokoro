use kokoromi_core::{cpu::cpu_manager::CpuManager, guest::{BASE, load_flat_image}};

fn main() {
    let image = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let cpus = CpuManager::new();
    let cpu = cpus.get_core(0).unwrap();
    load_flat_image(&cpus.ram, &image, BASE).unwrap();
    cpu.set_sp(BASE);
    cpu.set_pc(BASE);
    let start = std::time::Instant::now();
    cpu.run();
    println!("{:.3} ms/period", start.elapsed().as_secs_f64() * 50.0);
}
