#[cfg(test)]
mod tests {
    use crate::tests::gpu::harness::{GLOBAL_BASE, run_lanes, run_regs};
    use crate::tests::gpu_test::inst::isberd;

    #[test]
    fn isberd_maps_skews_and_loads_output_memory() {
        let map = isberd(1, 0, 5, 0, 2, false, false);
        let Some((regs, _)) = run_regs(&[map], &[7], &[], &[]) else { return };
        assert_eq!(regs[1], 12);

        let skew = isberd(1, 0, 0, 0, 1, true, false);
        let seeds: Vec<_> = (0..4).map(|_| vec![10]).collect();
        let Some((lanes, _)) = run_lanes(&[skew], &seeds, &[], &[]) else { return };
        assert_eq!(lanes.iter().map(|r| r[1]).collect::<Vec<_>>(), [10, 12, 14, 16]);

        let load = isberd(1, 0, 0, 0, 0, false, true);
        let Some((regs, _)) = run_regs(&[load], &[GLOBAL_BASE as u32 + 1], &[], &[0x4433_2211]) else { return };
        assert_eq!(regs[1], 0x22);
    }
}
