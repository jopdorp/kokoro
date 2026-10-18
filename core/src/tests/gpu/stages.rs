#[cfg(test)]
mod tests {
    use crate::gpu::sm86::{ResourceKind, Stage, emit_module};
    use crate::tests::gpu::harness::spirv_val;
    use crate::tests::gpu_test::inst::{ald, ald_phys, ast, ast_phys, exit, fmul, ipa, ldtram, mov_cbuf, pixld};

    const SCALE: u32 = 8;

    fn vertex_program() -> Vec<u128> {
        vec![ald(0, 0x80, 4), ald(4, 0x90, 4), mov_cbuf(SCALE, 0, 0), fmul(0, 0, SCALE), fmul(1, 1, SCALE), ast(0x70, 0, 4), ast(0x80, 4, 4), exit()]
    }

    fn fragment_program() -> Vec<u128> {
        let varying = (0..4).map(|c| ipa(c, 0x80 + 4 * c, 0));
        varying.chain([mov_cbuf(SCALE, 0, 0), fmul(0, 0, SCALE), fmul(1, 1, SCALE), fmul(2, 2, SCALE), exit()]).collect()
    }

    fn stage_interface(stage: Stage, program: &[u128]) -> Vec<ResourceKind> {
        let mut interface = Vec::new();
        let module = emit_module(None, stage, Default::default(), |decoder| {
            decoder.translate_program(program);
            interface = decoder.resources.iter().map(|r| r.kind).filter(|kind| !matches!(kind, ResourceKind::Cbuf(_))).collect();
        });
        spirv_val(&module);
        interface
    }

    #[test]
    fn vertex_program_reads_inputs_and_writes_position_and_varying() {
        use ResourceKind::*;
        let position = BuiltIn(crate::gpu::spirv::BuiltIn::Position as u32);
        let expected = [AttributeMap { output: false, per_vertex: false }, position, AttributeMap { output: true, per_vertex: false }];
        assert_eq!(stage_interface(Stage::Vertex, &vertex_program()), expected);
    }

    #[test]
    fn fragment_program_reads_varying_and_writes_colour() {
        let expected = [ResourceKind::Varying { location: 0, flat: false }, ResourceKind::FragmentOutput { location: 0 }];
        assert_eq!(stage_interface(Stage::Fragment, &fragment_program()), expected);
    }

    #[test]
    fn physical_attributes_use_indexed_geometry_maps() {
        use ResourceKind::AttributeMap;
        let program = [ald_phys(0, 2, 3), ast_phys(4, 0), exit()];
        let expected = [AttributeMap { output: false, per_vertex: true }, AttributeMap { output: true, per_vertex: false }];
        assert_eq!(stage_interface(Stage::Geometry, &program), expected);
    }

    #[test]
    fn ldtram_reads_fragment_per_vertex_attributes() {
        use ResourceKind::AttributeMap;
        let program = [ldtram(0, 0x80, false), ldtram(2, 0x84, true), exit()];
        let expected = [AttributeMap { output: false, per_vertex: true }, ResourceKind::FragmentOutput { location: 0 }];
        assert_eq!(stage_interface(Stage::Fragment, &program), expected);
    }

    #[test]
    fn pixld_reads_fragment_sample_state() {
        let program = [pixld(0, 0, 0), pixld(1, 1, 1), pixld(2, 2, 2), pixld(3, 3, 3), pixld(4, 4, 4), exit()];
        let expected = [
            ResourceKind::BuiltIn(crate::gpu::spirv::BuiltIn::SampleMask as u32),
            ResourceKind::BuiltIn(crate::gpu::spirv::BuiltIn::SamplePosition as u32),
            ResourceKind::BuiltIn(crate::gpu::spirv::BuiltIn::SampleId as u32),
            ResourceKind::BuiltIn(crate::gpu::spirv::BuiltIn::FullyCovered as u32),
            ResourceKind::FragmentOutput { location: 0 },
        ];
        assert_eq!(stage_interface(Stage::Fragment, &program), expected);
    }
}
