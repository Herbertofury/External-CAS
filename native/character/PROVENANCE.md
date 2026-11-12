# Character assembly provenance

The native persistence views and rig/TONE/provider resolution come from the
workspace tray, scene, assets and morph crates, with their existing schema pins.
Character assembly does not reinterpret missing native values as presets.

CASP/GEOM/RMAP and texture dependency behavior follows C. Marinetti and
contributors' GPL-3.0-or-later
[TS4SimRipper 862a32949e5156b371e2d2f83de4e37e0bb1afcc](https://github.com/CmarNYC-Tools/TS4SimRipper/tree/862a32949e5156b371e2d2f83de4e37e0bb1afcc).
Derived source keeps GPL-3.0-or-later notices. Source pins:

```text
RegionMap.cs
1399fde082d5e8740d6164136add3ff03de8beb8cc58cb56c9cfd6f346608837
GameFileHandler.cs
496ae8228784397c5710aa7755b7d827c53f2661fd62dc9b93a3a43f8da97da5
PreviewControl.cs
a8b8044ad5d1f768f57019e7073f5a09e479683fc06d7ad91c2b313d14f9ca3d
CASModifierTuning.cs
527a12b8982bd96c757fa78a42ec8c6615e9fd29a69ef9496bd5e3d26ef0f021
FNV.cs
a180b100f1f5aeb7d203525e6b5beb3a7bf5d6df4350ba6e1f93abe103cd75ff
Enums.cs
fa48d0ec2a7418ee30d8167682ca212d879494d94f8e9a906701fa7ffb0b006b
```

Combined-tuning structure/shared-reference semantics were independently checked
against the MIT-licensed
[S4TK models source 4345132fab79a92516095d22d9458b0db334dce5](https://github.com/sims4toolkit/models/tree/4345132fab79a92516095d22d9458b0db334dce5),
and separately installed maintained `@s4tk/models` 0.6.14. Its MIT notice is
retained in [third-party/S4TK-LICENSE.txt](third-party/S4TK-LICENSE.txt). The
independent oracle uses that separately supplied package; production Rust does
not depend on Node, C#, S4TK, game binaries or a running game.

The bounded PackedXml reader ports the recovered schema-checked observations
and S4TK `data-resource.ts` / `combined-tuning/serialization/binary-to-xml.ts`
reference semantics. It requires the declared metadata/node/attribute schemas
and rejects unknown schema offsets. Synthetic DATA fixtures and their retained
generator contain no game data. Generic class trees share this reader and the
same shared-reference expansion; they assign no class-specific defaults.

Reference deformation staging and region eligibility follow pinned
`PreviewControl.cs` `GetCurrentModel` / `isBodyTypeForRegion`, with primitive
math qualification documented in the morph crate. The reference tuning
selector follows `CASModifierScales` exact equality and documented defaults;
native applicability is not inferred from that source-tool agreement. The
controlled output explicitly excludes unqualified seam, head-ID transfer,
frame/conversion, pregnancy and final slot-target work. Every source-native
amount is retained per operation, including duplicate modifier occurrences.

The C# RMAP adapter links unchanged RegionMap/TGI/Enums source and includes only
a stub for an unused CASP sorting method. The C# tuning adapter extracts original
`ParseTuning` and its `Tuning` class only after whole-file SHA verification. Those
generated files/binaries, game resources, resolved private XML and user content
are never included in the source package. Neither reference tool behavior nor
reader agreement alone establishes game-native applicability or appearance.
