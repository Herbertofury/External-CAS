# Native CAS material construction investigation

This record distinguishes observed native behavior from selectors and rendering
states still being traced. Installed game version: 1.129.39.1030. No game process
was launched, read, imported as a runtime module, injected into or contacted.
All proprietary sections, disassembly, decompiled bodies, texture pixels and
historical dump metadata remain in ignored private storage.

## Exact source and static analysis boundary

The installed client SHA256 is
`95e40b75f76711c66b9f36a974ec04d43304a6f591d409d3d4019818f366237b`;
Simulation DLL SHA256 is
`c7108a5ac17628b72429fae0c09cbf07036f72ba36318591169f59f47ca9a3d0`.
The client's on-disk `.text` is encrypted (entropy 7.999994), so its raw
instruction-looking bytes do not establish valid renderer behavior. A bounded
offline decode using the existing authorized local installation produced a
non-executable private section with SHA256
`829377024c3e7970214cd1af1765994364402a28ed96761a7afb568f8e3c98fa`.
No license/key bytes were written. Original files stayed unchanged.

Analysis uses pinned section bytes over the original PE address map. Ghidra
12.2 and Capstone supply static views; neither executes the client. MSVC chained
unwind fragments must not be mistaken for whole function boundaries. Isolated
Unicorn 2.1.4 runs below use only explicitly bounded pure numeric routines with
separate virtual stack/output memory and no native or external I/O.

## Skin-detail lookup: native TONE dataflow and generator

Native entry RVA `01A3DC30` fills 256 four-byte entries using packed input
whose high 16 bits scale by float bits `3B360B61` (approximately 1/360) and
low 16 bits scale by `3C23D70A` (approximately 1/100). Entry lightness is
`index/256`, including the final entry at 255/256. Its color helper at
`019BFD70` uses the exact hue operation `h*(h*6)`, rather than replacing it with
conventional linear hue. Single-precision operation order, truncating integer
conversions and low-byte results matter.

Actual callers `01A20C20` and `01A20F10` clear 1,024 staging bytes before the
generator, create a 256×1 texture with internal format `0x3D` and flags `2`, and
upload that buffer. The generator writes B, G and R; the initialized alpha
remains zero. Native format-table store RVA `01374C99` maps internal `0x3D`
to DXGI format `87`, `B8G8R8A8_UNORM`. Creation at `012A5870` supplies that
format in the native `D3D11_TEXTURE2D_DESC` to `CreateTexture2D`; this case has
no typeless substitution. The initialized renderer has a distinct packed-zero
default-resource branch, described below; explicitly generating a zero HSL
lookup is not equivalent to selecting that native default resource.

The independently expressed Rust generator in
[native_lookup.rs](../../native/material/src/native_lookup.rs) matches all
150,528 output bytes from 147 adversarial native emulations exactly. Those
runs interpreted 2,670,696 native instructions, with no game entry point,
process contact or external I/O. Regression vectors contain only fabricated
numeric inputs and result hashes, never proprietary instruction bytes.

The canonical native TONE reader RVA `00398060` reads the serialized saturation
u16 followed by hue u16 as one little-endian u32 into native object `+0x148`.
Getter `00397F50`, layer builder `01A1BE20` (`+0xC0`) and copy `01A1D3E0`
(`+0x28 +0xC0 = +0xE8`) preserve that value through to the generator. Its next
u32 at `+0x14C` is opacity. The imported v12 TONE yields packed `000A0014` and
lookup BGRA SHA256 `2a39dde695fd350bfededfb776b3f9c5413df9245669d95d549b8ea805278c33`.
`resolve_native_tone_lookup` selects this source-pinned v12 path.
Missing values and stale source bytes fail explicitly. The distinct historical v9 format remains opaque;
it is not parsed as legacy v6-v8.

For zero packed input, `01A20C20` checks the renderer singleton pointer at
`029749C8`; this is an initialization check, not a boolean color setting.
Getter `011D6670` obtains the default-resource table through `012590C0` and
returns its `+0x80` texture. Table initialization `012591C0` calls the actual
grayscale helper `019BFC80`, creates a 256×1 internal-format `0x3D` texture,
uploads 1,024 bytes and stores that exact resource at `+0x80`. Entry i is
`[i,i,i,i]` in BGRA order, including final entry `[255,255,255,255]`.
The independently emulated native helper gives SHA256
`83a446ee1b8a6bd3a43e706b334d3566afab316a56f81c79e07434f8c8205277`.
`NativeToneLookup.selection` records the initialized-renderer default versus
the nonzero packed-TONE generation. No absent TONE value selects this branch.

## Ambient texture: exact native species selector

Native getter RVA `019F5800` returns texture type `00B2D882`, group zero. Its
observed integer-context mapping is:

- 1: instance `523358FB929BBD37`.
- 2 or 4: instance `F63401C193AC863E`.
- 3: instance `F9D2E9E815C80C9B`.
- 5: instance `C399C6E9BAC26732`.
- 6: instance `F04F38C823AB66C2`.

Material setup at `01A2C460` supplies its context field `+0x130`; the native
CAS remapper at `01A0FB20` supplies its smaller context field `+0x10`.
CAS setup `003FFEC0` creates the remapper and writes its context through setter
`003B0230` from the model record's `+0x38` field. That exact record field also
feeds an ASM parameter literally named `species` (name getter `01A0EA60`), with
value getter `01A0EA10` returning native literals `dog=2`, `cat=3`,
`smalldog=4`, `fox=5`, `horse=6`. Human is native enum value 1. The model record
contains the 20-byte native physical descriptor at `+0x28`; species is its fifth
word. The renderer's physical descriptor at `+0x120` is likewise supplied to
native RIG selection `01A37780`, placing the same species at `+0x130`.

`resolve_native_ambient` uses the explicit serialized native ExtendedSpecies;
missing, zero and unknown values do not receive a human default. For the actual
human import it resolves `00B2D882:00000000:523358FB929BBD37` at 64×128.
Payload SHA256 is `aac8524e032198ed57b4a0418595aaf35172106cec076a4cc99400cd5811a031`;
decoded RGBA SHA256 is `3c4994d4680721a2ab0cf76e3dcebc4d9e4a766051723c96d2758d754b10eb2a`.
Green and alpha both span 0–255. Original bytes and provider/config pins remain
retained. CASP shadows are not this fixed ambient resource.

The imported-Sim `CharacterMaterialInputs.globals` check resolves both native
globals, all 34 part images and every MTNF field, with zero source conflicts.
Its decoded inputs occupy 208,176,128 bytes. This is resource qualification,
not an app memory/frame-time benchmark or complete-material acceptance.

## Native GPU plane producers and binding joins

Exporter `019F2620` obtains a completed compositor stage through `01A1E450`,
then stores its texture in the renderer map through `01A2C350`. Material
updater `01A2F6E0` joins that map to actual shader binding hashes:

- Stage 1 → key 1 → `OverlayTexture`, hash `4DC0C8BC`.
- Stage 2 → key 4 → `SkinToneAndMakeupTexture`, hash `5E2CC5B9`.
- The same stage-2 path installs its generated lookup at key 16 →
  `SkinDetailHSL`, hash `3C410F96`.
- Stage 3 → key 8 → `ClothWithAlphaTexture`, hash `69EDEC9E`.
- Stage 4 → key 2 → `SpecularMap`, hash `AD528A60`.

The last binding name independently matches the installed PRECOMP name pool.
It is not `NormalMap` (`6E56548A`), which remains an independent per-mesh
source path. The GPU compositor mode is zero. Its stage 1 calls the detail
image jobs without the native CPU `SkinDetail` conversion; the nonzero CPU
mode additionally applies that conversion and its later layer path.

Part routing `01A1E4A0` obtains diffuse through getter `019FA240` (native CASP
`+0x208`) and shadow through `019FA3E0` (`+0x218`). Shadow jobs participate in
stages 1/3; native diffuse image jobs also receive getter `019FA230`
(`+0x278`), the actual ColorShiftMask. The current native CASP reader exposes
that late field from v47, while the recovered GPL reader gates it at v49;
v47/48 compatibility must therefore retain this discrepancy explicitly.
These routing facts do not yet qualify missing-resource policy, complete
ordered sources, masks, native HSL or the complete rendered planes.

## Native physique detail-layer weights and actual tuning

Native builder `01A1A600` examines all nine stored physique values. It applies
Heavy-dependent texture fading to Fit/Bony, and Fit-dependent fading to Lean.
Positive survivors sort descending with stable ties. When their total exceeds
one, each weight is multiplied by the reciprocal total before easing.
The easing helper `019F5E20` computes `1-powf(1-weight, exponent)`.
Schema exponent order is Heavy/Lean/Fit/Bony; stored weight order begins
Heavy/Fit/Lean/Bony. Axes 5–8 still affect normalization but the native
preset-name emitter has no image-name branch for them. Their original data
and effect are retained explicitly.

Loader `0032C720` reads the 48-byte `Client_CASTuning.Physique` schema:
fade start/end floats at offsets 16/20 and 24/28, easing at 32/36/40/44.
It derives each fade slope as `1/(end-start)` and bias as `-start*slope`.
When `end<=start`, native loading uses `end=start+0.01` first. Runtime
coefficients land at `+A8/+AC` and `+B0/+B4`, with easing at `+B8..+C4`.

[native_physique.rs](../../native/material/src/native_physique.rs) matches all
483 ordered preset-emitter calls from 64 controlled nine-weight profiles,
including all output float bits. Of these, 251 calls use axes with native
named-image branches. The original suppression/sort/normalization/easing code
is interpreted in isolated memory; allocator/string and explicit-input getters
are controlled stubs. The native imported `powf` name is verified and the
analysis process uses the Windows C runtime's pure math dependency. No game
module is loaded or executed as a process.

[native_physique_source.rs](../../native/material/src/native_physique_source.rs)
reuses the bounded existing SimData reader and full provider policy. It checks
actual numeric Species/Occult, native schema offsets and all nine stored
weights; empty interior terms cannot shift axis identities. The actual import
selects `545AC67A:004B5265:AF88EBAFBB8C2657`, preserves four source observations
and three effective historical tombstones, and produces two named weights.
All unknown SimData cells and original source bytes remain retained.
Multiple highest-priority tombstones are absence; lower historical values are
never revived. Mixed live/deleted highest ties remain unresolved.

## Qualified native CPU skin/detail numeric kernel

The native `SkinDetail` CPU operator invokes pixel kernel RVA `01A3D340`
through block preparation at `01A3D680`. Its numeric contract takes 16 prepared
detail-green bytes, 16 skin BGRA pixels, the actual lookup and an opacity weight.
It uses rounded integer multiplication, an arithmetic half-step toward the
lookup tint, two integer overlay stages and a 0..128 fixed-point interpolation.
Output alpha is 255. Skin input alpha is not consulted by this operation.
Opacity takes its low native byte, multiplies percent by 128 using the observed
single-precision constants, adds 0.5, truncates, then narrows to a byte. This
includes the native wrap behavior outside the ordinary percent range.

The native CPU lookup setter `01A3DF20` has a distinct packed-zero path: all four
bytes in entry i equal i, including the final 255 entry. This is proven native
construction, not an authored neutral lookup. The GPU initialized-renderer
default table is independently traced to the same grayscale helper.

[native_cpu_skin.rs](../../native/material/src/native_cpu_skin.rs) expresses
these factual operations independently. All 16,640 bytes from 260 adversarial
blocks (4,160 pixels) match bounded execution of the actual native CPU routines
exactly, including zero/nonzero lookup inputs, threshold colors, negative
intermediate differences, opacity boundaries and ignored input alpha. These
runs interpreted 676,753 native instructions with no external I/O or game
contact. This qualifies the numeric block operation, not native compressed-block
preparation, upstream layer order, full CPU skin construction or final placement.

The compressed-block preparation is now independently qualified for one
explicitly present detail/skin block through native `01A3D680`.
[native_cpu_bc1.rs](../../native/material/src/native_cpu_bc1.rs) preserves its
RGB565 expansion, 171/85 interpolation with +128 before shift8, rounded-up
three-color midpoint, transparent selector and all16 selector positions.
Across256 fabricated compressed-block pairs, all36,864 bytes of prepared
detail green, prepared skin BGRA and composed output match actual native
instruction execution exactly (729,805 interpreted instructions). This
qualification does not choose engine cache/presence flags, generate native
compressed inputs from arbitrary RGBA, or establish upstream layer order.

## Remaining construction gates

The native raw-image operator's prepared pixel blend is now qualified in
[native_blend.rs](../../native/material/src/native_blend.rs). Factory
`019E0780` selects `RLEBlend` for RLE2/LRLE inputs and a separate raw-image
operator for its other image branch. The raw-image operation reaches
`0135B3B0`, `012AFE10` and `012B9140`. It blends encoded bytes with integer
rounding, without applying the reference atlas's mathematical gamma conversion.
Opacity is quantized to a0..256 weight with `trunc(opacity*256+0.5)`;
the source-alpha weight has separate exact full/partial-opacity paths.
The caller explicitly chooses whether to interpolate source alpha or force
the source alpha opaque. Across256 fabricated blocks, all16,384 output bytes
match134,084 interpreted native instructions exactly. This is a pixel leaf
qualification; the runtime must still prove actual plane selection, alpha-mode
selection, native HSL, masks, compressed input branches and ordered placement.

The CASP shadow branch is independently qualified in
[`native_shadow.rs`](../../native/material/src/native_shadow.rs). The native
factory hash `B945453B` selects processor `019E4BD0`, dispatch `019E92B0` and
compressed leaf `0135EF60`. It multiplies prepared encoded RGB channels with
`(destination*shadow+128)>>8`, preserves destination alpha, skips transparent
BC1 pixels and ignores BC3 shadow alpha. All512 synthetic blocks /32,768 bytes
match368,361 bounded interpreted original instructions exactly.

The unshifted compressed diffuse branch is separately qualified in
[`native_compressed_blend.rs`](../../native/material/src/native_compressed_blend.rs).
Entry `01356D20`, full-opacity path `0134C260` and fill helper `013D5740` match
all768 synthetic blocks /49,152 bytes across464,076 original instructions.
BC3 alpha is interpolated in fixed-point blend-weight space; decoding ordinary
8-bit alpha and then multiplying opacity gives different rounding. Full-opacity
BC1 canonicalizes to an opaque four-color block, while fractional opacity
uses the actual endpoint-order branch. This API requires explicit prepared
ordinary-cache pixels. Native canvas initialization, masks, image adjustments,
resource/mip choices and full layer order remain separate gates.

- Qualify remaining native TONE source versions beyond the integrated v12 path.
- Qualify sampler, complete shader permutation and render-pass selection.
- Trace actual overlay/detail, skin/makeup and cloth composites, including
  masks, native color shifts, channel arithmetic and order.
- Qualify the CPU-skintone construction independently of the existing reference
  atlas. Shader feature `OPT_CPU_SKINTONE` alone does not supply its pixels.

Original CAS pixel-shader equation qualification is recorded separately in
[SIMSKIN_DX11_20261112.md](SIMSKIN_DX11_20261112.md). A correct equation with
unqualified texture construction does not qualify complete native appearance.
