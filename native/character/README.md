# Native character assembly

Rust-owned assembly of a native Sim's selected form/current outfit, actual CASP,
GEOM and RMAP resources, game-selected rig, TONE/material dependencies and morph
dependency plan. All source data is read only. No body, head, skin, outfit or
genetic preset is substituted when a native reference is missing.

`assemble_character(sim, catalog, policy, options)` returns `CharacterPlan`.
`assemble_character_with_progress` additionally accepts an `AtomicBool` and a
progress callback. Missing or malformed dependencies become diagnostics through
all stages; explicit cancellation stops publication of a new plan. Scene/morph
provider choices remain explicit or use the proven installed config policy.

Each `CharacterPart` retains its native slot/color shift, source observation,
parsed CASP, actual rest GEOMs with native rig binding, RMAP membership and material
candidate payloads. `ResourceObservation` retains full TGI, source index/payload
SHA256, provider candidates, effective config decision and immutable raw bytes.
Readers and payloads are reused within one assembly. Byte-identical full-TGI
providers may be equivalent; differing payloads remain unresolved without proven
priority or a deliberate choice. Resource work is bounded and cancellable.

`CharacterMesh.native_rest` is an explicitly labeled inspection step. It is not
a claim of complete morphed/native appearance. `morphed_rest` remains absent
until every required context/weight/application gate qualifies. Genetic part
references remain observations; they are not silently injected into the current
outfit. Parsed native head/outfit slots supply actual geometry.

Material dependencies expose native CASP roles and the source tool's documented
LRLE → RLE2 → DST texture-family order. A selected payload is not a qualified
decode/composite: `decoder_qualified` and complete-character gates remain false.
Skin/outfit/sculpt pixel composition is owned by the separate material crate.

`inspect_combined_xml` selectively retains the shared group and exact
`Client_CASModifierTuning` records. It resolves bounded shared references,
preserves context tokens, duplicate names, every scale binding and reconstructed
record XML, and reports cycles, missing references or unknown native tokens.
Its first-entry scale dictionary reproduces the reference parser's behavior;
that dictionary is not automatically applied to a native Sim. Unknown tokens
retain their exact spelling and receive no invented meaning.

`inspect_combined_data` reads the proven seven-table PackedXml DATA layout.
`inspect_combined_tuning` detects XML versus DATA. Declared schema references
must be valid; a matching row size never substitutes for an unknown schema.
`inspect_combined_classes(bytes, classes, cancel)` uses those same readers and
shared expander to return serializable, uninterpreted `ResolvedTuningRecord`
trees for exact requested classes, including camera and lighting work.
`inspect_tuning_classes` additionally accepts a real standalone `I` document
for explicitly authorized mod overrides. It preserves root attributes and
unknown class fields without synthesizing a combined wrapper or defaults.

`CharacterPlan.tuning` retains every selected combined container observation.
`select_reference_tuning` exposes the pinned tool's exact context predicate,
matching dictionaries and its documented fallback-scale rule separately from
game-native eligibility. Conflicting matching dictionaries remain unresolved.

`build_reference_deformation` produces controlled runtime copies from actual
native inputs in BGEO→DMAP→BOND stage order, retaining original DMAP mirror
positions and updating the runtime rig once after each BOND. Each operation
records applied, zero, source-ineligible or unresolved status. Missing native
IDs are never fabricated. Work budgets, cancellation and stale Sim revisions
are checked. Output `native_qualified` stays false while seam construction,
clothing conversions, pregnancy, slot targets and visual comparison remain
open. The strict `morphed_rest` acceptance field is not filled by inspection.

Source-derived CASP exclusion words, declared opposite-gender/fallback choices,
LOD0→LOD1 fallback and RMAP layer dominance are exposed as reference selection
steps. Original native slots/resources remain retained. They are separate from
native-game eligibility and visible-result acceptance.

`resolve_cas_stand_idle` resolves the installed human `CAS` ASM and its concrete
public `Idle` decision graph. Exact native age/sex and the selected stored form
supply selector inputs; unrelated controls retain declared ASM defaults with an
explicit engine-delivery gate. References and bounded scalar selectors resolve
without executing controller code. Positive random choices remain ordered and
the first reachable variant gives a deterministic initial preview. Native random
scheduling, transitions, ghost/frame delivery, events, IK and control channels
remain unqualified. A literal stored CLIP name must match the selected controller.

Current v18 CLIP rig references resolve separately as `declared_rig_source`.
Missing dependencies or different bone hash/parent ordering fail selection.
Different local binds remain reported without replacing the native Sim rig or
inventing retargeting. The actual 165-bone sample matches topology; four right-arm
binds differ by up to approximately 5.018e-6 per component. The
[sanitized receipt](evidence/cas-stand-idle-qualification.json) records these exact
bone hashes and values. It also records the installed source selection trace and
the independent current-reader comparison. Runtime source pins include the ASM,
CLIP and its explicit declared rig.

Current `CharacterPlan.complete` is deliberately false. Required remaining gates
include full source coverage, native part-selection/shoe/frame qualification,
native tuning source/context qualification,
combined deformation order and faithful material composition. The read-only
inspection output remains usable while those diagnostics are unresolved.

## Evidence

Twenty-four tests pass with `cargo test -p external-cas-character --offline -j 2`:
RMAP raw retention/base-region queries/every truncated prefix; shared tuning
graphs and actual name-to-scale binding; cyclic/missing/duplicate references;
NaN/DTD/malformed documents; every XML prefix; cancellation and unknown fields;
sex/frame restrictions, declared replacement order and exclusion-mask boundaries.
They also cover every DATA prefix, invalid schema/pointers, generic class
selectors, duplicate/shared cycles, current-priority equivalence versus
historical data, tombstones, deformation stage order, original-position mirror
semantics, work budgets, cancellation and stale revisions.

Actual imported-Sim qualification in an explicitly limited three-package scope:

- 15 native outfit slots, 17 actual GEOMs, 5,619 vertices. All GEOMs parsed and
  bound to the actual game-selected rig. The native Head slot was present.
- All 9 associated RMAPs /20 blocks exactly matched unchanged source
  `RegionMap.cs` fields and full mesh keys.
- One actual installed combined XML container yielded 7 modifier records and
  210 scale entries. Every context/scale bit matched unchanged C# `ParseTuning`.
  Independently expanded context tokens and ordered scale bindings matched
  maintained S4TK 0.6.14. Two records retain the unknown `UNUSED_FLAG` age token.
- Original Tray hashes and package index pins remained unchanged. No game,
  Mods, Tray or save writes occurred. No game process was contacted.

The subsequent 21-package base client/simulation/preload scope indexed 1,141,554
resources and resolved all 33 outfit material references plus the actual native
skin. The skin decoded 1024×2048; all diffuse/shadow inputs also decoded 1024×2048.
Five DST5 normal maps and RLES auxiliary streams remain explicit material
decoder limitations in that receipt.

Five supported actual binary combined containers yielded 26 modifier records
and 1,539 exact scale entries. Context tokens and every ordered name/scale bit
matched independent maintained S4TK; reconstructed records matched unchanged
C# `ParseTuning`. The malformed EP01 Full container remains refused. The
effective base/preload container had 9 records / 516 scales; the imported Sim's
reference context selected one three-scale dictionary.

Controlled reference stages applied 923 mesh/operation pairs across 17 actual
meshes, changing 4,390 final positions and 4,385 normals while retaining every
encoded GEOM byte. This measured 626ms in the debug inspection run and is not
an app interaction benchmark or native visual acceptance. Original source
conflicts stayed zero. Native embedded-record precedence/context and full
installed-pack coverage still require qualification.

Private receipts and fixtures stay in ignored local storage. The opt-in
`inspect_character` and `qualify_tuning_xml` examples require explicit source,
project, oracle and output paths; outputs are exclusive new files. Caller code
must retain/revalidate source/config observations before accepting an asynchronous
result or applying a project change. See [PROVENANCE.md](PROVENANCE.md).
