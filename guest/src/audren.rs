//! audren:u client: libnx's OpenAudioRenderer and switchbrew's RequestUpdate buffers.
// SwitchBrew documents no Switch 2 replacement for these IPC commands; the parameter and parcels use REv.
use crate::{cmif::{Buffer, Request}, svc};

pub struct Renderer { session: u32, event: u32, work: u32 }
impl Renderer {
    pub fn open(parameter: &[u32; 13], work_address: u64) -> svc::Result<Self> {
        let manager = crate::hid::service(*b"audren:u")?;
        let reply = svc::request(manager, &Request { command: 1, data: parameter, ..Default::default() })?;
        let size = (u64::from(reply.data[0]) | u64::from(reply.data[1]) << 32).next_multiple_of(0x1000);
        let work = svc::create_transfer_memory(work_address, size, 0)?;
        let mut data = [0; 18];
        data[..13].copy_from_slice(parameter);
        data[14..16].copy_from_slice(&[size as u32, (size >> 32) as u32]);
        let session = svc::request(manager, &Request { command: 0, send_pid: true, copy_handles: &[work, 0xffff8001], data: &data, ..Default::default() })?.move_handles[0];
        svc::close_handle(manager)?;
        let event = svc::request(session, &Request { command: 7, ..Default::default() })?.copy_handles[0];
        Ok(Self { session, event, work })
    }
    pub fn update(&self, input: &[u8], output: &mut [u8]) -> svc::Result<()> {
        svc::request(self.session, &Request { command: 4, send: &[Buffer { address: input.as_ptr() as u64, size: input.len() as u64 }], receive: &[Buffer { address: output.as_mut_ptr() as u64, size: output.len() as u64 }, Buffer { address: 0, size: 0 }], ..Default::default() })?;
        svc::request(self.session, &Request { command: 5, ..Default::default() })?;
        svc::wait_event(self.event)
    }
    pub fn wait(&self) -> svc::Result<()> { svc::wait_event(self.event) }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = svc::request(self.session, &Request { command: 6, ..Default::default() });
        for handle in [self.event, self.session, self.work] { let _ = svc::close_handle(handle); }
    }
}

pub struct Pcm<'a> {
    renderer: Renderer,
    memory: u64,
    input: &'a mut [u8],
    output: &'a mut [u8],
    ring: Option<u64>,
    started: bool,
}

impl<'a> Pcm<'a> {
    pub fn new(memory: u64, capture: bool) -> svc::Result<Self> {
        use scene::dungeon::sound::{PCM_BYTES, pcm_parameter, pcm_parcel_size};
        crate::nv::guest_slice(memory, PCM_BYTES).fill(0);
        let ring = capture.then_some(memory + 0x400);
        let input_address = (memory + PCM_BYTES as u64).next_multiple_of(0x1000);
        let input = crate::nv::guest_slice(input_address, pcm_parcel_size(1 + usize::from(capture)));
        let output_address = (input_address + input.len() as u64).next_multiple_of(0x1000);
        let output = crate::nv::guest_slice(output_address, 0x4000);
        let renderer = Renderer::open(&pcm_parameter(1 + u32::from(capture)), output_address + 0x4000)?;
        Ok(Self { renderer, memory, input, output, ring, started: false })
    }

    pub fn period(&mut self, pcm: &[[i16; 2]; scene::dungeon::sound::BLOCK]) -> svc::Result<Option<&[i16]>> {
        for channel in 0..2 {
            let out = crate::nv::guest_slice::<i16>(self.memory + channel as u64 * 0x200, scene::dungeon::sound::BLOCK);
            for (out, frame) in out.iter_mut().zip(pcm) { *out = frame[channel]; }
        }
        if self.started {
            self.renderer.wait()?;
        } else {
            let len = scene::dungeon::sound::pcm_parcel(self.input, self.memory, true, self.ring);
            self.renderer.update(&self.input[..len], self.output)?;
            self.started = true;
        }
        Ok(self.ring.map(|ring| &*crate::nv::guest_slice::<i16>(ring, 2 * scene::dungeon::sound::BLOCK)))
    }
}

pub struct Sound<'a> {
    renderer: Renderer, notes: &'a mut [scene::sound::Note], samples: &'a [(u64, u32)],
    input: &'a mut [u8], output: &'a mut [u8], pool: (u64, u64), ring: u64,
    pub periods: u64,
}
impl<'a> Sound<'a> {
    pub fn new(memory: u64, bytes: &[u8], samples: &'a [(u64, u32)], notes: &'a mut [scene::sound::Note]) -> svc::Result<Self> {
        crate::nv::guest_slice(memory, bytes.len()).copy_from_slice(bytes);
        let ring = memory + bytes.len() as u64;
        let end = (ring + 960).next_multiple_of(0x1000);
        let size = scene::sound::parcel_size(notes.len(), 2);
        let output = (end + size as u64).next_multiple_of(0x1000);
        let renderer = Renderer::open(&scene::sound::parameter(notes.len(), 2), output + 0x4000)?;
        Ok(Self { renderer, notes, samples, input: crate::nv::guest_slice(end, size), output: crate::nv::guest_slice(output, 0x4000), pool: (memory, end - memory), ring, periods: 0 })
    }
    pub fn trigger(&mut self, voice: usize, sample: usize, pitch: f32, volume: f32, pan: f32) {
        self.notes[voice] = scene::sound::Note::new(self.periods, self.samples, sample, pitch, volume, pan);
    }
    pub fn free_voice(&self) -> usize { scene::sound::free_voice(self.notes) }
    pub fn period(&mut self, biquad: &[u8; 10]) -> svc::Result<[f32; 2]> {
        scene::sound::parcel(self.input, self.notes, self.samples, self.pool, self.periods == 0, biquad, Some(self.ring));
        self.renderer.update(self.input, self.output)?;
        self.periods += 1;
        let ring = crate::nv::guest_slice::<i16>(self.ring, 480);
        Ok(core::array::from_fn(|channel| ring[channel * 240..][..240].iter().map(|s| f32::from(s.unsigned_abs()) / 32768.0).fold(0.0, f32::max)))
    }
}
