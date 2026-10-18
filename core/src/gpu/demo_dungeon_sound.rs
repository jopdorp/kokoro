//! Recorded dungeon sources are spatialized on the host in renderer-sized blocks, then the stereo pair is submitted
//! as ordinary PCM voices through audren.
use super::demo_dungeon::{Sfx, V3, World};
use crate::audio::HostMemory;
use crate::audio::parcel::put;
use crate::audio::renderer::{AudioRendererParameter, Renderer, work_buffer_size};
use crate::audio::spatial::{BLOCK, Mix, Mode, Source, Spatializer};
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const BASE: u64 = 0x10000;
const MAGIC: u32 = 0x56_4552 | (0x30 + 13) << 24;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sample {
    Room,
    Torch,
    Fire,
    River,
    Gurgle,
    Wind,
    Heart,
    Creak,
    Bats,
    Drip,
    Scream,
    Step,
    WetStep,
    GravelStep,
    WoodStep,
}

impl From<Sfx> for Sample {
    fn from(sfx: Sfx) -> Sample {
        match sfx {
            Sfx::Torch => Sample::Torch,
            Sfx::Fire => Sample::Fire,
            Sfx::River => Sample::River,
            Sfx::Wind => Sample::Wind,
            Sfx::Heart => Sample::Heart,
            Sfx::Creak => Sample::Creak,
            Sfx::Bats => Sample::Bats,
        }
    }
}

fn sample(kind: Sample) -> Vec<f32> {
    let name = match kind {
        Sample::Room => "room",
        Sample::Torch => "torch",
        Sample::Fire => "fire",
        Sample::River => "river",
        Sample::Gurgle => "gurgle",
        Sample::Wind => "wind",
        Sample::Heart => "heart",
        Sample::Creak => "creak",
        _ => return vec![0.0],
    };
    pcm(name)
}

fn pcm(name: &str) -> Vec<f32> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../demos/assets/dungeon-sound")
        .join(format!("{name}.pcm"));
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run demos/assets/fetch.sh", path.display()))
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes(bytes.try_into().unwrap()) as f32 / 32768.0)
        .collect()
}

const SAMPLES: [Sample; 15] = [
    Sample::Room,
    Sample::Torch,
    Sample::Fire,
    Sample::River,
    Sample::Gurgle,
    Sample::Wind,
    Sample::Heart,
    Sample::Creak,
    Sample::Bats,
    Sample::Drip,
    Sample::Scream,
    Sample::Step,
    Sample::WetStep,
    Sample::GravelStep,
    Sample::WoodStep,
];

fn looping(kind: Sample) -> bool {
    matches!(
        kind,
        Sample::Room
            | Sample::Torch
            | Sample::Fire
            | Sample::River
            | Sample::Gurgle
            | Sample::Wind
            | Sample::Heart
            | Sample::Bats
    )
}

fn wanted(kind: Sample, solo: u8) -> bool {
    solo == 0
        || matches!(
            (solo, kind),
            (1, Sample::Room)
                | (2, Sample::River | Sample::Gurgle)
                | (3, Sample::Torch | Sample::Fire)
        )
}

fn beat_gain(time: f32, start: f32, end: f32) -> f32 {
    ((time - start) / 0.8).clamp(0.0, 1.0) * ((end - time) / 0.8).clamp(0.0, 1.0)
}

struct Voice {
    sample: Sample,
    pos: V3,
    gain: f32,
    pitch: f32,
    cursor: f32,
    active: bool,
    tail: usize,
    variation: usize,
}

pub const DRIPS: usize = 6;

struct Engine {
    renderer: Renderer,
    memory: HostMemory,
    samples: Vec<Vec<f32>>,
    drips: Vec<Vec<f32>>,
    steps: Vec<Vec<f32>>,
    voices: Vec<Voice>,
    spatial: Spatializer,
    configured: bool,
    next_drip: usize,
    next_step: usize,
    creak: Option<usize>,
    time: f32,
    solo: u8,
    master_gain: f32,
    output: Vec<u8>,
}

static MODE: AtomicU8 = AtomicU8::new(0);

pub fn set_mode(mode: Mode) {
    MODE.store(u8::from(mode == Mode::Speakers), Ordering::Relaxed);
}

impl Engine {
    /// The renderer with one looping voice per source, then the drip and footstep voices.
    fn new(world: &World, sources: &[(Sfx, V3, f32)]) -> Engine {
        let samples = SAMPLES.map(sample).into();
        let drips = (0..4).map(|i| pcm(&format!("drip-{i}"))).collect();
        let surfaces = ["wet", "wet", "gravel", "wood"];
        let steps = (0..32)
            .map(|i| pcm(&format!("step-{}-{}", surfaces[i / 8], i % 8)))
            .collect();
        let rooms = world.acoustic_rooms();
        let cave = &rooms[world.cave];
        let centre = |min: V3, max: V3| std::array::from_fn(|i| (min[i] + max[i]) * 0.5);
        let cave_pos = centre(cave.min, cave.max);
        let doorway = cave
            .doorways
            .first()
            .map_or(cave_pos, |door| centre(door.min, door.max));
        let river = sources
            .iter()
            .find(|source| source.0 == Sfx::River)
            .map_or(cave_pos, |source| {
                [source.1[0], source.1[1], source.1[2] + 1.5]
            });
        let solo = match std::env::var("OBOROMI_DUNGEON_SOLO").as_deref() {
            Ok("room") => 1,
            Ok("river") => 2,
            Ok("torch") => 3,
            Ok("steps") => 4,
            Ok("drips") => 5,
            _ => 0,
        };
        let mut voices: Vec<Voice> = sources
            .iter()
            .enumerate()
            .map(|(i, &(sfx, pos, gain))| {
                let sample = sfx.into();
                let level = match sample {
                    Sample::Torch => 2.0,
                    Sample::Fire => 1.2,
                    Sample::River => 2.0,
                    Sample::Wind => 0.4,
                    Sample::Heart => 0.03,
                    Sample::Creak => 0.5,
                    _ => 0.0,
                };
                Voice {
                    sample,
                    pos: if matches!(sample, Sample::Wind | Sample::Heart) {
                        doorway
                    } else {
                        pos
                    },
                    gain: gain * level * if wanted(sample, solo) { 1.0 } else { 0.0 },
                    pitch: 0.97 + 0.06 * ((i * 7) % 11) as f32 / 10.0,
                    cursor: 0.0,
                    active: sample != Sample::Creak,
                    tail: usize::from(sample != Sample::Creak) * usize::MAX,
                    variation: 0,
                }
            })
            .collect();
        let creak = voices
            .iter()
            .position(|voice| voice.sample == Sample::Creak);
        voices.push(Voice {
            sample: Sample::Room,
            pos: cave_pos,
            gain: if wanted(Sample::Room, solo) { 1.0 } else { 0.0 },
            pitch: 1.0,
            cursor: 0.0,
            active: true,
            tail: usize::MAX,
            variation: 0,
        });
        voices.push(Voice {
            sample: Sample::Gurgle,
            pos: river,
            gain: if wanted(Sample::Gurgle, solo) {
                1.5
            } else {
                0.0
            },
            pitch: 1.0,
            cursor: 0.0,
            active: true,
            tail: usize::MAX,
            variation: 0,
        });
        for sample in [
            [Sample::Drip; DRIPS].as_slice(),
            &[Sample::Step, Sample::Step],
        ]
        .concat()
        {
            voices.push(Voice {
                sample,
                pos: [0.0; 3],
                gain: 0.0,
                pitch: 1.0,
                cursor: 0.0,
                active: false,
                tail: 0,
                variation: 0,
            });
        }
        let parameter = AudioRendererParameter {
            sample_rate: 48_000,
            sample_count: BLOCK as u32,
            mix_buffer_count: 2,
            voice_count: 2,
            sink_count: 1,
            revision: MAGIC,
            ..Default::default()
        };
        let mut work = vec![0; work_buffer_size(&parameter).unwrap() as usize];
        let mut renderer = Renderer::open(parameter, &mut work).unwrap();
        renderer.start();
        Engine {
            renderer,
            memory: HostMemory {
                base: BASE,
                bytes: vec![0; 0x1000],
            },
            samples,
            drips,
            steps,
            voices,
            spatial: Spatializer::new(&rooms, Mode::Headphones),
            configured: false,
            next_drip: 0,
            next_step: 0,
            creak,
            time: 0.0,
            solo,
            master_gain: 1.0,
            output: vec![0; 0x2_0000],
        }
    }

    pub fn move_source(&mut self, i: usize, pos: V3) {
        self.voices[i].pos = pos;
    }

    /// Plays a recorded drip or footstep at `pos`.
    pub fn play(&mut self, mut sample: Sample, pos: V3, gain: f32, pitch: f32) {
        if sample == Sample::Scream
            || self.solo != 0
                && !matches!(
                    (self.solo, sample),
                    (4, Sample::Step | Sample::WetStep | Sample::GravelStep) | (5, Sample::Drip)
                )
        {
            return;
        }
        let first = self.voices.len() - DRIPS - 2;
        let drip = sample == Sample::Drip;
        let slot = match sample {
            Sample::Drip => {
                self.next_drip = (self.next_drip + 1) % DRIPS;
                first + self.next_drip
            }
            _ => {
                self.next_step += 1;
                first + DRIPS + (self.next_step & 1)
            }
        };
        let variation = if drip {
            self.next_drip % 4
        } else {
            self.next_step * 5 % 8
        };
        let random_pitch = [0.97, 1.02, 0.99, 1.03, 1.0, 0.98, 1.01, 0.995][variation];
        let decibels = [-1.5, 1.0, -0.5, 1.5, 0.0, 0.5, -1.0, 0.25][variation];
        if !drip && self.time >= 54.0 {
            sample = Sample::WoodStep;
        }
        let level = if drip {
            if (26.0..=40.0).contains(&self.time) {
                0.45
            } else {
                0.14
            }
        } else {
            10.0f32.powf(decibels / 20.0)
        };
        self.voices[slot] = Voice {
            sample,
            pos,
            gain: gain * level,
            pitch: if drip {
                random_pitch
            } else {
                pitch * random_pitch
            },
            cursor: 0.0,
            active: true,
            tail: 800,
            variation,
        };
    }

    fn listen(&mut self, eye: V3, yaw: f32, dt: f32) {
        let previous = self.time;
        self.time += dt;
        if previous < 22.0 && self.time >= 22.0 {
            if let Some(creak) = self.creak {
                self.voices[creak].active = true;
                self.voices[creak].cursor = 0.0;
                self.voices[creak].tail = 800;
            }
        }
        self.spatial.set_mode(if MODE.load(Ordering::Relaxed) == 0 {
            Mode::Headphones
        } else {
            Mode::Speakers
        });
        self.spatial
            .set_listener(eye, [yaw.sin(), 0.0, -yaw.cos()], [0.0, 1.0, 0.0]);
    }

    /// Sends one update and renders `periods` periods of interleaved stereo.
    pub fn render(&mut self, periods: usize) -> Vec<i16> {
        if !self.configured {
            let parcel = self.parcel();
            self.renderer
                .request_update(&parcel, &mut self.output)
                .unwrap();
            self.configured = true;
        }
        let mut output = Vec::with_capacity(periods * BLOCK * 2);
        for _ in 0..periods {
            let mut mix = Mix::new();
            self.spatial.begin_block();
            for (id, voice) in self
                .voices
                .iter_mut()
                .enumerate()
                .filter(|(_, voice)| voice.active || voice.tail > 0)
            {
                let pcm = if voice.sample == Sample::Drip {
                    &self.drips[voice.variation]
                } else if matches!(
                    voice.sample,
                    Sample::Step | Sample::WetStep | Sample::GravelStep | Sample::WoodStep
                ) {
                    &self.steps
                        [(voice.sample as usize - Sample::Step as usize) * 8 + voice.variation]
                } else {
                    &self.samples[voice.sample as usize]
                };
                let mut mono = [0.0; BLOCK];
                for sample in &mut mono {
                    if !voice.active {
                        break;
                    }
                    let at = voice.cursor as usize;
                    if at >= pcm.len() {
                        if looping(voice.sample) {
                            voice.cursor %= pcm.len() as f32;
                        } else {
                            voice.active = false;
                            break;
                        }
                    }
                    let at = voice.cursor as usize;
                    let next = (at + 1).min(pcm.len() - 1);
                    *sample = pcm[at] + (pcm[next] - pcm[at]) * voice.cursor.fract();
                    voice.cursor += voice.pitch;
                }
                if !voice.active {
                    voice.tail = voice.tail.saturating_sub(1);
                }
                let mut stereo = [[0.0; 2]; BLOCK];
                let gain = voice.gain
                    * match voice.sample {
                        Sample::Heart | Sample::Wind => beat_gain(self.time, 18.0, 26.0),
                        Sample::River | Sample::Gurgle if self.time < 26.0 => 0.12,
                        Sample::River | Sample::Gurgle if self.time < 40.0 => {
                            0.12 + 0.88 * (self.time - 26.0) / 14.0
                        }
                        Sample::River | Sample::Gurgle if self.time < 54.0 => {
                            (1.0 - 0.72 * (self.time - 40.0) / 1.5).max(0.28)
                        }
                        Sample::River | Sample::Gurgle => 0.14,
                        Sample::Room if (18.0..26.0).contains(&self.time) => 0.45,
                        _ => 1.0,
                    };
                self.spatial.render(
                    &Source {
                        id,
                        position: voice.pos,
                        gain,
                    },
                    &mono,
                    &mut stereo,
                );
                mix.add(&stereo);
            }
            let mut reverb = [[0.0; 2]; BLOCK];
            self.spatial.finish_block(&mut reverb);
            mix.add(&reverb);
            for (frame, samples) in mix.finish().iter().enumerate() {
                let level = 53.0 * if self.time >= 40.0 { 0.88 } else { 1.0 };
                let peak = samples[0].abs().max(samples[1].abs()) * level;
                let limit = (0.80 / peak).min(1.0);
                self.master_gain = if limit < self.master_gain {
                    limit
                } else {
                    self.master_gain + (limit - self.master_gain) / 240.0
                };
                for channel in 0..2 {
                    let at = channel * 0x400 + frame * 2;
                    self.memory.bytes[at..at + 2].copy_from_slice(
                        &((samples[channel] * level * self.master_gain * 32767.0) as i16)
                            .to_le_bytes(),
                    );
                }
            }
            output.extend(self.renderer.step(&mut self.memory));
        }
        output
    }

    fn parcel(&self) -> Vec<u8> {
        let record = |len: usize, fields: &[(usize, &[u8])]| {
            let mut r = vec![0; len];
            fields.iter().for_each(|&(at, value)| put(&mut r, at, value));
            r
        };
        let mut pools = vec![0; 0x20 * 8];
        put(&mut pools, 0, &BASE.to_le_bytes());
        put(&mut pools, 8, &(self.memory.bytes.len() as u64).to_le_bytes());
        put(&mut pools, 0x10, &4u32.to_le_bytes());
        let channels = [record(0x70, &[(0, &0u32.to_le_bytes()), (4, &1f32.to_le_bytes())]), record(0x70, &[(0, &1u32.to_le_bytes()), (8, &1f32.to_le_bytes())])].concat();
        let mut voice_section = Vec::new();
        for i in 0..2 {
            let address = BASE + i as u64 * 0x400;
            let wave = record(0x38, &[(0, &address.to_le_bytes()), (8, &((BLOCK * 2) as u64).to_le_bytes()), (0x14, &(BLOCK as u32).to_le_bytes()), (0x18, &[1, 0, 0]), (0x1C, &(-1i32).to_le_bytes())]);
            voice_section.extend(record(0x170, &[(8, &[1, 1, 0, 2]), (0xC, &48_000u32.to_le_bytes()), (0x18, &1u32.to_le_bytes()), (0x1C, &1f32.to_le_bytes()), (0x20, &1f32.to_le_bytes()), (0x58, &0i32.to_le_bytes()), (0x5C, &u32::MAX.to_le_bytes()), (0x60, &wave), (0x140, &(i as i32).to_le_bytes()), (0x15C, &2u16.to_le_bytes())]));
        }
        let mixes = [record(0x20, &[(4, &1u32.to_le_bytes())]), record(0x930, &[(0, &1f32.to_le_bytes()), (4, &48_000u32.to_le_bytes()), (8, &2u32.to_le_bytes()), (0xC, &[1]), (0x924, &i32::MAX.to_le_bytes()), (0x928, &u32::MAX.to_le_bytes())])].concat();
        let sink = record(0x140, &[(0, &[1, 1]), (0x120, &2u32.to_le_bytes()), (0x124, &[0, 1])]);
        let sections = [record(0x10, &[(0, &MAGIC.to_le_bytes())]), pools, channels, voice_section, Vec::new(), mixes, sink, vec![0; 0x10]];
        let mut header = vec![0; 0x40];
        put(&mut header, 0, &MAGIC.to_le_bytes());
        for (slot, section) in [(0x4, 0), (0x8, 1), (0x10, 2), (0xC, 3), (0x14, 4), (0x18, 5), (0x1C, 6), (0x20, 7)] {
            put(&mut header, slot, &(sections[section].len() as u32).to_le_bytes());
        }
        let total = 0x40 + sections.iter().map(Vec::len).sum::<usize>();
        put(&mut header, 0x3C, &(total as u32).to_le_bytes());
        [header, sections.concat()].concat()
    }
}

const COMMANDS: usize = 1024;
static WORKER_NANOS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Command {
    Move { source: usize, pos: V3 },
    Play { sample: Sample, pos: V3, gain: f32, pitch: f32 },
    Listen { eye: V3, yaw: f32, dt: f32 },
}

struct CommandRing {
    slots: [UnsafeCell<MaybeUninit<Command>>; COMMANDS],
    written: AtomicUsize,
    read: AtomicUsize,
}

// Sound owns the only producer and its worker owns the only consumer; release/acquire publishes each slot.
unsafe impl Sync for CommandRing {}

impl CommandRing {
    fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| UnsafeCell::new(MaybeUninit::uninit())),
            written: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    fn push(&self, command: Command) -> bool {
        let written = self.written.load(Ordering::Relaxed);
        if written.wrapping_sub(self.read.load(Ordering::Acquire)) == COMMANDS {
            return false;
        }
        // SAFETY: only the producer writes this slot, and it cannot lap the consumer.
        unsafe { (*self.slots[written % COMMANDS].get()).write(command) };
        self.written.store(written.wrapping_add(1), Ordering::Release);
        true
    }

    fn pop(&self) -> Option<Command> {
        let read = self.read.load(Ordering::Relaxed);
        if read == self.written.load(Ordering::Acquire) {
            return None;
        }
        // SAFETY: the acquire load observed the producer's initialized slot.
        let command = unsafe { (*self.slots[read % COMMANDS].get()).assume_init_read() };
        self.read.store(read.wrapping_add(1), Ordering::Release);
        Some(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_ring_hands_off_in_order() {
        let ring = Arc::new(CommandRing::new());
        let consumer = Arc::clone(&ring);
        let thread = std::thread::spawn(move || {
            for source in 0..COMMANDS * 4 {
                loop {
                    if let Some(command) = consumer.pop() {
                        assert_eq!(command, Command::Move { source, pos: [source as f32; 3] });
                        break;
                    }
                    std::thread::yield_now();
                }
            }
        });
        for source in 0..COMMANDS * 4 {
            let command = Command::Move { source, pos: [source as f32; 3] };
            while !ring.push(command) {
                std::thread::yield_now();
            }
        }
        thread.join().unwrap();
    }
}

struct Worker {
    commands: Arc<CommandRing>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn new(mut engine: Engine) -> Self {
        let commands = Arc::new(CommandRing::new());
        let stop = Arc::new(AtomicBool::new(false));
        let thread_commands = Arc::clone(&commands);
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let period = Duration::from_millis(5);
            let mut deadline = Instant::now();
            while !thread_stop.load(Ordering::Acquire) {
                while let Some(command) = thread_commands.pop() {
                    match command {
                        Command::Move { source, pos } => engine.move_source(source, pos),
                        Command::Play { sample, pos, gain, pitch } => engine.play(sample, pos, gain, pitch),
                        Command::Listen { eye, yaw, dt } => engine.listen(eye, yaw, dt),
                    }
                }
                if crate::audio::queued() <= 3 * BLOCK {
                    let start = Instant::now();
                    let output = engine.render(1);
                    crate::audio::play(&[&output]);
                    WORKER_NANOS.fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
                }
                deadline += period;
                let now = Instant::now();
                if deadline > now {
                    std::thread::sleep(deadline - now);
                } else {
                    deadline = now;
                }
            }
        });
        Self { commands, stop, thread: Some(thread) }
    }

    fn send(&self, command: Command) {
        assert!(self.commands.push(command), "audio command ring full");
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

pub struct Sound {
    engine: Option<Engine>,
    worker: Option<Worker>,
}

impl Sound {
    pub fn new(world: &World, sources: &[(Sfx, V3, f32)], threaded: bool, eye: V3, yaw: f32) -> Self {
        let mut engine = Engine::new(world, sources);
        engine.listen(eye, yaw, 0.0);
        if threaded {
            Self { engine: None, worker: Some(Worker::new(engine)) }
        } else {
            Self { engine: Some(engine), worker: None }
        }
    }

    pub fn move_source(&mut self, source: usize, pos: V3) {
        if let Some(worker) = &self.worker {
            worker.send(Command::Move { source, pos });
        } else { self.engine.as_mut().unwrap().move_source(source, pos); }
    }

    pub fn play(&mut self, sample: Sample, pos: V3, gain: f32, pitch: f32) {
        if let Some(worker) = &self.worker {
            worker.send(Command::Play { sample, pos, gain, pitch });
        } else { self.engine.as_mut().unwrap().play(sample, pos, gain, pitch); }
    }

    pub fn listen(&mut self, _world: &World, eye: V3, yaw: f32, dt: f32) {
        if let Some(worker) = &self.worker {
            worker.send(Command::Listen { eye, yaw, dt });
        } else { self.engine.as_mut().unwrap().listen(eye, yaw, dt); }
    }

    pub fn render(&mut self, periods: usize) -> Vec<i16> {
        self.engine.as_mut().map_or_else(Vec::new, |engine| engine.render(periods))
    }

    pub fn worker_millis(&self) -> f64 {
        if self.worker.is_some() { WORKER_NANOS.swap(0, Ordering::Relaxed) as f64 / 1e6 } else { 0.0 }
    }
}
