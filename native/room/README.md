# Installed CAS room

This crate loads the installed CAS world through its object-definition, model,
embedded mesh, material and texture links. The application supplies the game
installation root; source packages and world files remain read-only. No game or
creator assets belong in this crate or in Git.

`load(&RoomRequest, &AtomicBool, progress)` returns original resource bytes and
exact provider/index/payload pins alongside render-ready mesh data. Provider
selection follows the installed `Resource.cfg` priorities within the declared
base-client scope. Explicit caller choices retain every observed alternative;
equal-priority, deleted or mismatched dependencies do not silently fall back.
This scope does not establish active DLC or mod entitlement.

`RoomMesh::render_vertices` distinguishes independently checked reference-tool
coordinates from the candidate projection using original MATD position and UV
parameters. Both preserve the packed source attributes. World transforms require
an explicit caller origin. Unknown material fields, chunks, alternate LODs and
camera/light resources remain available in their original bytes. Unsupported
mesh layouts, absent required parameters and nonfinite applied transforms fail
with a reason. Resource/mesh budgets, progress and cancellation are supported.

Real local qualification loaded three meshes and three textures. Across both
retained model variants, all five meshes (433 vertices) matched the independent
source-tool positions, normals, UVs and indices bit-for-bit. The three selected
texture RGBA buffers also matched the retained independent images. The native
application rendered the selected room without the temporary gradient. These
checks qualify resource decoding and assembly, not indistinguishable game
shading: CAS camera tuning, lighting, alpha-threshold semantics and final shader
behavior still require direct source evidence and visual comparison.

Run `cargo test -p external-cas-room --offline`. The examples inspect an explicitly
supplied local source; their output includes private source pins and must stay
outside tracked files.

`load_cas_tuning` reuses the native character crate's schema-checked combined
DATA/XML/shared-reference reader. It returns camera context/region/pose presets,
zoom and transition settings, and key/rim/ambient lighting values. Native SimData
partners supply omitted class values only after name/schema and explicit value
agreement. All original float bits, unknown fields and source trees remain
available. An independent unchanged S4TK reader matched nine resolved camera/light
trees and 476 binary scalar/vector cells across nine partners: four contexts,
34 camera regions, 17 pose overrides and five lighting profiles. Projection
conventions, runtime camera anchoring and lighting equations remain open.

`preset_rotations(&CasTuningScene)` exposes each selected `Client_CASTuning`
record's six view angles and rotation/lerp settings. Its matching SimData partner
supplies only observed class defaults after explicit XML bit agreement. Angle
order follows the source inline schema's byte offsets; omitted values without a
partner remain unknown. The native motion selector, easing and update cadence
are separate renderer/host qualification work.

`RoomRequest.mods` accepts an explicit Mods root, observed profile options,
optional selected package subset and optional already indexed catalog.
`discover_mods` reads configured package paths; `index_configured_mods` prepares
their indexes for room, tuning or UI consumers. Configured component globs retain
path depth and Unicode names; config/options/index observations are revalidated.
Only package data is read. Script files and DLLs are never executed or imported.
The highest matching numeric priority is retained, consistent with the original
[creator load-order study](https://forums.ea.com/discussions/the-sims-4-mods-and-custom-content-en/where-can-i-find-the-official-documentation-about-resource-cfg/9165523).
Equal-priority resource ties require an explicit provider; no alphabetic tie rule
is assumed. An explicit package subset remains visibly incomplete coverage.

Standalone CAS camera/light XML overrides are joined with combined records by
class and instance while preserving their original full resource keys. Source
priorities and explicit provider choices select the logical record; ambiguous
representations stop. The binary partner must agree with explicit markup values.
MTST material states/variants, SKIN hashes/inverse binds and packed blend inputs
are retained. The initial material request is state/variant zero and all source
alternatives remain available; live material switching is not inferred.

A real configured replacement exposed glass, mirror, cubemap, shadow and skin
palette requirements. Those are explicit unresolved gates. Its packed buffers
can be inspected under an explicitly named diagnostic source-bind-space option;
normal app requests keep that option false and stop on unqualified bindings.
Null object Rig links or identity inverse binds alone do not prove a runtime
palette. No generic material or room approximation satisfies this gate.

## Provenance

The crate is GPL-3.0-or-later. RCOL/MODL/MLOD/VRTF/material and DST layout work was
checked against Sims4Tools commit
`b5db166dd4b935abc9f47c4c998fce98c61bd4de` (GPL-3.0), including its shader-data
enumeration. The object-definition layout follows S4TK models commit
`4345132fab79a92516095d22d9458b0db334dce5` (MIT); its notice is retained in
`S4TK-LICENSE.txt`. Independent reference projection used sims-package2glb commit
`907c3d4c2cabac133e00520eb9a78a2516beca23` (MIT). World chunk framing was checked
against the LlamaLogic/Binary-Templates resource observations at commit
`68fa20f93120671fe80380943fe6752099ac7b1a`; no proprietary template, game bytes,
images or shaders are redistributed.
