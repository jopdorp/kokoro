---
social_title: oboromi demos
social_description: Watch guest programs and graphics demos running through oboromi.
---

# Demos

This page shows oboromi's graphics and sound demos. Every demo except Dungeon runs as a guest program. Every clip is captured from the emulator on an Intel laptop unless its caption says otherwise.

## Guest programs

Each Nintendo Relocatable Object (NRO) is loaded by the loader and runs on the emulated ARM CPU. It calls the kernel and services, draws through the `nvdrv` GPU driver service on the emulated GPU with Switch 2 shader machine code, and presents through the display service. A controller reaches it through the `hid` input service, while sound goes through the audio renderer.

### Triangle

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-triangle.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-triangle.mp4" type="video/mp4"></video>

*The guest asks the kernel and services for memory and a display, writes a command list, sends it through `nvdrv`, and presents the triangle.*

### Sprites

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-sprites.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-sprites.mp4" type="video/mp4"></video>

*The stick moves the layers through the `hid` service. Halfway through, a button press flashes the scene.*

### Bounce

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-bounce.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-bounce.mp4" type="video/mp4"></video>

*The guest runs the physics and submits the particle shader through `nvdrv`. Collision sounds go through the audio renderer.*

### BoomBox

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-boombox.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-boombox.mp4" type="video/mp4"></video>

*The ray-traced guest turns the BoomBox and moving lamp inside the 3.0 × 3.9 × 4.8 metre room. Ray queries add soft shadows, reflections and colour bounce. The CC0 track “Lofi Hip Hop Loop” by omfgdude goes through the audio renderer: pitch adds Doppler, volumes set cardioid directivity and panning, a biquad dulls the grille off-axis, and the room dimensions and surfaces set the reflections and reverb.*

![Ray-traced scene with the old and new lighting](https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-raytraced-gi-proof.png)

*The top row is v4 and the bottom row is v5. Look at the lamp's pool, the moving shadows and the wall colour on the BoomBox.*

### FlightHelmet

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-helmet.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-helmet.mp4" type="video/mp4"></video>

*The guest draws all six materials through `nvdrv`, one draw for each material.*

### Post

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-post.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-post.mp4" type="video/mp4"></video>

*The guest draws the pinwheel into one image, then samples it in a second pass for the screen effects.*

### Particles

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-particles.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-particles.mp4" type="video/mp4"></video>

*The guest runs the compute update, then submits one instanced draw through `nvdrv`.*

### Solar system birth

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-solar-showcase.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-solar-showcase.mp4" type="video/mp4"></video>

*This guest program starts 2,048 bodies in a young disk. Kepler drift plus mutual kicks move them around the star, and overlapping bodies accrete. The showcase took 1,088.569 ms per frame at the median and 1,450.911 ms at p95 on the Intel Arc laptop.*

### Julia

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-julia.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-julia.mp4" type="video/mp4"></video>

*The guest sends the Julia set shader through `nvdrv` and passes time through a small block of shader input data.*

### Mandelbrot CUDA

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-mandelbrot.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-mandelbrot.mp4" type="video/mp4"></video>

*A double-precision dive into the seahorse valley. The guest owns the frame memory, launches NVIDIA's CUDA sample kernel through `nvdrv`, and presents it.*

### Tensor upscale

<video controls muted loop playsinline style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-tensor.mp4" type="video/mp4"></video>

*Default: speed hacks are on, including fast tensor accumulation.*

<video controls muted loop playsinline style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-tensor-exact.mp4" type="video/mp4"></video>

*Exact: speed hacks are off.*

### Ray-traced scene

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-raytraced-poster.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-raytraced.mp4" type="video/mp4"></video>

*Default: speed hacks are on. The guest places the geometry, instances and commands in its memory, then issues the draw with the ray-query shader through `nvdrv`. The emulator builds the acceleration structure from that memory. The lamp is now the main light. Its pool sweeps the red and green walls as it orbits, the shadows swing with it, and the walls' colour bounces onto the BoomBox. Soft shadows, chrome, glass and the glossy floor remain.*

<video controls muted loop playsinline style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-raytraced-exact.mp4" type="video/mp4"></video>

*Exact: speed hacks are off, including the ray budget and approximate CPU execution.*

![Ray-traced scene with the old and new lighting](https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-raytraced-gi-proof.png)

*The top row is v4 and the bottom row is v5. Look at the lamp's pool, the moving shadows and the wall colour on the BoomBox.*

### Music

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-music.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-music.mp4" type="video/mp4"></video>

*The guest makes the instruments and sends the mix through the audio renderer.*

## Dungeon

Dungeon runs as a guest program through the emulated CPU, `nvdrv`, GPU and audio renderer.

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon-raytraced.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon-raytraced.mp4" type="video/mp4"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon.mp4" type="video/mp4"></video>

*Dungeon ray traced, default: speed hacks are on. Ray queries make every flame shadow dance, reflect torches on wet calcite and add traced limestone bounce. This clip was rendered offline on the Intel laptop.*

<video controls loop playsinline style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon-raytraced-exact.mp4" type="video/mp4"></video>

*Dungeon ray traced, exact: speed hacks are off.*

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon-raster.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon-raster.mp4" type="video/mp4"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon-realtime.mp4" type="video/mp4"></video>

*Dungeon raster, default: speed hacks are on. Baked shadows and planar reflections make the same tour lighter to render. This clip was rendered offline on the Intel laptop.*

<video controls loop playsinline style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon-raster-exact.mp4" type="video/mp4"></video>

*Dungeon raster, exact: speed hacks are off.*

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/dungeon-v8-rt-compare.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon-compare.mp4" type="video/mp4"></video>

*Dungeon raster is left and Dungeon ray traced is right over the same fifteen reveal seconds. Every flame moves its traced shadow; wet calcite catches the torches. Mean absolute RGB difference is 8.94% over all 900 frames.*

![Raster and ray-query dungeon frame](https://raw.githubusercontent.com/jopdorp/oboromi/assets/dungeon-v8-rt-compare.png)

*The same reveal frame. The difference is clear in the warm ceiling bounce, softer formation shadows and wet calcite reflections.*

## Measured

Median frame times on the Intel Arc laptop used for the captures:

| Demo | Variant | Time |
|---|---|---|
| Test frame, 1080p | exact | 4.2 ms |
| Julia set | exact | 4.7 ms |
| Triangle, 1080p | exact | 6.6 ms |
| CUDA Mandelbrot, 800x600 | exact | 10.2 ms |
| Dungeon raster | exact | 41.7 ms |
| Dungeon ray traced | exact | 232.5 ms |

These are laptop measurements, not native Switch 2 measurements.

## Share

Paste the dungeon [clip](https://jopdorp.github.io/oboromi/media/demo-dungeon.mp4), [poster](https://jopdorp.github.io/oboromi/media/demo-dungeon-poster.png), or [GIF teaser](https://jopdorp.github.io/oboromi/media/teaser-dungeon.gif) straight into Discord.
The BoomBox [clip](https://jopdorp.github.io/oboromi/media/fullcircle-boombox.mp4) and the sprites [clip](https://jopdorp.github.io/oboromi/media/fullcircle-sprites.mp4) and [GIF teaser](https://jopdorp.github.io/oboromi/media/teaser-sprites.gif) work there too.
