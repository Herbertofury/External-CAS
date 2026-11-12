# Native imported-Sim scene planning

`resolve_scene(&SimAppearance, &Catalog, &SceneOptions)` connects retained
native appearance to actual resource keys and provider observations. It
resolves the stored outfit/category index, the species/age/occult rig key,
TONE data and skin-set references, plus native sculpt/SMOD references. It does
not replace missing body, rig or skin content with generic assets.

The plan contains typed missing, ambiguous, unsupported and invalid-source
failures. `complete_sim` remains false because resource planning alone does
not apply the full morph, material, region and skin-detail pipelines.

The current form must resolve uniquely or be selected explicitly. Native
outfit IDs preserve every discovered full TGI/group and provider. Index order
does not imply load precedence. Explicit `provider_choices` may select a
source; same-full-TGI providers may also resolve as `ByteIdenticalProviders`
only after all their expanded bytes compare equal. All provider hashes and
provenance remain available. A representative selected provider in that state
does not claim game load precedence. Different full keys or payloads remain
ambiguous. `rig_payload` contains bytes only after these checks pass.

Payload reads reuse one `PackageReader` per package, including one initial
index/source revalidation and per-read change checks. Source-open failures are
retained during that planning pass rather than repeatedly retried.

TONE v6–12 common fields are bounded and decoded; the entire original payload
and any uninterpreted suffix remain retained. The native value shift is not
reset. Skin-set selection follows the observed native suntan state at
SimData.attributes(30) → suntan_tracker(27) → tan_level(1), or an explicit
override. The reference's untanned/base-state rule is recorded explicitly.
Texture-family alternatives are candidate observations, with no assumed
native texture priority. New v12 suffix parameters remain unimplemented.

## Evidence recorded 12 November 2026

Fifteen tests cover original-source FNV vectors, rig stage/species rules,
TONE versions/truncation/non-finite values/opaque suffixes, explicit current
selection, full-key/source ambiguity, byte-identical provider equivalence,
native suntan state, changed sources, invalid providers, explicit config
priority, equal-priority/CC refusal, config/version/package-census changes,
conditional overlap and highest-priority tombstones. The opt-in TONE
oracle was run using the pinned original C# TONE reader and matched the Rust
reader on the fixture's version, texture/overlay IDs, hue, saturation, opacity,
overlay count and makeup opacity.

An actual imported Sim was resolved read-only against three explicitly named
installed packages: 403,924 indexed resources, zero catalog issues, unchanged
index identities and unchanged original Tray hashes. Its current outfit has
15 slots; seven were unique in scope and eight had differing providers. The
native rig key had three providers in two payload classes (two equal, one
different). Installed numeric config priority selected the ClientDelta
provider above the two Full providers. The actual game rig decoded with 165
bones, and all 15 current-outfit slots resolved within the indexed scope.
The native TONE uniquely decoded
as v12 with three skin sets and native set zero; that set's texture was absent
from this limited scope. No complete-Sim rendering or game acceptance is
claimed. No game or Tray files were written.

The complete private scope, keys, IDs, hashes and results remain excluded from
Git under the workspace `.local/real-tray-check`. Full game/CC scope, applicable
provider precedence, required texture content and morph/material integration
still need qualification.

## Installed configuration policy

`GameInstallScope::discover(game_root)` reads base and installed pack/Delta
resource configs and the installed `Default.ini` game version. It records
exact config SHA256, line, pattern and explicit numeric priority for each
matched package. Discovery currently observed 468 configs and 5,112 configured
packages on game version 1.129.39.1030. Pack activation/entitlements are not
verified by finding a directory.

`scope.prepare(&catalog)` revalidates the config/version/package census and
binds a policy to that borrowed catalog. `resolve_key` chooses only a unique
highest observed numeric priority. Equal priorities, unknown/custom-content
providers and highest-priority tombstones remain unresolved; no alphabetical
or pack-directory precedence is invented. Unsupported directives disable
their config's automatic priorities. Unknown conditional PackedFile branches
block overlapping package patterns instead of being treated as false.

For a selected Sim, use `policy.provider_choices_for_keys(plan.required_keys())`
and preserve the returned decisions/coverage alongside `choices`. This avoids
materializing decisions for every catalog duplicate. A three-package catalog
is still incomplete against 5,112 discovered packages even if every selected
reference resolves. Unindexed packages, unknown catalog paths and unqualified
directives remain explicit; this policy does not certify game-wide scope.

The optional `CAS_SCENE_GAME_ROOT` on `resolve_import` performs this observed
policy pass, reports its evidence and revalidates it after resource loading.
It does not add user Mods or infer custom alphabetical conflict semantics.

```text
cargo test -p external-cas-scene --offline --jobs 2
cargo run -p external-cas-scene --offline --example resolve_import -- PRIVATE_PROJECT EXPLICIT_PACKAGE ...
```

Set `CAS_SCENE_TONE_SOURCE` to the trusted original `TONE.cs` to execute the
optional oracle with `cargo test -p external-cas-scene source_tone_reader_oracle -- --ignored`;
its script refuses a changed source hash. The example emits
private source and Sim identities on stdout; keep its receipt local.
