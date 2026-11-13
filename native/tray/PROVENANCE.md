# Native Tray module provenance and limits

New Rust implementation under GPL-3.0-or-later, preserving the recovered
External CAS project's license boundary. No game binaries, creator assets or
proprietary reference implementation source are included.

Schema facts were checked against CmarNYC-Tools/TS4SimRipper revision
`862a32949e5156b371e2d2f83de4e37e0bb1afcc` generated source (GPL-3.0):

- `src/Proto/Sims.cs`, SHA256
  `b9e8768175cadcc59a51bc6d613e1432ca79da14487d2f8258c5041df9a0e66f`:
  FamilyData.family_account is message field 1; AccountFamilyData accountid,
  familyid, familyname and repeated SimData are fields 1, 2, 3 and 6.
- `src/Proto/FileSerialization.cs`, SHA256
  `d3d493b967cabc5b6bf7cc1d6fd9de31c4513cb38ffc02bdd286af7a26c75dab`:
  SimData.sim_id and household_id are fixed64 fields 1 and 4; first_name and
  last_name are strings 5 and 6; gender, age and skin_tone are varints 7, 8, 10.
- `src/Proto/Exchange.cs`, SHA256
  `de29d591d8a588cc4895179a41aa77ec7fb681fc7589b010dafdee0c50ba05ed`:
  TrayMetadata.id/name/metadata are 1/4/10; SpecificData.hh_metadata is 2;
  household metadata sim_data is repeated 2; metadata Sim ID/name fields are
  5/3/4. Tray ID, family ID, native Sim ID and metadata Sim ID remain separate.
- `src/Proto/Outfits.cs`, SHA256
  `1c447b2093a051af7dffbd1de71fbd4e9ea775411b73d2b0a06ba6b1b1e56774`:
  OutfitList repeats OutfitData at field 1; OutfitData outfit_id/category,
  parts/body_types_list/part_shifts are 1/2, 5/7/12. BodyTypesList field 1
  repeats uint32 and ColorShiftList field 1 repeats uint64. OutfitData field15
  contains LayerIdsList, whose field1 repeats uint32; native_layer_ids reads
  this wrapper without synthesizing absent values and retains unknown bytes.
- `src/Proto/S4Common.cs`, SHA256
  `8bc900d52aee2bd74d8311ec9e8c4277da8d322dc14db17a40c9294ea12a71e0`:
  EA.Sims4.IdList field 1 repeats fixed64. Both packed and unpacked wire
  encodings are handled without changing their original encoding.

Recovered local research `tray-validation-lab/HOUSEHOLD-ROUNDTRIP.md`
records agreement between these FamilyData definitions and independently
inspected installed descriptors. `S4TI-IMAGE-SECTION.md` records the version-2
wrapper and image-section layout checked with fabricated inputs against the
installed S4TI reader. That is reader compatibility evidence, not native game
acceptance. This implementation retains even uninterpreted suffix bytes.

The older approximate filename-prefix grouping has deliberately been replaced
with metadata Tray ID plus explicit metadata Sim IDs. Distinct or ambiguous
household providers fail grouping. Portraits remain exact opaque files;
encrypted wrappers and alpha masks are not advertised as decoded previews.

The source-pinned recovered `tray_typed_edits.py` supplies the design of
expected-revision/identity/body-type guards and aligned-array checks. Rust
draft editing changes precisely one existing eight-byte fixed64 payload. No
field, list, wrapper, portrait, suffix or unknown byte is regenerated. The
schema evidence identity is recorded in schema-2 project manifests and checked
on reopen. Current file hashes and original source baseline hashes remain
separate. Undo/redo is session-owned; reopened drafts start a new history.

This module reads original native files and writes only new external project
directories. Projects contain byte-identical native copies and a SHA256
manifest, verified on reopen. Original paths are recorded privately to detect
subsequent source changes; they are never overwritten. Incomplete writes keep
an explicit recovery marker and cannot reopen as committed projects. No-op
preservation is literal bytes, not protobuf reserialization. Slot drafts are
structural candidates: CASP eligibility, rendering, load precedence, portrait
regeneration and game import remain unqualified. Native name editing is not
enabled. No game-ready Tray result is silently manufactured.

File identity checks are point-in-time observations. Double reading and hashes
detect observed changes but do not lock an entire multi-file library against a
malicious concurrent writer. The owner must still handle stale/concurrent game
state separately before any future synchronization.

## Native per-part color edits

`OutfitData.part_shifts` is field12 and `ColorShiftList.color_shift` is repeated
uint64 field1 (`Proto/Outfits.cs`), accepting ordinary or packed varints. The
stored occult path is Sim30 → attributes17 → tracker repeated3 → form outfits21.
The entire original wire representation remains the authority for unsupported
fields. Only selected values and necessary enclosing lengths are reconstructed.

The reference neutral value `0x4000000000000000` is explicit in generated
`PartData.color_shift`/distributor schema defaults and the reference reader's
absent-array handling. Four signed16 Q2.14 lanes are decoded in pinned
`PreviewControl.ImageStack`: brightness bits0–15, saturation16–31, hue32–47,
opacity48–63. This crate writes raw uint64 values and preserves untouched bits;
it does not infer slider quantization, allowed ranges or native compatibility.

The independent color oracle uses the unchanged generated schemas in the
pinned SimRipper source and protobuf-net 3.2.30. Its 55-file manifest consists of
ordinal-sorted `filename + NUL + lowercase SHA256 + LF` entries and has SHA256
`089452376ca4b45780b306f317a1b2c669873571f3383019b0bb958bc461f6fe`.
No generated schema payload, game library, user data or oracle binary is bundled.
The supplied color-mod archive was inspected read only; no mod code or DLL was
executed or reused in the Tray writer.
