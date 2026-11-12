# Native original-shader rendering boundary

The Rust renderer can create a D3D11 device over the actual wgpu DX12
device/queue and execute original shaders resolved from the local installed
PRECOMP. This does not host a game executable or call an internal game ABI.
It is an additive rendering path under construction; the application still
uses its existing wgpu renderer until complete source bindings are qualified.

`OriginalShaderBridge` owns a UI-thread immediate context. Shader creation
verifies the retained stage/program fingerprint and lets the D3D runtime
validate the original bytecode. Shared targets hold their wgpu allocation
through the lifetime of native views and carry a bridge identity; a target
cannot be acquired by another D3D11 context just because the DX12 device matches.

Each frame establishes an exact tracked render-attachment state through a
submitted wgpu pass, then acquires/releases that same resource on the same
queue, including callback failure. The native context flushes before wgpu
consumes the result. Production interop does not read pixels back to the CPU or
wait per frame. Target ownership/exclusive access is an explicit unsafe boundary.
Presentation, resize, device loss and application-frame profiling remain gates.

An actual RTX4090 qualification created the original CAS pixel shader and
verified 18 target operations over three sizes and RGBA32_FLOAT, RGBA8_UNORM
and RGBA8_SRGB formats. Every native target byte matches the independent wgpu
clear reference. UNORM rounding is checked against the nearest encoded value;
equal-distance half-byte ties do not assume CPU ties-even behavior. Nine
foreign-context acquisitions were refused and nine interrupted callbacks
recovered before the next successful draw. Private receipts retain the exact
installation/shader/device data; no proprietary bytes are distributed.

```powershell
cargo run -p external-cas-render --example qualify_shared_target -- 'ABSOLUTE_PRECOMP_PATH'
```

The example alone performs diagnostic readback. It does not draw a Sim and is
not application or full-CAS acceptance. The next renderer work must supply the
actual source planes, vertex packing, rig/pose, constants, feature/technique
selection, render states and final presentation described in
[the engine map](../../docs/engine/ENGINE_CAS_IMPLEMENTATION_MAP.md).
