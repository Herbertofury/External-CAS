# Original DX11 sampler contracts

These contracts describe the installed client identified by SHA256
`95e40b75f76711c66b9f36a974ec04d43304a6f591d409d3d4019818f366237b`.
They were recovered through read-only static analysis and bounded instruction
emulation. Original bytecode, binaries, disassembly and private resource paths
are not distributed. No running game or profile was contacted or modified.

## Source reference to D3D11 slot

The PRECOMP root's relative pointer/count at `+0x5C/+0x60` identifies the
12-byte sampler-state records. A pixel-stage record's relative pointer/count
at `+0x1C/+0x20` identifies its u16 sampler references. These are distinct
from the constant-preparation pointer/count at `+0x24/+0x28`.

Actual stage binder `0136D3B0` passes precisely the sampler pointer/count to
leaf `012A1E10`. That leaf decodes the slot as `reference >> 12` and the
runtime state ordinal as `reference & 0xFFF`. It indexes the handle vector
at global `02983938`, stores the selected handle at context
`0x8F0 + slot*8`, and marks bit `slot` in dirty mask `0x8E8`.

Draw binder `0129E7B0` finds the first and last dirty slots and passes their
contiguous range to `ID3D11DeviceContext::PSSetSamplers`. This is a direct
source-to-device contract rather than a numerical interpretation of the refs.

For original CAS PS3051 the references are `000B`, `100B`, `200B`, `3014`,
`4007`, `500A`. Slots 0..5 therefore use states 11,11,11,20,7,10. Slot 3's
SkinDetailHSL state 20 uses point filtering; replacing it with an ordinary
linear sampler changes the original lookup behavior.

## Packed state to descriptor

Original builder `0129B2F0` creates the D3D11 sampler descriptors. Each source
record has three raw u32 words. Preserve all raw bits; the builder does not
consume bits 26..29 of the third word.

- Word0 is border RGBA, low byte R, converted with separate float32
  multiplication by the installed float32 `1/255` constant.
- Word1 is the exact float32 bit pattern of MipLODBias.
- Word2 address U, V and W occupy bits 0..2, 3..5 and 6..8. Codes 1..4 map to
  D3D enum values 2..5; every other code maps to WRAP 1.
- Magnification, minification and mip filters occupy bits 9..11, 12..14 and
  15..17. Explicit caller knob A plus bit 30 replaces the mip code with 1.
  Explicit caller knob B plus bit 31 replaces magnification with 2. The actual
  user-option meanings of these two caller arguments remain unqualified.
- If any resulting filter code equals 2, the filter is ANISOTROPIC `0x55`.
  Otherwise its numeric value is `(min!=0 ? 4 : 0) | (mag!=0 ? 16 : 0) |
  (mip!=0 ? 1 : 0)`. Word2 bit 22 adds the comparison bit `0x80`.
- Comparison codes in bits 18..21 map 0..6 to D3D 1..7 and every other code
  to ALWAYS 8.
- MaxAnisotropy is `max(1 << ((word2 >> 23) & 7), callerFloor)`.
- MinLOD is exact zero and MaxLOD is float32 maximum `0x7F7FFFFF`.

The retained source records include state 20 `001C0012` (point), state 11
`001C1212` (min/mag linear, mip point), state 10 `001C9212` (trilinear), and
normal state 7 `C11C9012`, whose two high setting flags must remain explicit.
All four use U/V clamp and W wrap. These facts do not qualify their host SRV
formats, plane construction or the live selected shader permutation.

## Controlled original-producer qualification

An isolated interpreter executed builder `0129B2F0` for 1,152 controlled
raw-word/settings cases. One intercepted `CreateSamplerState` boundary per
case copied the exact 52-byte descriptor and returned a controlled handle.
The builder otherwise executed its original instructions without allocation
or native API calls. All descriptors match the independent functional
reference byte for byte: 219,453 instructions at 251 original addresses.

The cases vary all source filter fields, comparison modes, address codes,
anisotropy bits, border colors, bias values, caller flags and floors. Some
deliberately produce combinations a real D3D11 device would reject, including
anisotropy above 16. This qualifies descriptor production, not device acceptance.
An application must preserve unsupported settings and refuse invalid required
bindings instead of repairing them silently.

[Sanitized evidence](evidence/source-sampler-contracts.json) contains source
hashes, offsets, mappings and qualification counts without original code.
