//! Dungeon audio is guest-CPU DSP submitted as ordinary PCM; it is an emulator extension, not Switch DSP compatibility.

use core::f32::consts::{FRAC_PI_2, PI, TAU};

pub const RATE: usize = 48_000;
pub const BLOCK: usize = 240;
pub const HISTORY: usize = 1 << 17;
pub const FDN_SAMPLES: usize = 15_380;
pub const MAX_ROOMS: usize = 16;
pub const MAX_DOORWAYS: usize = 64;
pub const MAX_SOURCES: usize = 128;
#[cfg(feature = "std")]
pub fn coefficient_debug(cutoff: f32) -> f32 { coefficient(cutoff) }
pub const PCM_BYTES: usize = 0x1000;
const IMAGE_PATHS: usize = 3;
const GEOMETRY_BLOCKS: u64 = 10;
const SPEED: f32 = 343.0;
const HEAD_RADIUS: f32 = 0.0875;
const FDN_LENGTHS: [usize; 8] = [1423, 1559, 1699, 1871, 1999, 2131, 2281, 2417];
const REVISION: u32 = 0x3d76_4552;

pub type V3 = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode { Headphones, Speakers }

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Material { pub absorption: f32, pub scattering: f32 }

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Doorway { pub min: V3, pub max: V3, pub material: Material }

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Room {
    pub min: V3,
    pub max: V3,
    pub walls: [Material; 6],
    pub first_doorway: usize,
    pub doorway_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Source { pub id: usize, pub position: V3, pub gain: f32 }

pub fn decode_rooms(bytes: &[u8], rooms: &mut [Room], doorways: &mut [Doorway]) -> Option<(usize, usize)> {
    let (mut at, mut room_count, mut doorway_count) = (0, 0, 0);
    while at < bytes.len() {
        if room_count == rooms.len() || at + 76 > bytes.len() { return None; }
        let mut room = Room { min: v3(bytes, at)?, max: v3(bytes, at + 12)?, ..Room::default() };
        for wall in 0..6 {
            room.walls[wall] = Material { absorption: f32_at(bytes, at + 24 + wall * 8)?, scattering: f32_at(bytes, at + 28 + wall * 8)? };
        }
        let count = u32_at(bytes, at + 72)? as usize;
        at += 76;
        if doorway_count + count > doorways.len() || at + count * 32 > bytes.len() { return None; }
        room.first_doorway = doorway_count;
        room.doorway_count = count;
        for doorway in &mut doorways[doorway_count..doorway_count + count] {
            *doorway = Doorway {
                min: v3(bytes, at)?,
                max: v3(bytes, at + 12)?,
                material: Material { absorption: f32_at(bytes, at + 24)?, scattering: f32_at(bytes, at + 28)? },
            };
            at += 32;
        }
        rooms[room_count] = room;
        room_count += 1;
        doorway_count += count;
    }
    Some((room_count, doorway_count))
}

pub struct Mix { block: [[f32; 2]; BLOCK] }

impl Mix {
    pub const fn new() -> Self { Self { block: [[0.0; 2]; BLOCK] } }
    pub fn add(&mut self, source: &[[f32; 2]; BLOCK]) {
        for (out, input) in self.block.iter_mut().zip(source) { out[0] += input[0]; out[1] += input[1]; }
    }
    pub fn finish(mut self) -> [[f32; 2]; BLOCK] {
        for frame in &mut self.block { for sample in frame { *sample *= 0.5 / (1.0 + sample.abs()); } }
        self.block
    }
}

impl Default for Mix { fn default() -> Self { Self::new() } }

#[derive(Clone, Copy)]
struct Ear { shadow: f32, air: f32, material: f32, bass: f32, rear: f32, notch_lo: f32, notch_hi: f32 }
impl Ear { const ZERO: Self = Self { shadow: 0.0, air: 0.0, material: 0.0, bass: 0.0, rear: 0.0, notch_lo: 0.0, notch_hi: 0.0 }; }

#[derive(Clone, Copy)]
struct PathState { ears: [Ear; 2], gain: f32, delay: [f32; 2], initialized: bool }
impl PathState { const ZERO: Self = Self { ears: [Ear::ZERO; 2], gain: 0.0, delay: [0.0; 2], initialized: false }; }

#[derive(Clone, Copy, Default)]
struct Path { position: V3, gain: f32, absorption: f32 }

#[derive(Clone, Copy)]
struct SourceState {
    written: u64,
    paths: [PathState; IMAGE_PATHS + 1],
    images: [Path; IMAGE_PATHS],
    image_count: usize,
    image_listener: V3,
    image_source: V3,
    geometry_block: u64,
    present: bool,
}

impl SourceState {
    const ZERO: Self = Self {
        written: 0,
        paths: [PathState::ZERO; IMAGE_PATHS + 1],
        images: [Path { position: [0.0; 3], gain: 0.0, absorption: 0.0 }; IMAGE_PATHS],
        image_count: 0,
        image_listener: [f32::INFINITY; 3],
        image_source: [f32::INFINITY; 3],
        geometry_block: 0,
        present: false,
    };
}

#[derive(Clone, Copy)]
struct Parameters {
    delay: [f32; 2], gain: f32, ear_gain: [f32; 2], shadow: [f32; 2], air: f32,
    material: f32, bass: f32, bass_filter: f32, rear: f32, rear_filter: f32, notch: [f32; 2],
}

impl Parameters { const ZERO: Self = Self { delay: [0.0; 2], gain: 0.0, ear_gain: [0.0; 2], shadow: [0.0; 2], air: 0.0, material: 0.0, bass: 0.0, bass_filter: 0.0, rear: 0.0, rear_filter: 0.0, notch: [0.0; 2] }; }

#[derive(Clone, Copy)]
struct FdnState { at: [usize; 8], damp: [f32; 8] }
impl FdnState { const ZERO: Self = Self { at: [0; 8], damp: [0.0; 8] }; }

pub struct Spatializer<'a> {
    rooms: &'a [Room],
    doorways: &'a [Doorway],
    histories: &'a mut [f32],
    fdn: &'a mut [f32],
    source_count: usize,
    reveal_room: Option<usize>,
    mode: Mode,
    listener: V3,
    forward: V3,
    up: V3,
    states: [SourceState; MAX_SOURCES],
    fdn_states: [FdnState; MAX_ROOMS],
    room_weights: [f32; MAX_ROOMS],
    room_data: [(f32, f32); MAX_ROOMS],
    reverb_input: [[f32; BLOCK]; MAX_ROOMS],
    rooms_initialized: bool,
    block: u64,
}

impl<'a> Spatializer<'a> {
    pub fn new(rooms: &'a [Room], doorways: &'a [Doorway], source_count: usize, histories: &'a mut [f32], fdn: &'a mut [f32], mode: Mode) -> Option<Self> {
        if rooms.len() > MAX_ROOMS || doorways.len() > MAX_DOORWAYS || source_count > MAX_SOURCES || histories.len() < source_count * HISTORY || fdn.len() < rooms.len() * FDN_SAMPLES { return None; }
        histories[..source_count * HISTORY].fill(0.0);
        fdn[..rooms.len() * FDN_SAMPLES].fill(0.0);
        let reveal_room = rooms.iter().enumerate().max_by(|(_, a), (_, b)| room_volume(a).total_cmp(&room_volume(b))).map(|(i, _)| i);
        let mut room_data = [(0.0, 0.0); MAX_ROOMS];
        for (out, room) in room_data.iter_mut().zip(rooms) { *out = room_decay(room); }
        Some(Self {
            rooms, doorways, histories, fdn, source_count, reveal_room, mode,
            listener: [0.0; 3], forward: [0.0, 0.0, -1.0], up: [0.0, 1.0, 0.0],
            states: [SourceState::ZERO; MAX_SOURCES], fdn_states: [FdnState::ZERO; MAX_ROOMS],
            room_weights: [0.0; MAX_ROOMS], room_data, reverb_input: [[0.0; BLOCK]; MAX_ROOMS],
            rooms_initialized: false, block: 0,
        })
    }

    pub fn set_mode(&mut self, mode: Mode) { self.mode = mode; }
    pub fn set_listener(&mut self, pos: V3, forward: V3, up: V3) { self.listener = pos; self.forward = unit(forward); self.up = unit(up); }
    pub fn begin_block(&mut self) {
        let mut targets = [0.0; MAX_ROOMS];
        for (target, room) in targets.iter_mut().zip(self.rooms) {
            let outside = (0..3).map(|i| (room.min[i] - self.listener[i]).max(self.listener[i] - room.max[i]).max(0.0)).sum::<f32>();
            *target = (1.0 - outside / 0.8).clamp(0.0, 1.0);
        }
        let sum = targets[..self.rooms.len()].iter().sum::<f32>();
        if sum > 0.0 { for target in &mut targets[..self.rooms.len()] { *target /= sum; } }
        if self.rooms_initialized {
            for (weight, target) in self.room_weights.iter_mut().zip(targets).take(self.rooms.len()) { *weight += (target - *weight).clamp(-0.005, 0.005); }
        } else {
            self.room_weights = targets;
            self.rooms_initialized = true;
        }
        for input in &mut self.reverb_input[..self.rooms.len()] { input.fill(0.0); }
    }

    pub fn finish_block(&mut self, out: &mut [[f32; 2]; BLOCK]) {
        for room in 0..self.rooms.len() {
            let weight = self.room_weights[room];
            let (decay, damping) = self.room_data[room];
            let memory = &mut self.fdn[room * FDN_SAMPLES..][..FDN_SAMPLES];
            for (frame, dry) in out.iter_mut().zip(self.reverb_input[room]) {
                let wet = fdn_process(memory, &mut self.fdn_states[room], dry, decay, damping);
                frame[0] += wet[0] * weight;
                frame[1] += wet[1] * weight;
            }
        }
        self.block += 1;
    }

    pub fn render(&mut self, source: Source, mono: &[f32; BLOCK], out: &mut [[f32; 2]; BLOCK]) -> bool {
        if source.id >= self.source_count { return false; }
        out.fill([0.0; 2]);
        let previous = self.states[source.id];
        let refresh = !previous.present || self.block - previous.geometry_block >= GEOMETRY_BLOCKS
            && (distance_squared(self.listener, previous.image_listener) > 0.04 || distance_squared(source.position, previous.image_source) > 0.04);
        let (images, image_count) = if refresh { self.image_paths(source.position) } else { (previous.images, previous.image_count) };
        let direct = self.direct_path(source.position);
        let mut table = [Parameters::ZERO; IMAGE_PATHS + 1];
        table[0] = parameters(direct, source.gain, self.listener, self.forward, self.up, self.mode);
        for (parameter, path) in table[1..].iter_mut().zip(&images[..image_count]) { *parameter = parameters(*path, source.gain, self.listener, self.forward, self.up, self.mode); }
        let state = &mut self.states[source.id];
        if !state.present { state.present = true; }
        if refresh { state.images = images; state.image_count = image_count; state.image_listener = self.listener; state.image_source = source.position; state.geometry_block = self.block; }
        let history = &mut self.histories[source.id * HISTORY..][..HISTORY];
        for &sample in mono { history[(state.written as usize) & (HISTORY - 1)] = sample; state.written += 1; }
        let block_start = state.written - BLOCK as u64;
        if table[0].gain.abs() >= 0.001 {
            for (index, parameter) in table[..=image_count].iter().enumerate() { binaural_block(*parameter, &mut state.paths[index], history, block_start, index == 0, out); }
            for room in 0..self.rooms.len() {
                let send = table[0].gain * 0.08 * self.room_weights[room];
                for (dry, &sample) in self.reverb_input[room].iter_mut().zip(mono) { *dry += sample * send; }
            }
        }
        state.written = block_start + BLOCK as u64;
        true
    }

    fn direct_path(&self, source: V3) -> Path {
        let listener_room = self.rooms.iter().position(|room| contains(room, self.listener));
        let source_room = self.rooms.iter().position(|room| contains(room, source));
        if self.rooms.is_empty() || listener_room == source_room && source_room.is_some() { return Path { position: source, gain: distance_gain(length(sub(source, self.listener))), absorption: 0.0 }; }
        let Some(reveal) = self.reveal_room.filter(|&room| source_room == Some(room)) else { return Path::default() };
        let room = &self.rooms[reveal];
        let Some((door, position, distance)) = self.doorways[room.first_doorway..][..room.doorway_count].iter().map(|door| { let position = closest(door, self.listener); (door, position, length(sub(position, self.listener))) }).min_by(|a, b| a.2.total_cmp(&b.2)) else { return Path::default() };
        let route = length(sub(source, position)) + distance;
        let aperture = (1.0 - distance / 10.0).clamp(0.0, 1.0);
        let opening = 0.08 + 0.14 * aperture * aperture;
        Path { position, gain: distance_gain(route) * opening, absorption: 0.8 + door.material.scattering * 0.1 }
    }

    fn image_paths(&self, source: V3) -> ([Path; IMAGE_PATHS], usize) {
        let mut paths = [Path::default(); 42];
        let mut path_count = 0;
        let Some(room) = self.rooms.iter().find(|room| contains(room, source) && contains(room, self.listener)) else { return ([Path::default(); IMAGE_PATHS], 0) };
        let first = core::array::from_fn::<_, 6, _>(|wall| (mirror(source, room, wall), wall));
        for &(image, wall) in &first {
            if reflection_visible(image, self.listener, room, wall) {
                let material = room.walls[wall];
                paths[path_count] = Path { position: image, gain: distance_gain(length(sub(image, self.listener))) * (1.0 - material.absorption) * (1.0 - 0.5 * material.scattering), absorption: material.absorption };
                path_count += 1;
            }
        }
        for &(image, first_wall) in &first {
            for second_wall in 0..6 {
                if second_wall == first_wall { continue; }
                let image = mirror(image, room, second_wall);
                if reflection_visible(image, self.listener, room, second_wall) {
                    let a = (room.walls[first_wall].absorption + room.walls[second_wall].absorption) * 0.5;
                    paths[path_count] = Path { position: image, gain: distance_gain(length(sub(image, self.listener))) * (1.0 - a) * (1.0 - a) * 0.02, absorption: a };
                    path_count += 1;
                }
            }
        }
        for path in paths.iter_mut().take(path_count.min(6)) { path.gain *= 0.14; }
        paths[..path_count].sort_unstable_by(|a, b| length(sub(a.position, self.listener)).total_cmp(&length(sub(b.position, self.listener))));
        let count = path_count.min(IMAGE_PATHS);
        let mut images = [Path::default(); IMAGE_PATHS];
        images[..count].copy_from_slice(&paths[..count]);
        (images, count)
    }
}

fn fdn_process(memory: &mut [f32], state: &mut FdnState, input: f32, decay: f32, damping: f32) -> [f32; 2] {
    let mut taps = [0.0; 8];
    let mut offset = 0;
    for i in 0..8 { taps[i] = memory[offset + state.at[i]]; offset += FDN_LENGTHS[i]; }
    let sum = taps.iter().sum::<f32>() * 0.25;
    offset = 0;
    for i in 0..8 {
        let mixed = sum - taps[i] + input * 0.22;
        state.damp[i] += (mixed - state.damp[i]) * damping;
        memory[offset + state.at[i]] = state.damp[i] * decay;
        state.at[i] += 1;
        if state.at[i] == FDN_LENGTHS[i] { state.at[i] = 0; }
        offset += FDN_LENGTHS[i];
    }
    [(taps[0] + taps[2] - taps[4] - taps[6]) * 0.18, (taps[1] + taps[3] - taps[5] - taps[7]) * 0.18]
}

fn parameters(path: Path, source_gain: f32, listener: V3, forward: V3, up: V3, mode: Mode) -> Parameters {
    let offset = sub(path.position, listener);
    let distance = length(offset).max(0.05);
    let direction = scale(offset, 1.0 / distance);
    let right = unit(cross(forward, up));
    let azimuth = atan2(dot(direction, right), dot(direction, forward));
    let elevation = asin(dot(direction, up).clamp(-1.0, 1.0));
    let (side, _) = sin_cos(azimuth);
    let theta = azimuth.abs();
    let (theta_sin, _) = sin_cos(theta);
    let itd = HEAD_RADIUS / SPEED * if theta <= FRAC_PI_2 { theta + theta_sin } else { PI - theta + theta_sin };
    let delay = distance / SPEED * RATE as f32;
    let near = (1.0 / distance.max(0.35)).min(1.8);
    let rear = ((-dot(direction, forward)).max(0.0) * 0.18).clamp(0.0, 0.18);
    let pan = ((side + 1.0) * PI * 0.25).clamp(0.0, FRAC_PI_2);
    let (pan_sin, pan_cos) = sin_cos(pan);
    let far = [side.max(0.0), (-side).max(0.0)];
    let notch = 8_000.0 + elevation / FRAC_PI_2 * 2_000.0;
    Parameters {
        delay: core::array::from_fn(|ear| delay + if mode == Mode::Headphones { itd * RATE as f32 * far[ear] } else { 0.0 }),
        gain: path.gain * source_gain * near,
        ear_gain: core::array::from_fn(|ear| if mode == Mode::Speakers { [pan_cos, pan_sin][ear] } else { 1.0 - 0.5 * far[ear] }),
        shadow: core::array::from_fn(|ear| coefficient(14_000.0 - 11_000.0 * far[ear])),
        air: coefficient(18_000.0 / (1.0 + 0.045 * distance * distance)),
        material: coefficient(18_000.0 * (1.0 - 0.82 * path.absorption)),
        bass: (1.0 - distance).max(0.0) * 0.45,
        bass_filter: coefficient(220.0), rear, rear_filter: coefficient(3_500.0),
        notch: [coefficient(notch - 700.0), coefficient(notch + 700.0)],
    }
}

fn binaural_block(parameters: Parameters, previous: &mut PathState, history: &[f32], written: u64, direct: bool, out: &mut [[f32; 2]; BLOCK]) {
    if !previous.initialized { previous.gain = if direct { parameters.gain } else { 0.0 }; previous.delay = parameters.delay; previous.initialized = true; }
    let (from_gain, from_delay) = (previous.gain, previous.delay);
    let step = parameters.gain.max(previous.gain) * 0.005;
    let target_gain = parameters.gain.clamp(previous.gain - step, previous.gain + step);
    for (n, frame) in out.iter_mut().enumerate() {
        let t = (n + 1) as f32 / BLOCK as f32;
        let gain = lerp(from_gain, target_gain, t);
        for ear in 0..2 {
            let ear_delay = lerp(from_delay[ear], parameters.delay[ear], t);
            let mut sample = delayed(history, written + n as u64, ear_delay);
            sample = lowpass(sample, parameters.shadow[ear], &mut previous.ears[ear].shadow);
            sample = lowpass(sample, parameters.air, &mut previous.ears[ear].air);
            sample = lowpass(sample, parameters.material, &mut previous.ears[ear].material);
            let bass = lowpass(sample, parameters.bass_filter, &mut previous.ears[ear].bass);
            sample += bass * parameters.bass;
            let rear_low = lowpass(sample, parameters.rear_filter, &mut previous.ears[ear].rear);
            sample -= (sample - rear_low) * parameters.rear;
            let lo = lowpass(sample, parameters.notch[0], &mut previous.ears[ear].notch_lo);
            let hi = lowpass(sample, parameters.notch[1], &mut previous.ears[ear].notch_hi);
            sample -= (hi - lo) * 0.24;
            frame[ear] += sample * gain * parameters.ear_gain[ear] * (1.0 - parameters.rear);
        }
    }
    previous.gain = target_gain;
    previous.delay = parameters.delay;
}

fn delayed(history: &[f32], written: u64, age: f32) -> f32 {
    let position = written as f32 - age;
    if position < 0.0 || age >= HISTORY as f32 - 2.0 { return 0.0; }
    let base = position as u64;
    let fraction = position - base as f32;
    let a = history[(base as usize) & (HISTORY - 1)];
    let b = history[(base.wrapping_add(1) as usize) & (HISTORY - 1)];
    a + (b - a) * fraction
}

fn coefficient(cutoff: f32) -> f32 { 1.0 - exp(-TAU * cutoff.clamp(80.0, 20_000.0) / RATE as f32) }
fn lowpass(input: f32, a: f32, state: &mut f32) -> f32 { *state += (input - *state) * a; *state }

fn room_decay(room: &Room) -> (f32, f32) {
    let size = sub(room.max, room.min);
    let volume = size[0] * size[1] * size[2];
    let areas = [size[1] * size[2], size[1] * size[2], size[0] * size[2], size[0] * size[2], size[0] * size[1], size[0] * size[1]];
    let absorbed = (0..6).map(|i| areas[i] * room.walls[i].absorption.max(0.01)).sum::<f32>();
    let rt60 = (0.161 * volume / absorbed).clamp(0.15, 4.0);
    let mean = room.walls.iter().map(|wall| wall.absorption).sum::<f32>() / 6.0;
    (pow(10.0, -3.0 * 0.04 / rt60), (0.42 - mean * 0.25).clamp(0.12, 0.42))
}

fn reflection_visible(image: V3, listener: V3, room: &Room, wall: usize) -> bool {
    let axis = wall / 2;
    let plane = if wall.is_multiple_of(2) { room.min[axis] } else { room.max[axis] };
    let t = (plane - listener[axis]) / (image[axis] - listener[axis]);
    (0.0..=1.0).contains(&t) && (0..3).filter(|&i| i != axis).all(|i| { let p = listener[i] + (image[i] - listener[i]) * t; p >= room.min[i] && p <= room.max[i] })
}

fn mirror(point: V3, room: &Room, wall: usize) -> V3 { let mut image = point; let axis = wall / 2; let plane = if wall.is_multiple_of(2) { room.min[axis] } else { room.max[axis] }; image[axis] = 2.0 * plane - image[axis]; image }
fn distance_gain(distance: f32) -> f32 { 1.0 / distance.max(0.75) }
fn contains(room: &Room, point: V3) -> bool { (0..3).all(|i| point[i] >= room.min[i] && point[i] <= room.max[i]) }
fn room_volume(room: &Room) -> f32 { (room.max[0] - room.min[0]) * (room.max[1] - room.min[1]) * (room.max[2] - room.min[2]) }
fn closest(door: &Doorway, point: V3) -> V3 { core::array::from_fn(|i| point[i].clamp(door.min[i], door.max[i])) }
fn sub(a: V3, b: V3) -> V3 { core::array::from_fn(|i| a[i] - b[i]) }
fn scale(a: V3, s: f32) -> V3 { a.map(|v| v * s) }
fn dot(a: V3, b: V3) -> f32 { (0..3).map(|i| a[i] * b[i]).sum() }
fn cross(a: V3, b: V3) -> V3 { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
fn length(a: V3) -> f32 { sqrt(dot(a, a)) }
fn distance_squared(a: V3, b: V3) -> f32 { let d = sub(a, b); dot(d, d) }
fn unit(a: V3) -> V3 { scale(a, 1.0 / length(a).max(1e-9)) }
fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

#[cfg(target_arch = "aarch64")]
unsafe extern "C" {
    #[link_name = "sqrtf"] fn c_sqrt(value: f32) -> f32;
    #[link_name = "atan2f"] fn c_atan2(y: f32, x: f32) -> f32;
    #[link_name = "asinf"] fn c_asin(value: f32) -> f32;
    #[link_name = "expf"] fn c_exp(value: f32) -> f32;
    #[link_name = "powf"] fn c_pow(value: f32, exponent: f32) -> f32;
    #[link_name = "sinf"] fn c_sin(value: f32) -> f32;
    #[link_name = "cosf"] fn c_cos(value: f32) -> f32;
}

macro_rules! math {
    ($name:ident, $host:ident, $guest:ident, $($arg:ident),+) => {
        #[cfg(not(target_arch = "aarch64"))]
        fn $name($($arg: f32),+) -> f32 { $host($($arg),+) }
        #[cfg(target_arch = "aarch64")]
        fn $name($($arg: f32),+) -> f32 { unsafe { $guest($($arg),+) } }
    };
}
#[cfg(not(target_arch = "aarch64"))] fn host_sqrt(value: f32) -> f32 { value.sqrt() }
#[cfg(not(target_arch = "aarch64"))] fn host_atan2(y: f32, x: f32) -> f32 { y.atan2(x) }
#[cfg(not(target_arch = "aarch64"))] fn host_asin(value: f32) -> f32 { value.asin() }
#[cfg(not(target_arch = "aarch64"))] fn host_exp(value: f32) -> f32 { value.exp() }
#[cfg(not(target_arch = "aarch64"))] fn host_pow(value: f32, exponent: f32) -> f32 { value.powf(exponent) }
math!(sqrt, host_sqrt, c_sqrt, value);
math!(atan2, host_atan2, c_atan2, y, x);
math!(asin, host_asin, c_asin, value);
math!(exp, host_exp, c_exp, value);
math!(pow, host_pow, c_pow, value, exponent);

fn sin_cos(value: f32) -> (f32, f32) {
    #[cfg(not(target_arch = "aarch64"))]
    { value.sin_cos() }
    #[cfg(target_arch = "aarch64")]
    { unsafe { (c_sin(value), c_cos(value)) } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Sample { Torch, Fire, River, Wind, Heart, Creak, Bats, Room, Drip, Scream, Step, WetStep, GravelStep }

impl Sample {
    const fn looping(self) -> bool { !matches!(self, Self::Creak | Self::Drip | Self::Scream | Self::Step | Self::WetStep | Self::GravelStep) }
    const fn step(self) -> bool { matches!(self, Self::Step | Self::WetStep | Self::GravelStep) }
}

pub struct Samples<'a> {
    pub recordings: [&'a [i16]; 13],
    pub drips: [&'a [i16]; 3],
    pub steps: [[&'a [i16]; 6]; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voice {
    pub sample: Sample,
    pub position: V3,
    pub gain: f32,
    pub pitch: f32,
    cursor: f32,
    active: bool,
    tail: usize,
    variation: usize,
    step_set: usize,
}

impl Voice {
    pub const fn new(sample: Sample, position: V3, gain: f32, pitch: f32, active: bool) -> Self {
        Self { sample, position, gain, pitch, cursor: 0.0, active, tail: if active { usize::MAX } else { 0 }, variation: 0, step_set: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box3 { pub min: V3, pub max: V3 }
impl Box3 { fn contains(self, point: V3) -> bool { (0..3).all(|axis| point[axis] >= self.min[axis] && point[axis] <= self.max[axis]) } }

pub const DRIPS: usize = 4;
const EVENT_VOICES: usize = DRIPS + 3;

pub struct Storage<'a> { pub voices: &'a mut [Voice], pub histories: &'a mut [f32], pub fdn: &'a mut [f32] }
pub struct Environment<'a> { pub rooms: &'a [Room], pub doorways: &'a [Doorway], pub bridge: Box3, pub reveal: Box3, pub ambient: bool }

pub struct Engine<'a> {
    samples: Samples<'a>,
    voices: &'a mut [Voice],
    spatial: Spatializer<'a>,
    bridge: Box3,
    reveal: Box3,
    rng: u32,
    drip_at: u64,
    next_drip: usize,
    in_reveal: bool,
    reveal_at: u64,
    last_listener: V3,
    mix_gain: f32,
    rendered: u64,
    creak: Option<usize>,
    creak_played: bool,
    ambient: bool,
    next_step: usize,
}

impl<'a> Engine<'a> {
    pub fn new(samples: Samples<'a>, storage: Storage<'a>, environment: Environment<'a>, mode: Mode) -> Option<Self> {
        let Storage { voices, histories, fdn } = storage;
        let Environment { rooms, doorways, bridge, reveal, ambient } = environment;
        if voices.len() < EVENT_VOICES || voices.iter().any(|voice| samples.recordings[voice.sample as usize].is_empty()) || samples.drips.iter().chain(samples.steps.iter().flatten()).any(|clip| clip.is_empty()) { return None; }
        let creak = voices.iter().position(|voice| voice.sample == Sample::Creak);
        let source_count = voices.len();
        Some(Self { samples, voices, spatial: Spatializer::new(rooms, doorways, source_count, histories, fdn, mode)?, bridge, reveal, rng: 0xd21f_740b, drip_at: 12 * RATE as u64, next_drip: 0, in_reveal: false, reveal_at: 0, last_listener: [0.0; 3], mix_gain: 0.46, rendered: 0, creak, creak_played: false, ambient, next_step: 0 })
    }

    pub fn move_source(&mut self, source: usize, position: V3) -> bool {
        let Some(voice) = self.voices.get_mut(source) else { return false };
        voice.position = position;
        true
    }

    pub fn play(&mut self, sample: Sample, position: V3, gain: f32, pitch: f32) {
        if sample == Sample::Scream || self.ambient && sample == Sample::Drip { return; }
        let first = self.voices.len() - EVENT_VOICES;
        let slot = if sample == Sample::Drip {
            if self.rendered < self.drip_at { return; }
            let slot = first + self.next_drip;
            self.next_drip = (self.next_drip + 1) % DRIPS;
            self.drip_at = self.rendered + self.drip_timing().1;
            slot
        } else {
            self.next_step += 1;
            first + DRIPS + 1 + (self.next_step & 1)
        };
        let variation = self.next_step * 5 % 6;
        let decibels = [-2.0, 1.0, -1.0, 2.0, 0.0, 0.5][variation];
        let step = sample.step();
        let step_set = if self.bridge.contains(position) { 3 } else { match sample { Sample::WetStep => 1, Sample::GravelStep => 2, _ => 0 } };
        let position = if sample == Sample::Drip { self.voices[slot].position } else { position };
        self.voices[slot] = Voice { sample, position, gain: gain * if step { pow(10.0, decibels / 20.0) } else { 1.0 }, pitch: if step { 1.0 } else { pitch }, cursor: 0.0, active: true, tail: 800, variation, step_set };
    }

    pub fn listen(&mut self, position: V3, yaw: f32, dt: f32, mode: Mode) {
        let in_reveal = self.reveal.contains(position);
        if in_reveal != self.in_reveal {
            self.in_reveal = in_reveal;
            if in_reveal { self.reveal_at = self.rendered; }
            self.drip_at = self.rendered + if in_reveal { 2 * RATE as u64 } else { self.far_drip_delay() };
        }
        let reveal_age = (self.rendered - self.reveal_at) as f32 / RATE as f32;
        let moving = dt > 0.0 && length(sub(position, self.last_listener)) / dt > 0.1;
        let target = if !in_reveal { 0.46 } else if reveal_age < 6.0 { 0.46 + reveal_age / 6.0 * 0.54 } else if moving { 1.0 } else { 0.58 };
        self.mix_gain += (target - self.mix_gain) * (dt / 2.0).min(1.0);
        self.last_listener = position;
        let (sin, cos) = sin_cos(yaw);
        self.spatial.set_mode(mode);
        self.spatial.set_listener(position, [sin, 0.0, -cos], [0.0, 1.0, 0.0]);
    }

    pub fn render(&mut self) -> [[i16; 2]; BLOCK] {
        self.schedule();
        let mut mix = Mix::new();
        self.spatial.begin_block();
        for (id, voice) in self.voices.iter_mut().enumerate().filter(|(_, voice)| voice.active || voice.tail > 0) {
            let pcm = if voice.sample == Sample::Drip { self.samples.drips[voice.variation] } else if voice.sample.step() { self.samples.steps[voice.step_set][voice.variation] } else { self.samples.recordings[voice.sample as usize] };
            let mut mono = [0.0; BLOCK];
            for sample in &mut mono {
                if !voice.active { break; }
                let at = voice.cursor as usize;
                if at >= pcm.len() {
                    if voice.sample.looping() { voice.cursor %= pcm.len() as f32; } else { voice.active = false; break; }
                }
                let at = voice.cursor as usize;
                let next = (at + 1).min(pcm.len() - 1);
                let (a, b) = (pcm[at] as f32 / 32768.0, pcm[next] as f32 / 32768.0);
                *sample = a + (b - a) * (voice.cursor - at as f32);
                voice.cursor += voice.pitch;
            }
            if !voice.active { voice.tail = voice.tail.saturating_sub(1); }
            let mut stereo = [[0.0; 2]; BLOCK];
            let suspense = self.rendered >= 30 * RATE as u64 && self.rendered < 36 * RATE as u64;
            let gain = if voice.sample == Sample::Heart && !suspense { 0.0 } else { voice.gain };
            self.spatial.render(Source { id, position: voice.position, gain }, &mono, &mut stereo);
            mix.add(&stereo);
        }
        let mut reverb = [[0.0; 2]; BLOCK];
        self.spatial.finish_block(&mut reverb);
        mix.add(&reverb);
        let mixed = mix.finish();
        self.rendered += BLOCK as u64;
        core::array::from_fn(|frame| core::array::from_fn(|channel| (limit(mixed[frame][channel] * 36.0 * self.mix_gain) * 32767.0) as i16))
    }

    fn schedule(&mut self) {
        if !self.ambient { return; }
        let first = self.voices.len() - EVENT_VOICES;
        if self.rendered >= self.drip_at {
            let in_reveal = self.in_reveal;
            let (pitch, delay) = self.drip_timing();
            let voice = &mut self.voices[first + self.next_drip];
            voice.gain = if in_reveal { 0.34 } else { 0.055 };
            voice.pitch = pitch;
            voice.cursor = 0.0;
            voice.active = true;
            voice.tail = 800;
            voice.variation = self.next_drip % 3;
            self.next_drip = (self.next_drip + 1) % DRIPS;
            self.drip_at = self.rendered + if in_reveal { delay } else { self.far_drip_delay() };
        }
        if !self.creak_played && self.rendered >= RATE as u64 * 65 / 2 {
            if let Some(creak) = self.creak { self.voices[creak].active = true; }
            self.creak_played = true;
        }
    }

    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / 16_777_216.0
    }
    fn drip_timing(&mut self) -> (f32, u64) { (0.97 + self.random() * 0.06, ((2.0 + self.random() * 3.0) * RATE as f32) as u64) }
    fn far_drip_delay(&mut self) -> u64 { ((10.0 + self.random() * 10.0) * RATE as f32) as u64 }
}

pub fn step_cadence(walk_speed: f32) -> f32 { walk_speed.max(0.0) / 0.54 }
fn limit(sample: f32) -> f32 { let magnitude = sample.abs(); sample.signum() * if magnitude <= 0.72 { magnitude } else { 0.72 + 0.12 * (1.0 - exp(-(magnitude - 0.72) / 0.12)) } }

pub const fn pcm_parameter(sinks: u32) -> [u32; 13] {
    [RATE as u32, BLOCK as u32, 2, 0, 2, sinks, 0, 0, 0, 0, 0, 0, REVISION]
}

pub const fn pcm_parcel_size(sinks: usize) -> usize {
    0x40 + 0x10 + 0x100 + 0xe0 + 0x2e0 + 0x950 + 0x140 * sinks + 0x10
}

pub fn pcm_parcel(bytes: &mut [u8], memory: u64, first: bool, ring: Option<u64>) -> usize {
    let sinks = 1 + usize::from(ring.is_some());
    let sizes = [0x10, 0x100, 0xe0, 0x2e0, 0, 0x950, 0x140 * sinks, 0x10];
    let len = pcm_parcel_size(sinks);
    bytes[..len].fill(0);
    record(bytes, 0, &[(0, &REVISION.to_le_bytes()), (0x3c, &(len as u32).to_le_bytes())]);
    for (slot, section) in [0, 1, 3, 2, 4, 5, 6, 7].into_iter().enumerate() {
        record(bytes, 4 + slot * 4, &[(0, &(sizes[section] as u32).to_le_bytes())]);
    }
    let mut at = 0x40;
    record(bytes, at, &[(0, &REVISION.to_le_bytes())]);
    at += sizes[0];
    record(bytes, at, &[(0, &memory.to_le_bytes()), (8, &(PCM_BYTES as u64).to_le_bytes()), (0x10, &4u32.to_le_bytes())]);
    at += sizes[1];
    for channel in 0..2 {
        record(bytes, at + channel * 0x70, &[(0, &(channel as u32).to_le_bytes()), (4 + channel * 4, &1f32.to_le_bytes())]);
    }
    at += sizes[2];
    for channel in 0..2 {
        let voice = at + channel * 0x170;
        let address = memory + channel as u64 * 0x200;
        record(bytes, voice, &[(0, &(channel as u32).to_le_bytes()), (8, &[u8::from(first), 1, 0, 2]), (0xc, &(RATE as u32).to_le_bytes()), (0x18, &1u32.to_le_bytes()), (0x1c, &1f32.to_le_bytes()), (0x20, &1f32.to_le_bytes()), (0x58, &0i32.to_le_bytes()), (0x5c, &u32::MAX.to_le_bytes()), (0x140, &(channel as i32).to_le_bytes()), (0x15c, &2u16.to_le_bytes())]);
        record(bytes, voice + 0x60, &[(0, &address.to_le_bytes()), (8, &((BLOCK * 2) as u64).to_le_bytes()), (0x14, &(BLOCK as u32).to_le_bytes()), (0x18, &[1, 0, 0]), (0x1c, &(-1i32).to_le_bytes())]);
    }
    at += sizes[3];
    record(bytes, at, &[(4, &1u32.to_le_bytes())]);
    record(bytes, at + 0x20, &[(0, &1f32.to_le_bytes()), (4, &(RATE as u32).to_le_bytes()), (8, &2u32.to_le_bytes()), (0xc, &[1]), (0x924, &i32::MAX.to_le_bytes()), (0x928, &u32::MAX.to_le_bytes())]);
    at += sizes[5];
    record(bytes, at, &[(0, &[1, 1]), (0x120, &2u32.to_le_bytes()), (0x124, &[0, 1])]);
    if let Some(address) = ring {
        record(bytes, at + 0x140, &[(0, &[2, 1]), (0x20, &address.to_le_bytes()), (0x28, &960u32.to_le_bytes()), (0x2c, &2u32.to_le_bytes()), (0x30, &1u32.to_le_bytes()), (0x3c, &[0, 1])]);
    }
    len
}

fn record(bytes: &mut [u8], at: usize, fields: &[(usize, &[u8])]) {
    for &(offset, value) in fields { bytes[at + offset..at + offset + value.len()].copy_from_slice(value); }
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> { Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?)) }
fn f32_at(bytes: &[u8], at: usize) -> Option<f32> { Some(f32::from_bits(u32_at(bytes, at)?)) }
fn v3(bytes: &[u8], at: usize) -> Option<V3> { Some([f32_at(bytes, at)?, f32_at(bytes, at + 4)?, f32_at(bytes, at + 8)?]) }
