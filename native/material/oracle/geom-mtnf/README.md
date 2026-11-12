# GEOM MTNF source comparison

Development-only oracle for the unchanged GPL-3.0-or-later `GEOM.MTNF` class
and `ShaderParam` enum from SimRipper commit
`862a32949e5156b371e2d2f83de4e37e0bb1afcc`, C. Marinetti.
`prepare.ps1` verifies whole-file SHA256 pins, extracts complete declarations
without editing their code, and writes an exclusive generated source path.
Keep that generated file and all build outputs under ignored `.local` storage.
Build with MSBuild `GeneratedReference` pointing to that file and an ignored
`BaseIntermediateOutputPath`; no package restore source is needed.

The executable reads private JSON `{materials:[{key,hex}]}` from stdin and
returns every header/raw decoded word, normal/emission index getters and the
source serializer's word array. Never publish game/creator bytes or outputs.
Runtime Rust does not depend on this harness.
