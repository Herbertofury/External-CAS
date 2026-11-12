# Native RIG and CLIP pipeline

`external-cas-animation` parses authorized in-memory RIG v3.1 and CLIP v14 or current v18 / S3CLIP v2 resources and evaluates bone poses in Rust. There is no Node, Blender, Studio, browser or game dependency in the runtime crate.

```rust
use external_cas_animation::*;
# fn run(rig_bytes: &[u8], clip_bytes: &[u8]) -> Result<()> {
let rig = Rig::parse(rig_bytes)?;
let bind = rig.bind_pose()?;
let clip = Clip::parse(clip_bytes, FrameLayout::Conservative)?;
let pose = clip.sample_with_bind(&rig, &bind, 5.0, SampleOptions::default())?;
// pose.palette is Vec<[f32; 16]>, column-major, in RIG file bone order.
# Ok(()) }
```

The conservative path decodes packed 10-bit type-18 vectors (local translation and positive scale components) and 12-bit type-20 XYZW rotations. Quantization signs, offsets, scales, ordering and bounds are checked. Constant component sampling follows the recovered source-tool curve policy; it is not a claim about game interpolation. Near-unit quantized rotations are explicitly normalized within a 0.001 norm tolerance and the adjustment is reported. Unsupported channels, duplicate tracks, missing/ambiguous bone hashes, invalid values and singular binds fail sampling without returning a partial palette. Scaling composes after rotation independently of channel-table order; nonrigid palettes are flagged for inverse-transpose normal transformation. Negative/zero animated scales remain unsupported.

Original resource bytes, unknown channel regions, frame payloads/flags, events, IK slots/chains and trailing data are retained. Unsupported versions remain inspectable with original bytes. Parsed models are not native writers. Events and IK cause sampling to fail by default. A caller can select `AncillaryPolicy::PreserveAndIgnore` for a clearly labeled preview; counts and limitations remain in `SampledPose`. Initial object transforms are retained but not applied. Game events/IK, native interpolation, rig selection and full native appearance remain unqualified.

`UnsupportedChannelPolicy::PreserveAndSkipNonBoneTracks` additionally permits an explicit partial preview: zero-frame tracks hold the bind component, and non-bone control components (sub-target >=4) remain unevaluated. Every omitted track is reported in `SampledPose::skipped_channels` with its hash/type/component/reason. Active bone tracks with missing hashes, malformed values and unknown bone encodings still fail. The default policy remains `Reject`. This policy is useful for installed clips that mix supported skeletal motion with unsupported expression/control data; the UI must display its partial status.

All math uses column vectors. `Matrix4` stores column-major f64; viewport palettes are checked f32. Skin matrices are `posed_global * inverse_bind`. The default coordinate basis is the source basis shared with raw GEOM. If a caller converts mesh coordinates, it must select the same explicit basis for the palette; conversion conjugates each matrix. `YUpToZUp` and `ZUpToYUp` name caller declarations, not automatic game-axis detection.

For comparison with retained source-tool data, `Rig::bind_pose_from_globals` accepts explicitly verified loaded-bind globals. `PoseConvention::StudioLoadedBindBlender43` follows the recovered Blender 4.3.2 matrix-assignment/decomposition route, including ignoring the root rotation track. It requires those caller-supplied globals and does not attest to their provenance. The default source-local convention applies root tracks and raw rig bind transforms. A source-tool palette may require an additional root-derived basis conversion before use with raw GEOM, as documented by the recovered projected-pose route.

Run `cargo test -p external-cas-animation --offline`. Tests exercise truncation at every byte boundary, quantization, hierarchy/order/cycles, nonuniform-scale inverse binds, exact binding, unsupported preservation, event/IK policy, source-basis conversion and source-tool root policy. `examples/oracle_bridge.rs` is a private stdin/stdout comparison seam, not an app launcher; its output contains derived asset data and must be consumed privately.

The current v18 envelope retains eight uninterpreted metadata words (including
nonfinite float bit patterns), three namespace hashes and the explicit 64-bit
rig instance. `ClipMetadata.current_header` exposes these separately from the
legacy header byte projection and retains the exact declared codec extent.
Unsupported versions still preserve their original bytes. Legacy v14 behavior
and its event size checks are unchanged. Sixteen tests pass, including every
truncated v18 prefix and opaque-word/rig/extent retention.

An actual installed CAS standing clip matched a separately supplied, unmodified
current primary reader on the complete wrapper and 29,579 skeletal values.
Maximum absolute decoded difference was 1.1920928955078125e-7; 15,977 float bit
patterns differ because the recovered codec uses a different intermediate
precision. This is tolerance-qualified decoding, not bit-exact game behavior.
Its event, five IK records and 23 non-bone tracks remain preserved, unevaluated
preview data. See [the sanitized receipt](evidence/current-v18-qualification.json).

## Provenance

This is a Rust port of the recovered `rig.mjs`, `clip-metadata.mjs`, `clip-frames.mjs`, `clip-values.mjs`, `clip-sample.mjs`, `studio-bind-keyed-pose.mjs` and `studio-quaternion.mjs` under the retained `cas-public-1972/recovery/external-cas/research/resource-index` tree. Those implementations identify TS4CAS Tools by C. Marinetti, s4pi ClipResource/S3CLIP/IkConfiguration/ClipEvents by Cmar and Peter Jones, and Blender v4.3.2 quaternion/armature conversion as upstream references. They carry GPL-3.0-or-later provenance; the Blender reference carries GPL-2.0-or-later. This port retains GPL-3.0-or-later licensing. Preserve notices and corresponding-source obligations in distribution. No game/creator assets, private manifests or captured palettes are included.

Current envelope field boundaries were independently checked against the
separately installed `TS4AnimationBrowser.Core` 1.9.1
`ViewerV2/StudioClipDecoderV2.cs`. No license grant was found for that source;
it is not copied, linked into the product, or redistributed. The optional
[current-reader oracle](oracle/current-wrapper/README.md) links only an explicitly
supplied local source tree during development and pins all three input files.
