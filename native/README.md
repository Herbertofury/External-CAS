# Native External CAS

This is the active recovered Rust implementation. Its source publication
boundary is documented in `../docs/EXTERNAL_CAS_PUBLIC_SOURCE.md`. It is not an
accepted release. Complete Sim appearance, native household creation, game
synchronization, large-library acceptance and distribution gates remain open.

Build on Windows with Rust 1.99.0 and the committed dependency lock:

```powershell
pwsh -File scripts/build-native.ps1
```

Run `target/debug/external-cas.exe`. Wardrobe opens installed packages/folders;
Sims reads a Tray directory into an exclusive external project copy; Project
reopens verified native files and edits existing outfit slots. Draft changes
revalidate the selected source, preserve unsupported bytes, and create a verified
recovery revision under `%LOCALAPPDATA%/ExternalCAS/Recovery` before appearing in
the editor. Prior projects and original Tray files remain unchanged.

The viewport is Rust/wgpu, with depth, original GEOM, original decoded diffuse
textures, source Y-up coordinates and hash-bound GPU skinning. Poses reads CLIP
resources from selected package scope and an explicitly chosen RIG. Current
sampling is source-tool component playback: events, IK and unsupported controls
are retained but not executed. It is not proof of native game animation fidelity.

Normal imported-project viewing now assembles the actual Sim through `character`,
`morph`, `material` and `room`. It resolves the native current form/outfit, rig,
BGEO/DMAP/BOND resources, real TONE/detail/sculpt textures and layered outfit
diffuses. Selecting a wardrobe item still opens a separate item inspection;
Project → View full Sim returns to the assembled draft. No reference body or
invented skin is substituted. Missing and unqualified inputs stay explicit.

The first actual complete-Sim render uses 16 visible source meshes; replacing
the top with real installed CC renders 17 meshes/38,418 vertices with the native
packed color value retained. These are development observations, not visual or
game acceptance. Source-reference deformation, atlas rounding/ties, skin hue,
complete material/shader binding, full ASM scheduling, shadows and UI behavior
remain open; the original current CLIP18 standing idle is restored.
The real room geometry/textures and source camera/light tuning load independently;
current light-transfer equations and camera conventions remain candidates.

Runtime components do not require Node, a browser, Studio, Blender or the game.
Recovered Node/C#/Python tools are private/reference comparison tools only.
Never bundle installed packages, creator assets, derived textures, private
manifests or captured Sim portraits with a source commit or distributable.

Component validation commands:

```powershell
cargo test -p external-cas -p external-cas-assets -p external-cas-tray -p external-cas-animation -p external-cas-scene --locked --jobs 2
```

Real-source checks use explicitly configured private inputs. Their source
receipts and observed limitations live in each crate's validation documentation;
passing generated fixtures alone is not real-source or end-to-end acceptance.

Diagnostic CLI flags (`--assets`, `--select`, `--rig`, `--pose`, `--pose-tick`,
`--project`, `--hidden`, `--report`, `--capture`, `--exit-on-ready`) operate the
same native application. Captures show that application's own viewport, without
desktop/game interaction. Reports include errors and acceptance=false; automated
verification exits nonzero on a failed requested preview/pose. These controls
are validation aids, not a substitute for complete user workflow acceptance.

`--game-root ABSOLUTE_PATH --project PROJECT --sim-view` requests whole-Sim
rendering. `--ui-assets PATH_TO_UI_PACKAGE` loads the local original font/icon
sources. Repeatable `--character-assets PACKAGE` restricts only the diagnostic
character indexing scope and reports incomplete installed coverage; normal
loading uses configured installed packages. None of these flags contacts a game.

Applicable upstream GPL notices are retained in the individual crates and
`THIRD_PARTY_NOTICES.md`. The final combined distribution and license audit
must be completed before publishing a release.
