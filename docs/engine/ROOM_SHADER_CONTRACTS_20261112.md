# Installed room shader contracts

This continues the [CAS engine map](ENGINE_CAS_IMPLEMENTATION_MAP.md). It uses
the same installed PRECOMP SHA/build boundary as the
[SimSkin shader record](SIMSKIN_DX11_20261112.md). It establishes actual shader
requirements for the configured room's materials, not a complete source room
render or the runtime MTST/feature selection.

Original model/material/SKIN buffers and disassembly remain private. The current
configured room exposes FullBright, GlassForObjectsTranslucent, FlatMirror and
DropShadow families. Its normal loader remains strict about unqualified skin
palette/state dependencies. An explicitly labeled source-bind-space diagnostic
does not satisfy the finished room gate.

## Source-declared feature fields

Named shader-set metadata retains each field's name/hash, multiplier, start and
range. All names below reproduce their stored lowercase FNV1 hashes. This is
source metadata, not a naming inference from the shader's output.

FullBright (`14FA335E`) declares vertex color1, skinning2, alpha test4, fade8,
alpha blend16 and translucent32. GlassForObjectsTranslucent (`849CF021`)
declares diffuse1, edge darkening2, dirt overlay4, skinning8, normal16,
vertex-color/lamp32, interior64 and shadow-map type128/range4. FlatMirror
(`A68D9E29`) declares interior1, reflection-map2 and fade4; it has no skinning
field. DropShadow (`C09C7582`) declares fade1, interior2 and SSAO support4;
it has no skinning field either.

The same bit must not be transferred between families. For example bit2 means
skinning in FullBright and reflection-map use in FlatMirror. Source programs
also reference renderer-variable IDs, not only material parameters. A parsed
VRTF blend attribute or a null object Rig alone does not establish the selected
feature vector.

## FullBright geometry and source skin palette

Source mask0 selects candidate VS2404 / PS8522; mask2 selects VS2406 / PS8522.
Both candidates have the same recorded outer words except their mask and stage
reference. VS2404 has position/UV inputs and no blend input. VS2406 consumes
`BLENDINDICES0.x` as an unsigned integer, multiplies it by3 and indexes three
matrix rows from `SkinMatrices` at CB0 register7 onward. This variant consumes
one joint index; it does not read the accompanying weight attribute.

This confirms **direct indexing of the uploaded GPU palette**, not how that
palette is constructed or reordered. SKIN bones/inverse binds, MLOD joint
references, parent-bone identity, the object rig and the source rest/world
transforms remain distinct inputs. The configured room has vertices with index0
and full first weight, but its SKIN tables do not trivially match all MLOD joint
references. Uploading the first SKIN matrix or inventing a missing rest bone
would not be a qualified fix. Rest-space inspection is still possible under an
explicit diagnostic boundary while the engine palette producer is unresolved.

PS8522 samples DiffuseMap, multiplies by native diffuse RGBA and interpolated
vertex color, and outputs an alpha/bloom term controlled by BloomFactor. Its
source parameter layout is stage-specific: diffuse is at CB0 byte2880 and
BloomFactor at2896. The padding and other stage's matrix layout cannot be
collapsed into a generic shared constant layout. Empty serialized state slices
mean no local overrides; they do not prove arbitrary D3D default states equal
the engine's inherited context.

## Glass needs actual scene inputs

Glass mask1 (diffuse present) has two observed outer-context candidates:
VS7623 / PS29384 and VS2414 / PS8536. Adding source skinning bit8 produces
VS7625 / PS29390 or VS2416 / PS8542. The differing outer context words remain
raw; a chosen quality/platform meaning has not been asserted.

Even the no-diffuse candidate PS29383 binds `SpecularEnvMap` and
`translucentTexture`. Its constants include diffuse/specular, light directions
and colors, FresnelOffset, Opacity and RefractionDistortionScale, plus the light
constant buffer. Diffuse-enabled variants add their real material texture; they
do not remove the environment/scene dependencies. The selected material's DDS
file cannot replace the dynamic translucent scene input or cubemap.

The observed diffuse-present pass has raw overrides `{0x15,0}` and `{0x01,1}`.
Their source mechanical roles are blend-enable and depth-write-mask in the
maintained engine research, but current consumer/context qualification remains
separate. A single transparent source-over surface is not automatically the
engine's glass pass. Reflection/refraction, depth, inherited states and input
render targets must be constructed explicitly.

## Mirrors require a render graph and technique selection

FlatMirror mask2 positively names use of ReflectionTexture. The first current
outer-context candidate (effect25302) contains two techniques:

- Technique hash`3D3655D9` has VS29 / PS67 and no local state overrides.
- Technique hash`AEA01A50` has VS29 / PS68 followed by VS29 / PS67. The first
  pass overrides raw states`(0x15,1),(0x16,9),(0x17,3)`; the second changes
  source/destination factors with`(0x16,5),(0x17,6)`.

A second outer context preserves the same technique/state shape with a
different main PS reference. These are technique alternatives with ordered
passes, not a flat list from which the renderer can take a convenient first
shader. Selecting a mirror material's technique and rendering its reflected
scene requires the source camera/plane, clip/stencil/depth and render-target
contract. A static photograph or generic cubemap does not implement a live CAS
mirror.

Without the reflection-map feature, candidate PS64 instead samples a cube
SpecularEnvMap using a reflected view vector, reduces it by source luminance
weights and combines reflection/light constants. A separate PS65 returns half
the diffuse RGB. Those fallback/context branches do not justify replacing a
configured reflection-map path with a generic flat color.

## DropShadow is a source geometry/atlas/blend operation

Candidate VS2291 / PS8266 samples `DropShadowAtlas2`, rather than a generic
circle drawn by the editor. The fragment operation is:

```text
mask = 1-saturate(sampleAtlas.red+interpolatedTexcoord.z)
outputRGBA = 1-LightingTweaks.x*mask
```

The pass records raw states`(0x15,1),(0x01,0),(0x18,15),(0x16,9),(0x17,1)`.
The maintained consumer identifies blend enabled, depth writes off, RGBA write
mask and destination-color/zero factors, yielding a multiplicative shadow
operation. The source consumer mapping is build-scoped; the current app must
qualify that mapping and the inherited state rather than apply arbitrary
source-over alpha. Source geometry, packed position/height data, atlas selector
and the producer of the third texture-coordinate component are part of the
contract. A null VRTF pointer needs the actual implicit vertex declaration.

## Remaining implementation boundary

Native room decoding now preserves the right models/material alternatives and
actual DDS channels; DBPF Unknown2=0 no longer blocks the configured package.
Remaining root causes are runtime material state/variant and technique
selection, palette production and object transforms, implicit packed layouts,
dynamic reflection/refraction/shadow inputs, exact per-stage constants and
render-state inheritance. These are causal engine requirements. Adding more
generic material presets would not resolve them.

The local original-shader path in [native/shader](../../native/shader) can avoid
reconstructing these equations in a replacement shader. It still needs all of
the scene inputs and pass scheduling above. The same native source room must
be compared against a retained game control after those dependencies are
qualified, without contacting the currently reserved game session.
