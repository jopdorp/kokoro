// A softened velocity-staggered leapfrog disk. Gravity is tiled through shared memory; collision bins are sorted before merging so
// mass, momentum and results do not depend on CUDA scheduling. Units are star mass, AU and year / 2pi.

struct Body {
    float4 position_size;
    float4 velocity_mass;
    float4 colour;
};

constexpr unsigned TILE = 64;
constexpr unsigned GRID_SIDE = 128;
constexpr unsigned CELLS = GRID_SIDE * GRID_SIDE;
constexpr unsigned CELL_CAPACITY = 8;
constexpr unsigned TRAILS = 32;
constexpr unsigned SEEDS = 6;

__device__ unsigned cell(float3 p) {
    int x = int((p.x + 4.0f) * 16.0f);
    int y = int((p.z + 4.0f) * 16.0f);
    if (x < 0 || y < 0 || x >= int(GRID_SIDE) || y >= int(GRID_SIDE)) return CELLS;
    return unsigned(y) * GRID_SIDE + unsigned(x);
}

__device__ float radius(float mass) {
    return 0.012f + 0.8f * sqrtf(mass);
}

__device__ float3 acceleration(const Body* bodies, unsigned count, unsigned self, float3 position) {
    __shared__ float4 tile[TILE];
    float3 acceleration = make_float3(0.0f, 0.0f, 0.0f);
    for (unsigned base = 0; base < count; base += TILE) {
        unsigned source = base + threadIdx.x;
        if (threadIdx.x < TILE) tile[threadIdx.x] = source < count
                ? make_float4(bodies[source].position_size.x, bodies[source].position_size.y,
                              bodies[source].position_size.z, bodies[source].velocity_mass.w)
                : make_float4(0.0f, 0.0f, 0.0f, 0.0f);
        __syncthreads();
        unsigned end = min(TILE, count - base);
#pragma unroll 8
        for (unsigned j = 0; j < end; ++j) {
            float4 other = tile[j];
            float3 d = make_float3(other.x - position.x, other.y - position.y, other.z - position.z);
            float inverse = rsqrtf(d.x * d.x + d.y * d.y + d.z * d.z + 0.0004f);
            float pull = other.w * inverse * inverse * inverse;
            if (base + j != self) {
                acceleration.x += d.x * pull;
                acceleration.y += d.y * pull;
                acceleration.z += d.z * pull;
            }
        }
        __syncthreads();
    }
    return acceleration;
}

__device__ void appearance(Body& body, unsigned index, float dust_mass) {
    float scale = index == 0 ? 18.0f : fminf(12.0f, 1.0f + 2.5f * sqrtf(body.velocity_mass.w / dust_mass));
    body.position_size.w = 2.0f * (1.0f - scale);
    if (index == 0) body.colour = make_float4(1.0f, 0.55f, 0.12f, 1.0f);
    else if (body.velocity_mass.w > dust_mass * 24.0f) body.colour = make_float4(0.45f, 0.7f, 1.0f, 0.9f);
    else if (body.velocity_mass.w > dust_mass * 4.0f) body.colour = make_float4(0.62f, 0.64f, 0.68f, 0.48f);
    else body.colour = make_float4(0.72f, 0.46f, 0.24f, 0.35f);
}

extern "C" __global__ void solar_step(Body* bodies, Body* scratch, unsigned* cell_counts,
                                         unsigned* cell_slots, unsigned count, unsigned phase,
                                         float dt, float time, float dust_mass, unsigned trail_slot) {
    unsigned i = blockIdx.x * blockDim.x + threadIdx.x;
    if (phase == 0) {
        if (i >= count) return;
        Body body = bodies[i];
        if (body.velocity_mass.w == 0.0f) { scratch[i] = body; return; }
        float3 position = make_float3(body.position_size.x, body.position_size.y, body.position_size.z);
        float3 a = acceleration(bodies, count, i, position);
        float kick = time == 0.0f ? dt * 0.5f : dt;
        body.velocity_mass.x += a.x * kick;
        body.velocity_mass.y += a.y * kick;
        body.velocity_mass.z += a.z * kick;
        if (i != 0) {
            float r = sqrtf(position.x * position.x + position.z * position.z);
            float orbital = rsqrtf(r);
            float drag = 0.0025f / (1.0f + time * 0.02f);
            float target_x = -position.z / r * orbital - position.x / r * 0.002f;
            float target_z = position.x / r * orbital - position.z / r * 0.002f;
            body.velocity_mass.x += (target_x - body.velocity_mass.x) * drag;
            body.velocity_mass.y *= 1.0f - drag * 3.0f;
            body.velocity_mass.z += (target_z - body.velocity_mass.z) * drag;
        }
        body.position_size.x += body.velocity_mass.x * dt;
        body.position_size.y += body.velocity_mass.y * dt;
        body.position_size.z += body.velocity_mass.z * dt;
        appearance(body, i, dust_mass);
        scratch[i] = body;
        if (i > 0 && i <= SEEDS) {
            Body trail = body;
            trail.position_size.w = 1.7f;
            trail.colour.w = 0.035f;
            scratch[count + (i - 1) * TRAILS + trail_slot] = trail;
        }
        return;
    }
    if (phase == 1) {
        if (i < CELLS) cell_counts[i] = 0;
        return;
    }
    if (phase == 2) {
        if (i >= count || scratch[i].velocity_mass.w == 0.0f) return;
        unsigned key = cell(make_float3(scratch[i].position_size.x, scratch[i].position_size.y, scratch[i].position_size.z));
        if (key == CELLS) return;
        unsigned slot = atomicAdd(cell_counts + key, 1u);
        if (slot < CELL_CAPACITY) cell_slots[key * CELL_CAPACITY + slot] = i;
        return;
    }
    if (phase == 3) {
        if (i >= CELLS) return;
        unsigned size = min(cell_counts[i], CELL_CAPACITY);
        unsigned sorted[CELL_CAPACITY];
        for (unsigned j = 0; j < size; ++j) sorted[j] = cell_slots[i * CELL_CAPACITY + j];
        for (unsigned a = 1; a < size; ++a) {
            unsigned value = sorted[a], b = a;
            while (b && sorted[b - 1] > value) { sorted[b] = sorted[b - 1]; --b; }
            sorted[b] = value;
        }
        for (unsigned a = 0; a < size; ++a) {
            unsigned survivor = sorted[a];
            Body merged = scratch[survivor];
            float mass = merged.velocity_mass.w;
            if (mass == 0.0f || survivor == 0) continue;
            for (unsigned b = a + 1; b < size; ++b) {
                unsigned consumed = sorted[b];
                Body other = scratch[consumed];
                if (other.velocity_mass.w == 0.0f || consumed == 0) continue;
                float dx = other.position_size.x - merged.position_size.x;
                float dy = other.position_size.y - merged.position_size.y;
                float dz = other.position_size.z - merged.position_size.z;
                float reach = radius(mass) + radius(other.velocity_mass.w);
                if (dx * dx + dy * dy + dz * dz > reach * reach) continue;
                float next = mass + other.velocity_mass.w;
                merged.position_size.x = (merged.position_size.x * mass + other.position_size.x * other.velocity_mass.w) / next;
                merged.position_size.y = (merged.position_size.y * mass + other.position_size.y * other.velocity_mass.w) / next;
                merged.position_size.z = (merged.position_size.z * mass + other.position_size.z * other.velocity_mass.w) / next;
                merged.velocity_mass.x = (merged.velocity_mass.x * mass + other.velocity_mass.x * other.velocity_mass.w) / next;
                merged.velocity_mass.y = (merged.velocity_mass.y * mass + other.velocity_mass.y * other.velocity_mass.w) / next;
                merged.velocity_mass.z = (merged.velocity_mass.z * mass + other.velocity_mass.z * other.velocity_mass.w) / next;
                mass = next;
                merged.velocity_mass.w = mass;
                scratch[consumed].velocity_mass.w = 0.0f;
                scratch[consumed].colour.w = 0.0f;
            }
            scratch[survivor] = merged;
        }
        return;
    }
}

extern "C" __global__ void solar_contract(Body* bodies, unsigned count, unsigned steps) {
    if (blockIdx.x || threadIdx.x) return;
    for (unsigned step = 0; step < steps; ++step) {
        for (unsigned i = 0; i < count; ++i) {
            if (bodies[i].velocity_mass.w == 0.0f) continue;
            bodies[i].position_size.x += bodies[i].velocity_mass.x * 0.01f;
            bodies[i].position_size.y += bodies[i].velocity_mass.y * 0.01f;
            bodies[i].position_size.z += bodies[i].velocity_mass.z * 0.01f;
        }
        for (unsigned i = 0; i < count; ++i) {
            Body merged = bodies[i];
            float mass = merged.velocity_mass.w;
            if (mass == 0.0f) continue;
            for (unsigned j = i + 1; j < count; ++j) {
                Body other = bodies[j];
                if (other.velocity_mass.w == 0.0f) continue;
                float dx = other.position_size.x - merged.position_size.x;
                float dy = other.position_size.y - merged.position_size.y;
                float dz = other.position_size.z - merged.position_size.z;
                if (dx * dx + dy * dy + dz * dz > 0.0025f) continue;
                float next = mass + other.velocity_mass.w;
                merged.position_size.x = (merged.position_size.x * mass + other.position_size.x * other.velocity_mass.w) / next;
                merged.position_size.y = (merged.position_size.y * mass + other.position_size.y * other.velocity_mass.w) / next;
                merged.position_size.z = (merged.position_size.z * mass + other.position_size.z * other.velocity_mass.w) / next;
                merged.velocity_mass.x = (merged.velocity_mass.x * mass + other.velocity_mass.x * other.velocity_mass.w) / next;
                merged.velocity_mass.y = (merged.velocity_mass.y * mass + other.velocity_mass.y * other.velocity_mass.w) / next;
                merged.velocity_mass.z = (merged.velocity_mass.z * mass + other.velocity_mass.z * other.velocity_mass.w) / next;
                mass = next;
                merged.velocity_mass.w = mass;
                bodies[j].velocity_mass.w = 0.0f;
            }
            bodies[i] = merged;
        }
    }
}
