---
social_title: oboromi Switch 2 emulator
social_description: See oboromi run Switch 2 graphics demos through its emulator.
---

# oboromi

This is the documentation of oboromi, an early Nintendo Switch 2 emulator written in Rust. It builds on the CPU, service, GPU and front-end foundations laid by its maintainer and contributors. Included AArch64 programs now run through the loader, kernel, services, audio renderer and GPU path.

A game hands the Switch 2's GPU small programs called shaders, for example one that computes the colour of each pixel of a sky. Those shaders are NVIDIA machine code. The emulator rewrites them into SPIR-V, the shader format of the cross-vendor graphics API Vulkan. Then an Intel, AMD or NVIDIA card in a PC can run them.

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/julia-demo-poster.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/julia-demo.mp4" type="video/mp4"></video>

*The Julia demo runs on the emulator and dispatches Switch 2 shader machine code, translated to SPIR-V, on an Intel laptop GPU.*

| Start here | For |
|---|---|
| [Architecture map](architecture/README.md) | One picture of the whole emulator, and which page answers what |
| [Decision records](adr/README.md) | The big design choices, each explained in plain words first |
| [Glossary](glossary.md) | Every GPU term the pages use, with one plain sentence and an example |
| [SM86 instruction table (PDF)](sm86.pdf) | Every instruction encoding of the Switch 2's GPU, generated from NVIDIA's table |
