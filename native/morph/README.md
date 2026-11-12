# Native morphology

Rust-owned SCULPT, SMOD, BGEO, DMAP and BOND decoding, actual native appearance
dependency planning, and independently compared deformation mathematics.

The crate never writes packages, game profiles, Tray files or save files.
Parsed resources retain their entire original payload. Transformed GEOMs retain
the original byte buffer and every other parsed vertex attribute. There is no
mesh writer or game acceptance assertion in this crate.

## API

```rust,ignore
let plan = external_cas_morph::build_morph_plan(
    &native_sim, &scene_plan, &catalog, Some(&installed_policy),
    &external_cas_morph::MorphOptions::default(),
)?;
```

`MorphPlan` binds the native Sim SHA256 and the scene's resolved form. It exposes
native age, species, sex, frame traits, original physique values, source resources,
full linked TGIs, selected providers, payload hashes, parsed operations and
unresolved issues. Readers are reused per package. Package-index/source checks
remain enforced at reads. Differing providers require proven installed config
priority or an explicit choice; only byte-identical full-TGI providers can be
treated as equivalent without that priority.

Native physique names and FNV keys follow the pinned source. Zero and negative
physique entries remain visible and follow the source's positive-only selection;
no neutral physique is invented. Sculpts use last-native-entry per region, then
ascending region order. Face and body modifiers retain native wire order. Link
selection uses the first delayed BGEO key, as the source does; the remaining raw
keys are preserved. Age/gender metadata is retained and is not misrepresented as
a verified game eligibility rule.

`Bgeo::parse`, `Dmap::parse`, `Sculpt::parse`, `SimModifier::parse` and
`Bond::parse` expose independently usable readers. Application APIs operate on
copies and return explicit errors for missing required metadata:

- `apply_bgeo(geometry, bgeo, lod, weight, copy_face_morphs)` requires real GEOM
  vertex IDs. Duplicate IDs remain distinct vertices. No IDs are synthesized.
- `apply_dmap(geometry, original_positions, shape, weight, normal)` uses actual
  UV set 1, first UV-stitch record and its first coordinate, native robe/morph
  tag weights, original X sign, source float arithmetic and edge sampling.
  Normals use source map-relative coordinates, not an invented UV resampling.
- `apply_bond_mesh(geometry, rig, bond, weight)` accumulates adjustment position
  deltas and updates normals in source order, using original byte skin weights,
  native palette hashes, descendants and the pre-adjustment rig.
- `apply_bond_rig(rig, bond, weight)` updates a separate runtime rig copy. Its
  returned local rotations follow the source's balanced quaternion view. Mesh
  deformation is a separate operation; changing the rig alone is insufficient.

Weight zero is an explicit no-op for geometry, including unavailable metadata.
Nonzero operations fail on unresolved required IDs, palettes, bones or fields.
BGEO/BOND opaque suffixes block application. SCULPT/SMOD suffixes remain visible
as planning issues. Parsing alone does not authorize discarding them.

## Qualification

Run `cargo test -p external-cas-morph --offline -j 2`. Eleven tests cover native
frame-trait paths, physique names and parsing, stale revisions, missing fields,
malformed/truncated payloads, signed/duplicate IDs, first seam semantics,
positive/negative/zero weights, accumulated skeletal offsets and byte retention.

Read-only local qualification compared full float arrays to the recovered
unchanged C# source, including actual installed resources selected for a real
imported Sim. Sanitized counts are in
[evidence/morph-qualification.json](evidence/morph-qualification.json).

- BGEO: 6 cases, 9,222 vertex observations on two actual installed head GEOMs.
- DMAP v5: 16 cases, 39,144 observations on the recovered authoring-tool body
  reference fixtures; these fixtures are explicitly test inputs.
- Native linked DMAP v7 shape/normal pairs: 13 pairs, 8 installed body/head/feet
  GEOMs, 208 cases and 264,004 observations. Native physique amounts were used.
  Modifier tests used explicit weights because their tuning remained unresolved.
- Native linked BOND rig: all 32 resources, 96 cases, 15,840 bone observations
  against the actual 165-bone, game-selected rig.
- Native linked BOND meshes: all 32 resources, 8 installed GEOMs, 512 cases and
  649,856 observations. Every resulting position and normal array matched.

Opt-in examples `qualify_math`, `qualify_native_dmaps`, `qualify_bond_rig`,
`qualify_bond_mesh` and `resolve_morph_import` consume explicit private fixture,
project, package and oracle paths. Receipts are written exclusively to new
requested files. No fixture assets, project contents or user identities are
included here. The BOND mesh oracle adapter extracts unmodified source methods
only after whole-file SHA256 checks; generated source and binaries stay ignored.

## Remaining acceptance gates

`MorphPlan.complete` remains false. Native combined-tuning discovery and scale
binding, frame/clothing conversion, pregnancy, missing-ID transfer, mesh-region
eligibility and the combined application order still need qualification.
`ModifierScales::Qualified` is an explicit caller evidence boundary, not a tuning
discovery shortcut. Defaults leave all native modifier scales unresolved.

The real imported Sim sample resolved all 92 SCULPT/SMOD resources and all 137
linked morph resources (79 BGEO, 26 DMAP, 32 BOND) in an explicitly limited
three-package installed scope. Its 87 modifier tuning scales remained unresolved
and one SCULPT contained an opaque suffix. Original Tray source hashes and
package index pins remained unchanged. These are useful native dependencies,
not a declaration of complete game-equivalent Sim rendering or game acceptance.

See [PROVENANCE.md](PROVENANCE.md) for source and license boundaries.
