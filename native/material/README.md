# Native material and color core

Rust owns texture decoding, exact native packed color values, reference-derived
pixel operations and the application's GPU material parameters. Inputs remain
read-only. No converted texture package or native game-hook DLL is required by
the standalone renderer.

`color::PackedColorShift` preserves the complete native uint64. Its four signed
Q2.14 lanes are brightness, saturation, hue and opacity in increasing bit order.
Per-axis changes preserve all other48bits. Existing values are never converted
through a float round trip; explicitly requested float edits use a documented
nearest/ties-even quantization policy pending engine quantization qualification.

The owner's supplied tyjokr November5,2026 update is the local behavior base:
selected-swatch ranges/defaults take precedence, with explicit fallback ranges
for unavailable metadata. Regular UI order is hue, opacity, saturation, value.
Tights/socks/base layers retain opacity; wholly transparent content is not
invented. Physical specularity controls do not prove support in a regular state.
No mod binaries, scripts, GFX resources or private derived data are distributed.

`shift_reference_rgb` ports the pinned GPL SimRipper HSV math for component
comparison. The viewport uses native GPU uniforms in encoded RGB space before
lighting, preserving the unmodified path at neutral. This does not establish
tyjokr/game auxiliary-mask, specularity, material-layer or complete appearance
parity. These require original-game readback and visual comparisons.

`texture` retains the proven RLE2/RLES/BC3 reader previously in the app. LRLE is
decoded by `external-cas-assets`; generic PNG/JPEG/DDS use `image`. Unsupported
auxiliary streams and material families remain reported. The isolated
`skin::soft_light_reference` recovers the exact first reference-tool skin blend
branch with actual caller-supplied tone/detail planes. Native selection, GDI
staging, tan/sculpt/outfit/hue layers and game shading remain separate gates.

Provenance: C. Marinetti and contributors, TS4SimRipper GPL-3.0-or-later,
revision862a32949e5156b371e2d2f83de4e37e0bb1afcc. `ColorShift.cs` SHA-256
597c38b506b8ca45741e5835e390eb34b05b7751c2cd9cb8edb57a31c1d802d6.
Exact packed lane interpretation comes from PreviewControl.ImageStack. The
skin branch was recovered from the owner's private pinned reference comparison.

Tests cover every signed16-bit lane for all four axes, untouched native bits,
explicit quantization/ranges, neutral identity, hue wrapping, invalid states and
skin layer/cancellation boundaries. The existing installed RLE pixel comparison
remains explicitly ignored until its private manifest/evidence are configured.
`examples/color_observations.rs` emits private derived RGB values only for an
explicit pinned input set; redirect output under ignored `.local/` and never
publish it. Component comparisons and item previews do not close the full CAS
color, save, material, synchronization or release gates.

`texture` now also restores complete DST1/DST5 mip streams without mutating the
source. DST5 first-mip BC3 pixels match independent Pillow exactly for five
actual installed body normals (3,801,088 pixels); full restored DDS bytes match
the original unchanged GPL s4pi `DSTResource.Unshuffle` method. DST3 remains
explicitly unsupported. Tests reject truncated/extra streams, invalid mip
counts and cube/volume inputs. No normal-channel remapping occurs in decoding.
Provenance: Keyi Zhang and s4pi contributors, Sims4Tools revision
`fff19365a12711879bad26481a393a6fbc62c465`,
[DSTResource.cs](https://github.com/s4ptacle/Sims4Tools/blob/fff19365a12711879bad26481a393a6fbc62c465/s4pi%20Wrappers/ImageResource/DSTResource.cs).

`shader` contains explicitly candidate-scoped CPU references recovered from
actual installed DX11 SimSkin and separate SimSkinCAS shader families. The
source-backed CAS path uses separate overlay, skin/makeup, cloth, detail-lookup,
ambient and optionally normal inputs; it does not treat a baked reference-tool
atlas as the complete native material. See
[source contracts and unresolved boundaries](../../docs/engine/SIMSKIN_DX11_20261112.md).
`cargo run -p external-cas-material --example shader_reference` reads diagnostic
JSON from stdin and emits independently testable albedo/normal/lighting/RGB.

`build_reference_atlas(plan, catalog, policy, cancel, progress)` composes only
actual selected native TONE/detail/sculpt/outfit inputs. It retains every
resource observation, layer order, raw native color shift, dimensions and
unresolved operation. Required native skin/detail failure returns an error.
No authored reference-tool mouth or replacement skin is inserted. Output
`native_qualified` remains false for native material/placement/rounding gates.

The actual imported-Sim base-scope check applied all 33 native layers at
1,024×2,048: one TONE, six age/physique detail layers, five sculpt textures,
15 outfit diffuse textures and six shadows. It retained 61 full-TGI source
observations and unchanged original Tray/package identities. The measured
debug build took 9,625 ms off the UI thread. This is not an interaction speed
claim or native visual acceptance. Private receipts and the derived PNG stay
under ignored local storage. `qualify_character_atlas` takes explicit source
and exclusive new output paths and revalidates them before publishing results.

`resolve_character_material_inputs` returns source-preserving per-CASP image
planes and per-GEOM MTNF observations separately from the reference atlas.
It retains native packed shifts, composition/sort/texture-space values, actual
shader hashes and full provider pins. Repeated full-TGI images share decoded
storage. Source changes, decode failures, unknown image roles, resource/decoded
memory budgets and missing content remain diagnostics; cancellation publishes
no new result. Blue/alpha normal channels and original RLES auxiliary bytes stay
unchanged. The helper does not choose an engine shader permutation or invent
global textures, UV transforms, material constants, color spaces or samplers.

The imported native Sim supplied 34 images across 15 parts: 15 diffuse, six
shadow, five normal and eight specular. All decoded with zero source conflicts,
using 208,142,336 decoded bytes. This is an off-thread resource qualification,
not a frame-time or complete-character performance claim. `SkinDetailHSL` and
`CASAmbientIntensityTexture` were explicit missing global bindings in that first
receipt. Their subsequent native resolution is described below. The reference atlas belongs to
an explicitly labeled reference/CPU-skintone preview, not the native five-plane
GPU-skintone contract.

`geom_material::GeomMaterial` preserves parameter order, duplicate hashes,
offsets, types, unknown words and original bytes. Typed float lookup refuses
duplicates/nonfinite values and supplies no defaults. The two reference-tool
normal/emission index getters are exposed with a container-specific guard;
`0x10004` is not globally interpreted as either an index or an inline TGI.
Every header, 784 value words and full word-array serialization for all 17 real
meshes matched the unchanged pinned SimRipper `GEOM.MTNF` reader exactly. All
11 nonempty GEOM normal references matched their CASP normal TGI. See the
[sanitized material-input receipt](evidence/character-inputs-qualification.json).

Framing provenance: Peter L Jones, GPL-3.0-or-later s4pi `MTNF.cs` and
`ShaderData.cs`, revision `fff19365a12711879bad26481a393a6fbc62c465`.
Container-specific getter provenance: C. Marinetti, GPL-3.0-or-later SimRipper
`GEOM.cs`, revision `862a32949e5156b371e2d2f83de4e37e0bb1afcc`.
The optional [MTNF oracle](oracle/geom-mtnf/README.md) verifies source SHA256
before compiling complete unchanged declarations in private build storage.
Native material and game-visible parity remain unqualified.

`CharacterMaterialInputs.globals` now resolves the actual native species ambient
texture and the source-pinned v12 TONE lookup. No ambient context is
guessed when serialized ExtendedSpecies is missing/zero/unknown. The lookup's
256×1 BGRA bytes and native DXGI resource format 87 are independently qualified
against 147 isolated executions of the actual numeric routine, matching all
150,528 output bytes. Native TONE reader/getter/copy dataflow establishes packed
`(hue << 16) | saturation`. Packed zero selects the actual initialized-renderer
default table: native grayscale helper19BFC80 creates `[i,i,i,i]` BGRA entries,
stored as256×1 format87 by initializer12591C0. Selection metadata distinguishes
this resource from explicit HSL generation, which differs at zero. Missing
TONE values remain errors. No game process is used by runtime or qualification.
See [native construction evidence](../../docs/engine/CAS_MATERIAL_CONSTRUCTION_20261112.md).

`native_cpu_bc1` adds explicit native CPU compressed detail/skin preparation.
All256 synthetic block pairs (4,096 pixels;36,864 prepared/composed bytes)
match bounded original native instruction execution exactly. Its palette
rounding is intentionally distinct from a universal GPU BC1 decoder. It
does not choose missing-block/cache policy, upstream layers or full placement.

`native_blend` exposes the actual raw-image job's encoded-byte blend with an
explicit native alpha mode and finite opacity. A4×4 block API and cancellable
same-size RGBA plane API share the qualified integer operation. All256
synthetic blocks (4,096 pixels;16,384 bytes) match original native instruction
execution exactly. This supplies no inferred image layers, HSL, masks or flags.

`native_shadow` supplies the distinct native CASP shadow operation on explicit
BC1/BC3-color blocks and prepared BGRA pixels, plus row-major placement into an
existing RGBA plane. It preserves destination alpha, skips transparent BC1
pixels and ignores BC3 source alpha. All512 blocks (8,192 pixels;32,768 bytes)
match bounded original native instruction execution exactly. No canvas,
resource/mip choice, RLE stream parser or complete layer order is inferred.
See [shadow evidence](evidence/native-shadow-qualification.json).

`native_compressed_blend` qualifies the native unshifted BC1/BC3 image-job
operation with an explicit ordinary prepared-cache destination. It retains
the separate full-opacity compressed path and expands BC3 alpha directly to
blend weights before interpolation. All768 cases (12,288 pixels;49,152 bytes)
match the original native routines exactly, including fractional opacity,
reversed/equal endpoints and transparent selectors. This is not a replacement
for source selection, native HSL/masks, canvas flags or ordered assembly.
See [compressed-blend evidence](evidence/native-compressed-blend-qualification.json).

`native_physique` and `native_physique_source` provide the actual native detail
weight order and source-pinned CASTuning parameters. The native builder matches
483 ordered preset calls across 64 controlled nine-weight profiles, including
exact float bits. Real-import selection uses numeric SimData Species/Occult
and checked schema offsets, preserves unknown fields and effective tombstones,
and refuses ambiguous contexts or malformed weight lists. Native image naming,
full frame/occult eligibility and complete plane composition remain separate.

`native_cpu_skin` supplies the actual native CPU skin/detail numeric operation
as a separate 4×4 block API. Its explicit inputs are prepared detail-green,
skin BGRA, native lookup and raw opacity; no layer or source is replaced.
All 260 blocks / 4,160 pixels matched isolated execution of the unchanged native
numeric routines byte for byte. Native compressed-block preparation, complete
input ordering and final atlas placement remain separate gates. This component
does not silently replace the existing reference atlas with a native claim.
