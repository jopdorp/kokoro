#!/bin/sh
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
fixtures="$root/core/tests/fixtures"
nvcc=/usr/local/cuda-13.1/bin/nvcc
objcopy="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-objcopy"
"$nvcc" -std=c++17 -arch=sm_86 -cubin "$fixtures/tensor-upscale.cu" -o "$fixtures/tensor-upscale-sm86.cubin"
"$objcopy" --dump-section .text.tensor_upscale="$fixtures/tensor-upscale-sm86.bin" --dump-section .nv.constant0.tensor_upscale="$fixtures/tensor-upscale-cbuf0.bin" "$fixtures/tensor-upscale-sm86.cubin"
