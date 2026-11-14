---
name: native-cas-acceptance
description: Carry External CAS native editing, source-fidelity, Tray persistence and explicit game synchronization through identity-checked tests while preserving private assets and independent operation.
---

# Native CAS acceptance

Use for native CAS functionality, installed-source UI/rendering, outfit/Tray edits, bridge changes and acceptance decisions. Read `docs/ACCEPTANCE.md`, `docs/VALIDATION.md`, `docs/RECOVERY.md`, `docs/PROFILING.md` and the relevant `native/` crate before changing code. Check current branches/handoffs; preserve unfinished work and reserved game sessions.

## Identity and source boundaries

Record source commit, Cargo.lock digest, build/toolchain identity, installed game version, selected source scope and disposable profile identity. Consult the current Windows workflow and `scripts/build-native.ps1` instead of inventing build steps. Pin the actual interpreter dependency; do not casually switch the Ruffle fork or bundle its upstream test corpus.

Read proprietary game fonts, graphics, textures and creator content only from authorized local installations. Keep private fixture inputs and hashes/receipts exposing personal paths outside public commits. Public fixtures must be independently redistributable; a readable file is not redistribution permission.

## Independent application acceptance

Run the workspace's locked tests/build on the supported Windows toolchain, then launch the exact resulting binary. Record build success separately from launch and real workflow acceptance.

Exercise clean launch → discovery/indexing → real Sim import → actual CC/outfit → edit → original CLIP pose/sequence controls → save copy → close → reopen and independent readback. Verify missing/broken resources, canceled background work, duplicate providers and unsupported fields are explicit and preserved. Compare actual original materials, camera/room, rig/skin/morph behavior rather than substituting a garment preview or synthetic frame for a complete Sim.

Ensure the core application remains usable without Apex, hybrids, optional bridge connections or a running game. Optional synchronization should add capability, not become a hidden dependency of browsing/editing/persistence.

## Writes and synchronization

- Preserve original Tray files and private inputs. Edit disposable copies with revision checks, exact selected Sim/outfit identity and explicit backup/rollback.
- Test undo/redo, canceled writes, stale concurrent revision, failed-write/crash recovery, save/close/reopen and repeated apply. Verify unsupported outfits/occult/custom data survives, not just the edited field.
- Game synchronization is a separate qualified CAS or Live operation. Confirm ownership of the test session and current identity before applying. Do not restore a previous session's state into a different profile.
- For Live editing, independently verify the exact selected Sim, review the intended changes, apply without silently entering in-game CAS, read back, reload and confirm persistence/rollback. Historical Apex-dependent evidence is not standalone External CAS proof.

## Performance and delivery gates

Use `scripts/profile-native.ps1` and `docs/PROFILING.md` as the starting measurement contract. Measure the populated native app at actual library scale: hardware/source scope, cache state, startup, input-to-visible latency, scroll/frame times, memory and long sessions. Preserve data density, feature completeness and fidelity; a fixture benchmark does not prove app improvement.

Retain failures and last-good outputs. Complete license/distribution, clean-environment package launch and recovery gates before calling an installer/release accepted. Report unit/fixture tests, native app, real game, source publication and packaged release independently. Never check off an acceptance item solely because its code exists or a historical report passed.
