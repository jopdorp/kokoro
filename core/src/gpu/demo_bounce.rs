//! The host bounce oracle: a dozen balls under gravity in a box, stepped once per audio period so every wall and
//! ball impact lands in the period it happens. Each impact restarts a free audren voice on a short knock, pitched by
//! the ball's size, as loud as the impact and panned by x. The NVIDIA driver's SM86 build of demos/shaders/particles.vert
//! and particles.frag draws the walls and the balls as one instanced draw of round quads.
use super::engine::Gpu;
use super::frame::cache_root;
use super::{VkState, device};
use crate::audio::demo_music::{Sound, pcm};
use crate::audio::play;
use crate::tests::gpu_test::inst::bra;
use crate::tests::gpu::engine::{bytes, entry};
use scene::bounce::{self, assets::{walls, instances}, Ball, BALLS, HALF, SOFT};
pub use scene::bounce::{WIDTH, HEIGHT};
use std::f32::consts::TAU;
use std::sync::Mutex;

const BASE: u64 = 0x1000_0000;
pub const TARGET: u64 = BASE + bounce::TARGET;
fn named(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

pub struct Bounce {
    pub sound: Sound,
    balls: [Ball; BALLS],
}

impl Bounce {
    pub fn new() -> Self {
        let balls = bounce::balls();
        let time = (0..6000).map(|n| n as f32 / 48_000.0);
        let knock = pcm(time.map(|t| 0.7 * (TAU * 520.0 * t).sin() * (-45.0 * t).exp() + 0.3 * (TAU * 1390.0 * t).sin() * (-70.0 * t).exp()));
        Self { sound: Sound::new(&[knock], 16), balls }
    }

    fn physics(&mut self) -> Vec<(usize, f32)> {
        let mut impacts = Vec::new();
        bounce::physics(&mut self.balls, f32::hypot, |i, speed| impacts.push((i, speed)));
        impacts
    }

    /// Advances the balls one period and knocks a voice for every impact in it.
    pub fn step(&mut self) {
        let impacts = self.physics();
        for (i, speed) in impacts.into_iter().filter(|&(_, speed)| speed > SOFT) {
            let ball = &self.balls[i];
            let voice = self.sound.free_voice();
            self.sound.trigger(voice, 0, 0.08 / ball.radius, (speed / 8.0).min(0.6), ball.at[0] / HALF[0]);
        }
    }
}

struct Scene<'a> {
    gpu: Gpu<'a>,
    bounce: Bounce,
    walls: Vec<[f32; 2]>,
    frame: u32,
}

impl<'a> Scene<'a> {
    fn new(state: &'static VkState) -> Result<Self, String> {
        let mut gpu = Gpu::new(state, bounce::MAPPED as usize, Some((&cache_root(), "demos")), *super::SPEED_HACKS.lock().unwrap()).map_err(named)?;
        gpu.space.map(BASE, 0, bounce::MAPPED);
        let walls = walls();
        let count = (walls.len() + 2 * BALLS) as u32;
        let mut words = Vec::new();
        bounce::init_segment(BASE, count, &mut words);
        let program = |code: &[u8]| [code, &bra(-16).to_le_bytes()].concat();
        for (va, data) in [
            (BASE + bounce::VERTEX_PROGRAM, program(include_bytes!("../../tests/fixtures/driver-shaders/particles.vert.bin"))),
            (BASE + bounce::PIXEL_PROGRAM, program(include_bytes!("../../tests/fixtures/driver-shaders/particles.frag.bin"))),
            (BASE + bounce::VS_POINTERS, bytes(&bounce::pointers(BASE))),
            (BASE + bounce::VS_UNIFORMS, floats(&bounce::uniforms())),
            (BASE + bounce::PUSHBUFFER, bytes(&words)),
        ] {
            gpu.write(va, &data).map_err(named)?;
        }
        gpu.submit(&[entry(BASE + bounce::PUSHBUFFER, words.len())])?;
        Ok(Self { gpu, bounce: Bounce::new(), walls, frame: 0 })
    }

    /// Clears the box, draws the walls and the balls where they are and waits for the frame's report.
    fn draw(&mut self) -> Result<(), String> {
        let instances = instances(&self.bounce.balls, &self.walls);
        self.frame += 1;
        let mut words = Vec::new();
        bounce::frame(BASE, instances.len() / 12, self.frame, &mut words);
        self.gpu.write(BASE + bounce::INSTANCES, &floats(&instances)).map_err(named)?;
        self.gpu.write(BASE + bounce::DRAW_SEGMENT, &bytes(&words)).map_err(named)?;
        self.gpu.submit(&[entry(BASE + bounce::DRAW_SEGMENT, words.len())])?;
        let mut released = [0; 4];
        self.gpu.read(BASE + bounce::RELEASE, &mut released).map_err(named)?;
        match u32::from_le_bytes(released) == self.frame {
            true => Ok(()),
            false => Err(format!("frame {} did not release its report", self.frame)),
        }
    }
}

pub fn render_bounce_frame(seconds: f32) -> Result<(), String> {
    static SCENE: Mutex<Option<Scene<'static>>> = Mutex::new(None);
    let mut slot = SCENE.lock().unwrap();
    let scene = match &mut *slot {
        Some(scene) => scene,
        empty => empty.insert(Scene::new(device().map_err(named)?)?),
    };
    for _ in 0..scene.bounce.sound.due(seconds) {
        scene.bounce.step();
        play(&[scene.bounce.sound.period()]);
    }
    scene.draw()?;
    scene.gpu.present(TARGET, WIDTH, HEIGHT).map_err(named)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scene::bounce::{GRAVITY, SLOP};

    #[test]
    fn bounce_does_not_gain_energy_or_sink_into_contacts() {
        let mut bounce = Bounce::new();
        let energy = |bounce: &Bounce| bounce.balls.iter().map(|ball| {
            ball.radius.powi(2) * (0.5 * ball.velocity.iter().map(|v| v * v).sum::<f32>() + GRAVITY * (ball.at[1] + HALF[1]))
        }).sum::<f32>();
        let mut previous = energy(&bounce);
        let mut periods = 0;
        let mut knocks = 0;
        let mut most_knocks = 0;
        for frame in 0..600 {
            let target = (frame + 1) * 200 / 60;
            while periods < target {
                let period_knocks = bounce.physics().into_iter().filter(|&(_, speed)| speed > SOFT).count();
                knocks += period_knocks;
                most_knocks = most_knocks.max(period_knocks);
                periods += 1;
            }
            let current = energy(&bounce);
            assert!(current <= previous + 1e-5, "frame {frame} gained {} energy", current - previous);
            previous = current;
            let floor_overlap = bounce.balls.iter().map(|ball| ball.radius - ball.at[1] - HALF[1]).fold(0.0, f32::max);
            let ball_overlap = (0..BALLS).flat_map(|i| (i + 1..BALLS).map(move |j| (i, j))).map(|(i, j)| {
                let [x, y] = [0, 1].map(|axis| bounce.balls[j].at[axis] - bounce.balls[i].at[axis]);
                bounce.balls[i].radius + bounce.balls[j].radius - x.hypot(y)
            }).fold(0.0, f32::max);
            assert!(floor_overlap.max(ball_overlap) <= SLOP + 1e-5, "frame {frame} overlap {}", floor_overlap.max(ball_overlap));
        }
        assert!(most_knocks <= 4, "{most_knocks} collision sounds in one period");
        eprintln!("collision sounds: {knocks}, at most {most_knocks} per period");
    }
}
