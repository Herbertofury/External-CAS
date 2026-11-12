# Installed-game CAS fidelity — 2026-11-12

The target is the entire installed CAS experience: UI, room, Sim, behavior and
features. The source requirement is [issue #1](https://github.com/Herbertofury/External-CAS/issues/1).
Extraction, decoded metadata or a similar layout does not satisfy this target.
Game/creator resources stay local and read-only; the shipped code must resolve
them from the selected installation. Source UI reuse takes precedence over
invented replacement art. Any necessary replacement needs a measured reason and
separate fidelity/feature acceptance.

## Evidence custody

Exact provider paths, full resource keys/ordinals, selected payload digests,
reference screenshots and extracted diagnostic assets are deliberately private:

- [Reference capture/provider pins](../../.local/fidelity/ui-room-reference-pins.json)
- [Installed library names and source metadata](../../.local/fidelity/gfx-font-atlas-observations.json)
- [Native resource/outline/atlas validation](../../.local/fidelity/native-ui-assets.json)
- [Independent BC5 and FreeType validation](../../.local/fidelity/native-independent-validation.json)
- [Independent compacted-glyph comparison](../../.local/fidelity/native-glyph-oracle-comparison.json)
- [Source symbol static rendering comparisons](../../.local/fidelity/source-symbol-golden-results.json)
- [Actual native source navigation controls](../../.local/fidelity/source-controls-20261112)
- [Actual app with installed UI resources](../../.local/fidelity/native-gallery-20261112T125529381994)
- [Actual complete Sim and source room](../../.local/fidelity/native-character-app-20261112T143213513504/app.json)

Those links resolve only in this checkout and must remain ignored. The app source
and test fixtures contain no extracted game artwork, fonts or user content.
The pure-EA control capture is the layout baseline; historical Apex-modified
ActionScript/source is research context and must not be represented as an
untouched installed-game baseline.

## Source visual and behavior checklist

- [x] Locate the actual installed CAS movie, imported CASLibrary,
  ComponentLibrary and gfxfontlib through the game UI package.
- [x] Resolve source font aliases from actual GFX symbol metadata: headline and
  bold headline use the two Sims Sans faces; text/subtitle/body use the three
  BentonSans faces. Preserve symbols and JP/KR/Traditional Chinese faces.
- [x] Decode all source font curves/kerning natively and load them in memory in
  the actual app; validate independent curves and font consumer acceptance.
- [x] Resolve and decode two original high-resolution UI atlases, with 65 CAS and
  108 Component sub-image rectangles. Preserve source channel meaning explicitly.
- [ ] Evaluate exported source symbols and their real state timelines, bitmap
  matrices, placements and color transforms; replace temporary authored marks.
  Six original navigation symbols and 24 first-state frames now render in the
  actual app and passed independent static comparisons. Other components and
  source animation/state transitions remain open.
- [ ] Verify the source atlas channel presentation and compare actual rendered
  native symbols against authorized installed control captures.
- [ ] Reuse source panels, button states, clipping, swatches, filter chips,
  scrollbar/cell geometry, outfit tabs and native font size/baseline rules.
- [ ] Match top-left identity/age/voice/personality modules and contextual
  body/face/feature controls; prevent flags from being interpreted as ordinals.
  SimData gender is the AgeGender bitfield (male/female/unisex flags); raw CASP
  catalog gender is a different high-byte representation.
- [ ] Match actual household portraits, selection, rotation, undo/redo,
  randomization and action strip. Existing buttons must call real native editing
  or persistence handlers. Do not present unsupported controls as functional.
- [ ] Match source behavior for body, face, genetics, age/species, outfits,
  swatches, hair, makeup, accessories, identity, traits, occult/custom forms and
  hybrid fields. Mark each tested supported patch and preserve unknown data.
- [ ] Retain approved search/filter/favorites, expandable catalog and catalog
  sizing/column controls as the source-native UI behavior is mapped.
- [ ] Qualify all-item/swatches color editing from the supplied update, including
  actual source ranges, defaults/resets, opacity/transparency and separate
  specular controls; see [the typed metadata boundary](COLOR_SLIDERS_SOURCE_20261112.md).

The current native floating layout uses real parsed Sim metadata and existing
catalog/Sims/pose/project handlers. It does not yet reproduce the complete source
display list or game service semantics. Its catalog is an actual native control,
and its current control geometry/icons remain temporary.

## Room, full Sim and camera checklist

- [x] Recover the actual CAS world → object definition → model chain and private
  source/provider pins. Tiny world camera/light payloads alone do not establish
  the actual CAS camera or lighting system.
- [x] Retain the recovered RCOL/MODL/MLOD/MATD source research and independent
  packed-buffer comparisons. No game model/texture is promoted into Git.
- [ ] Load the room model natively with the correct embedded/external mesh
  variants, materials and game coordinate transforms.
- [ ] Resolve MATD position/UV transforms and source shader behavior. Prior
  extracted GLB projections are authoring candidates, not visual parity evidence.
- [ ] Resolve real CAS camera/light tuning and contextual zoom/shift controls.
  Preserve opaque/unknown tuning rather than inventing source-backed values.
- [ ] Render the assembled actual Sim, source skin/eyes/hair/materials,
  proportions, morphs, outfit layers, contact shadow and valid pose deformation.
- [ ] Golden comparisons cover multiple body types/ages/species/skin tones,
  outfits, accessories and actual CC under the same source camera/light state.

The app now draws the actual source curved backdrop and two floor planes, with
their real geometry and texture pixels. Position/normal/UV/index arrays match
the retained independent reference. Source MATD alpha/shadow behavior remains
open. Four camera contexts, 34 regions, 17 pose overrides and five lighting
profiles have been decoded and independently compared, including SimData class
defaults. Projection and lighting equations are not established by those records.

The current whole-Sim path renders actual native current-outfit geometry,
source-reference BGEO→DMAP→BOND deformation, the actual rig and a skin/outfit
atlas from 33 real layers in the first fixture. It does not use an invented body,
authored mouth or default skin. The private app receipt retains 297 selected
resource pins. The A-pose, skin/material response, shadows, morphology seams and
remaining source UI are visible gaps against the retained same-Sim game capture.
These observations do not establish indistinguishable appearance.

## Qualification still required

- [ ] Versioned visual/interaction checks for supported installed game builds,
  pack combinations and CC changes; invalidate stale selected source pins.
- [ ] Profile actual populated native startup/input/scrolling/frame time/memory;
  report full measurements and source fidelity alongside any performance change.
- [ ] Complete independent clean-launch → import → assembled appearance → edit
  → pose → save → close → reopen equivalence and safe Tray readback.
- [ ] Qualify selected Live Sim → external full CAS → explicit Apply back to
  Live, with stale-state detection, backups, readback/reload and no forced entry
  into in-game CAS. Do not use or interrupt the active Apex game session.
- [ ] Package and license the exact qualified Windows build, preserving source
  assets locally and reporting unsupported boundaries honestly.

Implementation boundary and API are documented in
[native/fidelity](../../native/fidelity/README.md). This checklist remains open.
