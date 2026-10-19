# Dungeon guest shaders, SM86 SASS

Pod tj7n3k5ucaloxo, RunPod Secure RTX 3090 EU-CZ-1, driver 580.126.20 (host CUDA 13.0), runpod/pytorch:1.0.2-cu1281-torch280-ubuntu2404, rig vk_cache_dump rebuilt with bindings[32]. Sources: /home/jopdorp/development/kokoromi-dungeon-shaders/demos/shaders/guest-dungeon (read-only), BINDINGS exactly as in dungeon-sass.needpod.
Raster stages below were rebuilt from the current demos/shaders GLSL (set/binding remap in demos/shaders/guest-dungeon) with Mesa 25.2.7 NAK via tools/nak-compile.sh. The ray-query stages retain the driver build above. NAK temporary-array dereferences are lowered before compilation; regenerated stages have no STL/LDL.
Files under core/tests/fixtures/driver-shaders/: graphics .bin = 0x80-byte SPH + code, compute .bin = code only; .sass = nvdisasm --print-raw.

Columns: words = 128-bit instructions; regs = NAK allocator count for NAK rows, otherwise highest R referenced + 1 (64/128-bit destination tails counted), RZ excluded; STL/LDL = local-memory store/load count; offsets = every distinct constant offset in the descriptor bank (c[0x1d] graphics, c[0x0] compute) read from the SASS; other banks = further banks referenced (c[0x1] is indexed constant tables, c[0x0][0x4] in the vertex stages); SHA-256 is for the .bin.

| stage | words | regs | STL/LDL | descriptor offsets | other banks | SHA-256 |
|---|---|---|---|---|---|---|
| dungeon.vert (NAK) | 153 | 19 | 0/0 | 0x1d: 0x28 0x2c 0x90 0x94 0x98 0x9c | - | ff2b01db9b12ff727db34d06599f450ed331f2a4fe1167075d68626cd8eff1bf |
| dungeon.frag (NAK) | 5709 | 55 | 0/0 | 0x1d: 0x28 0x2c 0x30 0x34 0x38 0x3c 0x40 0x44 0x48 0x4c 0x50 0x54 0x58 0x5c 0x70 0x74 0x78 0x7c 0x80 0x84 0x88 0x8c | - | a65096a47e65fbab889aa107e411097752e2387092dcfb0f0f997adcedff6a5d |
| dungeon_rt.vert | 110 | 18 | 0/0 | 0x1d: 0x28 0xa0 0xa8 | - | a68cd86eebd33c8e9036f3fd3dfa4a75aa38e2a71ad68cda48e86166a37038f2 |
| dungeon_rt.frag | 9504 | 70 | 0/0 | 0x1d: 0x28 0x30 0x38 0x40 0x44 0x50 0x58 0x60 0x68 0x70 0x78 0x80 0x88 0x90 0x98 | 0x1 | 54c59446a640e6b41d4c2a83133bd8c9f6e593c9b0ec11e968f9200728acbe8b |
| dungeon_cones.vert (NAK) | 295 | 22 | 0/0 | 0x1d: 0x28 0x2c 0x90 0x94 0x98 0x9c | 0x0 | a8c35f87cda462de23ee69826f85a31e48b64131f0e73f6ca5e6e25b9a28cb8d |
| dungeon_cones.frag (NAK) | 5709 | 55 | 0/0 | 0x1d: 0x28 0x2c 0x30 0x34 0x38 0x3c 0x40 0x44 0x48 0x4c 0x50 0x54 0x58 0x5c 0x70 0x74 0x78 0x7c 0x80 0x84 0x88 0x8c | - | a65096a47e65fbab889aa107e411097752e2387092dcfb0f0f997adcedff6a5d |
| dungeon_cones_rt.vert | 215 | 23 | 0/0 | 0x1d: 0x28 0xa0 0xa8 | 0x0 0x1 | a54633dc25851d3c75652fb4e8d129b78c682f3c31c11735fe10f453c811872c |
| dungeon_cones_rt.frag | 9504 | 70 | 0/0 | 0x1d: 0x28 0x30 0x38 0x40 0x44 0x50 0x58 0x60 0x68 0x70 0x78 0x80 0x88 0x90 0x98 | 0x1 | 54c59446a640e6b41d4c2a83133bd8c9f6e593c9b0ec11e968f9200728acbe8b |
| dungeon_sprite.vert (NAK) | 459 | 32 | 0/0 | 0x1d: 0x8 0xc 0x30 0x34 0x38 0x3c | 0x0 | ed09b6a510dab7cc5147a750efaf1109d690d8aa45f6a4c8f43c00dd2b2afdea |
| dungeon_sprite.frag (NAK) | 1335 | 39 | 0/0 | 0x1d: 0x8 0xc 0x10 0x14 0x18 0x1c 0x20 0x24 0x28 0x2c | - | 33a4f4e5d4c20149d74287c976e97d7316f736ce7a629004fbeb84614f28e5a3 |
| dungeon_water.comp (NAK) | 812 | 25 | 0/0 | 0x0: 0x30 0x34 0x40 0x44 0x48 0x4c 0x50 0x54 0x58 0x5c 0x60 0x64 0x68 0x6c | - | cd63bd2b127dcb693cf285715e4f797755c6ac62d29726829d9a78976bdd7c31 |
| dungeon_light.comp (NAK) | 191 | 19 | 0/0 | 0x0: 0x30 0x34 0x40 0x44 0x48 0x4c 0x50 0x54 0x58 0x5c | - | f460cf7a6b3da206542689ad72be19f7472e1ef70ade0f22477787838b0937eb |
| dungeon_particles.comp (NAK) | 2491 | 37 | 0/0 | 0x0: 0x30 0x34 0x40 0x44 0x48 0x4c 0x50 0x54 0x58 0x5c | - | 20e77f942a104e7e839e0720222930e5b9e1915d760970b7b466f94a2ee5cd46 |
| dungeon_fog.comp (NAK) | 1474 | 36 | 0/0 | 0x0: 0x30 0x34 0x40 0x44 0x48 0x4c 0x50 0x54 0x58 0x5c 0x60 0x64 0x68 0x6c 0x70 0x74 0x78 0x7c | - | 34277ab7348bf400cf9d22c0661fd04bf8dff0b1155d827bbd694748eb4f923b |
| dungeon_post.comp (NAK) | 2784 | 39 | 0/0 | 0x0: 0x30 0x34 0x40 0x44 0x48 0x4c 0x50 0x54 0x58 0x5c 0x60 0x64 0x68 0x6c 0x70 0x74 0x78 0x7c 0x80 0x84 0x88 0x8c | - | e669e671e24d577c0c9e68af7e06d704d368a58d76091844f25e6be098be1f1b |

Notes: dungeon.frag and dungeon_cones.frag are byte-identical, as are dungeon_rt.frag and dungeon_cones_rt.frag (same frag source per pair). Driver multi-EXIT programs were taken whole (the longest nvdisasm-clean run). No stage spills.
