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

The production draw API now accepts the original paired stages, an explicit
native input declaration/encoded vertex stream, immutable geometry, per-stage
constant buffers, channel/color-space-specific material planes (including
unmodified BC1/BC3), samplers and pass states. All required metadata constants
and pixel textures must be present. Missing vertex inputs, foreign owners,
duplicate bindings and changed source revisions are refused. Depth/color/stencil
clears are explicit so ordered passes can preserve earlier depth.

`wrap_depth_target` and `draw_scene_with_shared_depth` also share the actual
wgpu `Depth32Float` allocation. The ownership pass transitions color and depth
together, then releases both even after a failed native callback. It preserves
the wgpu allocation until native references are released. Unsupported formats,
foreign contexts and mismatched extents are refused before acquisition.

The original paired-stage GPU qualification now checks both directions:
wgpu's nearer depth blocks the original shader draw, and the original draw's
depth blocks a later wgpu triangle. All 512 additional pixels matched within
7.5e-9. This is the production room/Sim occlusion boundary, with no per-frame
CPU copy or wait; it does not qualify the app's complete material/frame binding.
See [the sanitized receipt](evidence/shared-depth-qualification.json).

An additional controlled GPU case executes the original normal/skinned CAS
VS924 + PS3051 through this API. All 256 pixels match expected encoded-channel
values within 7.5e-9, including the source alpha output of zero. The latest case
uses the qualified initial Short4/UNORM/Short2 attribute packet and the original
six sampler records with explicit controlled settings. It does not qualify
complete source mesh grouping, tag context or deformation-stage selection.
Source-native material construction, complete
feature selection, inherited state and actual application integration remain open.

```powershell
cargo run -p external-cas-render --example qualify_original_draw -- 'ABSOLUTE_PRECOMP_PATH'
```

`EncodedCasMesh` requires explicit position/normal/tangent packing stages and
both authored UV sets. Bone hashes map to their actual unique rig indices in a
bounded 60-entry packet palette. Missing fields, mismatched raw weights, source
body 0x72's unqualified tag context and excess palette entries are refused.
Authored geometry remains untouched. Storage is compatible with the inspected
VS924; it does not pretend to reproduce every native batching decision.

The native sampler descriptor port matches all 1,152 original producer cases
byte for byte. The two source control flags and anisotropy floor remain explicit
inputs; live settings selection is a separate gate. Invalid descriptors are
left for strict D3D validation rather than silently repaired.
