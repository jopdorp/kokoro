#!/bin/sh
# Rebuilds the flat AArch64 guest images and prints their sha256.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
rustup target list --installed | grep -qx aarch64-unknown-none || rustup target add aarch64-unknown-none
objcopy="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-objcopy"
rustup component list --installed | grep -q "^llvm-tools" || rustup component add llvm-tools
RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=$root=." cargo build --release --target aarch64-unknown-none --manifest-path "$root/guest/Cargo.toml" --target-dir "$root/target/guest"
mkdir -p "$root/core/tests/fixtures/guest"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/guest" "$root/core/tests/fixtures/guest/guest.bin"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/homebrew" "$root/core/tests/fixtures/guest/homebrew.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/triangle" "$root/core/tests/fixtures/guest/triangle.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/sprites" "$root/core/tests/fixtures/guest/sprites.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/bounce" "$root/core/tests/fixtures/guest/bounce.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/helmet" "$root/core/tests/fixtures/guest/helmet.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/raytraced" "$root/core/tests/fixtures/guest/raytraced.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/post" "$root/core/tests/fixtures/guest/post.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/particles" "$root/core/tests/fixtures/guest/particles.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/solar" "$root/core/tests/fixtures/guest/solar.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/julia" "$root/core/tests/fixtures/guest/julia.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/mandelbrot" "$root/core/tests/fixtures/guest/mandelbrot.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/music" "$root/core/tests/fixtures/guest/music.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/tensor_upscale" "$root/core/tests/fixtures/guest/tensor-upscale.nro"
"$objcopy" -O binary "$root/target/guest/aarch64-unknown-none/release/dungeon" "$root/core/tests/fixtures/guest/dungeon.nro"
sha256sum "$root/core/tests/fixtures/guest/"*.bin "$root/core/tests/fixtures/guest/"*.nro
