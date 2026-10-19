use super::cpu_manager::MEMORY_BASE;
use super::{Cpu, CpuContext, CpuError, GuestRam};
use crate::gpu::memory::{PAGE, pages};
use std::cell::Cell;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use crate::soc::{Bus, IrqLine};
use unicorn_engine::{Arch, Arm64Insn, Context, HookType, MemType, Mode, Prot, Query, RegisterARM64, RegisterARM64CP, TlbType, Unicorn, uc_error};

const EXCP_SWI: u32 = 2;
const EXCP_PREFETCH_ABORT: u32 = 3;
const EXCP_DATA_ABORT: u32 = 4;
const INSTRUCTIONS_PER_BLOCK: u64 = 64;

pub use super::TICK_HZ;
const CLREX: u64 = 0xffff_0000_0000;

struct Clock {
    epoch: Instant,
    virtual_time: Cell<bool>,
    block_ticks: Cell<u64>,
    ticks: Cell<u64>,
    core: Cell<u32>,
}

impl Clock {
    fn now(&self) -> u64 {
        if self.virtual_time.get() { self.ticks.get() } else { (self.epoch.elapsed().as_nanos() * u128::from(TICK_HZ) / 1_000_000_000) as u64 }
    }
}

struct RunState {
    svc: Option<u32>,
    clock: Clock,
    dirty_pages: Vec<u64>,
    guest_kernel: bool,
    irq: bool,
    fiq: bool,
    waiting: bool,
    cntv_ctl: u64,
    cntv_cval: u64,
    exception_taken: bool,
    last_data: u64,
    bus: Option<Arc<Mutex<Bus>>>,
    resume_pc: Option<u64>,
}

type Emu = Unicorn<'static, RunState>;

struct Engine {
    emu: Emu,
    initial: Context,
    views: Vec<Context>,
    active: usize,
    hooked: usize,
}

// All Unicorn handles and their internal Rc state remain behind the engine mutex.
unsafe impl Send for Engine {}

impl Deref for Engine {
    type Target = Emu;
    fn deref(&self) -> &Emu { &self.emu }
}
impl DerefMut for Engine {
    fn deref_mut(&mut self) -> &mut Emu { &mut self.emu }
}

impl Engine {
    fn clear_exclusive(&mut self) -> Result<(), uc_error> {
        // Unicorn contexts include the software monitor; CLREX prevents ABA success after a handoff.
        let pc = self.pc_read()?;
        let result = self.emu_start(CLREX, CLREX + 4, 0, 0);
        self.reg_write(RegisterARM64::PC, pc)?;
        result
    }
}

/// Logical CPU view. All views owned by a `CpuManager` share one serialized engine.
pub struct UnicornCPU {
    emu: Arc<Mutex<Engine>>,
    ram: Arc<GuestRam>,
    pub core_id: u32,
    view: usize,
    timer_irq: Option<IrqLine>,
}

fn cp(op1: u32, crn: u32, crm: u32, op2: u32, value: u64) -> RegisterARM64CP {
    RegisterARM64CP::new().op0(3).op1(op1).crn(crn).crm(crm).op2(op2).val(value)
}

fn physical(uc: &mut Unicorn<'_, RunState>, va: u64) -> Result<(u64, u64), u64> {
    let mut sctlr = cp(0, 1, 0, 0, 0);
    uc.reg_read_arm64_coproc(&mut sctlr).map_err(|_| 4u64)?;
    if sctlr.val & 1 == 0 { return Ok((va, 3)); }
    let mut tcr = cp(0, 2, 0, 2, 0);
    uc.reg_read_arm64_coproc(&mut tcr).map_err(|_| 4u64)?;
    let upper = va >> 63 != 0;
    let bits = 64 - if upper { tcr.val >> 16 & 63 } else { tcr.val & 63 };
    if !(25..=48).contains(&bits) || (upper && tcr.val >> 30 & 3 != 2) || (!upper && tcr.val >> 14 & 3 != 0) { return Err(4); }
    let mut ttbr = cp(0, 2, 0, u32::from(upper), 0);
    uc.reg_read_arm64_coproc(&mut ttbr).map_err(|_| 4u64)?;
    let mut level = 3 - (bits - 13) / 9;
    let mut table = ttbr.val & 0x0000_ffff_ffff_f000;
    loop {
        let shift = 12 + 9 * (3 - level);
        let mut bytes = [0; 8];
        uc.mem_read(table + (va >> shift & 0x1ff) * 8, &mut bytes).map_err(|_| 20 + level)?;
        let descriptor = u64::from_le_bytes(bytes);
        if descriptor & 3 == 3 && level < 3 {
            table = descriptor & 0x0000_ffff_ffff_f000;
            level += 1;
        } else if descriptor & 1 != 0 {
            if descriptor >> 10 & 1 == 0 { return Err(8 + level); }
            let mask = (1 << shift) - 1;
            return Ok((descriptor & 0x0000_ffff_ffff_f000 & !mask | va & mask, level));
        } else {
            return Err(4 + level);
        }
    }
}

fn exception(uc: &mut Unicorn<'_, RunState>, syndrome: u64, offset: u64) -> Result<(), uc_error> {
    let pstate = uc.reg_read(RegisterARM64::PSTATE)?;
    let pc = uc.pc_read()?;
    let lower = pstate & 0xf == 0;
    if lower {
        let sp = uc.reg_read(RegisterARM64::SP)?;
        uc.reg_write(RegisterARM64::SP_EL0, sp)?;
        let el1_sp = uc.reg_read(RegisterARM64::SP_EL1)?;
        uc.reg_write(RegisterARM64::SP, el1_sp)?;
    }
    uc.reg_write_arm64_coproc(&cp(0, 4, 0, 0, pstate))?;
    uc.reg_write(RegisterARM64::ELR_EL1, pc)?;
    uc.reg_write(RegisterARM64::ESR_EL1, syndrome)?;
    uc.reg_write(RegisterARM64::PSTATE, pstate & 0xf000_0000 | 0x3c5)?;
    let vbar = uc.reg_read(RegisterARM64::VBAR_EL1)?;
    // Unicorn rebuilds the execution hflags when this CP register is written.
    uc.reg_write_arm64_coproc(&cp(0, 12, 0, 0, vbar))?;
    let vector = vbar + if lower { 0x400 } else { 0x200 } + offset;
    uc.reg_write(RegisterARM64::PC, vector)
}

fn fault_address(uc: &mut Unicorn<'_, RunState>) -> u64 {
    let pc = uc.pc_read().unwrap_or(0);
    let mut bytes = [0; 4];
    if physical(uc, pc).is_ok_and(|(pa, _)| uc.mem_read(pa, &mut bytes).is_ok()) {
        let instruction = u32::from_le_bytes(bytes);
        if instruction & 0x3b00_0000 == 0x3900_0000 {
            let base = if instruction >> 5 & 31 == 31 {
                uc.reg_read(RegisterARM64::SP).unwrap_or(0)
            } else {
                let mut value = 0;
                unsafe { unicorn_engine::uc_reg_read(uc.get_handle(), RegisterARM64::X0 as i32 + (instruction >> 5 & 31) as i32, (&raw mut value).cast()) };
                value
            };
            return base.wrapping_add(u64::from(instruction >> 10 & 0xfff) << (instruction >> 30));
        }
    }
    uc.get_data().last_data
}

fn new_emu(bus: Option<Arc<Mutex<Bus>>>) -> Result<Emu, uc_error> {
    let mut emu = Unicorn::new_with_data(Arch::ARM64, Mode::LITTLE_ENDIAN, RunState { svc: None, clock: Clock { epoch: Instant::now(), virtual_time: Cell::new(false), block_ticks: Cell::new(192), ticks: Cell::new(0), core: Cell::new(0) }, dirty_pages: Vec::new(), guest_kernel: false, irq: false, fiq: false, waiting: false, cntv_ctl: 0, cntv_cval: 0, exception_taken: false, last_data: 0, bus, resume_pc: None })?;
    emu.add_intr_hook(|uc, intno| {
        if intno == EXCP_SWI {
            let mut insn = [0; 4];
            let pc = uc.pc_read().unwrap_or(0);
            let immediate = uc.mem_read(pc.wrapping_sub(4), &mut insn).map_or(0, |()| (u32::from_le_bytes(insn) >> 5) & 0xffff);
            if uc.get_data().guest_kernel {
                let _ = exception(uc, 0x15 << 26 | u64::from(immediate), 0);
                return;
            }
            if immediate != 0 || u32::from_le_bytes(insn) == 0xd400_0001 {
                uc.get_data_mut().svc = Some(immediate);
            }
        }
        if uc.get_data().guest_kernel && matches!(intno, EXCP_PREFETCH_ABORT | EXCP_DATA_ABORT) {
            let far = uc.reg_read(RegisterARM64::FAR_EL1).unwrap_or(0);
            let far = if far == 0 { fault_address(uc) } else { far };
            let ec = if intno == EXCP_PREFETCH_ABORT { 0x21 } else { 0x25 };
            let status = physical(uc, far).map_or_else(|status| status, |(_, level)| 12 + level);
            let syndrome = ec << 26 | if intno == EXCP_DATA_ABORT { 1 << 6 | status } else { status };
            let _ = uc.reg_write(RegisterARM64::FAR_EL1, far);
            let _ = exception(uc, syndrome, 0);
            return;
        }
        let _ = uc.emu_stop();
    })?;
    emu.add_insn_sys_hook_arm64(Arm64Insn::UC_ARM64_INS_MRS, 1, 0, |uc, reg, cp| {
        if uc.get_data().guest_kernel && cp.op1 != 3 && uc.reg_read(RegisterARM64::PSTATE).is_ok_and(|pstate| pstate & 0xf == 0) {
            let _ = exception(uc, 0, 0);
            return true;
        }
        if let Some(bus) = uc.get_data().bus.clone()
            && let Some(value) = bus.lock().unwrap().gic.sysreg((cp.op0, cp.op1, cp.crn, cp.crm, cp.op2), None)
        {
            let stopped = uc.reg_write(reg, value).and_then(|()| uc.emu_stop()).is_ok();
            if stopped { uc.get_data_mut().resume_pc = uc.pc_read().ok().map(|pc| pc + 4); }
            return stopped;
        }
        let data = uc.get_data();
        let value = match (cp.op0, cp.op1, cp.crn, cp.crm, cp.op2) {
            (3, 3, 14, 0, 0) => TICK_HZ,
            (3, 3, 14, 0, 1 | 2) => data.clock.now(),
            (3, 3, 14, 3, 0) => data.cntv_cval.wrapping_sub(data.clock.now()) as u32 as u64,
            (3, 3, 14, 3, 1) => data.cntv_ctl | u64::from(data.cntv_ctl & 1 != 0 && data.clock.now() >= data.cntv_cval) << 2,
            (3, 3, 14, 3, 2) => data.cntv_cval,
            (3, 0, 0, 0, 5) => 0x8000_0000 | u64::from(data.clock.core.get()),
            (3, 3, 4, 2, 0) => uc.reg_read(RegisterARM64::NZCV).unwrap_or(0),
            (3, 3, 4, 2, 1) => uc.reg_read(RegisterARM64::PSTATE).unwrap_or(0) & 0x3c0,
            (3, 3, 4, 4, 0) => uc.reg_read(RegisterARM64::FPCR).unwrap_or(0),
            (3, 3, 4, 4, 1) => uc.reg_read(RegisterARM64::FPSR).unwrap_or(0),
            (3, 3, 13, 0, 2) => uc.reg_read(RegisterARM64::TPIDR_EL0).unwrap_or(0),
            (3, 3, 13, 0, 3) => uc.reg_read(RegisterARM64::TPIDRRO_EL0).unwrap_or(0),
            (3, 0, 4, 2, 0) => uc.reg_read(RegisterARM64::PSTATE).unwrap_or(0) & 1,
            (3, 0, 4, 2, 2) => uc.reg_read(RegisterARM64::PSTATE).unwrap_or(0) & 0xc,
            (3, 0, 5, 2, 0) => uc.reg_read(RegisterARM64::ESR_EL1).unwrap_or(0),
            (3, 0, 6, 0, 0) => uc.reg_read(RegisterARM64::FAR_EL1).unwrap_or(0),
            (3, 0, 1, 0, 0) | (3, 0, 2, 0, 0..=2) | (3, 0, 4, 0, 0..=1) | (3, 0, 4, 1, 0) | (3, 0, 10, 2, 0) | (3, 0, 12, 0, 0) => {
                let mut cp = *cp;
                if uc.reg_read_arm64_coproc(&mut cp).is_err() { return false; }
                cp.val
            }
            _ => return false,
        };
        uc.reg_write(reg, value).is_ok()
    })?;
    emu.add_insn_sys_hook_arm64(Arm64Insn::UC_ARM64_INS_MSR, 1, 0, |uc, reg, cp| {
        if uc.get_data().guest_kernel && cp.op1 != 3 && uc.reg_read(RegisterARM64::PSTATE).is_ok_and(|pstate| pstate & 0xf == 0) {
            let _ = exception(uc, 0, 0);
            return true;
        }
        let now = uc.get_data().clock.now();
        if let Some(bus) = uc.get_data().bus.clone()
            && bus.lock().unwrap().gic.sysreg((cp.op0, cp.op1, cp.crn, cp.crm, cp.op2), Some(cp.val)).is_some()
        {
            let stopped = uc.emu_stop().is_ok();
            if stopped { uc.get_data_mut().resume_pc = uc.pc_read().ok().map(|pc| pc + 4); }
            return stopped;
        }
        match (cp.op0, cp.op1, cp.crn, cp.crm, cp.op2) {
            (3, 3, 4, 2, 0) => return uc.reg_write(RegisterARM64::NZCV, cp.val).is_ok(),
            (3, 3, 4, 2, 1) => {
                let pstate = uc.reg_read(RegisterARM64::PSTATE).unwrap_or(0);
                return uc.reg_write(RegisterARM64::PSTATE, pstate & !0x3c0 | cp.val & 0x3c0).is_ok();
            }
            (3, 3, 4, 4, 0) => return uc.reg_write(RegisterARM64::FPCR, cp.val).is_ok(),
            (3, 3, 4, 4, 1) => return uc.reg_write(RegisterARM64::FPSR, cp.val).is_ok(),
            (3, 3, 13, 0, 2) => return uc.reg_write(RegisterARM64::TPIDR_EL0, cp.val).is_ok(),
            (3, 3, 14, 3, 0) => uc.get_data_mut().cntv_cval = now.wrapping_add(cp.val as u32 as i32 as i64 as u64),
            (3, 3, 14, 3, 1) => uc.get_data_mut().cntv_ctl = cp.val & 3,
            (3, 3, 14, 3, 2) => uc.get_data_mut().cntv_cval = cp.val,
            (3, 0, 1, 0, 0) | (3, 0, 2, 0, 0..=2) | (3, 0, 4, 0, 0..=1) | (3, 0, 4, 1, 0) | (3, 0, 10, 2, 0) | (3, 0, 12, 0, 0) => return uc.reg_write_arm64_coproc(cp).is_ok(),
            _ => return false,
        }
        let _ = reg;
        true
    })?;
    emu.add_mem_hook(HookType::MEM_INVALID, 1, 0, |uc, kind, address, _, _| {
        if !uc.get_data().guest_kernel { return false; }
        let instruction = matches!(kind, MemType::FETCH_UNMAPPED | MemType::FETCH_PROT);
        let write = matches!(kind, MemType::WRITE_UNMAPPED | MemType::WRITE_PROT);
        let permission = matches!(kind, MemType::READ_PROT | MemType::WRITE_PROT | MemType::FETCH_PROT);
        let ec = if instruction { 0x21 } else { 0x25 };
        let status = physical(uc, address).map_or_else(|status| status, |(_, level)| if permission { 12 + level } else { 4 + level });
        let _ = uc.reg_write(RegisterARM64::FAR_EL1, address);
        let _ = exception(uc, ec << 26 | u64::from(write) << 6 | status, 0);
        uc.get_data_mut().exception_taken = true;
        let _ = uc.emu_stop();
        false
    })?;
    emu.add_mem_hook(HookType::MEM_READ | HookType::MEM_WRITE, 1, 0, |uc, _, address, _, _| {
        if uc.get_data().guest_kernel { uc.get_data_mut().last_data = address; }
        true
    })?;
    emu.mem_map(CLREX, 0x1000, Prot::READ | Prot::EXEC)?;
    emu.mem_write(CLREX, &0xd503_3f5fu32.to_le_bytes())?;
    Ok(emu)
}

impl UnicornCPU {
    /// Create a new Unicorn instance with 8MB of memory (Legacy/Test mode)
    pub fn new() -> Option<Self> {
        Self::new_with_shared_mem(0, GuestRam::new(8 * 1024 * 1024))
    }

    /// Create a new Unicorn instance mapping `ram` at guest address 0
    pub fn new_with_shared_mem(core_id: u32, ram: Arc<GuestRam>) -> Option<Self> {
        Self::with_memory(core_id, ram, 0, None, None, |_| Ok(()))
    }

    /// One core with `ram` at `ram_base` and every bus region trapping into `bus`, entered at EL1h with DAIF masked
    /// as the arm64 boot protocol requires. Unicorn resets into EL3, so SCR_EL3.{NS,RW} and HCR_EL2.RW make EL1
    /// non-secure AArch64 and CNTHCTL_EL2 stops EL2 trapping the EL1 timer.
    pub fn soc(ram: Arc<GuestRam>, ram_base: u64, bus: Arc<Mutex<Bus>>) -> Option<Self> {
        let timer_irq = IrqLine::default();
        bus.lock().unwrap().gic.connect(27, timer_irq.clone());
        Self::with_memory(0, ram, ram_base, Some(bus.clone()), Some(timer_irq), |emu| {
            for region in bus.lock().unwrap().regions() {
                let (base, reads, writes) = (region.start, bus.clone(), bus.clone());
                emu.mmio_map(base, region.end - base,
                    Some(move |_: &mut Unicorn<'_, RunState>, offset, size| reads.lock().unwrap().read(base + offset, size).unwrap_or(0)),
                    Some(move |_: &mut Unicorn<'_, RunState>, offset, size, value| { writes.lock().unwrap().write(base + offset, size, value); }))?;
            }
            emu.ctl_set_tlb_type(TlbType::CPU)?;
            emu.get_data_mut().guest_kernel = true;
            Ok(())
        })
    }

    fn with_memory(core_id: u32, ram: Arc<GuestRam>, ram_base: u64, bus: Option<Arc<Mutex<Bus>>>, timer_irq: Option<IrqLine>, map: impl FnOnce(&mut Emu) -> Result<(), uc_error>) -> Option<Self> {
        let mut emu = new_emu(bus)
            .inspect_err(|e| {
                eprintln!("Failed to create Unicorn instance for core {}: {:?}", core_id, e);
            })
            .ok()?;

        // Safety: the core holds `ram`, so the mapping outlives the Unicorn instance.
        unsafe {
            emu.mem_map_ptr(ram_base, ram.size() as u64, Prot::ALL, ram.base().cast())
                .inspect_err(|e| {
                    eprintln!("Failed to map shared memory for core {}: {:?}", core_id, e);
                })
                .ok()?;
        }
        map(&mut emu).ok()?;

        // Initialize stack pointer to end of memory, offset by core ID to avoid collision
        // Give each core 1MB of stack space at the top of memory
        let stack_top = ram_base + ram.size() as u64 - (core_id as u64 * 0x100000);
        let _ = emu.reg_write(RegisterARM64::SP, stack_top);

        let initial = emu.context_init().ok()?;
        let views = vec![emu.context_init().ok()?];
        Some(Self { emu: Arc::new(Mutex::new(Engine { emu, initial, views, active: 0, hooked: 0 })), ram, core_id, view: 0, timer_irq })
    }

    /// Locks the core and takes the RAM turn, first hooking writes to the ranges the GPU mapped since the last lock.
    fn lock(&self) -> (MutexGuard<'_, Engine>, MutexGuard<'_, ()>) {
        let mut emu = self.emu.lock().unwrap();
        let turn = self.ram.turn();
        if emu.active != self.view {
            emu.clear_exclusive().expect("CLREX");
            let Engine { emu: uc, views, active, .. } = &mut *emu;
            uc.context_save(&mut views[*active]).expect("save core");
            *active = self.view;
            uc.context_restore(&views[*active]).expect("restore core");
            uc.get_data().clock.core.set(self.core_id);
        }
        let watched = self.ram.watched();
        // One hook per GPU mapping: Unicorn tests every guest store against each hook's range, and a hit only sets page bits.
        for range in &watched[emu.hooked..] {
            let mark = |uc: &mut Unicorn<RunState>, _, pa, len: usize, _| {
                let dirty = &mut uc.get_data_mut().dirty_pages;
                for page in pages(pa, len as u64) {
                    if dirty.last() != Some(&page) { dirty.push(page); }
                }
                true
            };
            emu.add_mem_hook(HookType::MEM_WRITE, range.start, range.end - 1, mark).expect("write hook");
        }
        emu.hooked = watched.len();
        drop(watched);
        (emu, turn)
    }

    pub(super) fn logical_view(&self, core_id: u32) -> Self {
        let mut engine = self.emu.lock().unwrap();
        let mut context = engine.context_init().expect("core context");
        context.reg_write(RegisterARM64::SP, self.ram.size() as u64 - u64::from(core_id) * 0x100000).expect("core stack");
        engine.views.push(context);
        Self { emu: Arc::clone(&self.emu), ram: Arc::clone(&self.ram), core_id, view: core_id as usize, timer_irq: self.timer_irq.clone() }
    }

    #[cfg(test)]
    pub(crate) fn benchmark_cache(&self, start: u64, end: u64) {
        let mut emu = self.lock().0;
        emu.ctl_remove_cache(start, end).unwrap();
    }

    pub(crate) fn set_block_ticks(&self, ticks: u64) {
        self.lock().0.get_data().clock.block_ticks.set(ticks);
    }

    pub(crate) fn set_virtual_time(&self, enabled: bool) {
        self.lock().0.get_data().clock.virtual_time.set(enabled);
    }

    pub fn ticks(&self) -> u64 { self.lock().0.get_data().clock.now() }

    pub(crate) fn idle_until(&self, deadline: u64) {
        let mut emu = self.lock().0;
        let data = emu.get_data_mut();
        if data.clock.virtual_time.get() {
            data.clock.ticks.set(data.clock.ticks.get().max(deadline));
        } else {
            let ticks = deadline.saturating_sub(data.clock.now());
            drop(emu);
            std::thread::sleep(Duration::from_nanos((u128::from(ticks) * 1_000_000_000 / u128::from(TICK_HZ)) as u64));
        }
    }

    pub(crate) fn run_slice(&self, deadline: u64, core: usize) -> Result<bool, uc_error> {
        let (mut emu, _turn) = self.lock();
        emu.get_data_mut().svc = None;
        emu.get_data().clock.core.set(core as u32);
        let virtual_time = emu.get_data().clock.virtual_time.get();
        let remaining = deadline.saturating_sub(emu.get_data().clock.now());
        if remaining == 0 {
            if let Some(line) = &self.timer_irq { line.set(emu.get_data().cntv_ctl & 3 == 1 && emu.get_data().clock.now() >= emu.get_data().cntv_cval); }
            return Ok(true);
        }
        let (irq, fiq, waiting) = { let data = emu.get_data(); (data.irq, data.fiq, data.waiting) };
        let pstate = emu.reg_read(RegisterARM64::PSTATE)?;
        if fiq && pstate & 0x40 == 0 || irq && pstate & 0x80 == 0 {
            emu.get_data_mut().waiting = false;
            exception(&mut emu, 0, if fiq { 0x100 } else { 0x80 })?;
        } else if waiting {
            let data = emu.get_data();
            if virtual_time {
                let idle = if data.cntv_ctl & 3 == 1 && data.cntv_cval <= deadline { data.cntv_cval } else { deadline };
                data.clock.ticks.set(data.clock.ticks.get().max(idle));
            }
            if let Some(line) = &self.timer_irq { line.set(data.cntv_ctl & 3 == 1 && data.clock.now() >= data.cntv_cval); }
            return Ok(true);
        }
        let block_ticks = emu.get_data().clock.block_ticks.get();
        let count = if virtual_time && block_ticks != 0 { remaining.div_ceil(block_ticks).saturating_mul(INSTRUCTIONS_PER_BLOCK).min(usize::MAX as u64) as usize } else { 0 };
        let timeout = if virtual_time { 0 } else { (u128::from(remaining) * 1_000_000).div_ceil(u128::from(TICK_HZ)).min(u128::from(u64::MAX)) as u64 };
        let pc = emu.pc_read()?;
        let result = emu.emu_start(pc, u64::MAX, timeout, count);
        self.flush_dirty(&mut emu);
        let resume_pc = emu.get_data_mut().resume_pc.take();
        if let Some(pc) = resume_pc { emu.reg_write(RegisterARM64::PC, pc)?; }
        let exception_taken = emu.get_data().exception_taken;
        emu.get_data_mut().exception_taken = false;
        if !exception_taken { result?; }
        if emu.get_data().guest_kernel {
            let pc = emu.pc_read()?;
            let mut instruction = [0; 4];
            if physical(&mut emu, pc.wrapping_sub(4)).is_ok_and(|(pa, _)| emu.mem_read(pa, &mut instruction).is_ok()) && u32::from_le_bytes(instruction) == 0xd503_207f {
                emu.get_data_mut().waiting = true;
            }
        }
        let sliced = if virtual_time { emu.get_data().svc.is_none() } else { emu.query(Query::TIMEOUT)? != 0 };
        if virtual_time && block_ticks != 0 {
            let elapsed = if sliced && resume_pc.is_none() { remaining } else { block_ticks };
            let clock = &emu.get_data().clock;
            clock.ticks.set(clock.ticks.get().saturating_add(elapsed));
        }
        if let Some(line) = &self.timer_irq { line.set(emu.get_data().cntv_ctl & 3 == 1 && emu.get_data().clock.now() >= emu.get_data().cntv_cval); }
        Ok(sliced)
    }

    fn flush_dirty(&self, emu: &mut Engine) {
        let dirty = &mut emu.get_data_mut().dirty_pages;
        dirty.sort_unstable();
        dirty.dedup();
        for page in dirty.drain(..) { self.ram.mark_cpu_written(page * PAGE, PAGE); }
    }

    pub(crate) fn context(&self) -> Result<Context, uc_error> { self.lock().0.context_init() }

    pub(crate) fn save(&self, context: &mut Context) -> Result<(), uc_error> {
        let (mut emu, _turn) = self.lock();
        emu.clear_exclusive()?;
        emu.context_save(context)
    }

    pub(crate) fn restore(&self, context: &Context) -> Result<(), uc_error> {
        self.lock().0.context_restore(context)
    }

    pub(crate) fn thread_context(&self, entry: u64, arg: u64, stack: u64, tls: u64, handle: u32) -> Result<Context, uc_error> {
        let (emu, _turn) = self.lock();
        let previous = emu.context_init()?;
        emu.context_restore(&emu.initial)?;
        let mut context = emu.context_init()?;
        emu.context_restore(&previous)?;
        for (reg, value) in [(RegisterARM64::PC, entry), (RegisterARM64::SP, stack), (RegisterARM64::X0, arg), (RegisterARM64::X1, u64::from(handle)), (RegisterARM64::TPIDRRO_EL0, tls)] {
            context.reg_write(reg, value)?;
        }
        Ok(context)
    }

    pub(crate) fn tls(&self) -> u64 { self.lock().0.reg_read(RegisterARM64::TPIDRRO_EL0).unwrap_or(0) }

    /// Run the core until halt or breakpoint
    pub fn run(&self) -> u64 {
        let (mut emu, _turn) = self.lock();
        let pc = emu.reg_read(RegisterARM64::PC).unwrap_or(0);

        // Run until we hit a BRK instruction or error
        let result = emu.emu_start(pc, 0xFFFF_FFFF_FFFF_FFFF, 0, 0);
        self.flush_dirty(&mut emu);
        match result {
            Ok(_) => 1, // Success - normal completion
            Err(e) => {
                // BRK instruction causes an error, which is expected
                if format!("{e:?}").contains("EXCEPTION") {
                    1 // Success - terminated by BRK
                } else {
                    eprintln!("Emulation error: {e:?}");
                    0 // Failure - actual emulation error
                }
            }
        }
    }

    /// Execute a single step
    pub fn step(&self) -> u64 {
        let (mut emu, _turn) = self.lock();
        let pc = emu.reg_read(RegisterARM64::PC).unwrap_or(0);

        let result = emu.emu_start(pc, pc + 4, 0, 1);
        self.flush_dirty(&mut emu);
        match result {
            Ok(_) => 0,
            Err(_) => 1,
        }
    }

    /// Halt execution
    pub fn halt(&self) {
        let mut emu = self.lock().0;
        let _ = emu.emu_stop();
    }

    /// Read register Xn (0-30)
    pub fn get_x(&self, reg_index: u32) -> u64 {
        let emu = self.lock().0;
        if reg_index > 30 {
            return 0;
        }

        let reg = match reg_index {
            0 => RegisterARM64::X0,
            1 => RegisterARM64::X1,
            2 => RegisterARM64::X2,
            3 => RegisterARM64::X3,
            4 => RegisterARM64::X4,
            5 => RegisterARM64::X5,
            6 => RegisterARM64::X6,
            7 => RegisterARM64::X7,
            8 => RegisterARM64::X8,
            9 => RegisterARM64::X9,
            10 => RegisterARM64::X10,
            11 => RegisterARM64::X11,
            12 => RegisterARM64::X12,
            13 => RegisterARM64::X13,
            14 => RegisterARM64::X14,
            15 => RegisterARM64::X15,
            16 => RegisterARM64::X16,
            17 => RegisterARM64::X17,
            18 => RegisterARM64::X18,
            19 => RegisterARM64::X19,
            20 => RegisterARM64::X20,
            21 => RegisterARM64::X21,
            22 => RegisterARM64::X22,
            23 => RegisterARM64::X23,
            24 => RegisterARM64::X24,
            25 => RegisterARM64::X25,
            26 => RegisterARM64::X26,
            27 => RegisterARM64::X27,
            28 => RegisterARM64::X28,
            29 => RegisterARM64::X29,
            30 => RegisterARM64::X30,
            _ => return 0,
        };

        emu.reg_read(reg).unwrap_or(0)
    }

    /// Write register Xn
    pub fn set_x(&self, reg_index: u32, value: u64) {
        let mut emu = self.lock().0;
        if reg_index > 30 {
            return;
        }

        let reg = match reg_index {
            0 => RegisterARM64::X0,
            1 => RegisterARM64::X1,
            2 => RegisterARM64::X2,
            3 => RegisterARM64::X3,
            4 => RegisterARM64::X4,
            5 => RegisterARM64::X5,
            6 => RegisterARM64::X6,
            7 => RegisterARM64::X7,
            8 => RegisterARM64::X8,
            9 => RegisterARM64::X9,
            10 => RegisterARM64::X10,
            11 => RegisterARM64::X11,
            12 => RegisterARM64::X12,
            13 => RegisterARM64::X13,
            14 => RegisterARM64::X14,
            15 => RegisterARM64::X15,
            16 => RegisterARM64::X16,
            17 => RegisterARM64::X17,
            18 => RegisterARM64::X18,
            19 => RegisterARM64::X19,
            20 => RegisterARM64::X20,
            21 => RegisterARM64::X21,
            22 => RegisterARM64::X22,
            23 => RegisterARM64::X23,
            24 => RegisterARM64::X24,
            25 => RegisterARM64::X25,
            26 => RegisterARM64::X26,
            27 => RegisterARM64::X27,
            28 => RegisterARM64::X28,
            29 => RegisterARM64::X29,
            30 => RegisterARM64::X30,
            _ => return,
        };

        let _ = emu.reg_write(reg, value);
    }

    /// Read SP
    pub fn get_sp(&self) -> u64 {
        let emu = self.lock().0;
        emu.reg_read(RegisterARM64::SP).unwrap_or(0)
    }

    /// Write SP
    pub fn set_sp(&self, value: u64) {
        let mut emu = self.lock().0;
        let _ = emu.reg_write(RegisterARM64::SP, value);
    }

    /// Read PC
    pub fn get_pc(&self) -> u64 {
        let emu = self.lock().0;
        emu.reg_read(RegisterARM64::PC).unwrap_or(0)
    }

    /// Write PC
    pub fn set_pc(&self, value: u64) {
        let mut emu = self.lock().0;
        let _ = emu.reg_write(RegisterARM64::PC, value);
    }

    /// Take the immediate of the SVC that stopped the last run
    pub fn svc_pending(&self) -> Option<u32> {
        self.lock().0.get_data_mut().svc.take()
    }

    /// Write TPIDRRO_EL0, the thread-local region pointer user code reads
    pub fn set_tls(&self, value: u64) {
        let _ = self.lock().0.reg_write(RegisterARM64::TPIDRRO_EL0, value);
    }

    pub fn read_mem(&self, vaddr: u64, buf: &mut [u8]) -> Result<(), uc_error> {
        let emu = self.lock().0;
        let base = if emu.get_data().guest_kernel { MEMORY_BASE } else { 0 };
        if vaddr.checked_sub(base).and_then(|offset| offset.checked_add(buf.len() as u64)).is_none_or(|end| end > self.ram.size() as u64) {
            return Err(uc_error::READ_UNMAPPED);
        }
        emu.mem_read(vaddr, buf)
    }

    pub fn write_mem(&self, vaddr: u64, bytes: &[u8]) -> Result<(), uc_error> {
        let (mut emu, _turn) = self.lock();
        let base = if emu.get_data().guest_kernel { MEMORY_BASE } else { 0 };
        let Some(offset) = vaddr.checked_sub(base).filter(|offset| offset.checked_add(bytes.len() as u64).is_some_and(|end| end <= self.ram.size() as u64)) else {
            return Err(uc_error::WRITE_UNMAPPED);
        };
        emu.mem_write(vaddr, bytes)?;
        self.ram.mark_cpu_written(offset, bytes.len() as u64);
        Ok(())
    }

    /// Write a 32-bit value to emulated memory
    pub fn write_u32(&self, vaddr: u64, value: u32) {
        let _ = self.write_mem(vaddr, &value.to_le_bytes());
    }

    /// Read a 32-bit value from emulated memory
    pub fn read_u32(&self, vaddr: u64) -> u32 {
        let mut bytes = [0u8; 4];
        if self.read_mem(vaddr, &mut bytes).is_ok() {
            u32::from_le_bytes(bytes)
        } else {
            0
        }
    }

    /// Write a 64-bit value to emulated memory
    pub fn write_u64(&self, vaddr: u64, value: u64) {
        let _ = self.write_mem(vaddr, &value.to_le_bytes());
    }

    /// Read a 64-bit value from emulated memory
    pub fn read_u64(&self, vaddr: u64) -> u64 {
        let mut bytes = [0u8; 8];
        if self.read_mem(vaddr, &mut bytes).is_ok() {
            u64::from_le_bytes(bytes)
        } else {
            0
        }
    }
}

impl Cpu for UnicornCPU {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn run(&self) -> u64 { UnicornCPU::run(self) }
    fn step(&self) -> u64 { UnicornCPU::step(self) }
    fn halt(&self) { UnicornCPU::halt(self) }
    fn get_x(&self, index: u32) -> u64 { UnicornCPU::get_x(self, index) }
    fn set_x(&self, index: u32, value: u64) { UnicornCPU::set_x(self, index, value) }
    fn get_sp(&self) -> u64 { UnicornCPU::get_sp(self) }
    fn set_sp(&self, value: u64) { UnicornCPU::set_sp(self, value) }
    fn get_pc(&self) -> u64 { UnicornCPU::get_pc(self) }
    fn set_pc(&self, value: u64) { UnicornCPU::set_pc(self, value) }
    fn svc_pending(&self) -> Option<u32> { UnicornCPU::svc_pending(self) }
    fn set_tls(&self, value: u64) { UnicornCPU::set_tls(self, value) }
    fn tls(&self) -> u64 { UnicornCPU::tls(self) }
    fn read_mem(&self, address: u64, bytes: &mut [u8]) -> Result<(), CpuError> { UnicornCPU::read_mem(self, address, bytes).map_err(|_| CpuError) }
    fn write_mem(&self, address: u64, bytes: &[u8]) -> Result<(), CpuError> { UnicornCPU::write_mem(self, address, bytes).map_err(|_| CpuError) }
    fn ticks(&self) -> u64 { UnicornCPU::ticks(self) }
    fn idle_until(&self, deadline: u64) { UnicornCPU::idle_until(self, deadline) }
    fn run_slice(&self, deadline: u64, core: usize) -> Result<bool, CpuError> { UnicornCPU::run_slice(self, deadline, core).map_err(|_| CpuError) }
    fn context(&self) -> Result<CpuContext, CpuError> { UnicornCPU::context(self).map(CpuContext::Unicorn).map_err(|_| CpuError) }
    fn save(&self, context: &mut CpuContext) -> Result<(), CpuError> {
        let CpuContext::Unicorn(context) = context else { return Err(CpuError) };
        UnicornCPU::save(self, context).map_err(|_| CpuError)
    }
    fn restore(&self, context: &CpuContext) -> Result<(), CpuError> {
        let CpuContext::Unicorn(context) = context else { return Err(CpuError) };
        UnicornCPU::restore(self, context).map_err(|_| CpuError)
    }
    fn thread_context(&self, entry: u64, arg: u64, stack: u64, tls: u64, handle: u32) -> Result<CpuContext, CpuError> {
        UnicornCPU::thread_context(self, entry, arg, stack, tls, handle).map(CpuContext::Unicorn).map_err(|_| CpuError)
    }
    fn set_virtual_time(&self, enabled: bool) { UnicornCPU::set_virtual_time(self, enabled) }
    fn set_block_ticks(&self, ticks: u64) { UnicornCPU::set_block_ticks(self, ticks) }
    fn set_guest_kernel(&self, enabled: bool) {
        let (mut emu, _turn) = self.lock();
        if emu.get_data().guest_kernel == enabled { return; }
        let old = if enabled { 0 } else { MEMORY_BASE };
        let new = if enabled { MEMORY_BASE } else { 0 };
        emu.mem_unmap(old, self.ram.size() as u64).expect("unmap RAM");
        unsafe { emu.mem_map_ptr(new, self.ram.size() as u64, Prot::ALL, self.ram.base().cast()).expect("map RAM") };
        emu.ctl_set_tlb_type(TlbType::CPU).expect("native MMU");
        emu.get_data_mut().guest_kernel = enabled;
        let stack = new + self.ram.size() as u64 - u64::from(self.core_id) * 0x10_0000;
        emu.reg_write(RegisterARM64::SP, stack).expect("stack");
        emu.reg_write(RegisterARM64::SP_EL1, stack).expect("EL1 stack");
    }
    fn raise_irq(&self, line: u32, level: bool) {
        let mut emu = self.lock().0;
        if line == 0 { emu.get_data_mut().fiq = level } else if line == 1 { emu.get_data_mut().irq = level }
    }
}
