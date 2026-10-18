use crate::audio::demo_music::{music, sequence};
use crate::gpu::demo_bounce::Bounce;

#[test]
fn music_demo_renders_the_same_non_silent_periods_every_run() {
    let run = || {
        let mut sound = music();
        (0..400).flat_map(|_| {
            sequence(&mut sound);
            sound.period().to_vec()
        }).collect::<Vec<i16>>()
    };
    let first = run();
    assert!(first.iter().any(|&s| s != 0));
    assert_eq!(first, run());
}

#[test]
fn bounce_demo_knocks_a_voice_within_the_first_second() {
    let mut bounce = Bounce::new();
    assert!((0..200).any(|_| {
        bounce.step();
        bounce.sound.period().iter().any(|&s| s != 0)
    }));
}

#[test]
fn guest_dungeon_spatializer_matches_host_pcm() {
    use crate::audio::spatial as host;
    use scene::dungeon::sound as guest;

    let material = host::Material { absorption: 0.18, scattering: 0.32 };
    let doorway = host::Doorway { min: [-0.05, -1.0, -0.8], max: [0.05, 1.5, 0.8], material: host::Material::OPEN };
    let host_rooms = [
        host::Room { min: [-5.0, -2.0, -4.0], max: [0.0, 3.0, 4.0], walls: [material; 6], doorways: vec![doorway] },
        host::Room { min: [0.0, -2.0, -4.0], max: [6.0, 3.0, 4.0], walls: [material; 6], doorways: vec![doorway] },
    ];
    let guest_material = guest::Material { absorption: 0.18, scattering: 0.32 };
    let guest_doorways = [guest::Doorway { min: doorway.min, max: doorway.max, material: guest::Material { absorption: doorway.material.absorption, scattering: doorway.material.scattering } }];
    let guest_rooms = [
        guest::Room { min: host_rooms[0].min, max: host_rooms[0].max, walls: [guest_material; 6], first_doorway: 0, doorway_count: 1 },
        guest::Room { min: host_rooms[1].min, max: host_rooms[1].max, walls: [guest_material; 6], first_doorway: 0, doorway_count: 1 },
    ];
    let mut history = vec![0.0; guest::HISTORY];
    let mut fdn = vec![0.0; guest_rooms.len() * guest::FDN_SAMPLES];
    let mut host = host::Spatializer::new(&host_rooms, host::Mode::Headphones);
    let mut guest = guest::Spatializer::new(&guest_rooms, &guest_doorways, 1, &mut history, &mut fdn, guest::Mode::Headphones).unwrap();
    assert!((guest::coefficient_debug(14_000.0) - (1.0 - (-std::f32::consts::TAU * 14_000.0 / 48_000.0).exp())).abs() < 1e-6, "{}", guest::coefficient_debug(14_000.0));
    for block in 0..40 {
        let listener = [-1.4 + block as f32 * 0.05, 0.2, 0.1];
        let forward = [0.7, 0.0, -0.7];
        host.set_listener(listener, forward, [0.0, 1.0, 0.0]);
        guest.set_listener(listener, forward, [0.0, 1.0, 0.0]);
        let mono = std::array::from_fn(|n| if block == 0 && n == 0 { 1.0 } else { ((n * 7919 + block * 101) % 257) as f32 / 1024.0 - 0.125 });
        let mut host_out = [[0.0; 2]; host::BLOCK];
        let mut guest_out = [[0.0; 2]; guest::BLOCK];
        host.begin_block();
        guest.begin_block();
        host.render(&host::Source { id: 0, position: [2.5, 0.4, -1.2], gain: 0.7 }, &mono, &mut host_out);
        assert!(guest.render(guest::Source { id: 0, position: [2.5, 0.4, -1.2], gain: 0.7 }, &mono, &mut guest_out));
        host.finish_block(&mut host_out);
        guest.finish_block(&mut guest_out);
        for (sample, (host, guest)) in host_out.iter().flatten().zip(guest_out.iter().flatten()).enumerate() {
            let pcm = |sample: f32| (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
            assert_eq!(pcm(*host), pcm(*guest), "block {block} sample {sample}: {host} != {guest}");
        }
    }
}

#[test]
fn guest_dungeon_events_match_round_five_pcm() {
    use crate::audio::spatial as host;
    use scene::dungeon::sound as guest;

    let torch: Vec<i16> = (0..521).map(|n| ((n * 7919 % 2048) as i16 - 1024) * 8).collect();
    let steps: [[Vec<i16>; 6]; 4] = std::array::from_fn(|set| std::array::from_fn(|variation| (0..367).map(|n| ((n * (97 + set * 13 + variation * 7) % 1024) as i16 - 512) * 12).collect()));
    let mut recordings = [&torch[..]; 13];
    recordings[guest::Sample::Step as usize] = &steps[0][0];
    let step_refs = std::array::from_fn(|set| std::array::from_fn(|variation| &steps[set][variation][..]));
    let samples = guest::Samples { recordings, drips: [&torch; 3], steps: step_refs };
    let positions = [[2.0, 0.4, -1.0], [1.0, 0.0, 1.0], [2.0, 0.0, 1.0], [3.0, 0.0, 1.0], [4.0, 0.0, 1.0]];
    let mut voices = [
        guest::Voice::new(guest::Sample::Torch, positions[0], 0.45, 1.01, true),
        guest::Voice::new(guest::Sample::Drip, positions[1], 0.0, 1.0, false),
        guest::Voice::new(guest::Sample::Drip, positions[2], 0.0, 1.0, false),
        guest::Voice::new(guest::Sample::Drip, positions[3], 0.0, 1.0, false),
        guest::Voice::new(guest::Sample::Drip, positions[4], 0.0, 1.0, false),
        guest::Voice::new(guest::Sample::Scream, positions[0], 0.0, 1.0, false),
        guest::Voice::new(guest::Sample::Step, positions[0], 0.0, 1.0, false),
        guest::Voice::new(guest::Sample::Step, positions[0], 0.0, 1.0, false),
    ];
    let material = guest::Material { absorption: 0.18, scattering: 0.32 };
    let rooms = [guest::Room { min: [-5.0, -2.0, -4.0], max: [6.0, 3.0, 4.0], walls: [material; 6], first_doorway: 0, doorway_count: 0 }];
    let mut history = vec![0.0; voices.len() * guest::HISTORY];
    let mut fdn = vec![0.0; guest::FDN_SAMPLES];
    let outside = guest::Box3 { min: [20.0; 3], max: [21.0; 3] };
    let bridge = guest::Box3 { min: [-0.5, -1.0, -0.5], max: [0.5, 1.0, 0.5] };
    let storage = guest::Storage { voices: &mut voices, histories: &mut history, fdn: &mut fdn };
    let environment = guest::Environment { rooms: &rooms, doorways: &[], bridge, reveal: outside, ambient: false };
    let mut actual = guest::Engine::new(samples, storage, environment, guest::Mode::Headphones).unwrap();

    struct Event<'a> { id: usize, pcm: &'a [i16], position: [f32; 3], gain: f32, pitch: f32, cursor: f32, active: bool }
    let host_room = host::Room { min: rooms[0].min, max: rooms[0].max, walls: [host::Material { absorption: 0.18, scattering: 0.32 }; 6], doorways: vec![] };
    let mut expected = host::Spatializer::new(&[host_room], host::Mode::Headphones);
    let mut events = vec![Event { id: 0, pcm: &torch, position: positions[0], gain: 0.45, pitch: 1.01, cursor: 0.0, active: true }];
    assert!((guest::step_cadence(0.7) - 1.3).abs() < 0.01);
    for block in 0..8 {
        let listener = [-1.0 + block as f32 * 0.03, 0.2, 0.1];
        actual.listen(listener, 0.7, 0.005, guest::Mode::Headphones);
        expected.set_listener(listener, [0.7f32.sin(), 0.0, -0.7f32.cos()], [0.0, 1.0, 0.0]);
        if block == 0 {
            actual.play(guest::Sample::Step, [1.2, 0.0, -0.7], 0.04, 1.0);
            events.push(Event { id: 7, pcm: &steps[0][5], position: [1.2, 0.0, -0.7], gain: 0.04 * 10.0f32.powf(0.5 / 20.0), pitch: 1.0, cursor: 0.0, active: true });
        }
        if block == 2 {
            actual.play(guest::Sample::WetStep, [0.0; 3], 0.04, 1.0);
            events.push(Event { id: 6, pcm: &steps[3][4], position: [0.0; 3], gain: 0.04, pitch: 1.0, cursor: 0.0, active: true });
        }
        let actual = actual.render();
        let mut mix = host::Mix::new();
        expected.begin_block();
        for event in &mut events {
            let mut mono = [0.0; host::BLOCK];
            for sample in &mut mono {
                if !event.active { break; }
                let at = event.cursor as usize;
                if at >= event.pcm.len() {
                    if event.id == 0 { event.cursor %= event.pcm.len() as f32; } else { event.active = false; break; }
                }
                let at = event.cursor as usize;
                let next = (at + 1).min(event.pcm.len() - 1);
                let (a, b) = (event.pcm[at] as f32 / 32768.0, event.pcm[next] as f32 / 32768.0);
                *sample = a + (b - a) * event.cursor.fract();
                event.cursor += event.pitch;
            }
            let mut stereo = [[0.0; 2]; host::BLOCK];
            expected.render(&host::Source { id: event.id, position: event.position, gain: event.gain }, &mono, &mut stereo);
            mix.add(&stereo);
        }
        let mut reverb = [[0.0; 2]; host::BLOCK];
        expected.finish_block(&mut reverb);
        mix.add(&reverb);
        let expected = mix.finish().map(|frame| frame.map(|sample| {
            let sample = sample * 36.0 * 0.46;
            let magnitude = sample.abs();
            let limited = sample.signum() * if magnitude <= 0.72 { magnitude } else { 0.72 + 0.12 * (1.0 - (-(magnitude - 0.72) / 0.12).exp()) };
            (limited * 32767.0) as i16
        }));
        for (sample, (expected, actual)) in expected.iter().flatten().zip(actual.iter().flatten()).enumerate() {
            assert_eq!(expected, actual, "block {block} sample {sample}: {expected} != {actual}");
        }
    }
}

#[test]
fn guest_dungeon_pcm_voice_reads_each_scheduled_period() {
    use crate::audio::{HostMemory, renderer::{AudioRendererParameter, Renderer, work_buffer_size}};
    use scene::dungeon::sound as guest;

    let words = guest::pcm_parameter(1);
    let parameter = AudioRendererParameter {
        sample_rate: words[0], sample_count: words[1], mix_buffer_count: words[2],
        sub_mix_count: words[3], voice_count: words[4], sink_count: words[5],
        effect_count: words[6], performance_frame_count: words[7], splitter_count: words[9],
        splitter_destination_count: words[10], revision: words[12],
    };
    let mut work = vec![0; work_buffer_size(&parameter).unwrap() as usize];
    let mut renderer = Renderer::open(parameter, &mut work).unwrap();
    let base = 0x10_0000;
    let mut memory = HostMemory { base, bytes: vec![0; guest::PCM_BYTES] };
    let write = |memory: &mut HostMemory, period: i16| {
        for channel in 0..2 {
            for frame in 0..guest::BLOCK {
                let sample = period + frame as i16 + channel as i16 * 300;
                let at = channel * 0x200 + frame * 2;
                memory.bytes[at..at + 2].copy_from_slice(&sample.to_le_bytes());
            }
        }
    };
    write(&mut memory, 10);
    let mut parcel = vec![0; guest::pcm_parcel_size(1)];
    guest::pcm_parcel(&mut parcel, base, true, None);
    renderer.request_update(&parcel, &mut vec![0; 0x4000]).unwrap();
    renderer.start();
    for period in [10, 20] {
        write(&mut memory, period);
        let output = renderer.step(&mut memory);
        let expected: Vec<_> = (0..guest::BLOCK).flat_map(|frame| [period + frame as i16, period + frame as i16 + 300]).collect();
        assert_eq!(output, expected);
    }
}
