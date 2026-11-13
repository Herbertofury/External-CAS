# External CAS acceptance and evidence

The full objective is the owner's November 12, 2026 request in the current Codex
chat and its attached goal-objective.md. This is a working source checkout,
not an accepted release. Checked boxes require direct current evidence.

Latest qualified component checkpoint (13 November UTC): 325 workspace tests
passed; 17 explicit private-source checks were excluded from ordinary CI. The
actual Windows app applied the source camera equations, rendered the imported
Sim with real CC, committed two native edits to a separate recovery revision,
closed and reopened it. Camera, source pins and the retained Sim image crop
matched exactly; originals remained unchanged. Full source material/UI/game
acceptance is still false. These observations do not check the full gates below.

Current source/evidence links:

- [Original UI/editor bridge](../native/source_ui/README.md) and
  [actual application receipt](../native/source_ui/evidence/desktop-editor-integration.json).
- [Camera/frame contracts](engine/CAS_CAMERA_LIGHT_ROTATION_20261112.md),
  [source math receipt](engine/evidence/cas-camera-source-math.json) and
  [app source](../native/app/src/viewport.rs).
- [Shared room/native depth](../native/render/evidence/shared-depth-qualification.json)
  and [native clothing export](../native/material/evidence/native-bc3-export-qualification.json).

- [x] Isolate work from active Apex checkout and game session: branch
  `external-cas/standalone-native-20261112`, base `1ba0986` in a new checkout.
- [x] Preserve ignored/unpublished source, dirty changes and private input references.
  Evidence: `EXTERNAL_CAS_RECOVERY_20261112.md` and private recovery hash manifest.
- [x] Establish the owner-selected public repository and wiki:
  https://github.com/Herbertofury/External-CAS and its wiki. Product source must
  exclude unrelated Apex history and all proprietary/private inputs.
- [ ] Complete source and dependency license notices and distribution audit.
- [ ] Native Rust UI and GPU viewport; Rust-owned resource loading, scene,
  rendering, editing and animation. No Node/browser runtime dependency.
- [ ] Asset discovery and persistent indexed browse/search/filter/preview at
  installed-library scale, with cancellation, duplicate providers, missing and
  unsupported resources made visible.
- [ ] Real native Sim import/creation; outfits, slots, skin tones, colors,
  body/face genetics and unsupported fields preserved.
- [ ] Accurate body, hair, accessories, skin/materials, lighting, camera and
  room across multiple real Sims/body types/outfits.
- [ ] Reuse the actual installed CAS UI resources, icons, fonts and room locally;
  validate complete control/behavior parity. Document necessary native engine
  ports and measured benefits without removing game features or fidelity.
- [ ] Recovered real CLIP posing and sequence playback restored in Rust with
  rig binding, play/pause/seek/reset, deformation and clothing correctness.
  Explicitly record event/IK support and unsupported state preservation.
- [ ] Editing undo/redo, durable projects, save/close/reopen identical result.
- [ ] Genuine Tray authoring/editing with validation, explicit backups, stale
  source checks, failed-write/crash recovery and byte/visual comparisons.
- [ ] Independent full external workflow: clean launch → discovery → real Sim
  → real CC → edit → pose → save → close → reopen and readback.
- [ ] Separate qualified CAS and Live synchronization with conflict review,
  stale/concurrent change refusal, readback and reload in disposable profiles.
  Active Apex session remains reserved to its owning chat.
- [ ] Live workflow: click/select the exact Sim in Live, edit it in the external
  full CAS, review and explicitly Apply back to Live without entering in-game CAS.
  Validate identity, revisions, idempotence, readback, persistence and rollback,
  preserving outfits, CC, occult/hybrid and unsupported state.
- [ ] Actual Windows populated-app measurements: startup, input-to-visible,
  scrolling, loading, frame times, memory and long sessions; record hardware,
  source scope, cache state and reproducible measurement procedure.
- [ ] Missing/broken CC, unsupported fields, cancellation, failed writes,
  crashes, concurrent changes and recovery exercised on disposable copies.
- [ ] Reproducible Windows build and installer/portable package; exact package
  clean-environment launch, upgrade/recovery, checksums, corresponding source,
  license notices, setup instructions and candid remaining limitations.
- [ ] Push/merge/release only after the complete acceptance audit succeeds.

Do not infer end-to-end acceptance from unit tests, a rendered garment, a
synthetic frame, an opened file or an unpackaged debug launch.

Public milestone and implementation gates:
https://github.com/Herbertofury/External-CAS/milestone/1 and issues #1–#4.
