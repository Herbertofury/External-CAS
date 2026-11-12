# Native External CAS link core

Rust owns mode selection, target identity, revision checks, opaque draft data,
undo/redo, conflict reconciliation, Apply lifecycle and durable checkpoints.
The Python code under `bridge/` is a narrow game compatibility boundary.
Importing either component contacts nothing and starts no server.

This is component implementation, not qualified Sims synchronization. No live
or in-game CAS write contract is qualified, installed or enabled. Actual game
selection/readback, full unknown/form preservation, live visual refresh and
save/reload acceptance remain outstanding in a disposable game session. The
active Apex session and all game/profile/save files remain untouched.

## Controller integration

`LinkController::new(LinkLimits)` starts in `Standalone` and `Disconnected`.
`request_connect` must precede `connected(RuntimeSession, Capabilities)`.
Game launch, selection events and reconnect never change mode or edit target.
`observe_selection` returns an observation/switch proposal only. The UI displays
the exact save lineage, world, household, Sim, form, game/adapter build,
connection, confirmed revision and pending/conflict status from the controller.

`request_switch(SwitchTarget, SwitchDecision)` implements explicit
save/stash/resolve/cancel hooks. Dirty contexts stay retained on every path.
A save acknowledgment must match the draft version and actual caller-supplied
checkpoint hash. Resolve returns a request for reconciliation, not permission
to discard. Standalone has a separate draft from all linked Sims. Names never
identify Sims. Missing/unsupported forms remain explicit, not another form.

`issue_snapshot_request` produces an exact identity/session/navigation/request
ticket. `accept_snapshot` rejects obsolete responses, departed targets and
wrong identities. Current native observations use `Coverage::Partial`; they
cannot become a complete replacement Sim. Opaque field blobs retain the exact
bytes the adapter supplies. Whole outfit protobufs include packed 64-bit
color-shift arrays and unknown fields; no arrays are reconstructed from mesh
or swatch previews. Snapshot hashes bind every supplied opaque field and
coverage state, not only the currently visible outfit.

Each `ContextId` has independent `edit`, `undo` and `redo`. Shared immutable
byte buffers avoid copying unchanged large outfit blobs for each edit.
`reconcile_sim` performs a three-way merge and requires a decision for each
concurrently changed field. Unknown blobs participate byte for byte. Household
and memory/history budgets fail explicitly; no draft is evicted or truncated.
`retained_bytes` counts retained blob bytes, not process or allocator overhead.

## Apply and recovery

`prepare_apply` checks exact mode/target/build/session/base/draft revisions,
complete preservation coverage and per-field read/write/rollback capabilities.
An authenticated peer claiming qualification cannot enable writes: the app
must also register local `WriteQualification` evidence for that exact build,
adapter and field. Production currently has **zero** such profiles. Test
profiles are explicitly named fixtures and are never published as game support.

The lifecycle is `prepare_apply` → `review_apply` → `send_apply` →
`acknowledge_apply` → `readback_apply`. Persist the sent transaction checkpoint
before transport I/O. An acknowledgment does not commit. Only exact full
readback commits; edits made externally while Apply is running remain intact.
Operation IDs prevent duplicate Apply. Timeouts, disconnect and reopening an
unresolved operation enter an indeterminate state: query status and read back,
never repeat Apply automatically. `operation_readback_request` binds recovery
to the original operation and session.

Rollback requires the exact observed post-write revision and the original
qualified target/capabilities. It reverses only explicitly changed fields and
must reproduce the complete preimage. Changes to unrelated/unknown fields
require recovery instead of silently replacing them. No path enters game CAS
as a fallback for unsupported live changes.

## Durable context store

`LinkStore::open(absolute_app_owned_path, StoreLimits)` opens a private SQLite
store. Use an app data location such as `ExternalCAS/Link/contexts.sqlite` or
an explicit disposable test root. Foreign databases and unknown schemas are
refused. `load(LinkLimits)` returns `LoadedCheckpoint { controller, receipt }`;
the controller remains disconnected and must revalidate before writing.

`checkpoint(&controller, expected_generation)` atomically stores standalone
state, each complete Sim context, and every operation journal record in one
SQLite transaction. `CheckpointReceipt` holds absolute path, generation and
SHA256. The app's small session journal stores that receipt, not a large
household snapshot. An existing store must be loaded/qualified before updates.
Generation comparison refuses competing app writes. WAL/FULL durability,
content hashes, bounded serialization and complete publication retain the last
committed state after failed writes. Unknown serialized fields/versions fail
without downgrade writes. `load_cancellable` and `checkpoint_cancellable`
accept cancellation; checks surround SQLite calls, rows and serialization.
Cancellation cannot interrupt an individual SQLite/JSON decoding call.
Limits are explicit errors; original records remain in place.

## Local protocol and compatibility boundary

`protocol::AuthenticatedCodec` uses a random 256-bit pairing secret, fresh
session nonce, direction and monotonic sequence, HMAC-SHA256, typed JSON and a
caller-configured frame bound. Encoded output is bounded while serializing.
Secrets have redacted Debug and are not serialized. Pair only through an
explicit user-controlled local action; never put the secret in a URL or log.
The codec authenticates bytes, not a peer's claim of game compatibility.
Transport failures require closing/reconciling the channel, not guessing a
new sequence or resending an Apply. Socket helpers bind/check loopback only;
all socket/codec work belongs on workers. No console/eval RPC exists.

The Python adapter is game-thread-affine and uses actual installed API names.
Its bounded dispatcher must be called explicitly by a game-thread scheduler;
there are no automatic hooks, background SimInfo access or profile writes.
It reports partial observations and refuses all writes/rollback. Identity is
read from native persistence GUID/slot, zone world, household and Sim; if an
identity field is unavailable, linking fails rather than fabricating one.
`save_sim` was found to traverse interactions/inventory/location, so it is not
called as a supposedly harmless full-snapshot getter. Full snapshot semantics
still require isolated game research.

## Verification and provenance

`cargo test -p external-cas-link --tests` exercises mode/draft preservation,
128 same-name fixture Sims, stale asynchronous results, concurrent changes,
undo, readback/echo, timeout/rollback and real disposable SQLite failures,
concurrent generations and an abrupt writer exit before commit. These are
model/storage tests, not a game or performance acceptance claim. The abrupt
exit test invokes its intentionally ignored child itself.

Set `CAS_LINK_PYTHON` to an explicit interpreter and run
`cargo test -p external-cas-link --test python_protocol -- --ignored` for native
and Python HMAC/opaque-revision conformance. This passed with Python 3.13 and
the private matching Python 3.7.9 runtime. Four Python fixture tests also pass
under 3.7.9, without game imports.

`evidence/installed-api-signatures.json` records read-only signature/hash
inventory for installed Windows game **1.129.39.1030**. The audit reads bytecode
with its matching Python 3.7 interpreter and never imports or executes it.
Only names/signatures/hashes are retained; no EA binary or source is copied.
The audit proves API existence, not runtime behavior or support.

Maintained primary references include [Lot 51 Core's event documentation](https://github.com/lot51/core-library/blob/main/README.md)
for game-thread scheduling and lifecycle events, and
[Sims4CommunityLibrary outfit utilities](https://github.com/DeviantGameMods/Sims4CommunityLibrary/blob/main/Scripts/sims4communitylib/utils/cas/common_outfit_utils.py)
for explicit outfit refresh calls. Their APIs inform research; these components
are not bundled and their documented behavior is not a native preservation
guarantee. No proprietary API/source or private Apex implementation is copied.

This crate and the original bridge source are GPL-3.0-or-later. Rust dependency
licenses include serde/serde_json, SHA2, HMAC, getrandom and rusqlite; their
transitive notices belong in the packaged app. No game binaries, saves, creator
assets, decoded thumbnails or pairing secrets are distributed here.
