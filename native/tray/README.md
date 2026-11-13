# Native Tray inspection and project snapshots

`external-cas-tray` reads native household, metadata and portrait files in Rust.
It retains their complete original bytes, including unsupported fields and
opaque wrappers. Household metadata is projected from the documented
FamilyData / AccountFamilyData / SimData schema; metadata Sim IDs and native
Sim IDs are distinct. See [PROVENANCE.md](PROVENANCE.md).

Public entry points:

- `OutfitAppearance::native_layer_ids`: lossless read-only projection of the
  schema-proven parallel uint32 layer IDs. Missing wrappers remain absent;
  malformed, duplicate, overflowing or misaligned data fails explicitly.

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
- `ProjectDraft::color_shifts` / `apply_color_shift`: exact native uint64
  per-part color-shift reads and reversible in-memory edits, with old-value,
  revision and full native slot/form identity checks. Packed/unpacked varints
  and necessary enclosing lengths are handled surgically. Other values,
  unknown fields, encodings, metadata and portraits stay intact.
- `MissingColorShiftPolicy::MaterializeNeutralOthers` is explicit: absent or
  empty arrays can be created for the known existing part count with the
  schema/reference neutral `0x4000000000000000` for unselected parts. An
  explicit zero stays zero. Partial nonempty arrays remain unsupported.
- `save_as_with_progress`: supports cancellable edited-project writes and
  leaves an incomplete recovery marker on cancellation, collision or a
  concurrent source conflict. The original and parent project remain intact.

`OutfitSlot.occult_form_index` distinguishes base and stored occult outfits;
`None` preserves the earlier base-slot identity. The raw 64-bit write boundary
does not clamp or quantize shader lanes. Slider conversion belongs to the
material/UI pipeline. A changed varint can resize native messages; v1/v2 wrapper
lengths update while known embedded image bytes remain exact. Variable-length
edits refuse relocation of an unsupported opaque suffix.

Interrupted writes retain `SNAPSHOT_INCOMPLETE`; reopen refuses to treat such
a snapshot as committed. These are local project snapshots, not newly created
or game-qualified edited native Tray exports. Portrait files remain opaque; no encrypted
portrait or alpha-mask decode is claimed. Native editing and game acceptance
remain unqualified.

## Validation recorded 12 November 2026

`cargo test -p external-cas-tray --offline --jobs 2` passed 26 tests covering
exact preservation, unknown protobuf fields/groups, opaque suffix retention,
embedded-image bytes, required-payload truncation, integer overflow, changed
wire types, duplicate owners, exact family association, cancellation,
snapshot/reopen conflicts, refusal to overwrite, stale scans, traversal and
incomplete recovery markers, actual write collision failure, cancellation
during copying, packed/unpacked outfit slot edits, byte-exact undo/redo,
malformed array refusal, schema/revision/old-ID/type conflicts, and edited
save-as/reopen with distinct original/current hashes.

Color-specific checks additionally cover 1–10 byte varints, 127/16383 enclosing
length boundaries, packed/unpacked/mixed lists, noncanonical tag/length
preservation, exact whole-file expectations, base/occult identity, v1/v2 empty
array materialization, mixed CASP/color history, real write collisions,
cancellation, late source changes and incomplete recovery.

A real disposable household color check read all 114 native part shifts with
the independent original generated schemas and protobuf-net 3.2.30. Four
independent schema readbacks agreed. A zero-shift test shrank the household
from 6,868 to 6,859 bytes, exercising nested length changes; undo restored every
byte. A subsequent hue-lane edit changed exactly one native byte, retained the
other 48 bits, saved/reopened identically and could be reversed after reopen
to the original household bytes. All five original files and non-household
copies remained unchanged. This qualifies the project writer boundary, not
native Tray import or in-game appearance acceptance.

The opt-in `qualify_color` example requires explicit existing/new project,
private report and independent schema-oracle paths. Its oracle links 55
separately supplied original schema files; verify their aggregate pin with
`oracle/color/verify.ps1`. Generated fixtures, source-dependent oracle binaries
and private receipts stay ignored. Sanitized results are in
[evidence/color-qualification.json](evidence/color-qualification.json).

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
