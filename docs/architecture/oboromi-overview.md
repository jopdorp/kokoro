# Emulator overview

oboromi builds on a Rust core, a Qt front end, emulated ARM cores, a system-service registry, an NVIDIA instruction table and a SPIR-V writer. The current path connects those foundations into one machine: the loader starts an AArch64 image, the kernel handles its calls, services drive graphics and audio, and the window presents the result.

| Part | Where | What works today |
|---|---|---|
| CPU | `core/src/cpu` | Dynarmic and Unicorn execute eight AArch64 cores over one RAM block |
| Kernel | `core/src/kernel` | Threads, scheduling, handles, memory regions, events and IPC dispatch |
| Services | `core/src/nn` | Service manager, GPU driver, display, binder, audio, input, time and filesystem paths |
| Loader | `core/src/loader` | NRO and flat-image loading |
| Audio | `core/src/audio` | Voices, mixes, effects, output, measured HRIR spatialisation and raw audio output |
| GPU | `core/src/gpu` | Command processing, SASS translation, compute, graphics, textures, tensors and ray queries |
| Front end | `gui-qt` | Input, demo controls, frames and sound |
| Demo images | `guest`, `scene` | `no_std` AArch64 images and shared scene encoders |

## How does a frame reach the window?

An image loaded by the emulator calls the GPU driver services, writes commands into emulated RAM and submits them. The GPU engine translates the Switch 2 shader machine code, runs it through Vulkan and writes the result back. The display service queues that buffer, then `display::present` notifies the Qt window.

The same route runs the Dungeon, ray-traced BoomBox, Mandelbrot precision dive, solar-system birth and the smaller demos. Tests may call lower layers directly as oracles, but the demo buttons use the emulator path.

Commercial games and firmware do not boot yet. Game-format loading and the remaining system surface are still in progress.

Verified by: the loader, kernel, service, audio and GPU suites, plus the fixed-frame demo tests in `core/src/guest.rs`.

Sources: `core/src/{guest,kernel}.rs`, `core/src/cpu`, `core/src/nn`, `core/src/audio`, `core/src/gpu`, `guest`, `scene`, `gui-qt`.
