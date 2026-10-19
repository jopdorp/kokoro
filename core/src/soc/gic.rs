//! GICv3 for one core: a distributor (SPIs) at offset 0, one redistributor (SGIs and PPIs) at `GICR`, and the
//! CPU interface as system registers. A single security state (GICD_CTLR.DS = 1) with affinity routing always on,
//! which is how Linux's irq-gic-v3 drives it from non-secure EL1. Device lines are sampled into the pending state
//! after every bus access and event; group 1 signals the core's IRQ line, group 0 its FIQ line.
use super::{Device, EventQueue, IrqLine};

pub const GICR: u64 = 0x4_0000;
pub const SIZE: u64 = GICR + 0x20_0000;
const SGI: u64 = 0x1_0000;
const INTIDS: usize = 512;
const WORDS: usize = INTIDS / 32;
const SPURIOUS: u32 = 1023;
const PIDR2: u64 = 0xffe8;

#[derive(Clone, Copy)]
enum Bits {
    Group,
    Enable,
    Latch,
    Active,
    Modifier,
}

pub struct Gic {
    ctlr: u32,
    bits: [[u32; WORDS]; 5],
    levels: [u32; WORDS],
    config: [u32; INTIDS / 16],
    priority: [u8; INTIDS],
    route: [u64; INTIDS],
    inputs: Vec<(u32, IrqLine)>,
    asleep: bool,
    pmr: u8,
    bpr: [u8; 2],
    ap: [[u64; 4]; 2],
    eoi_mode: bool,
    enable: [bool; 2],
    running: Vec<u8>,
    pub fiq: IrqLine,
    pub irq: IrqLine,
}

impl Default for Gic {
    fn default() -> Self {
        let mut config = [0; INTIDS / 16];
        config[0] = 0xaaaa_aaaa;
        Self {
            ctlr: 0,
            bits: [[0; WORDS]; 5],
            levels: [0; WORDS],
            config,
            priority: [0; INTIDS],
            route: [0; INTIDS],
            inputs: Vec::new(),
            asleep: true,
            pmr: 0,
            bpr: [0, 1],
            ap: [[0; 4]; 2],
            eoi_mode: false,
            enable: [false; 2],
            running: Vec::new(),
            fiq: IrqLine::default(),
            irq: IrqLine::default(),
        }
    }
}

impl Gic {
    pub fn connect(&mut self, intid: u32, line: IrqLine) {
        self.inputs.push((intid, line));
    }

    fn bit(&self, bits: Bits, intid: usize) -> bool {
        self.bits[bits as usize][intid / 32] >> (intid % 32) & 1 != 0
    }
    fn set(&mut self, bits: Bits, intid: usize, on: bool) {
        let word = &mut self.bits[bits as usize][intid / 32];
        *word = *word & !(1 << (intid % 32)) | u32::from(on) << (intid % 32);
    }
    fn edge(&self, intid: usize) -> bool {
        self.config[intid / 16] >> (intid % 16 * 2 + 1) & 1 != 0
    }
    fn pending(&self, intid: usize) -> bool {
        self.bit(Bits::Latch, intid) || !self.edge(intid) && self.levels[intid / 32] >> (intid % 32) & 1 != 0
    }
    fn group_mask(&self, group: usize) -> u8 {
        0xffu8.checked_shl(u32::from(self.bpr[group]) + 1).unwrap_or(0)
    }

    /// The highest-priority pending interrupt of `group`, and whether it may preempt the core now.
    fn best(&self, group: usize) -> Option<(usize, bool)> {
        if !self.enable[group] || self.ctlr >> group & 1 == 0 {
            return None;
        }
        let intid = (0..INTIDS).filter(|&i| self.bit(Bits::Group, i) as usize == group && self.bit(Bits::Enable, i) && self.pending(i) && !self.bit(Bits::Active, i)).min_by_key(|&i| self.priority[i])?;
        let priority = self.priority[intid];
        let mask = self.group_mask(group);
        Some((intid, priority < self.pmr && self.running.last().is_none_or(|&running| priority & mask < running)))
    }

    pub fn sample(&mut self) {
        for (intid, line) in &self.inputs {
            let (word, bit) = (*intid as usize / 32, 1 << (intid % 32));
            let level = line.level();
            if level && self.levels[word] & bit == 0 && self.edge(*intid as usize) {
                self.bits[Bits::Latch as usize][word] |= bit;
            }
            self.levels[word] = self.levels[word] & !bit | if level { bit } else { 0 };
        }
        self.fiq.set(self.best(0).is_some_and(|(_, signal)| signal));
        self.irq.set(self.best(1).is_some_and(|(_, signal)| signal));
    }

    fn acknowledge(&mut self, group: usize) -> u64 {
        let Some((intid, true)) = self.best(group) else {
            return SPURIOUS.into();
        };
        self.set(Bits::Latch, intid, false);
        self.set(Bits::Active, intid, true);
        self.running.push(self.priority[intid] & self.group_mask(group));
        intid as u64
    }

    fn end(&mut self, intid: u64) {
        if intid < INTIDS as u64 {
            self.running.pop();
            if !self.eoi_mode {
                self.set(Bits::Active, intid as usize, false);
            }
        }
    }

    /// An ICC_* access by (op0, op1, CRn, CRm, op2); None when the encoding is not a GIC CPU interface register.
    pub fn sysreg(&mut self, encoding: (u32, u32, u32, u32, u32), write: Option<u64>) -> Option<u64> {
        let (3, 0, crn, crm, op2) = encoding else {
            return None;
        };
        let value = write.unwrap_or(0);
        let read = match (crn, crm, op2) {
            (4, 6, 0) => {
                if write.is_some() {
                    self.pmr = value as u8;
                }
                self.pmr.into()
            }
            (12, 8, 0) if write.is_none() => self.acknowledge(0),
            (12, 12, 0) if write.is_none() => self.acknowledge(1),
            (12, 8, 1) | (12, 12, 1) if write.is_some() => { self.end(value & 0xff_ffff); 0 }
            (12, 11, 1) if write.is_some() => {
                if value < INTIDS as u64 {
                    self.set(Bits::Active, value as usize, false);
                }
                0
            }
            (12, 8, 2) if write.is_none() => self.best(0).map_or(SPURIOUS.into(), |(intid, _)| intid as u64),
            (12, 12, 2) if write.is_none() => self.best(1).map_or(SPURIOUS.into(), |(intid, _)| intid as u64),
            (12, 8, 3) | (12, 12, 3) => {
                let group = (crm == 12) as usize;
                if write.is_some() {
                    self.bpr[group] = value as u8 & 7;
                }
                self.bpr[group].into()
            }
            (12, 11, 3) if write.is_none() => self.running.last().map_or(0xff, |&running| running).into(),
            (12, 12, 4) => {
                if write.is_some() {
                    self.eoi_mode = value & 2 != 0;
                }
                7 << 8 | u64::from(self.eoi_mode) << 1
            }
            (12, 12, 5) => 7,
            (12, 12, 6 | 7) => {
                let group = op2 as usize - 6;
                if write.is_some() {
                    self.enable[group] = value & 1 != 0;
                }
                self.enable[group].into()
            }
            (12, 11, 5..=7) if write.is_some() => {
                if value & 0x00ff_01ff_00ff_0000 == 0 && value & 1 != 0 {
                    self.set(Bits::Latch, (value >> 24 & 15) as usize, true);
                }
                0
            }
            (12, 8, 4..=7) | (12, 9, 0..=3) => {
                let (group, register) = if crm == 8 { (0, op2 - 4) } else { (1, op2) };
                if write.is_some() { self.ap[group][register as usize] = value; }
                self.ap[group][register as usize]
            }
            _ => return None,
        };
        self.sample();
        Some(read)
    }

    /// The interrupt-state register blocks shared by GICD (SPIs) and the redistributor's SGI frame (SGIs and PPIs).
    fn state(&mut self, offset: u64, size: usize, write: Option<u64>, private: bool) -> u64 {
        let word = (offset as usize & 0x7f) / 4;
        let owned = |first: usize| {
            if private { first < 32 } else { (32..INTIDS).contains(&first) }
        };
        let bits = match offset {
            0x80..0x100 => Bits::Group,
            0x100..0x200 => Bits::Enable,
            0x200..0x300 => Bits::Latch,
            0x300..0x400 => Bits::Active,
            _ => Bits::Modifier,
        };
        match offset {
            0x80..0x400 | 0xd00..0xd80 if owned(word * 32) => {
                let asserted = if matches!(bits, Bits::Latch) { self.levels[word] & !self.edge_word(word) } else { 0 };
                let current = &mut self.bits[bits as usize][word];
                match (bits, write) {
                    (_, None) => {}
                    (Bits::Group | Bits::Modifier, Some(value)) => *current = value as u32,
                    (_, Some(value)) if offset & 0x80 != 0 => *current &= !(value as u32),
                    (_, Some(value)) => *current |= value as u32,
                }
                (*current | asserted).into()
            }
            0x400..0x800 if owned(offset as usize - 0x400) => {
                let first = offset as usize - 0x400;
                let bytes = &mut self.priority[first..(first + size).min(INTIDS)];
                if let Some(value) = write {
                    bytes.iter_mut().enumerate().for_each(|(i, p)| *p = (value >> (i * 8)) as u8);
                }
                bytes.iter().rev().fold(0, |acc, &p| acc << 8 | u64::from(p))
            }
            0xc00..0xd00 if owned((offset as usize - 0xc00) * 4) => {
                let index = (offset as usize - 0xc00) / 4;
                if let (Some(value), true) = (write, index > 0) {
                    self.config[index] = value as u32 & 0xaaaa_aaaa;
                }
                self.config[index].into()
            }
            _ => 0,
        }
    }

    fn edge_word(&self, word: usize) -> u32 {
        (0..32).filter(|bit| self.edge(word * 32 + bit)).fold(0, |acc, bit| acc | 1 << bit)
    }

    fn access(&mut self, offset: u64, size: usize, write: Option<u64>) -> u64 {
        let read = match offset {
            0x0 => {
                if let Some(value) = write {
                    self.ctlr = value as u32 & 3;
                }
                u64::from(self.ctlr) | 0x50
            }
            0x4 => (WORDS as u64 - 1) | 8 << 19,
            0x8 | GICR_IIDR => 0x43b,
            0x6100..0x7000 => {
                let (intid, high) = ((offset as usize - 0x6000) / 8, offset & 4 != 0);
                if let Some(value) = write {
                    self.route[intid] = if size == 8 {
                        value
                    } else if high {
                        self.route[intid] as u32 as u64 | value << 32
                    } else {
                        self.route[intid] & !0xffff_ffff | value
                    };
                }
                if high { self.route[intid] >> 32 } else { self.route[intid] }
            }
            PIDR2 | GICR_PIDR2 => 0x3b,
            GICR_TYPER => 1 << 4,
            GICR_WAKER => {
                if let Some(value) = write {
                    self.asleep = value & 2 != 0;
                }
                u64::from(self.asleep) * 6
            }
            0x80..0x1000 => self.state(offset, size, write, false),
            GICR_SGI..GICR_SGI_END => self.state(offset - GICR - SGI, size, write, true),
            _ => 0,
        };
        self.sample();
        read
    }
}

const GICR_IIDR: u64 = GICR + 4;
const GICR_TYPER: u64 = GICR + 8;
const GICR_WAKER: u64 = GICR + 0x14;
const GICR_PIDR2: u64 = GICR + PIDR2;
const GICR_SGI: u64 = GICR + SGI + 0x80;
const GICR_SGI_END: u64 = GICR + SGI + 0x1000;

impl Device for Gic {
    fn read(&mut self, offset: u64, size: usize, _: &mut EventQueue) -> u64 {
        self.access(offset, size, None)
    }
    fn write(&mut self, offset: u64, size: usize, value: u64, _: &mut EventQueue) {
        self.access(offset, size, Some(value));
    }
}
