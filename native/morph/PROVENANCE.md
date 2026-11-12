# Morphology provenance

The Rust math/layout ports follow C. Marinetti and contributors' GPL-3.0-or-later
[TS4SimRipper revision 862a32949e5156b371e2d2f83de4e37e0bb1afcc](https://github.com/CmarNYC-Tools/TS4SimRipper/tree/862a32949e5156b371e2d2f83de4e37e0bb1afcc).
Copyright notices and GPL obligations apply to derived code. The crate license is
GPL-3.0-or-later. Reference-tool behavior is separately labeled from native game
qualification. No private decompiled product code was reused.

Pinned source files (SHA256):

```text
BGEO/DMAP links/application: PreviewControl.cs
a8b8044ad5d1f768f57019e7073f5a09e479683fc06d7ad91c2b313d14f9ca3d
Geometry math: Geometry.cs
231b6a332f5929eae91e0345392a88d0ca76dca74ddcca87d15c672beeb7c920
GEOM native IDs, tags, seams and skeletal methods: GEOM.cs
9e332175017405284ca0744025ae8cc3269f7e6ccd9caa28306711e451d99693
Rig and descendant adjustment: RIG.cs
3d9ea554c5428c28c1eecba95f608ec0dcb17ed3379c9d3f6a53998d1cc7e6fd
BOND resource layout: BOND.cs
d86adf8236dffd5447eb1bd58d4ce48469cb5914ea3c0b64c3b49e1937c84640
TGI ITG layout: TGI.cs
56c8992118e229701ca6151d3c3f15b19b2940a72d394480a7112987287f243d
Enums and frame-trait IDs: Enums.cs
fa48d0ec2a7418ee30d8167682ca212d879494d94f8e9a906701fa7ffb0b006b
DisplaySim native physique/sculpt/modifier ordering: Form1.cs
16b013f21f2b398d2c0e5994d46c83503b08505e334fdc8095021dde596d4234
BGEO.cs
480e8c004a758c57d048db3de9cf31bbcfa768c12983d87992ebd24bb0c34939
DMAP.cs
3b5b9ac63c8974b3e5abcc3496c4b268ec5d4ecd2f02e74c594eb0fb033770d3
Sculpt.cs
a6ddc8adc110a522d06cfd50416f9949c739a70a3e3fa2ca7e3bbe97d29682f3
SMOD.cs
2bf8db958b15de433febc76f94c3046ed7c88faa51f04b0ffa7d29433bd1dd3d
```

Additional reference layouts are `BGEO.cs`, `DMAP.cs`, `Sculpt.cs`, `SMOD.cs`.
The preexisting recovered reference oracle projects independently execute the
original BGEO/DMAP readers and application methods. The new `oracle` adapter
links original RIG/BOND/Geometry/TGI/Enums files and extracts `LoadBONDMorph`,
`GetTotalBoneWeight`, `SetupDeltas`, `UpdatePositions`, `BoneMorpher` verbatim.
Its geometry storage shim receives already-parsed Rust GEOM arrays; the
qualification therefore establishes deformation arithmetic and layout readers
at their stated boundaries, not an independent C# GEOM parser comparison.
The assets crate independently qualifies its GEOM fields.

Sims persistence paths come from the tray crate's pinned authoritative schema:
SimData physique12, facial18; attributes30 → trait_tracker10 → trait_ids1.
The trait IDs for human frame changes are 136878 feminine /136877 masculine.
Base and occult form native bytes remain retained by the tray crate.

Game and CC resources are read only from the user's authorized installation and
explicit private fixtures. They are neither bundled nor relicensed by this
crate. Authoring-tool body fixtures are used only as labeled math tests; no
fallback authoring body or skin is selected for a native Sim.
