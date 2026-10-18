#![cfg(test)]

use crate::gpu::cubin::Arg;
use crate::tests::gpu::harness::run_kernel;

const CUBIN: &[u8] = include_bytes!("../../../tests/fixtures/solar/solar-sm86.cubin");

fn bytes(bodies: &[[f32; 12]]) -> Vec<u8> {
    bodies.iter().flatten().flat_map(|value| value.to_le_bytes()).collect()
}

fn bodies(bytes: &[u8]) -> Vec<[f32; 12]> {
    bytes.chunks_exact(48).map(|body| core::array::from_fn(|i| f32::from_le_bytes(body[i * 4..][..4].try_into().unwrap()))).collect()
}

fn totals(bodies: &[[f32; 12]]) -> (f32, [f32; 3]) {
    let mass = bodies.iter().map(|body| body[7]).sum();
    let momentum = core::array::from_fn(|axis| bodies.iter().map(|body| body[4 + axis] * body[7]).sum());
    (mass, momentum)
}

#[test]
fn solar_merges_conserve_mass_and_momentum_deterministically_for_100_steps() {
    let body = [0; 48];
    let counts = vec![0xff; 128 * 128 * 4];
    let slots = vec![0; 128 * 128 * 8 * 4];
    if let Some(output) = run_kernel(CUBIN, "solar_step", [64, 1, 1], [256, 1, 1], &[
        Arg::Buffer(&body), Arg::Buffer(&body), Arg::Buffer(&counts), Arg::Buffer(&slots),
        Arg::Value(&1u32.to_le_bytes()), Arg::Value(&1u32.to_le_bytes()), Arg::Value(&0.01f32.to_le_bytes()),
        Arg::Value(&0.0f32.to_le_bytes()), Arg::Value(&1e-6f32.to_le_bytes()), Arg::Value(&0u32.to_le_bytes()),
    ]) { assert!(output[2].iter().all(|&byte| byte == 0)); }
    let input = bytes(&[
        [-0.20, 0.00, 0.00, 0.0, 0.20, 0.10, 0.00, 2.0, 0.0, 0.0, 0.0, 0.0],
        [ 0.20, 0.00, 0.00, 0.0,-0.20,-0.10, 0.00, 3.0, 0.0, 0.0, 0.0, 0.0],
        [ 0.00,-0.30, 0.00, 0.0, 0.05, 0.25, 0.00, 1.5, 0.0, 0.0, 0.0, 0.0],
        [ 0.10, 0.30, 0.00, 0.0,-0.05,-0.25, 0.00, 0.5, 0.0, 0.0, 0.0, 0.0],
    ]);
    let run = || run_kernel(CUBIN, "solar_contract", [1; 3], [1; 3], &[
        Arg::Buffer(&input), Arg::Value(&4u32.to_le_bytes()), Arg::Value(&100u32.to_le_bytes()),
    ]).map(|output| output[0].clone());
    let Some(first) = run() else { return };
    let second = run().unwrap();
    assert_eq!(first, second);
    let (initial_mass, initial_momentum) = totals(&bodies(&input));
    let final_bodies = bodies(&first);
    let (final_mass, final_momentum) = totals(&final_bodies);
    assert!(final_bodies.iter().filter(|body| body[7] != 0.0).count() < 4);
    assert!((initial_mass - final_mass).abs() <= 1e-6);
    assert!(initial_momentum.into_iter().zip(final_momentum).all(|(before, after)| (before - after).abs() <= 1e-6));
    println!("mass error {:.3e}, momentum error [{:.3e}, {:.3e}, {:.3e}]", (initial_mass - final_mass).abs(),
        (initial_momentum[0] - final_momentum[0]).abs(), (initial_momentum[1] - final_momentum[1]).abs(), (initial_momentum[2] - final_momentum[2]).abs());
}
