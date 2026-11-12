# Native External CAS

This is the active Rust implementation, recovered from the newer local source
lineage described in `../docs/RECOVERY.md`. It is not an
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

The current wardrobe viewport is an item preview. It does not substitute a
reference body/skin for the imported Sim's stored physique, sculpts, TONE,
genetics or occult forms. `scene` resolves those actual native inputs and reports
missing/ambiguous dependencies while their full material/morph pipeline is built.

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

Applicable upstream GPL notices are retained in the individual crates and
`THIRD_PARTY_NOTICES.md`. The final combined distribution and license audit
must be completed before publishing a release.
