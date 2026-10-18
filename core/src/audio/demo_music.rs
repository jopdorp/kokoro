//! The music demo: a 16-bar loop driven the way a game drives audren. Four instruments are synthesised into PCM16
//! once, every note restarts a voice on its wave buffer through a RequestUpdate parcel, and the final mix runs a
//! low-pass biquad and a stereo delay before the device sink. `Sound` is that renderer setup, the bounce demo reuses it.
use super::renderer::{AudioRendererParameter, Renderer, work_buffer_size};
use scene::sound::{self, Note};
pub use scene::sound::assets::{pcm, instruments};
use super::{HostMemory, play};
use std::sync::Mutex;

const BASE: u64 = 0x10_0000;
const REV13: u32 = sound::REVISION;
const EFFECTS: usize = 2;
/// Periods queued ahead of wall time, so the sink never drains dry between frames.
const LEAD: u64 = 10;

pub struct Sound {
    renderer: Renderer,
    memory: HostMemory,
    samples: Vec<(u64, u32)>,
    notes: Vec<Note>,
    sent: bool,
    pub periods: u64,
    skipped: u64,
    seconds: f32,
}

impl Sound {
    /// A renderer with `voices` mono voices over one memory pool holding `samples`, all 48 kHz PCM16.
    pub fn new(samples: &[Vec<i16>], voices: usize) -> Self {
        let (bytes, placed) = sound::assets::place(samples, BASE);
        let parameter = AudioRendererParameter {
            sample_rate: 48_000,
            sample_count: 240,
            mix_buffer_count: 2,
            voice_count: voices as u32,
            sink_count: 1,
            effect_count: EFFECTS as u32,
            revision: REV13,
            ..Default::default()
        };
        let mut work = vec![0; work_buffer_size(&parameter).unwrap() as usize];
        let mut renderer = Renderer::open(parameter, &mut work).unwrap();
        renderer.start();
        let memory = HostMemory { base: BASE, bytes };
        let notes = vec![Note::default(); voices];
        Self { renderer, memory, samples: placed, notes, sent: false, periods: 0, skipped: 0, seconds: 0.0 }
    }

    /// Restarts `voice` on `sample` from the next period, `pan` from -1 (left) to 1 (right).
    pub fn trigger(&mut self, voice: usize, sample: usize, pitch: f32, volume: f32, pan: f32) {
        self.notes[voice] = Note::new(self.periods, &self.samples, sample, pitch, volume, pan);
    }

    /// The voice whose note ends first, a silent one if any.
    pub fn free_voice(&self) -> usize {
        sound::free_voice(&self.notes)
    }

    fn parcel(&mut self) -> Vec<u8> {
        let mut bytes = vec![0; sound::parcel_size(self.notes.len(), 1)];
        sound::parcel(&mut bytes, &mut self.notes, &self.samples, (BASE, self.memory.bytes.len() as u64), !self.sent, &sound::assets::biquad(), None);
        self.sent = true;
        bytes
    }

    /// Sends this period's parcel and renders it.
    pub fn period(&mut self) -> &[i16] {
        let parcel = self.parcel();
        self.renderer.request_update(&parcel, &mut vec![0; 0x4000]).unwrap();
        self.periods += 1;
        self.renderer.step(&mut self.memory)
    }

    /// The periods wall time `seconds` asks for since the last call, at 200 a second whatever the frame rate. A
    /// restarted clock starts over.
    pub fn due(&mut self, seconds: f32) -> u64 {
        if seconds < self.seconds {
            (self.periods, self.skipped) = (0, 0);
        }
        self.seconds = seconds;
        let due = (f64::from(seconds) * 200.0) as u64 + LEAD;
        // A long stall (the first frame's shader compiles) would leave a backlog the sink then plays late.
        if due > self.periods + self.skipped + 4 * LEAD {
            self.skipped = due - LEAD - self.periods;
        }
        due.saturating_sub(self.periods + self.skipped)
    }
}

pub fn music() -> Sound { Sound::new(&instruments(), 8) }
pub fn sequence(sound: &mut Sound) {
    scene::sound::sequence(sound.periods, |n| 2f32.powf(n as f32 / 12.0), |voice, sample, pitch, volume, pan| sound.trigger(voice, sample, pitch, volume, pan));
}

pub fn play_music(seconds: f32) {
    static MUSIC: Mutex<Option<Sound>> = Mutex::new(None);
    let mut slot = MUSIC.lock().unwrap();
    let sound = slot.get_or_insert_with(music);
    let mut peak = [0u16; 2];
    for _ in 0..sound.due(seconds) {
        sequence(sound);
        let samples = sound.period();
        for frame in samples.chunks_exact(2) {
            for channel in 0..2 { peak[channel] = peak[channel].max(frame[channel].unsigned_abs()); }
        }
        play(&[samples]);
    }
    let (width, height) = (1920, 1080);
    let mut pixels = [12, 16, 24, 255].repeat(width * height);
    for channel in 0..2 {
        let top = height - 40 - usize::from(peak[channel]) * (height - 80) / 32768;
        for y in 40..height - 40 {
            let colour = if y >= top { [48, 210, 160, 255] } else { [28, 36, 48, 255] };
            for x in width / 3 * (channel + 1) - 36..width / 3 * (channel + 1) + 36 {
                pixels[(y * width + x) * 4..][..4].copy_from_slice(&colour);
            }
        }
    }
    let cursor = (seconds * 60.0).round() as usize % width;
    pixels[(height - 1) * width * 4 + cursor * 4..][..4].copy_from_slice(&[48, 210, 160, 255]);
    crate::display::present(width as u32, height as u32, &pixels);
}
