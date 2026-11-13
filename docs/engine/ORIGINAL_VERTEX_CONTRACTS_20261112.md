# Original paired CAS vertex/pixel contract

This continues the [SimSkin contract](SIMSKIN_DX11_20261112.md) for the same
installed1.129.39.1030 client/PRECOMP hashes. It records an actual source pair,
not a qualified live feature choice. Original instructions/bytes remain local.
The Rust renderer can use the locally installed original stages; their source
inputs, deformation, binding and pass context still have to be correct.

## Source candidate and pass

SimSkinCAS effect68912 has source mask0x90 (normal0x80 plus skinning0x10),
outer words retaining0x20400 and0x301. Its source-named technique
SimSkinWithTone (47B7D9C5) pairs VS924 with PS3051. Each pass contains zero local
state overrides; that does not qualify arbitrary inherited blend/depth state.
The other technique is SimShadow (CC4D4EDA): VS925 deliberately emits only a
zero clip position and has no pixel shader. It must not be scheduled as a
working geometric depth/shadow pass merely because it appears second.

VS924 SHA256 is
`0c09f08016b8b2fd008624108d6c7b986e9ed798542b6ab89d08247d220a7b1e`.
PS3051 SHA256 is
`7fe86f137295a1cef91003a59c0440f28d01524cabe12c5d80717f2a3a164f47`.
The [feature interpreter](SOURCE_FEATURE_VM_20261112.md) can evaluate explicit
caller lists; it does not authorize selecting this mask for every Sim.

## Source stage metadata and constants

Both VS and PS records use the relative pointer at+0x24 and count at+0x28 for
u16 constant-preparation indices. The52-byte VS record additionally has an
input-metadata pointer at+0x2C and BYTE count at+0x30. VS924's24 bytes are eight
triples of register, semantic index and raw usage/type byte. They identify
POSITION0, NORMAL0, TEXCOORD0, TEXCOORD1, COLOR1, BLENDINDICES0, BLENDWEIGHT0
and TANGENT0, in registers0..7 respectively. Blend indices have raw usage0x23,
matching the original DXBC UINT signature; other inputs are float. Preserve
that raw byte instead of silently interpreting every usage as an unflagged enum.
Current input-layout consumer1373730 explicitly masks this byte with0x0F
at13739BD before indexing the literal2055170 semantic-name table. This is
distinct from the feature interpreter's renderer-ID conversion. It must never
rename input TEXCOORD usage2 to TANGENT merely because renderer featureID2
means a tangent capability.

VS924 preparation indices are31,2,3. Its stage-specific CB0 is3024 bytes:

- ObjectToWorldSpaceMatrix: offset0,64 bytes.
- ObjectToClipSpaceMatrix: offset64,64 bytes.
- SkinMatrices: offset128,2880 bytes,60 joints of three float4 rows.
- NormalMapUVxform: offset3008,16 bytes.

CB2 is the288-byte RenderSystemCbuffer, including camera position at0,
WorldToClip at16, LocalToClip at80, viewDistanceBasis at144, WorldToView at160
and ViewToWorld at224. CB5 is208-byte SkyDataCbuffer. This candidate's pixel
signature does not consume the vertex color/fog output, but the original VS
still declares those buffers. Its zero matrix defaults are metadata, not usable
runtime transforms. Matrix dot instructions consume row vectors; a column-major
host matrix cannot be copied without its matching packing convention.

PS3051 has a DIFFERENT stage CB0,3072 bytes/preparation918. LightDirections and
LightColors occupy offsets0 and64; NormalMapScale is at3008, second-overlay
opacity3024, rim falloff3040 and ambient intensity3056. Its six texture roles
match the normal-enabled unskinned candidate, but copying that candidate's
192-byte constant layout would silently read the wrong source values.

## Actual packed vertex decode and deformation

The original VS consumes the source's encoded data, rather than an arbitrary
float approximation of a compatible-looking input struct. Its functional decode
is position multiplied by CB1.x/8192; UV0/UV1 equal
`(inputUV*CB1.w+32768)/65535`. Normal input is normalized after
`inputNormal*CB1.y-0.5`. Tangent input uses
`inputTangent*CB1.z*(255/127)-128/127`. Normal UV subsequently uses
`UV0*xform.zw-xform.xy`.

Each joint selects three float4 palette rows. Four weights combine those rows
directly; there is no extra sum normalization. Position is transformed by the
weighted3x4 matrix, then object/world/clip rows. Normal uses the weighted3x3
rows and independently divides each resulting component by its corresponding
weighted row's XYZ length before the object transform. Tangent uses those rows
without that row-length correction; its emitted handedness is-1. A generic
inverse-transpose substitute or normalized-weight repair is not this contract.

## Native declaration formats and CB1 scale production

Current native declaration construction12AF6F0 maps the declared format through
the table at2055120 and source semantic names at2055170. Native formats0..16
map to DXGI numeric values41,16,6,2,30,28,37,13,28,37,13,35,11,0,0,34,10.
Unsupported zero mappings stay unsupported. For example raw UByte4 is UINT,
ColorUByte4 is UNORM, and Short2/Short4 are SNORM input formats. Their source
byte sizes and mappings are preserved in the sanitized evidence.

Layout construction12AF550 initializes four scales to1. For Position, Normal,
Tangent and UV it replaces the corresponding x,y,z,w scale with32767 only for
native Short2/Short4 (formats6/7). Normalized-short formats9/10 use the same DXGI
formats but retain scale1. Changing the format without its scale changes the
actual vertex decode.

Setter12A20B0 stores four explicit scale arguments in context440..44C and marks
them dirty. Current draw binder129E540 creates/caches a16-byte buffer from that
exact vector and binds slot1 to both VSSetConstantBuffers and
PSSetConstantBuffers. The source layout-constructor to setter call edge still
needs complete qualification. Controlled original-stage comparisons should
precede accepting the actual deformed Sim, especially nonuniform scale and
packed edge values.

## Initial GEOM attribute packing

The installed initial mesh producer is `01A29A40`. It reads the GEOM interface's
raw vertex bytes and ordered attribute declarations, rather than treating the
serialized floats as the native GPU layout. Independent native primitives live
in `external_cas_assets::gpu_vertex`; they refuse non-finite inputs and native
signed-i32 conversion overflow. Original source fields remain authoritative.

- Position helper `019F6020` stores the low signed16 bits of
  `floor(f32(f32(position * 8192) + 0.5))` for XYZ. It writes only six bytes;
  the fourth Short4 word is not initialized by this helper.
- Normal helper `019F5FE0` stores the low byte of
  `trunc(f32(f32(normal * 127) + 128))` for XYZ. Its fourth byte is untouched.
- Tangent uses `trunc(f32(f32(tangent + 1) * 127.5))` for XYZ, stores the low
  bytes and writes the fourth byte as `0x80`.
- UV occurrence1 takes `abs(U)` before wrapping. Components above1 become
  `x-trunc(x)`; those below0 become `(x-trunc(x))+1`. Thus negative integers
  map to1, whereas positive integers above1 map to0. Encoding is the low16
  bits of `roundTiesEven(f32(x*65535))-32768`, matching native `CVTSS2SI`
  with the explicitly controlled default MXCSR value `0x1F80`.

An isolated interpreter executed the original helper leaves and exact UV and
tangent instruction blocks for 2,613 controlled cases, including negative,
half-step, wrapping and UV1 absolute-U inputs. All bytes match the independent
functional reference: 76,895 instructions at 136 original addresses, zero
intercepted calls, native I/O or game contact. An optional ignored Rust test
compares the retained private receipt directly with these public primitives.
The evidence contains hashes and functional contracts, not original programs.

This is not a complete mesh producer. Joint bytes are remapped through the
selected rig/part palettes; raw weight bytes are copied unchanged. Native tag
usage7 has part-specific behavior, including a source-R rescale by the float32
constant `4.047618865966797` and a part-code `0x72` branch. Those behaviors must
be joined to the correct source palettes and context before full-mesh acceptance.
Later deformation update `1A0D2A0` also has different rounding/saturation paths;
these initial primitives must not silently replace those update contracts.

### Explicit output profiles after deformation and seam averaging

The final stores in DMAP workers `01A39090` and `01A3B3F0` use nearest-even
`CVTPS2DQ(position*8192)`, signed16 saturation for X/Y, and the low signed16
word for Z. The API is `pack_post_deformation_position`. Normal stores use
`trunc(normal*127+128)`, signed16 then unsigned8 saturation, leaving W untouched;
the API is `pack_post_deformation_normal`. Its input is the vector after the
source's optional normalization, not a request to normalize an arbitrary vector.
The source worker gates that normalization and uses hardware reciprocal-square-root
plus two refinement steps. This numeric stage and its applicability are separate.

Seam worker `01A0D2A0` averages selected already-resolved contributors and writes
tangent XYZ with the saturated `127*t+128` rule, preserving W. The API
`pack_seam_average_tangent` accepts the actual post-average vector. It must not
be substituted for the initial tangent producer, and it does not imply that a
DMAP worker updates tangents. No profile is chosen automatically by these APIs.

The exact output instruction blocks match 1,565 controlled cases against the
compiled Rust primitives, including quantization ties and saturation/wrapping
edges: 28,152 original instructions at 54 addresses, zero intercepted calls,
native I/O or game contact. Together with the initial tests, the five Rust tests
pass all 4,178 retained original-instruction cases. The private updated receipt
is consumed only with explicit `EXTERNAL_CAS_UPDATED_GEOM_RECEIPT`.
[Sanitized updated-profile evidence](../../native/assets/evidence/updated-geom-packing.json)
keeps the source-stage and complete-deformation gates explicit.

[Sanitized pair evidence](evidence/cas-normal-skinning-pair.json) retains offsets,
names, hashes and raw declaration metadata, not original shader programs.
