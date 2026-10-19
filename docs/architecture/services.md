# Services

The project's own test programs reach the emulated hardware through a small service layer. It is a stand-in, not an emulation of Nintendo's OS, and follows Switch 1 interfaces because those are known. The kernel opens a named port, returns a session handle, parses the HIPC and CMIF request, calls the service object and writes the reply into the same IPC buffer.

```mermaid
sequenceDiagram
    participant P as Program on the emulator
    participant K as Kernel
    participant S as Service
    P->>K: Connect to a named port
    K-->>P: Session handle
    P->>K: SendSyncRequest
    K->>S: Command and buffers
    S-->>K: Result and reply
    K-->>P: Reply in the IPC buffer
```

## Which paths work?

| Area | Implemented path |
|---|---|
| Kernel IPC | Sessions, handles, domains, buffer descriptors and service dispatch |
| GPU driver | `nvdrv`, nvmap, address spaces, channels, sync points and submission |
| Display | VI sessions, binder and one real buffer slot |
| Audio | `audren` and `audout` |
| Input | HID state and vibration service surface used by the demos |
| Time and applet | The calls used by the included images |
| Filesystem | Files, directories and storage used by the loader and Dungeon assets |

Commands outside the paths used by the test programs return errors. No Switch 2 service ABI was used to build this layer.

Verified by: the kernel IPC, nvdrv, VI, binder, audio, input, time, applet and filesystem tests, plus the end-to-end demo tests.

Sources: `core/src/kernel`, `core/src/nn`, `guest/src/{cmif,nv,display,audren}.rs`.
