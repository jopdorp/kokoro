---
social_title: Kokoromi demos
social_description: Watch graphics and sound demos running on Kokoromi.
---

# Demos

Download the [Linux emulator, compiler SDK, and every demo NRO](https://github.com/jopdorp/kokoromi/releases/tag/kokoromi-2026-10-19).

To make your own, see [Build a program](../build-a-program.md).

This page shows Kokoromi's graphics and sound demos. Every clip is captured from the emulator.

## What this does and does not show

Kokoromi emulates the hardware: an AArch64 CPU, an Ampere GPU, display and an audio DSP path. The GPU translator is validated against desktop sm_86 NVIDIA silicon. Command and shader-header layouts come from NVIDIA's published headers via Mesa.

Nintendo supplies the OS; Kokoromi does not emulate it. The small service layer in this repository is a stand-in that lets our own test programs drive the emulated GPU, display and audio. It follows Switch 1 interfaces because those are known.

Every demo is our own program built with our SDK. No Switch 2 binary, shader, firmware or exploit was used, and no retail or third-party binary has run. The demos show the hardware emulation running end to end with code written for it, not Switch 2 software compatibility.

## Running on the emulator {#guest-programs}

Each Nintendo Relocatable Object (NRO) is loaded by the emulated CPU, uses Ampere machine code through the GPU driver path, and sends sound through the audio renderer.

### Triangle

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-triangle.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-triangle.mp4" type="video/mp4"></video>

*A single triangle is drawn from a compact command list and presented to the display.*

### Sprites

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-sprites.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-sprites.mp4" type="video/mp4"></video>

*The stick moves the layers through the `hid` service. Halfway through, a button press flashes the scene.*

### Bounce

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-bounce.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-bounce.mp4" type="video/mp4"></video>

*Particles bounce under a simple physics simulation. Each collision makes a sound.*

### BoomBox

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-boombox.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-boombox.mp4" type="video/mp4"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/demo-raytraced.mp4" type="video/mp4"></video>

*The ray-traced scene turns the BoomBox and moving lamp inside the 3.0 × 3.9 × 4.8 metre room, with soft shadows, reflections and colour bounce. The CC0 track “Lofi Hip Hop Loop” by omfgdude demonstrates Doppler, cardioid directivity, panning, grille filtering, reflections and reverb.*

![Ray-traced scene with the old and new lighting](https://raw.githubusercontent.com/jopdorp/kokoromi/assets/demo-raytraced-gi-proof.png)

*The top row is v4 and the bottom row is v5. Look at the lamp's pool, the moving shadows and the wall colour on the BoomBox.*

### FlightHelmet

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-helmet.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-helmet.mp4" type="video/mp4"></video>

*The helmet is rendered with six materials, one draw for each material.*

### Post

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-post.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-post.mp4" type="video/mp4"></video>

*The pinwheel is drawn into one image, then sampled in a second pass for the screen effects.*

### Particles

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-particles.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-particles.mp4" type="video/mp4"></video>

*A compute update moves the particles before one instanced draw.*

### Solar system birth

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/demo-solar-showcase.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/demo-solar-showcase.mp4" type="video/mp4"></video>

*The simulation starts 2,048 bodies in a young disk; Kepler drift and mutual kicks move them around the star while overlapping bodies accrete.*

### Julia

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-julia.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-julia.mp4" type="video/mp4"></video>

*A small block of shader input data changes the Julia set over time.*

### Mandelbrot CUDA

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-mandelbrot.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-mandelbrot.mp4" type="video/mp4"></video>

*A double-precision dive into the seahorse valley, rendered with NVIDIA's CUDA sample kernel.*

### Tensor upscale

<div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:1rem">
<div><strong>Default</strong><video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-tensor.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-tensor.mp4" type="video/mp4"></video></div>
<div><strong>Exact</strong><video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-tensor-exact.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-tensor-exact.mp4" type="video/mp4"></video></div>
</div>

*This upscales a half-resolution frame with tensor-core matrix instructions. The default path takes 78.1 ms and exact takes 155.9 ms on an A40; the clips are visually identical, with only the default path's rounding changing their digests.*

### Ray-traced scene

![Ray-traced scene with the old and new lighting](https://raw.githubusercontent.com/jopdorp/kokoromi/assets/demo-raytraced-gi-proof.png)

*The top row is v4 and the bottom row is v5. Look at the lamp's pool, the moving shadows and the wall colour on the BoomBox.*

### Music

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-music.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/fullcircle-music.mp4" type="video/mp4"></video>

*Synthesized instruments build the mix heard in the clip.*

### Dungeon

<div style="display:flex;flex-wrap:wrap;gap:1rem">
<div style="flex:1 1 280px"><video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/demo-dungeon-raster.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/demo-dungeon-raster.mp4" type="video/mp4"></video><p><strong>Raster</strong></p></div>
<div style="flex:1 1 280px"><video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/demo-dungeon-raytraced.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/kokoromi/assets/demo-dungeon-raytraced.mp4" type="video/mp4"></video><p><strong>Ray traced</strong></p></div>
</div>

*The raster tour moves from the torch-lit hallway into the limestone cave, then follows the river past the fire. The ray-traced clip is the first 20 seconds of the tour at the full ray budget.*

## Measured

Median frame times for demos with published measurements:

| Demo | Variant | Time |
|---|---|---|
| Test frame, 1080p | exact | 4.2 ms |
| Julia set | exact | 4.7 ms |
| Triangle, 1080p | exact | 6.6 ms |
| CUDA Mandelbrot, 800x600 | exact | 10.2 ms |
| Tensor upscale | default, A40 | 78.1 ms |
| Tensor upscale | exact, A40 | 155.9 ms |
| Dungeon raster | default, A40 | 647 ms uncontended |

These are emulator measurements, not native Switch 2 measurements.

## Share

Paste the dungeon [clip](https://jopdorp.github.io/kokoromi/media/demo-dungeon-raster.mp4), [poster](https://jopdorp.github.io/kokoromi/media/demo-dungeon-raster.png), or [GIF teaser](https://jopdorp.github.io/kokoromi/media/teaser-dungeon.gif) straight into Discord.
The BoomBox [clip](https://jopdorp.github.io/kokoromi/media/fullcircle-boombox.mp4) and the sprites [clip](https://jopdorp.github.io/kokoromi/media/fullcircle-sprites.mp4) and [GIF teaser](https://jopdorp.github.io/kokoromi/media/teaser-sprites.gif) work there too.
