# Build a program

The triangle is the smallest of the project's own test programs that runs on the emulator. Its source is [`guest/src/bin/triangle.rs`](https://github.com/jopdorp/oboromi/blob/guest-path/guest/src/bin/triangle.rs).

## What you need

These versions are known to work:

| Tool | Version |
| --- | --- |
| Rust and Cargo | 1.92.0 |
| Rust target | `aarch64-unknown-none` |
| Rust LLVM `objcopy` | 21.1.3 |
| glslangValidator | 16.2.0 |
| Mesa NAK | 25.2.7 |
| CUDA tools for the NVIDIA path | 13.1 |

Install the Rust target and LLVM tools:

```sh
rustup target add aarch64-unknown-none
rustup component add llvm-tools
./demos/assets/fetch.sh
```

The build runs on a Linux x86-64 host. The asset fetch is needed because the crate's build script prepares data shared by all its binaries.

## Build the CPU program

The program is `no_std` and starts at `_start`. It asks `sm` for services, then sends IPC requests to `nvdrv` and `vi` to allocate GPU memory, submit work and present a frame. Programs that need controls or sound use the same path to reach `hid` and `audren`.

```sh
cargo build --release --target aarch64-unknown-none \
  --manifest-path guest/Cargo.toml --target-dir target/guest --bin triangle
objcopy="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-objcopy"
"$objcopy" -O binary target/guest/aarch64-unknown-none/release/triangle triangle.nro
```

## Build shaders

The repository's exact fixtures come from GLSL compiled through the NVIDIA driver on an NVIDIA card. For a local build, Mesa's NAK compiler produces the shader header and SM86 SASS:

```sh
TMPDIR="$HOME/.cache/tmp" \
NAK_OFFLINE=/path/to/mesa-25.2.7/build/src/nouveau/compiler/nak-offline \
NVDISASM=/usr/local/cuda-13.1/bin/nvdisasm \
BINDINGS=u1,s2 tools/nak-compile.sh demos/shaders/post.vert sass-out
```

`BINDINGS` is the descriptor shim: `u` is a uniform buffer, `s` a sampler, `b` a storage buffer and `a` an acceleration structure. Omit it for shaders without bindings. Put the resulting `.bin` files under `core/tests/fixtures/driver-shaders`; programs such as `guest/src/bin/dungeon.rs` copy them to mapped GPU memory with `shader(address, bytes)`. The triangle is smaller and writes its built-in shader bytes through `scene::triangle_scene` instead.

## Run it

Capture one frame from the NRO:

```sh
cargo run --release -p oboromi-core --example capture -- \
  ./triangle.nro --seconds 1 --fps 1 --out "$HOME/.cache/oboromi-triangle"
```

For parity checks, use exact mode and compare the CPU path with the reference renderer:

```sh
OBOROMI_EXACT=1 cargo test -p oboromi-core guest_triangle_end_to_end -- --nocapture
cargo test -p oboromi-core nak_compiled_triangle_matches_driver -- --nocapture
```

## Limits

- NAK does not compile ray queries; use the NVIDIA driver path for them.
- Speed hacks are enabled by default. Set `OBOROMI_EXACT=1` for exact results.
- The supported host is Linux x86-64.
- The loader accepts NRO and flat AArch64 images, including an optional NRO RomFS. It does not load retail game formats.
