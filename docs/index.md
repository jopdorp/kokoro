---
social_title: Kokoromi Switch 2 emulator
social_description: See Kokoromi run its own graphics demos through the emulated hardware.
---

# Kokoromi

This is the documentation of Kokoromi, an early Nintendo Switch 2 emulator written in Rust. It builds on the CPU, service, GPU and front-end foundations laid by its maintainer and contributors. The project's own AArch64 programs run through the loader, kernel, stand-in services, audio renderer and GPU path.

The project's test programs hand the emulated Ampere GPU small programs called shaders, for example one that computes the colour of each pixel of a sky. Those shaders are NVIDIA machine code. The emulator rewrites them into SPIR-V, the shader format of the cross-vendor graphics API Vulkan. Then an Intel, AMD or NVIDIA card in a PC can run them.

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/julia-demo-poster.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/julia-demo.mp4" type="video/mp4"></video>

*The Julia demo dispatches Ampere machine code, translated to SPIR-V.*

| Start here | For |
|---|---|
| [Architecture map](architecture/README.md) | One picture of the whole emulator, and which page answers what |
| [Decision records](adr/README.md) | The big design choices, each explained in plain words first |
| [Glossary](glossary.md) | Every GPU term the pages use, with one plain sentence and an example |
| [SM86 instruction table (PDF)](sm86.pdf) | The desktop Ampere instruction encodings used by the translator, generated from NVIDIA's table |
