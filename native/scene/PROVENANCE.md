# Source provenance

Rust implementation and adaptations are GPL-3.0-or-later. Rig-name/key rules,
TONE layout, and reference lookup behavior were researched from
CmarNYC-Tools/TS4SimRipper revision
`862a32949e5156b371e2d2f83de4e37e0bb1afcc`, copyright C. Marinetti and later
contributors. Preserve the project GPL license and copyright notices.

Source SHA256 pins checked locally:

- `src/Form1.cs`:
  `16b013f21f2b398d2c0e5994d46c83503b08505e334fdc8095021dde596d4234`.
  `DisplaySim`, `GetTS4Rig`: actual native outfit state, shape references,
  TONE/suntan handling and rig lookup. The source's external authoring-file
  fallback is deliberately omitted.
- `src/PreviewControl.cs`:
  `a8b8044ad5d1f768f57019e7073f5a09e479683fc06d7ad91c2b313d14f9ca3d`.
  `GetRigPrefix`: species and age prefixes, including infant/toddler/child
  human and small-dog child behavior.
- `src/Enums.cs`:
  `fa48d0ec2a7418ee30d8167682ca212d879494d94f8e9a906701fa7ffb0b006b`.
  Native species, age/gender, occult and resource type values.
- `src/TONE.cs`:
  `3abce3053d23a246c5b11f1fc8ec1337720028c5f6af50da41bf5ecab42d70d0`.
  The unmodified reader is compiled by the optional oracle with minimal type
  adapters for its surrounding enum/tag types. Original TONE reader code is
  not copied into the repository. Tests compare its public results.

Native protobuf field evidence is inherited from `external-cas-tray`.
Additional suntan paths are declared in `src/Proto/SimObjectAttributes.cs`,
SHA256 `fb6af41d83a4845aa5ea3b473841ed583e8028f2bf049b5b540bda2c7941decf`:
PersistableSimInfoAttributes.suntan_tracker is message field 27;
PersistableSuntanTracker.tan_level is uint32 field 1. The same source declares
trait_tracker field 10; its trait_ids can affect body-frame rendering and
must be handled before applying physique names based only on gender.

FNV-1 vectors were obtained by directly compiling and executing the original
`FNV.cs` method with PowerShell Add-Type. Resource names are lower-case ASCII;
the operation multiplies before xor. This is not FNV-1a.

No proprietary reference application source, game binaries, creator assets or
private Sim data is included. Primary-source behavior supplies a compatibility
rule to test; it does not prove the running game's current precedence or
visual equivalence. All scope limitations remain explicit in ScenePlan.

## Numeric installed-game priorities

Observed installed files directly declare priorities: base
`Data/Client/Resource.cfg` assigns ClientDeltaBuild files -20 and
ClientFullBuild files -30; `Data/Simulation/Resource.cfg` similarly assigns
SimulationDeltaBuild -20 and SimulationFullBuild -30. Their observed SHA256
values were respectively
`66e8d6902ae64f9ac4393ea112c95f77098ce835e3b216a633742c02a6f8f6b6`
and `d0ac15042c2d4131171254f41f6b496b8033c0884ea26650998a7e27ac3cb463`.
Actual observations, rather than these example hashes, are pinned and checked
by GameInstallScope; a changed config invalidates its policy.

Maxis's original *Creating Custom Content and Mods for The Sims 4*, version
1.0, describes Resource.cfg numeric priority and same-resource-key overriding.
The original author document is preserved in the
[SimsEdit archive](https://github.com/SimsEdit/Sims-4-Modding-Docs) and is
[readable here](https://www.scribd.com/document/650420230/TS4-Custom-Content-Guide).
This is the original Maxis/EA document hosted by an archive, not a newly
published current engine specification. The numeric rule is applied to
current observed installed priorities; native qualification remains separate.

`GameFileHandler.cs` at the pinned SimRipper revision, SHA256
`496ae8228784397c5710aa7755b7d827c53f2661fd62dc9b93a3a43f8da97da5`,
sorts Client build package paths and keeps the first full-TGI provider. That
corroborates Delta-before-Full reference behavior for the observed base paths,
but its general alphabetical order is not adopted as game precedence.

No evidence here establishes current equal-priority ordering across pack
folders, custom Resource.cfg wildcard-depth/alphabetical conflicts, runtime
conditional configuration or active pack entitlement. Those cases remain
typed unresolved states. Higher-priority tombstones never cause an automatic
fallback to an older lower-priority resource. Unknown providers are never
silently excluded when generating choices.
