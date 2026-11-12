# External CAS

A Rust-native Windows application for the complete Sims 4 Create-a-Sim
experience outside the game. The app owns the room, rendering, editing, posing
and animation; the game is a separate synchronization and verification boundary.

**Active development. This is a source checkpoint, not a finished application
or qualified release. No accepted installer is available yet.**

The target is the actual installed CAS interface, assets, room, appearance and
behavior, with responsive large-library editing. Game fonts and resources are
read locally from an authorized installation; proprietary assets and creator
content are never bundled here. Temporary control marks/backgrounds do not
establish fidelity. Improvements require actual application measurements.

See the [wiki](https://github.com/Herbertofury/External-CAS/wiki),
[milestone](https://github.com/Herbertofury/External-CAS/milestone/1) and
[acceptance checklist](docs/ACCEPTANCE.md).

## Current implementation

- Native egui/wgpu app, Rust resource loading, original GEOM/textures and GPU
  skinning; actual installed CLIP previews with explicit unsupported controls.
- Persistent metadata/scoped indexed search, virtualized real thumbnails,
  source swatches, background cancellation and bounded thumbnail caching.
- Installed CAS fonts/atlases decoded in Rust and kept in memory; source
  symbol/display-list behavior remains under construction.
- Lossless native Tray project copies, revision-checked existing outfit-slot
  edits, undo/redo, verified recovery and pinned session restoration.
- Actual Sim appearance, installed resource priority, native outfit/rig/TONE
  and morphology planning, with missing/ambiguous/unsupported dependencies exposed.

The current viewport is an item preview. Full imported-Sim material/morph
composition, the native room/control behavior, complete creation/editing,
Tray/game acceptance, synchronization and release packaging remain open.
Component comparisons do not imply a complete CAS workflow.

## Build and run

Windows, Rust 1.99.0, Cargo and the MSVC build toolchain are required.

```powershell
pwsh -File scripts/build-native.ps1
.\target\debug\external-cas.exe
```

Choose installed packages/folders in Wardrobe, or read a Tray household into a
separate project in Sims. Original packages and Tray files are read-only.
Existing-slot edits create verified external recovery revisions before being
published in the editor. The app currently makes no changes to a running game.

Optional `--ui-assets "<game>\Data\Client\UI.package"` loads the installed CAS
fonts/atlases locally. No extracted resources are written or redistributed.
See [native component details](native/README.md) and
[profiling instructions](docs/PROFILING.md).

```powershell
cargo test --workspace --locked --jobs 2
```

Tests that require private installed inputs are opt-in and say so in their
component documentation. Default fixture tests are not real-game acceptance.

## Source and content boundaries

External CAS is separate from TD1/Apex, CAS-Helper and Sims4Foundry. The native
source was recovered and ported in an isolated checkout; this repository omits
unrelated Apex history, private evidence, Sim data and game/CC resources.

The combined application is GPL-3.0-or-later; individual component licenses and
upstream provenance remain applicable. See [LICENSE](LICENSE) and
[native notices](native/THIRD_PARTY_NOTICES.md). Do not publish private captures,
catalogs, extracted resources or source packages in issues or pull requests.
