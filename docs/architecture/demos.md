---
social_title: oboromi demos
social_description: Watch guest programs and graphics demos running through oboromi.
---

# Demos

This page shows oboromi's graphics and sound demos. Every clip is captured from the emulator on an Intel laptop unless its caption says otherwise.

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

*The BoomBox plays the CC0 track “Lofi Hip Hop Loop” by omfgdude. The guest places it in 3D the way a game does, through the audio renderer's voice parameters. Pitch adds Doppler as it orbits, while volumes set distance and panning, and a filter dulls it when the grille turns away. The reverb effect gives it a lively room instead of a box.*

### FlightHelmet

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-helmet.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-helmet.mp4" type="video/mp4"></video>

*The guest draws all six materials through `nvdrv`, one draw for each material.*

### Post

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-post.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-post.mp4" type="video/mp4"></video>

*The guest draws the pinwheel into one image, then samples it in a second pass for the screen effects.*

### Particles

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-particles.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-particles.mp4" type="video/mp4"></video>

*The guest runs the compute update, then submits one instanced draw through `nvdrv`.*

### Julia

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-julia.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-julia.mp4" type="video/mp4"></video>

*The guest sends the Julia set shader through `nvdrv` and passes time through a small block of shader input data.*

### Mandelbrot CUDA

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-mandelbrot.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-mandelbrot.mp4" type="video/mp4"></video>

*NVIDIA's CUDA sample kernel runs unchanged through the translator. The guest owns the frame memory, launches the kernel through `nvdrv` each frame with the new zoom, and presents it.*

### Music

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-music.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/fullcircle-music.mp4" type="video/mp4"></video>

*The guest makes the instruments and sends the mix through the audio renderer.*

## Still driven from the host

### Ray-traced scene

<video controls muted loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-raytraced-poster.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-raytraced.mp4" type="video/mp4"></video>

*Light now bounces once, so the red and green walls tint the BoomBox and the tint follows the moving lamp. Soft shadows, chrome, glass and the glossy floor work as before. The shaders are Switch 2 shader machine code through the translator, but the command stream is still built on the host. This clip was rendered offline on an RTX 3090 because the Intel laptop runs this shader slowly.*

![Ray-traced scene without and with bounce light](https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-raytraced-gi-proof.png)

*The top row is without bounce light and the bottom row is with it. Look at the BoomBox picking up the wall colour near the lamp.*

## Dungeon

Dungeon is a native Vulkan tech demo. It does not use the emulated CPU or GPU, but it drives the audio renderer and sets the bar for the guest version.

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon-poster.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon.mp4" type="video/mp4"></video>

*Showcase: ray queries add soft shadows, reflections and darker crevices. This clip was rendered offline on the Intel laptop.*

<video controls loop playsinline poster="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon-realtime-poster.png" style="width:100%"><source src="https://raw.githubusercontent.com/jopdorp/oboromi/assets/demo-dungeon-realtime.mp4" type="video/mp4"></video>

*Realtime: baked shadows and planar reflections make the same tour lighter to render. This clip was rendered offline on the Intel laptop.*

![Raster and ray-query dungeon frame](https://raw.githubusercontent.com/jopdorp/oboromi/assets/dungeon-v6-rt-compare.png)

*Raster is left and ray queries are right. The difference is small in this frame and clearest in the water.*

## Measured

Median frame times on the Intel Arc laptop used for the captures:

| Demo | Time |
|---|---|
| Test frame, 1080p | 4.2 ms |
| Julia set | 4.7 ms |
| Triangle, 1080p | 6.6 ms |
| CUDA Mandelbrot, 800x600 | 10.2 ms |
| Dungeon realtime | 23.8 ms |
| Dungeon showcase | 144.5 ms |

These are laptop measurements, not native Switch 2 measurements.

## Share

Paste the dungeon [clip](https://jopdorp.github.io/oboromi/media/demo-dungeon.mp4), [poster](https://jopdorp.github.io/oboromi/media/demo-dungeon-poster.png), or [GIF teaser](https://jopdorp.github.io/oboromi/media/teaser-dungeon.gif) straight into Discord.
The ray-traced [clip](https://jopdorp.github.io/oboromi/media/demo-raytraced.mp4) and [GIF teaser](https://jopdorp.github.io/oboromi/media/teaser-raytraced.gif), and the sprites [clip](https://jopdorp.github.io/oboromi/media/fullcircle-sprites.mp4) and [GIF teaser](https://jopdorp.github.io/oboromi/media/teaser-sprites.gif), work there too.
