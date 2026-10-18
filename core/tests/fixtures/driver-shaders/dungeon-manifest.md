# Dungeon guest shaders, SM86 SASS

Pod tj7n3k5ucaloxo, RunPod Secure RTX 3090 EU-CZ-1, driver 580.126.20 (host CUDA 13.0), runpod/pytorch:1.0.2-cu1281-torch280-ubuntu2404, rig vk_cache_dump rebuilt with bindings[32]. Sources: /home/jopdorp/development/oboromi-dungeon-shaders/demos/shaders/guest-dungeon (read-only), BINDINGS exactly as in dungeon-sass.needpod.
Files under core/tests/fixtures/driver-shaders/: graphics .bin = 0x80-byte SPH + code, compute .bin = code only; .sass = nvdisasm --print-raw.

Columns: words = 128-bit instructions; regs = highest R referenced + 1 (64/128-bit destination tails counted), RZ excluded; STL/LDL = local-memory store/load count; offsets = every distinct constant offset in the descriptor bank (c[0x1d] graphics, c[0x0] compute) read from the SASS; other banks = further banks referenced (c[0x1] is indexed constant tables, c[0x0][0x4] in the vertex stages); SHA-256 is for the .bin.

| stage | words | regs | STL/LDL | descriptor offsets | other banks | SHA-256 |
|---|---|---|---|---|---|---|
| dungeon.vert | 110 | 18 | 0/0 | 0x1d: 0x28 0x90 0x98 | - | f9a193890c3ae7475a9e9140459422c1756342f32e7e0a6b86394c8a7c39cfa3 |
| dungeon.frag | 2694 | 46 | 0/0 | 0x1d: 0x28 0x30 0x38 0x40 0x48 0x50 0x58 0x70 0x78 0x80 0x88 | 0x1 | 7eb6b5f0c6698499dd8cf5b9233dfe2bae5f23408ad37537e4856b8d9cdad5b1 |
| dungeon_rt.vert | 110 | 18 | 0/0 | 0x1d: 0x28 0xa0 0xa8 | - | a68cd86eebd33c8e9036f3fd3dfa4a75aa38e2a71ad68cda48e86166a37038f2 |
| dungeon_rt.frag | 9504 | 70 | 0/0 | 0x1d: 0x28 0x30 0x38 0x40 0x44 0x50 0x58 0x60 0x68 0x70 0x78 0x80 0x88 0x90 0x98 | 0x1 | 54c59446a640e6b41d4c2a83133bd8c9f6e593c9b0ec11e968f9200728acbe8b |
| dungeon_cones.vert | 215 | 23 | 0/0 | 0x1d: 0x28 0x90 0x98 | 0x0 0x1 | 43a8e2d789ad47bcd6e1df7debc887aad185ea3dd6fd97242c7efb3eeb90b862 |
| dungeon_cones.frag | 2694 | 46 | 0/0 | 0x1d: 0x28 0x30 0x38 0x40 0x48 0x50 0x58 0x70 0x78 0x80 0x88 | 0x1 | 7eb6b5f0c6698499dd8cf5b9233dfe2bae5f23408ad37537e4856b8d9cdad5b1 |
| dungeon_cones_rt.vert | 215 | 23 | 0/0 | 0x1d: 0x28 0xa0 0xa8 | 0x0 0x1 | a54633dc25851d3c75652fb4e8d129b78c682f3c31c11735fe10f453c811872c |
| dungeon_cones_rt.frag | 9504 | 70 | 0/0 | 0x1d: 0x28 0x30 0x38 0x40 0x44 0x50 0x58 0x60 0x68 0x70 0x78 0x80 0x88 0x90 0x98 | 0x1 | 54c59446a640e6b41d4c2a83133bd8c9f6e593c9b0ec11e968f9200728acbe8b |
| dungeon_sprite.vert | 190 | 30 | 0/0 | 0x1d: 0x8 0x30 0x38 | 0x0 0x1 | edfaadf23d7db44f32b2a3496fd7bad5d653227368ef8f4aec0091ca878662c9 |
| dungeon_sprite.frag | 1097 | 38 | 0/0 | 0x1d: 0x8 0x10 0x18 0x20 0x28 | - | 862409ca3ab36ceda979039c2a041bb6dfca78753b1d579d3fd1c594e9461ed6 |
| dungeon_water.comp | 160 | 24 | 0/0 | 0x0: 0x30 0x40 0x48 0x50 0x58 0x60 0x68 | - | 876230f1259de6e63451428321184579205d76cdc4c21781c191d2af98df12a8 |
| dungeon_light.comp | 80 | 10 | 0/0 | 0x0: 0x30 0x40 0x48 0x50 0x58 | - | b210ef77cd81aec36690528bd79122dc978509f473ff57ff3c84c08e688c7fb5 |
| dungeon_particles.comp | 1872 | 62 | 0/0 | 0x0: 0x30 0x40 0x48 0x50 0x58 | - | bde54c15d8a886395850b03ba9108d4aead7352ac646e8d5dee337187a78215f |
| dungeon_fog.comp | 598 | 43 | 0/0 | 0x0: 0x30 0x40 0x48 0x50 0x58 0x60 0x68 0x70 0x78 | - | 1316c063061a3c21f1b16e0f7883c0ec0ea77171a7030d972a46fd24cba9ecd3 |
| dungeon_post.comp | 841 | 70 | 0/0 | 0x0: 0x30 0x40 0x48 0x50 0x58 0x60 0x68 0x70 0x78 0x80 0x88 | - | 1380632ffe4e70b5a095ffb2c588a4c51d58c5dadcd9fdc5030d793cea4c64fd |

Notes: dungeon.frag and dungeon_cones.frag are byte-identical, as are dungeon_rt.frag and dungeon_cones_rt.frag (same frag source per pair). Multi-EXIT programs were taken whole (the longest nvdisasm-clean run). No stage spills.
The fire sprite and particle stages were rebuilt on pod f8d7yaymimfujn, A40 SM86, driver 580.159.03.
