//! SoC mode: guest-physical addresses route to RAM at `RAM_BASE` or to a device's register block, devices drive
//! interrupt lines into the GIC, and device work that takes emulated time (a UART character, a DMA transfer) runs
//! from one event queue ordered by the core's 19.2 MHz tick clock, so a run is deterministic under virtual time.
//! Addresses and interrupt numbers follow mainline Linux's tegra234.dtsi; `t234-min.dts` describes exactly these.
pub mod gic;
pub mod uart;

use crate::cpu::{Cpu, DynarmicCPU, DynarmicGroup, GuestRam, UnicornCPU};
use crate::cpu::cpu_manager::{CpuBackend, CpuManager};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::io::{BufReader, Read, Write};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const RAM_BASE: u64 = 0x8000_0000;
pub const RAM_SIZE: usize = 0x4000_0000;
pub const GIC_BASE: u64 = 0x0f40_0000;
pub const UART_A: u64 = 0x0310_0000;
pub const UART_A_SPI: u32 = 112;
const DTB_OFFSET: u64 = 0x0400_0000;
const INITRD_OFFSET: u64 = 0x0420_0000;
const SLICE_TICKS: u64 = 0x1000;

/// A level-sensitive wire: the driving side sets it, the receiving side samples it.
#[derive(Clone, Default)]
pub struct IrqLine(Arc<AtomicBool>);

impl IrqLine {
    pub fn set(&self, level: bool) {
        self.0.store(level, Ordering::Relaxed)
    }
    pub fn level(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
/// Device callbacks in emulated-tick order; callbacks due at the same tick run in the order they were scheduled.
#[derive(Default)]
pub struct EventQueue {
    due: BinaryHeap<Reverse<(u64, u64, usize, u32)>>,
    scheduled: u64,
    now: u64,
    device: usize,
}

impl EventQueue {
    pub fn now(&self) -> u64 {
        self.now
    }

    /// Calls the current device's `event(kind)` `delay` ticks from now.
    pub fn after(&mut self, delay: u64, kind: u32) {
        self.scheduled += 1;
        self.due.push(Reverse((self.now + delay, self.scheduled, self.device, kind)));
    }

    fn next(&self) -> Option<u64> {
        self.due.peek().map(|Reverse(event)| event.0)
    }

    fn pop(&mut self, until: u64) -> Option<(usize, u32)> {
        self.due.peek().filter(|Reverse(event)| event.0 <= until)?;
        let Reverse((at, _, device, kind)) = self.due.pop()?;
        (self.now, self.device) = (at, device);
        Some((device, kind))
    }
}
pub trait Device: Send {
    fn read(&mut self, offset: u64, size: usize, events: &mut EventQueue) -> u64;
    fn write(&mut self, offset: u64, size: usize, value: u64, events: &mut EventQueue);
    fn event(&mut self, _kind: u32, _events: &mut EventQueue) {}
    fn irq_lines(&self) -> Vec<IrqLine> {
        Vec::new()
    }
}

pub struct Bus {
    ram: Arc<GuestRam>,
    ram_base: u64,
    pub gic: gic::Gic,
    devices: Vec<(Range<u64>, Box<dyn Device>)>,
    events: EventQueue,
}

impl Bus {
    pub fn new(ram: Arc<GuestRam>, ram_base: u64) -> Self {
        Self { ram, ram_base, gic: gic::Gic::default(), devices: Vec::new(), events: EventQueue::default() }
    }

    /// Maps `device` at `base` and wires its interrupt lines to consecutive INTIDs from `intid`.
    pub fn attach(&mut self, base: u64, len: u64, device: Box<dyn Device>, intid: u32) {
        for (line, intid) in device.irq_lines().into_iter().zip(intid..) {
            self.gic.connect(intid, line);
        }
        self.devices.push((base..base + len, device));
    }

    pub fn regions(&self) -> Vec<Range<u64>> {
        std::iter::once(GIC_BASE..GIC_BASE + gic::SIZE).chain(self.devices.iter().map(|(range, _)| range.clone())).collect()
    }

    fn ram_offset(&self, pa: u64, size: usize) -> Option<usize> {
        let offset = usize::try_from(pa.checked_sub(self.ram_base)?).ok()?;
        offset.checked_add(size).filter(|&end| end <= self.ram.size()).map(|_| offset)
    }

    fn device<T>(&mut self, pa: u64, size: usize, access: impl FnOnce(&mut dyn Device, u64, &mut EventQueue) -> T) -> Option<T> {
        let end = pa.checked_add(size as u64)?;
        let result = if pa >= GIC_BASE && end <= GIC_BASE + gic::SIZE {
            access(&mut self.gic, pa - GIC_BASE, &mut self.events)
        } else {
            let (index, (range, device)) = self.devices.iter_mut().enumerate().find(|(_, (range, _))| pa >= range.start && end <= range.end)?;
            self.events.device = index;
            access(device.as_mut(), pa - range.start, &mut self.events)
        };
        self.gic.sample();
        Some(result)
    }

    /// A load of `size` bytes, or None when nothing decodes `pa` (an external abort on hardware).
    pub fn read(&mut self, pa: u64, size: usize) -> Option<u64> {
        if !matches!(size, 1 | 2 | 4 | 8) {
            return None;
        }
        let Some(offset) = self.ram_offset(pa, size) else {
            return self.device(pa, size, |device, offset, events| device.read(offset, size, events));
        };
        let mut bytes = [0; 8];
        self.ram.read(offset as u64, &mut bytes[..size]);
        Some(u64::from_le_bytes(bytes))
    }

    pub fn write(&mut self, pa: u64, size: usize, value: u64) -> Option<()> {
        if !matches!(size, 1 | 2 | 4 | 8) {
            return None;
        }
        let Some(offset) = self.ram_offset(pa, size) else {
            return self.device(pa, size, |device, offset, events| device.write(offset, size, value, events));
        };
        self.ram.write(offset as u64, &value.to_le_bytes()[..size]);
        Some(())
    }

    /// Runs every event due by `now` and returns when the next one is due.
    pub fn advance(&mut self, now: u64, mut raise_irq: impl FnMut(u32, bool)) -> Option<u64> {
        self.gic.sample();
        raise_irq(0, self.gic.fiq.level());
        raise_irq(1, self.gic.irq.level());
        while let Some((index, kind)) = self.events.pop(now) {
            self.events.device = index;
            self.devices[index].1.event(kind, &mut self.events);
            self.gic.sample();
            raise_irq(0, self.gic.fiq.level());
            raise_irq(1, self.gic.irq.level());
        }
        self.events.now = self.events.now.max(now);
        self.events.next()
    }
}

/// Points the /chosen linux,initrd-start and linux,initrd-end properties the DTB already carries at the initramfs.
fn set_initrd(dtb: &mut [u8], start: u64, end: u64) -> Option<()> {
    let word = |dtb: &[u8], at: usize| Some(u32::from_be_bytes(dtb.get(at..at + 4)?.try_into().ok()?) as usize);
    let (strings, mut at, mut found) = (word(dtb, 12)?, word(dtb, 8)?, 0);
    loop {
        match word(dtb, at)? {
            1 => at += 4 + ((dtb.get(at + 4..)?.iter().position(|&byte| byte == 0)? + 4) & !3),
            2 | 4 => at += 4,
            3 => {
                let (len, name) = (word(dtb, at + 4)?, &dtb.get(strings + word(dtb, at + 8)?..)?);
                let value = match &name[..name.iter().position(|&byte| byte == 0)?] {
                    b"linux,initrd-start" => Some(start),
                    b"linux,initrd-end" => Some(end),
                    _ => None,
                };
                if let Some(value) = value.filter(|_| len <= 8) {
                    dtb.get_mut(at + 12..at + 12 + len)?.copy_from_slice(&value.to_be_bytes()[8 - len..]);
                    found += 1;
                }
                at += 12 + ((len + 3) & !3);
            }
            9 => return (found == 2).then_some(()),
            _ => return None,
        }
    }
}

/// Lays the Image, DTB and initramfs out in RAM per the arm64 boot protocol and returns (entry, DTB address).
pub fn load(ram: &GuestRam, kernel: &[u8], dtb: &mut [u8], initrd: &[u8]) -> Result<(u64, u64), String> {
    let header = |at: usize| kernel.get(at..at + 8).map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()));
    if kernel.get(0x38..0x3c) != Some(b"ARM\x64") {
        return Err("not an arm64 Linux Image".into());
    }
    let (text_offset, image_size) = (header(8).unwrap_or(0), header(16).unwrap_or(0).max(kernel.len() as u64));
    if text_offset.checked_add(image_size).is_none_or(|end| end > DTB_OFFSET) || dtb.len() > 0x20_0000 {
        return Err("the Image or DTB does not fit the boot layout".into());
    }
    let initrd_start = RAM_BASE + INITRD_OFFSET;
    set_initrd(dtb, initrd_start, initrd_start + initrd.len() as u64).ok_or("the DTB needs /chosen linux,initrd-start and linux,initrd-end")?;
    crate::guest::load_segments(ram, 0, &[(text_offset, kernel), (DTB_OFFSET, dtb), (INITRD_OFFSET, initrd)]).ok_or("the boot files do not fit RAM")?;
    Ok((RAM_BASE + text_offset, RAM_BASE + DTB_OFFSET))
}

/// Boots a kernel on one core with the console on UART A, stdin feeding its receiver; returns when the core stops.
pub fn boot(kernel: &std::path::Path, dtb: &std::path::Path, initrd: &std::path::Path) -> Result<(), String> {
    let read = |path: &std::path::Path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    let ram = GuestRam::new(RAM_SIZE);
    let (entry, fdt) = load(&ram, &read(kernel)?, &mut read(dtb)?, &read(initrd)?)?;
    let input = Arc::new(Mutex::new(VecDeque::new()));
    let stdin = input.clone();
    std::thread::spawn(move || BufReader::new(std::io::stdin()).bytes().map_while(Result::ok).for_each(|byte| stdin.lock().unwrap().push_back(byte)));
    let console = |byte| {
        let mut out = std::io::stdout();
        let _ = out.write_all(&[byte]).and_then(|()| out.flush());
    };
    let mut bus = Bus::new(ram.clone(), RAM_BASE);
    bus.attach(UART_A, 0x1_0000, Box::new(uart::Uart::new(Box::new(console), input)), 32 + UART_A_SPI);
    let bus = Arc::new(Mutex::new(bus));
    let cpu: Box<dyn Cpu> = match CpuManager::default_backend() {
        CpuBackend::Unicorn => Box::new(UnicornCPU::soc(ram, RAM_BASE, bus.clone()).ok_or("CPU engine")?),
        CpuBackend::Dynarmic => {
            let group = DynarmicGroup::new().ok_or("Dynarmic monitor")?;
            Box::new(DynarmicCPU::soc(&group, 0, ram, bus.clone()).ok_or("CPU engine")?)
        }
    };
    cpu.set_virtual_time(true);
    for (register, value) in [(0, fdt), (1, 0), (2, 0), (3, 0)] {
        cpu.set_x(register, value);
    }
    cpu.set_pc(entry);
    loop {
        let now = cpu.ticks();
        let next = bus.lock().unwrap().advance(now, |line, level| cpu.raise_irq(line, level)).unwrap_or(u64::MAX).min(now + SLICE_TICKS);
        match cpu.run_slice(next, 0) {
            Ok(true) => {}
            stopped => {
                return Err(format!("core stopped at pc {:#x} ({})", cpu.get_pc(), if stopped.is_ok() { "exception" } else { "fault" }));
            }
        }
    }
}

