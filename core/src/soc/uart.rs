//! 16550-compatible UART as Tegra wires it (registers 4 bytes apart). Each byte takes one character time (10 bits
//! at the divisor-latch baud rate) in the transmit shift register before it reaches the console; the receiver takes
//! one byte per character time from the host input queue.
use super::{Device, EventQueue, IrqLine};
use crate::cpu::TICK_HZ;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// The clock-frequency the device tree gives this UART.
pub const CLOCK_HZ: u64 = 1_843_200;
const FIFO: usize = 16;
const SHIFTED: u32 = 0;
const SAMPLED: u32 = 1;

pub struct Uart {
    console: Box<dyn FnMut(u8) + Send>,
    input: Arc<Mutex<VecDeque<u8>>>,
    rx: VecDeque<u8>,
    tx: VecDeque<u8>,
    shifting: Option<u8>,
    sampling: bool,
    thre_interrupt: bool,
    overrun: bool,
    fifo: bool,
    divisor: u16,
    ier: u8,
    lcr: u8,
    mcr: u8,
    scr: u8,
    irq: IrqLine,
}

impl Uart {
    pub fn new(console: Box<dyn FnMut(u8) + Send>, input: Arc<Mutex<VecDeque<u8>>>) -> Self {
        Self { console, input, rx: VecDeque::new(), tx: VecDeque::new(), shifting: None, sampling: false, thre_interrupt: false, overrun: false, fifo: false, divisor: 0, ier: 0, lcr: 0, mcr: 0, scr: 0, irq: IrqLine::default() }
    }

    fn character_ticks(&self) -> u64 {
        10 * 16 * u64::from(self.divisor.max(1)) * TICK_HZ / CLOCK_HZ
    }
    fn depth(&self) -> usize {
        if self.fifo { FIFO } else { 1 }
    }
    fn loopback(&self) -> bool {
        self.mcr & 0x10 != 0
    }

    fn receive(&mut self, byte: u8) {
        if self.rx.len() < self.depth() { self.rx.push_back(byte) } else { self.overrun = true }
    }

    fn start_shift(&mut self, events: &mut EventQueue) {
        if self.shifting.is_none()
            && let Some(byte) = self.tx.pop_front()
        {
            self.shifting = Some(byte);
            self.thre_interrupt |= self.tx.is_empty();
            events.after(self.character_ticks(), SHIFTED);
        }
    }

    fn interrupt_id(&self) -> u8 {
        match () {
            () if self.ier & 4 != 0 && self.overrun => 0x6,
            () if self.ier & 1 != 0 && !self.rx.is_empty() => 0x4,
            () if self.ier & 2 != 0 && self.thre_interrupt => 0x2,
            () => 0x1,
        }
    }

    fn update(&mut self, events: &mut EventQueue) {
        if !self.sampling {
            self.sampling = true;
            events.after(self.character_ticks(), SAMPLED);
        }
        self.irq.set(self.interrupt_id() != 1);
    }
}

impl Device for Uart {
    fn read(&mut self, offset: u64, _: usize, events: &mut EventQueue) -> u64 {
        let dlab = self.lcr & 0x80 != 0;
        let value = match offset >> 2 {
            0 if dlab => self.divisor as u8,
            0 => self.rx.pop_front().unwrap_or(0),
            1 if dlab => (self.divisor >> 8) as u8,
            1 => self.ier,
            2 => {
                let id = self.interrupt_id();
                if id == 0x2 {
                    self.thre_interrupt = false;
                }
                id | if self.fifo { 0xc0 } else { 0 }
            }
            3 => self.lcr,
            4 => self.mcr,
            5 => {
                let empty = self.tx.is_empty();
                let lsr = u8::from(!self.rx.is_empty()) | u8::from(self.overrun) << 1 | u8::from(empty) << 5 | u8::from(empty && self.shifting.is_none()) << 6;
                self.overrun = false;
                lsr
            }
            6 if self.loopback() => (self.mcr & 2) << 3 | (self.mcr & 1) << 5 | (self.mcr & 4) << 4 | (self.mcr & 8) << 4,
            6 => 0xb0,
            7 => self.scr,
            _ => 0,
        };
        self.update(events);
        value.into()
    }

    fn write(&mut self, offset: u64, _: usize, value: u64, events: &mut EventQueue) {
        let (byte, dlab) = (value as u8, self.lcr & 0x80 != 0);
        match offset >> 2 {
            0 if dlab => self.divisor = self.divisor & 0xff00 | u16::from(byte),
            0 if self.loopback() => self.receive(byte),
            0 => {
                if self.tx.len() < self.depth() {
                    self.tx.push_back(byte);
                }
                self.thre_interrupt = false;
                self.start_shift(events);
            }
            1 if dlab => self.divisor = self.divisor & 0xff | u16::from(byte) << 8,
            1 => {
                self.thre_interrupt |= byte & 2 != 0 && self.ier & 2 == 0 && self.tx.is_empty();
                self.ier = byte & 0xf;
            }
            2 => {
                self.fifo = byte & 1 != 0;
                if byte & 2 != 0 {
                    self.rx.clear();
                }
                if byte & 4 != 0 {
                    self.tx.clear();
                }
            }
            3 => self.lcr = byte,
            4 => self.mcr = byte & 0x1f,
            7 => self.scr = byte,
            _ => {}
        }
        self.update(events);
    }

    fn event(&mut self, kind: u32, events: &mut EventQueue) {
        if kind == SHIFTED {
            let byte = self.shifting.take().expect("a byte in the shift register");
            (self.console)(byte);
            self.start_shift(events);
        } else {
            self.sampling = false;
            let byte = if self.loopback() { None } else { self.input.lock().unwrap().pop_front() };
            if let Some(byte) = byte {
                self.receive(byte);
            }
        }
        self.update(events);
    }

    fn irq_lines(&self) -> Vec<IrqLine> {
        vec![self.irq.clone()]
    }
}
