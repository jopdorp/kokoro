#include <cuda_fp16.h>
#include <mma.h>

using namespace nvcuda;

constexpr int HALF_WIDTH = 960;
constexpr int HALF_HEIGHT = 540;
constexpr int WIDTH = 1920;
constexpr int HEIGHT = 1080;

__device__ float weight(int tap, int phase) {
    if (phase == 0) return float(tap == 1);
    if (tap == 1 || tap == 2) return 0.5625f;
    if (tap == 0 || tap == 3) return -0.0625f;
    return 0.0f;
}

extern "C" __global__ void tensor_upscale(const uchar4* source, uchar4* target) {
    __shared__ half left[16 * 16];
    __shared__ half right[16 * 16];
    const int lane = threadIdx.x;
    const int tile_x = blockIdx.x * 16;
    const int tile_y = blockIdx.y * 16;

    for (int channel = 0; channel < 4; channel++) {
        for (int i = lane; i < 256; i += 32) {
            const int row = i >> 4;
            const int column = i & 15;
            const int x = min(max(tile_x / 2 + column - 3, 0), HALF_WIDTH - 1);
            const int y = min(max(tile_y / 2 + row - 3, 0), HALF_HEIGHT - 1);
            left[i] = __float2half_rn(reinterpret_cast<const unsigned char*>(&source[y * HALF_WIDTH + x])[channel]);
            right[i] = __float2half_rn(weight(row - column / 2 - 2, column & 1));
        }
        __syncthreads();
        wmma::fragment<wmma::matrix_a, 16, 16, 16, half, wmma::row_major> a;
        wmma::fragment<wmma::matrix_b, 16, 16, 16, half, wmma::row_major> b;
        wmma::fragment<wmma::accumulator, 16, 16, 16, float> c;
        wmma::load_matrix_sync(a, left, 16);
        wmma::load_matrix_sync(b, right, 16);
        wmma::fill_fragment(c, 0.0f);
        wmma::mma_sync(c, a, b, c);
        __syncthreads();
#pragma unroll
        for (int i = 0; i < c.num_elements; i++) {
            const int part = i & 3;
            const int row = (lane >> 2) + (part >= 2 ? 8 : 0);
            const int column = 2 * (lane & 3) + (part & 1) + (i >= 4 ? 8 : 0);
            left[row * 16 + column] = __float2half_rn(c.x[i]);
        }
        for (int i = lane; i < 256; i += 32) {
            const int row = i >> 4;
            const int column = i & 15;
            right[i] = __float2half_rn(weight(column - row / 2 - 2, row & 1));
        }
        __syncthreads();
        wmma::load_matrix_sync(a, right, 16);
        wmma::load_matrix_sync(b, left, 16);
        wmma::fill_fragment(c, 0.0f);
        wmma::mma_sync(c, a, b, c);
#pragma unroll
        for (int i = 0; i < c.num_elements; i++) {
            const int part = i & 3;
            const int x = tile_x + 2 * (lane & 3) + (part & 1) + (i >= 4 ? 8 : 0);
            const int y = tile_y + (lane >> 2) + (part >= 2 ? 8 : 0);
            if (x < WIDTH && y < HEIGHT && x >= WIDTH / 2 - 2) {
                reinterpret_cast<unsigned char*>(&target[y * WIDTH + x])[channel] = x < WIDTH / 2 + 2 ? 255 : __float2uint_rn(min(max(c.x[i], 0.0f), 255.0f));
            }
        }
        __syncthreads();
    }
}
