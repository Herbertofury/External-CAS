# Native Tray inspection and project snapshots

`external-cas-tray` reads native household, metadata and portrait files in Rust.
It retains their complete original bytes, including unsupported fields and
opaque wrappers. Household metadata is projected from the documented
FamilyData / AccountFamilyData / SimData schema; metadata Sim IDs and native
Sim IDs are distinct. See [PROVENANCE.md](PROVENANCE.md).

Public entry points:

- `scan_library` / `scan_library_with_cancel`: associate a household's files
  through its exact metadata ID and metadata Sim IDs; report broken and
  ambiguous sources rather than choosing a provider silently.
- `read_document`: original bytes, SHA256 and a typed read-only summary.
- `TrayDocument::native_sims`: exact native SimData slices for the editor.
- `create_project`: create a new external directory containing byte-identical
  native files and a manifest. Existing destinations are never overwritten.
- `reopen_project`: validate every copied file's size and SHA256 before use.
- `TrayProject::check_source_conflicts`: separately identify changed or missing
  originals without overwriting them or invalidating a safe independent copy.
- `ProjectDraft::from_project`, `slots`, `apply_slot`, `undo`, `redo`, and
  `save_as`: edit an existing outfit CASP fixed64 slot in memory with expected
  revision, old-ID, Sim SHA and body-type checks. Known part/body/color-shift
  arrays must align. Every other native byte remains untouched. Save-as creates
  an exclusive new project with original baseline pins separate from edited
  current-file pins; the earlier project is retained.
- `create_project_with_progress`: report actual file progress and cancel before
  commit; an interrupted copy retains an incomplete recovery marker.

Interrupted writes retain `SNAPSHOT_INCOMPLETE`; reopen refuses to treat such
a snapshot as committed. These are local project snapshots, not newly created
or game-qualified edited native Tray exports. Portrait files remain opaque; no encrypted
portrait or alpha-mask decode is claimed. Native editing and game acceptance
remain unqualified.

## Validation recorded 12 November 2026

`cargo test -p external-cas-tray --offline --jobs 2` passed sixteen tests covering
exact preservation, unknown protobuf fields/groups, opaque suffix retention,
embedded-image bytes, required-payload truncation, integer overflow, changed
wire types, duplicate owners, exact family association, cancellation,
snapshot/reopen conflicts, refusal to overwrite, stale scans, traversal and
incomplete recovery markers, actual write collision failure, cancellation
during copying, packed/unpacked outfit slot edits, byte-exact undo/redo,
malformed array refusal, schema/revision/old-ID/type conflicts, and edited
save-as/reopen with distinct original/current hashes.

A separate real read-only check inspected one existing native household:
five files totaling 91,714 bytes, one Sim and a version-2 household wrapper.
The family contained household metadata, a native household, two household
portraits and one Sim portrait. The five-byte suffix decoded as a zero-image
section. All five copied native files matched their original SHA256 hashes
after project reopen; originals still matched their pre-copy hashes. There
were no parser warnings. No original files were written and no game process
or API was contacted.

A structural edit check on that disposable project changed exactly eight
contiguous bytes in one existing outfit slot using a deliberately unqualified
test ID. Every byte outside the selected fixed64 slot remained unchanged,
including metadata, portraits, wrappers and unknown data. Undo restored the
original household exactly; redo, save-as and reopen preserved the edited
bytes exactly. Original source hashes were unchanged. This demonstrates draft
wire preservation; the test ID is not an eligibility-approved CAS asset, and
the edited household has not been imported into the game.

The private paths, IDs, names, hashes and full receipt are excluded from Git
under the workspace's `.local/real-tray-check`. This is one real sample and
does not establish arbitrary-version support, edited native saving, appearance
fidelity or game import compatibility.

The reproducible local check is available as an example:

```text
cargo run -p external-cas-tray --offline --example validate_local -- SOURCE_TRAY NEW_EXTERNAL_PROJECT
```

It writes only a new project directory and emits a private JSON receipt on
stdout. Keep that receipt local; it contains source identities and Sim data.
