# Audio

The audio renderer accepts voices, mixes, effects and sinks through the `audren` service. It produces one fixed period at a time and feeds a ring buffer used by Qt audio output. `audout` also plays queued PCM buffers.

| Renderer fact | Value |
|---|---|
| Sample rate | 48 kHz |
| Frames per period | 240 |
| Period | 5 ms |
| Output | Interleaved signed 16-bit stereo |

## What is implemented?

| Part | Support |
|---|---|
| Voices | PCM, ADPCM, resampling, pitch, volume ramps and biquads |
| Routing | Mixes, splitters and circular sinks |
| Effects | Delay, reverb, limiter, compressor, auxiliary send and de-pop |
| Output | Device sink, raw output and Qt ring buffer |
| Spatial extension | Distance, obstruction, reflections and measured binaural filtering |

The Dungeon spatialiser uses the SADIE II D1 measured HRIR set. It interpolates the measured grid and applies a fixed-tap filter without allocating in the audio path. The ray-traced BoomBox uses one source with cardioid directivity. The renderer path itself stays an emulator extension where it goes beyond the documented console service.

| Check | Result |
|---|---:|
| HRIR grid spacing | 15 degrees |
| Filter length | 160 taps |
| Right-side interaural delay | 39 samples |
| Right-side level difference | 15.73 dB |

The Dungeon runs on the emulator and submits its audio through `audren`. Its captured PCM is byte-equal to the reference path.

Verified by: the audio service, DSP, renderer and fixed-event PCM tests in `core/src/tests/audio`, plus the Dungeon sound tests.

Sources: `core/src/audio`, `core/src/nn/{audren,audout}.rs`, `core/src/audio/spatial.rs`, `scene/src/dungeon/sound`, `demos/assets/README`.
