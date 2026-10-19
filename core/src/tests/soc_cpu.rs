use crate::cpu::cpu_manager::MEMORY_BASE;
use crate::cpu::{Cpu, DynarmicCPU, DynarmicGroup, GuestRam, UnicornCPU};
use std::process::Command;
use std::sync::Arc;

fn payload() -> Vec<u8> {
    let directory = std::path::PathBuf::from(std::env::var("TMPDIR").unwrap_or_else(|_| "/home/jopdorp/.cache/tmp".into())).join(format!("oboromi-soc-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("gate.s");
    let object = directory.join("gate.o");
    let binary = directory.join("gate.bin");
    std::fs::write(
        &source,
        r#"
.section .text
.org 0
    ldr x0, =0x80004000
    msr vbar_el1, x0
    ldr x0, =0x80001000
    msr ttbr0_el1, x0
    ldr x0, =0x180803f20
    msr tcr_el1, x0
    mov x0, #0xff
    msr mair_el1, x0
    mrs x0, sctlr_el1
    orr x0, x0, #1
    msr sctlr_el1, x0
    isb
    mov x0, #0x10000
    ldr x1, [x0]
    mov x2, #0x55
    str x2, [x0]
    ldr x3, =0x80002000
    ldr x4, =0x80200401
    str x4, [x3]
    dsb ishst
    tlbi vaae1is, x0
    dsb ish
    isb
    ldr x5, [x0]
    ldr x6, =0x8000a000
    stp x1, x5, [x6]
    ldr x0, =0x80008000
    msr elr_el1, x0
    mov x0, #0
    msr spsr_el1, x0
    ldr x0, =0x8000f000
    msr sp_el0, x0
    eret
.ltorg
.org 0x4200
    mrs x10, esr_el1
    mrs x11, far_el1
    ldr x12, =0x8000a000
    stp x10, x11, [x12, #16]
    mrs x13, elr_el1
    add x13, x13, #4
    msr elr_el1, x13
    eret
.ltorg
.org 0x4400
    mrs x10, esr_el1
    ldr x12, =0x8000a000
    str x10, [x12, #32]
    mov x0, #50
    msr cntv_tval_el0, x0
    mov x0, #1
    msr cntv_ctl_el0, x0
    mrs x0, spsr_el1
    bic x0, x0, #0xc0
    msr spsr_el1, x0
    eret
.ltorg
.org 0x4480
    ldr x10, =0x8000a000
    ldr x11, [x10, #40]
    add x11, x11, #1
    str x11, [x10, #40]
    mov x0, #2
    msr cntv_ctl_el0, x0
    eret
.ltorg
.org 0x8000
    svc #0x12
    wfi
    wfi
1:  b 1b
"#,
    )
    .unwrap();
    assert!(Command::new("llvm-mc").args(["-triple=aarch64", "-filetype=obj"]).arg(&source).arg("-o").arg(&object).status().unwrap().success());
    assert!(Command::new("llvm-objcopy").args(["-O", "binary"]).arg(&object).arg(&binary).status().unwrap().success());
    let image = std::fs::read(binary).unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    image
}

fn write64(ram: &GuestRam, offset: u64, value: u64) {
    ram.write(offset, &value.to_le_bytes());
}

#[test]
fn stage1_exceptions_timer_and_irq_work_on_both_backends() {
    let image = payload();
    for backend in ["dynarmic", "unicorn"] {
        let ram = GuestRam::new(0x240_000);
        ram.write(0, &image);
        write64(&ram, 0x1000, MEMORY_BASE + 0x2000 | 3);
        write64(&ram, 0x1010, MEMORY_BASE | 1 << 10 | 1 << 6 | 1);
        write64(&ram, 0x2000, MEMORY_BASE | 1 << 10 | 2 << 6 | 1);
        write64(&ram, 0x1_0000, 0x1122_3344_5566_7788);
        write64(&ram, 0x21_0000, 0x8877_6655_4433_2211);
        let cpu: Box<dyn Cpu> = if backend == "dynarmic" {
            let group = DynarmicGroup::new().unwrap();
            Box::new(DynarmicCPU::new(&group, 0, Arc::clone(&ram)).unwrap())
        } else {
            Box::new(UnicornCPU::new_with_shared_mem(0, Arc::clone(&ram)).unwrap())
        };
        cpu.set_guest_kernel(true);
        cpu.set_virtual_time(true);
        cpu.set_block_ticks(1);
        cpu.set_pc(MEMORY_BASE);
        for _ in 0..100 {
            cpu.run_slice(cpu.ticks() + 1_000, 0).unwrap_or_else(|_| panic!("{backend} failed at {:#x}", cpu.get_pc()));
            if cpu.read_u64(MEMORY_BASE + 0xa028) == 1 {
                break;
            }
            cpu.idle_until(cpu.ticks() + 100);
        }
        assert_eq!(cpu.read_u64(MEMORY_BASE + 0xa000), 0x1122_3344_5566_7788, "{backend} nonidentity translation");
        assert_eq!(cpu.read_u64(MEMORY_BASE + 0xa008), 0x8877_6655_4433_2211, "{backend} TLBI");
        let abort = cpu.read_u64(MEMORY_BASE + 0xa010);
        assert_eq!((abort >> 26, abort & 0x3f), (0x25, 0xe), "{backend} permission syndrome");
        assert_eq!(cpu.read_u64(MEMORY_BASE + 0xa018), 0x1_0000, "{backend} FAR");
        let svc = cpu.read_u64(MEMORY_BASE + 0xa020);
        assert_eq!((svc >> 26, svc & 0xffff), (0x15, 0x12), "{backend} SVC pc={:#x} magic={:#x}", cpu.get_pc(), cpu.read_u64(MEMORY_BASE + 0xa028));
        assert_eq!(cpu.read_u64(MEMORY_BASE + 0xa028), 1, "{backend} timer IRQ after WFI");
        cpu.set_interrupt_lines(true, false);
        cpu.run_slice(cpu.ticks() + 1_000, 0).unwrap();
        cpu.set_interrupt_lines(false, false);
        assert_eq!(cpu.read_u64(MEMORY_BASE + 0xa028), 2, "{backend} IRQ hook after WFI");
    }
}
