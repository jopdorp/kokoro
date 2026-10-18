#!/bin/sh
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/core/tests/fixtures/solar"
mkdir -p "$out"
/usr/local/cuda-13.1/bin/nvcc -arch=sm_86 -O3 -use_fast_math -cubin "$root/demos/shaders/solar.cu" -o "$out/solar-sm86.cubin"
set -- $(readelf -SW "$out/solar-sm86.cubin" 2>/dev/null | awk '$2 == ".text.solar_step" { print $5, $6 }')
dd if="$out/solar-sm86.cubin" of="$out/solar-step-sm86.bin" bs=1 skip=$((0x$1)) count=$((0x$2)) status=none
/usr/local/cuda-13.1/bin/nvdisasm "$out/solar-sm86.cubin" >/dev/null
