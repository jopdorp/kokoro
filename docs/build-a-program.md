# Build a program

The triangle is the smallest of the project's own test programs that runs on the emulator. Its source is [`guest/src/bin/triangle.rs`](https://github.com/jopdorp/kokoromi/blob/guest-path/guest/src/bin/triangle.rs).

## Set up

The emulator needs 64-bit Linux and a Vulkan 1.3 driver. Intel, AMD and NVIDIA drivers work. An NVIDIA card is not required: use lavapipe to run on the CPU when no Vulkan GPU is available.

These versions are known to work:

| Tool | Version |
| --- | --- |
| Rust and Cargo | 1.92.0 |
| Rust target | `aarch64-unknown-none` |
| Rust LLVM `objcopy` | 21.1.3 |
| glslangValidator | 16.2.0 |
| Mesa NAK | 25.2.7 |
| CUDA tools | 13.1 |

### From the release

Download the [kokoromi-2026-10-19 release](https://github.com/jopdorp/kokoromi/releases/tag/kokoromi-2026-10-19), check it, then unpack it:

```sh
url=https://github.com/jopdorp/kokoromi/releases/download/kokoromi-2026-10-19
curl -LO "$url/SHA256SUMS"
curl -LO "$url/kokoromi-sdk-kokoromi-2026-10-19.tar.gz"
curl -LO "$url/kokoromi-linux-x86_64-kokoromi-2026-10-19.tar.gz"
curl -LO "$url/kokoromi-demos-kokoromi-2026-10-19.tar.gz"
sha256sum -c SHA256SUMS
tar xzf kokoromi-sdk-kokoromi-2026-10-19.tar.gz
tar xzf kokoromi-linux-x86_64-kokoromi-2026-10-19.tar.gz
tar xzf kokoromi-demos-kokoromi-2026-10-19.tar.gz
```

- The SDK contains the Rust runtime, triangle template, linker script, shared scene code and NAK tools.
- The emulator contains `bin/kokoromi`, `bin/kokoromi-gui` and its README.
- The demos archive contains fourteen NROs, their checksums and asset-fetch scripts.

Install Rust, then build the SDK template:

```sh
rustup target add aarch64-unknown-none
rustup component add llvm-tools
cd kokoromi-sdk-kokoromi-2026-10-19
make
```

The result is `build/triangle.nro`. Use `make BIN=name` for another file under `guest/src/bin`.

### From source

```sh
git clone --branch guest-path https://github.com/jopdorp/kokoromi.git
cd kokoromi
rustup target add aarch64-unknown-none
rustup component add llvm-tools
./demos/assets/fetch.sh
cargo build --release -p kokoromi-core --bin kokoromi -j 4
```

The asset fetch supplies the models and audio used by the larger programs. The emulator is `target/release/kokoromi`.

### Choose how it runs

Speed hacks and the Dynarmic CPU backend are the defaults.

| Choice | Switch |
| --- | --- |
| Speed hacks | no switch |
| Exact | `KOKOROMI_EXACT=1` |
| Dynarmic CPU | no switch |
| Unicorn CPU | `KOKOROMI_CPU=unicorn` |
| lavapipe CPU Vulkan | `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json` |

## Build the CPU program

The program is `no_std` and starts at `_start`. It asks `sm` for services, then sends IPC requests to `nvdrv` and `vi` to allocate GPU memory, submit work and present a frame. Programs that need controls or sound reach `hid` and `audren` the same way.

```sh
cargo build --release --target aarch64-unknown-none \
  --manifest-path guest/Cargo.toml --target-dir target/guest --bin triangle
objcopy="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-objcopy"
"$objcopy" -O binary target/guest/aarch64-unknown-none/release/triangle triangle.nro
```

## Build shaders

CUDA C compute kernels compile to Switch 2 machine code on any machine with the CUDA toolkit; no card is needed. `nvcc` runs `ptxas`:

```sh
/usr/local/cuda-13.1/bin/nvcc -arch=sm_86 -cubin \
  core/tests/fixtures/cuda-seed.cu -o cuda-seed.cubin
```

Graphics and ray shaders use one of the following paths.

### With an NVIDIA card

This path needs an Ampere or newer card and the proprietary Linux driver. The driver compiles GLSL to exact Switch 2 machine code when the capture tool creates a Vulkan pipeline. Ray queries are supported.

```sh
glslangValidator -V shader.vert -o vert.spv
glslangValidator -V shader.frag -o frag.spv
gcc -O2 -o vk_cache_dump vk_cache_dump.c -lvulkan
./vk_cache_dump pipeline-cache.bin vert.spv frag.spv
python3 scan_blob.py sass-out pipeline-cache.bin
```

Use the `.sph.bin` header followed by `.words.bin` as the graphics program.

### Without an NVIDIA card

Mesa's NAK compiler works offline with any Vulkan GPU, or no GPU. Its raster output differs from the driver's output by at most one bit. NAK ray-query support is in progress, so ray shaders need the card path until it lands.

Linux distributions do not package Mesa's internal NAK library or `nak-offline`. Build the compiler from Mesa 25.2.7. On Ubuntu, install the normal Mesa build packages plus Rust 1.92, `bindgen` and `cbindgen`:

```sh
sudo apt install build-essential meson ninja-build pkg-config python3-mako \
  python3-yaml bison flex libdrm-dev libudev-dev libelf-dev libzstd-dev \
  zlib1g-dev libglvnd-dev llvm-21-dev glslang-tools
cargo install bindgen-cli --version 0.71.1
cargo install cbindgen --version 0.29.4
```

Build only the Nouveau compiler and the offline wrapper:

```sh
curl -LO https://archive.mesa3d.org/mesa-25.2.7.tar.xz
tar xf mesa-25.2.7.tar.xz
cp tools/nak-offline.c mesa-25.2.7/src/nouveau/compiler/nak_offline.c
cd mesa-25.2.7
sed -i 's/with_gallium_rusticl or with_nouveau_vk or/with_gallium_rusticl or with_any_nouveau or/' meson.build
sed -i '0,/if with_nouveau_vk/s//if with_any_nouveau/' src/compiler/meson.build
sed -i '0,/if with_nouveau_vk/s//if with_any_nouveau/' src/nouveau/meson.build
sed -i '0,/if with_nouveau_vk/s//if with_any_nouveau/' src/nouveau/headers/meson.build
printf '%s\n' "executable('nak-offline', files('nak_offline.c'), include_directories : [inc_include, inc_src], dependencies : [idep_nak, idep_vtn, idep_mesautil])" >> src/nouveau/compiler/meson.build
export BINDGEN_EXTRA_CLANG_ARGS="-isystem $(ls -d /usr/lib/gcc/x86_64-linux-gnu/*/include | tail -1)"
meson setup build . -Dgallium-drivers=nouveau -Dvulkan-drivers= \
  -Dplatforms= -Dglx=disabled -Dopengl=false -Dbuild-tests=false -Dmesa-clc=auto
ninja -C build -j 4 src/nouveau/compiler/nak-offline
```

Allow about two minutes for the clean compiler build. `tools/nak-compile.sh` finds `nak-offline` and `nvdisasm` on `PATH`, or uses `NAK_OFFLINE` and `NVDISASM`:

```sh
cd ..
TMPDIR="$HOME/.cache/tmp" \
NAK_OFFLINE="$PWD/mesa-25.2.7/build/src/nouveau/compiler/nak-offline" \
NVDISASM=/usr/local/cuda-13.1/bin/nvdisasm \
BINDINGS=u1,s2 tools/nak-compile.sh demos/shaders/post.vert sass-out
```

`BINDINGS` is the descriptor shim: `u` is a uniform buffer, `s` a sampler, `b` a storage buffer and `a` an acceleration structure. Omit it for shaders without bindings. Put the resulting `.bin` files under `core/tests/fixtures/driver-shaders`; programs such as `guest/src/bin/dungeon.rs` copy them to mapped GPU memory with `shader(address, bytes)`. The triangle writes its built-in shader bytes through `scene::triangle_scene`.

## Run a demo

Run the shipped triangle:

```sh
cd kokoromi-demos-kokoromi-2026-10-19
../kokoromi-linux-x86_64-kokoromi-2026-10-19/bin/kokoromi run demos/triangle.nro
```

It draws a red, green and blue triangle on white at 1920×1080, then prints the first-frame digest. The packaged Qt application provides a window:

```sh
../kokoromi-linux-x86_64-kokoromi-2026-10-19/bin/kokoromi-gui
```

From a source checkout, capture one frame or encode a clip:

```sh
cargo run --release -p kokoromi-core --example capture -- \
  /path/to/demos/triangle.nro --seconds 1 --fps 1 --out "$HOME/.cache/triangle"
cargo run --release -p kokoromi-core --example capture -- \
  /path/to/demos/triangle.nro --seconds 8 --fps 60 --out "$HOME/.cache/triangle-clip"
ffmpeg -framerate 60 -i "$HOME/.cache/triangle-clip/frame_%04d.ppm" \
  -c:v libx264 -crf 20 -pix_fmt yuv420p triangle.mp4
```

## Limits

- NAK does not compile ray queries; use the NVIDIA card path for them.
- The loader accepts NRO and flat AArch64 images, including an optional NRO RomFS. It does not load retail game formats.
