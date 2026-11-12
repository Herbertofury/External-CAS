# Native External CAS assets

Read-only Rust DBPF/CASP/GEOM/RIG decoding for the native editor. No game process
is contacted and no package, profile, save, Tray or creator file is changed.
Original payload bytes remain the authority for subsequent editing and saving.

`PackageIndex::open` streams the DBPF 2.1 index (all eight shared-field masks).
It keeps complete unsigned TGI keys, duplicate entries, raw storage markers and
tombstones. `PackageIndex::reader` reopens and revalidates the source metadata,
header/index SHA256 and locators. A reader amortizes that check over multiple
payloads. Payload reads support uncompressed, zlib (`0x5A42`) and RefPack
(`0xFFFF`), verify declared expanded lengths, check ordinary source changes and
accept cancellation. A caller-configurable `ReadLimits` defaults to 512 MiB per
resource; a budget failure is explicit, never truncation. Unknown compression,
storage markers, layouts and versions return a typed unsupported error.
`reader_cancellable` also cancels index revalidation. Retain a payload SHA256
when accepting an asset selection and use `read_pinned_resource` on subsequent
operations to detect same-size rewrites even when source timestamps and index
metadata are preserved. That is a payload check, not a whole-file snapshot.

`Catalog::scan_roots` incrementally visits explicit files/folders, accepts
cancellation, reports genuine counts and per-source errors, and preserves all
providers. Its `unique` lookup refuses ambiguous keys. Directory order does not
establish game or mod load precedence. Child links are skipped and counted;
choose a resolved target explicitly to scan it. Counts and package indexes use
memory proportional to the actual library. No library-size cap is imposed.
`casp_candidates(instance_id)` reports every full-TGI/provider candidate for a
native Tray outfit's instance-only reference; `unique_casp_instance` refuses
both cross-group and cross-package ambiguity. Discovery builds separate
CASP-instance and THUM-instance maps; a missing lookup does not iterate the
entire resource catalog. `texture_candidates` reports an
exact texture reference and specifically documented same-group/instance
LRLE/RLE2/DST/DDS alternatives. These retain missing and duplicate providers and
never silently choose a group, source or load priority.

`CasPart::parse` supports the recovered version 26–52 layouts, including labels,
ordered unknown/duplicate tags, appearance category fields, colors, complete
reference table and role-bearing LOD/texture references. It exposes convenient
`lod0_meshes` and `diffuse` fields without choosing a provider.
Its `compatibility` metadata retains every raw part-exclusion word (including
the variable v51+ list), modifier exclusions, version-present opposite-gender
and fallback instance IDs, and LOD descriptor count. Older cached metadata is
explicitly unknown and produces a cache miss; it never becomes zero flags.
Restriction helpers follow the source's flag bits. `has_mesh()` follows
SimRipper's LOD-descriptor test, independently of usable LOD0 references.

`Geometry::parse` decodes RCOL/GEOM versions 12–15, positions, normals, all UV
sets, triangle indices, raw joint/weight bytes, palette hashes and material
bytes. `weights` is exactly raw-byte / 255, not an inferred normalization.
Unrecognized attributes remain explicitly listed. A palette parse failure is
reported separately from usable rest geometry. The original GEOM is not
rewritten. `Rig::parse` supports version 3.1 bones/hierarchy/chains and rejects
cycles. `Rig::bind_geometry` requires every used palette hash to map uniquely.
It never chooses an age/species rig or substitutes missing bones.
GEOM tangent (usage6), tag (usage7), and signed vertex ID (usage10) are optional
per-vertex fields; absent fields remain absent. Ordered UV/seam stitch records
and the entire original payload remain available. `stitch_uvs(vertex)` returns
the first matching UV-stitch record, as the source DMAP reader does. It does
not fabricate vertex IDs or replace UV stitches with seam records.

## Native gallery thumbnails

`Catalog::thumbnail_candidates(instance)` returns all full `3C1AF1F2` keys and
all providers. Group `00000002` is the female thumbnail model and `00000102`
the male model according to TS4CAS Tools; other groups remain unknown. These
are hints, not group/load precedence. `ThumbnailDesignIndex::register` builds
alongside parsed or cached CASP metadata. Its `choices` separates same-instance
exact images from declared-parent design images and same-package designs with
the same nonzero OutfitId, complete LOD0 references and body context. Related
design images are never described as an exact color swatch.

`inspect_thumbnails` batches providers by source package, revalidates sources,
retains original bytes and SHA256 pins, reports encoding/dimensions/failures,
and groups proven byte-identical payloads. `equivalent_payload()` returns an
image only when inspection completed and all candidates have identical bytes.
Differing models, resolutions, formats or providers are not silently chosen.
Its caller-supplied resource budget also bounds total retained source bytes.

`thumbnail_decode::decode_pinned_thumbnail` runs natively with `image` JPEG/PNG
codecs. It verifies the supplied payload hash, validates dimensions/budgets,
and handles s4pi's ALFA PNG red-plane alpha. It supports both standard ALFA
insertion and the exact duplicated JFIF-tail structure written by s4pi
`ToJFIF`; it does not skip arbitrary corrupt bytes. PNG alpha is preserved.
Unknown ALFA layouts, JPEG-encoded/segmented alpha, animation, malformed data
and mismatched alpha dimensions remain explicit errors. EXIF orientation and
ICC transforms are not applied. Codec work belongs on a worker: cancellation
is checked around codecs and during compositing, not inside codec internals.
The image allocation budget is best effort, not a process peak-memory limit.

`evidence/thumbnail-qualification.json` records 49 real installed thumbnails
for 79 CASPs: 38 exact-swatch matches and 41 explicitly related-design matches.
DBPF payload hashes and exact matching agree with the recovered readers.
All 754,208 alpha pixels agree with independent Pillow decoding of the original
container. All sampled alpha planes happen to be opaque; nonopaque red-channel
alpha is tested separately. JPEG RGB differs between codecs for 88,875 pixels,
at most eight channel levels; pixel-identical game appearance is not claimed.
The example/oracle tools exchange source pixels through pipes and publish only
hashes and metadata. No asset bytes or images are included in the evidence.

`lrle::decode_lrle` decodes legacy version 0 and V002 mip-zero LRLE textures,
including embedded channel runs, palette runs, tiled placement, BGRA-to-RGBA
and exact alpha. Its cancellable variant checks obsolete work within long
runs. `decode_lrle_v2` remains available when a caller deliberately requires
V002. Budgets are configurable, source bytes remain unchanged and unknown
versions fail explicitly. This does not establish native color space or a
complete material shader.

Native GEOM positions are retained unchanged. The recovered Blender reader
converts native `(x,y,z)` to Blender `(x,-z,y)`; SimRipper's Collada path leaves
coordinates unchanged for Y_UP and rotates for Z_UP. Use a consistent Y-up
camera/rig/material/pose convention or convert the entire scene together.

The metadata checks detect ordinary concurrent writes, but are not an atomic
filesystem snapshot against a writer deliberately preserving timestamps.
Header/index hashes authenticate only observed index bytes, not all payloads.
Use separate whole-source pins or an immutable copy for stronger provenance.
No game load order, full material parity, native rig selection, animation
events/IK, morph semantics or native saving is established by these readers.

## API

```rust,ignore
let package = PackageIndex::open(path)?;
let mut reader = package.reader()?;
let bytes = reader.read_resource(&package.entries[ordinal], ReadLimits::default())?;
let casp = CasPart::parse(&bytes)?;
// Resolve casp.lod0_meshes and casp.diffuse across the explicitly selected
// source set; duplicates require an explicit provider decision.
```

## Verification

`cargo test -p external-cas-assets --jobs 2` covers all shared index flags,
unsigned keys, raw/zlib/RefPack, overlapping copies, invalid/truncated streams,
foreign locators, changed files, configured budgets, duplicate providers,
cancellation, all recovered CASP versions, GEOM skin data and malformed extents.
It also covers LRLE opcodes, tile orientation, alpha, truncation, cancellation,
instance/group/provider ambiguity and a timestamp-preserving payload rewrite.

The optional installed-source test requires `CAS_ASSET_TEST_MANIFEST` pointing
to the existing **private** recovery manifest. It streams and verifies every
source hash, compares selected decoded payload hashes, reads the actual CASP,
GEOM and authoring RIG, and binds every vertex. To compare all index locators,
CASP reference roles and GEOM position/normal/UV/joint/weight/index/palette bytes
against recovered Node readers, also set `CAS_ASSET_ORACLE_ROOT` to their
`research/resource-index` folder. `CAS_ASSET_NODE` can select an explicit Node
executable. These tests only read the authorized sources and do not persist
asset bytes. Without the environment variables, real-source evidence is not run.

`evidence/cc-qualification.json` records parity for five explicitly selected
installed CC samples: 79 CASPs and 25 version-14 GEOMs. Every parsed reference
and decoded geometry field agrees with the recovered readers. The filename-
selected skin sample is a tattoo/skin overlay; it does not qualify native TONE.
`evidence/cc-texture-qualification.json` records 18 actual LRLE textures (four
legacy and fourteen V002) whose entire decoded pixel hashes match the original
SimRipper C# decoder; fourteen V002 textures also match recovered JavaScript.
It includes representative RLE2/RLES JavaScript pixel hashes for the native
app's separate decoder to verify. Original inputs and private configurations
remain local and excluded from publication. These are component comparisons,
not full-Sim, material, game, app-performance or full-library acceptance.

The installed-source test is explicitly ignored in the default fixture run.
After configuring the private environment, execute it with
`cargo test -p external-cas-assets --test installed -- --ignored --nocapture`.
An explicit run without its required manifest fails instead of appearing passed.

The read-only `inspect` example accepts explicit package files and an optional
`--key TGI`; it prints labels/counts/bounds and content hashes, not asset bytes.

## Provenance and licensing

This crate is GPL-3.0-or-later. `LICENSE-GPL-3.0.txt` contains the license text.
Do not relicense these recovered format implementations as MIT merely because
the native UI uses permissive dependencies.

The Rust ports were made from the user's recovered November 4–5 External CAS
research (`dbpf.mjs`, `casp.mjs`, `geom.mjs`, `palette.mjs`, `rig.mjs`). That
research retains these authoritative bases and pinned comparison oracles:

- [TS4CAS Tools CASP](https://github.com/CmarNYC-Tools/TS4CASTools/blob/67d00ebb9016092b8f64ef94ec7ffb5329cf3342/src/XmodsDataLib/CASP.cs),
  [RIG](https://github.com/CmarNYC-Tools/TS4CASTools/blob/67d00ebb9016092b8f64ef94ec7ffb5329cf3342/src/XmodsDataLib/RIG.cs)
  and rotations, C. Marinetti, GPL-3.0-or-later.
- [Blender Sims 4 GEOM reader](https://github.com/blancodagoat/blender-sims4-geom/tree/551b560e9e3929a9f840f787be3cfba91da149d7),
  GPL-3.0-or-later; the recovered v12 comparison explicitly normalizes the
  reference's seam-count expectation and is not an unmodified-reader validation.
- [SimRipper](https://github.com/CmarNYC-Tools/TS4SimRipper), GPL-3.0-or-later;
  versioned palette layout researched at commit
  `862a32949e5156b371e2d2f83de4e37e0bb1afcc`.
  LRLE decoding retains CmarNYC and tau534's format implementation provenance
  from `LRLE.cs` at the same commit. The independent pixel test compiles that
  original decoder in memory, removing only unused UI imports.
- [s4py DBPF reader](https://github.com/thequux/s4py/blob/892150e78cfd0b6ca51258d5e0d740eb9adffcd1/lib/s4py/package/dbpf.py),
  used by the earlier recovery as an independent DBPF payload oracle.
- [s4pi ThumbnailResource](https://github.com/s4ptacle/Sims4Tools/blob/fff19365a12711879bad26481a393a6fbc62c465/s4pi%20Wrappers/ImageResource/ThumbnailResource.cs),
  Sims 4 Tools development team, Keyi Zhang and CmarNYC, GPL-3.0-or-later.
  `ToJFIF` and `UpdateAlpha` establish the container and red-plane semantics.
  TS4CAS Tools `ClonePackProperties.cs` FindCloneColorThumb establishes the
  full-instance thumbnail lookup and gender model groups; its `CASP.cs`
  identifies OutfitId as the design grouping field. No undocumented game
  precedence was inferred from a tool's package iteration order.

`flate2` is MIT/Apache-2.0; `sha2` is MIT/Apache-2.0. `image` is MIT/Apache-2.0
with JPEG/PNG codecs only; transitive licenses belong in app notices. Test-only `tempfile` is
MIT/Apache-2.0 and `serde_json` is MIT/Apache-2.0. No game binaries, source
packages, creator assets, extracted geometry, textures or private manifests are
included in this crate.
