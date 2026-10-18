# Current coverage and limits

The project's own demos run through the loader, CPU, kernel, stand-in services, GPU and audio paths. No retail or third-party binary has run, and Nintendo's OS is not emulated.

| Area | Current state |
|---|---|
| CPU | Dynarmic by default; Unicorn for exact and comparison runs |
| Kernel | Threads, scheduling, handles, memory, IPC and events used by the demos |
| Service stand-ins | GPU, display, audio, input, time, applet and filesystem paths used by the demos |
| GPU commands | 3D and compute classes, draw state, textures, tensors, ray queries and presentation |
| Translator | Broad compute and graphics coverage with structured control flow |
| Audio | Renderer, raw output, DSP effects and measured-HRIR spatial extension |
| Loader | The project's own NRO images; no retail game formats |

## Translator coverage

| Corpus | Handled |
|---|---:|
| CUDA sample kernels | 768 / 818 |
| GENGA instruction cases | 167 / 167 |

The remaining CUDA cases include unsupported runtime facilities such as device-side formatted output. Named instruction gaps remain for sparse tensor operations, image-footprint results, some graphics transport forms and other hardware behaviour without a safe mapping.

## Accuracy and speed

Anything that is not bit-exact against the hardware oracle is a speed hack. Speed hacks are on by default. Exact mode is one opt-in.

| Mode | Selection |
|---|---|
| Default | Dynarmic, fast tensor path, bounded ray work and non-blocking pipeline compilation |
| Exact | `OBOROMI_EXACT=1` or `SpeedHacks::none()` |
| CPU comparison | `OBOROMI_CPU=unicorn` |

## Hardware-backed limits

The translator is checked against desktop sm_86 Ampere silicon. NVIDIA's SM86 and SM88 instruction tables are byte-identical, but no Switch 2 shader has been seen and no code here has been validated on a physical Switch 2.

`FOOTPRINT` fields have been measured on SM86, while its complete raw result and portable Vulkan mapping remain open. `BAR.SCAN` runs only with a full-warp count in the tested forms, but its result was not observable through the documented barrier readers. Those instructions therefore remain named failures instead of guessed implementations.

Verified by: the subsystem suites named on each architecture page, the CUDA corpus probe and the silicon fixture scoreboards.

Sources: `core/src/cpu`, `core/src/kernel`, `core/src/nn`, `core/src/audio`, `core/src/gpu`, `guest`, `scene`.
