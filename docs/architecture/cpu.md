# CPU and shared memory

The CPU side runs AArch64 code on eight emulated cores over one RAM block. Dynarmic is the default backend. Unicorn remains available for exact runs and for comparison.

| Setting | Result |
|---|---|
| Default | Dynarmic JIT |
| `KOKOROMI_CPU=unicorn` | Unicorn |
| `KOKOROMI_EXACT=1` | Unicorn and no speed hacks |
| Programmatic exact mode | `SpeedHacks::none()` with `CpuBackend::Unicorn` |

Dynarmic covers system-call exits, slice deadlines, shared exclusives, self-modifying-code invalidation and watched GPU writes. Unicorn runs with instruction or timeout budgets instead of a callback on every basic block. Watched writes are flushed once per slice.

## Measured CPU cost

| Work | Unicorn | Dynarmic |
|---|---:|---:|
| One exact audio period | 12.41 ms | 1.553 ms |

The audio period is 5 ms. The measurement includes the same exact renderer work on each backend.

## How do the CPU and GPU share memory?

The cores and GPU use the same `GuestRam` mapping. A turn lock prevents simultaneous access. Writes to GPU-backed ranges mark pages dirty; the engine refreshes only those pages before it runs. GPU results are downloaded before the CPU resumes.

| Memory fact | Value |
|---|---|
| RAM | 12 GiB |
| Address policy | Virtual address equals physical address; no MMU yet |
| Coherence authority | `GuestRam` and `MemoryTracker` |
| Tracking unit | One GPU page |

## What does the kernel add?

The kernel catches `SVC`, dispatches it outside the CPU lock and resumes with a result. It owns the handle table, memory regions, IPC sessions, events, threads, timers and scheduler. Unsupported calls return an error rather than silent success.

Verified by: CPU self-tests, kernel and thread tests, the shared-memory engine tests, and the broad demo gate on both CPU backends.

Sources: `core/src/cpu/{cpu_manager,dynarmic_interface,unicorn_interface,guest_ram}.rs`, `core/dynarmic/bridge.cpp`, `core/src/kernel`.
