# Alternatives and fidelity benchmarks

## Product boundary
External CAS is a standalone editor. The primary fidelity benchmark is the user's installed Sims 4 Create-a-Sim, with supported installed CAS mods. An adjacent utility is not evidence that complete CAS behavior or safe live synchronization has been solved.

## Reference set
| Reference | Role | Evidence and next test |
|---|---|---|
| Installed The Sims 4 CAS | Primary behavior, appearance and interaction baseline, not redistributed content. | Compare authorized local scenes and controls at a pinned game build. Game updates require renewed verification. |
| [Sims 4 Tray Importer](https://luniversims.fr/files/file/73-sims-4-tray-importer/) | Adjacent Tray/CC workflow reference. Published changelog documents embedded-image, trait, CC and image-cache work. | Compare household identity, dependency explanations and missing-content handling. Do not equate embedded images with rendered Sim fidelity. |
| [Sims 4 Studio](https://sims4studio.com/) | Candidate specialist reference for creator workflows. | Official homepage timed out in this review; current capabilities/version were not verified. Inspect official documentation before asserting parity or selecting implementation code. |

## Where improvement must be demonstrated
- Preserve familiar installed-game room, assets, controls and behavior without bundling proprietary game resources or creator packages.
- Independently qualify complete Sim composition, materials, morphs, animation, outfit categories and supported modded rooms/UI. Unsupported resources must be visible, never silently substituted as if faithful.
- Make Standalone, in-game CAS sync and Live Link explicit modes. Game launch or selection changes must never hijack an unrelated draft.
- Provide large-household navigation with stable Sim identity, per-Sim drafts and clear pending/applied state.
- Verify bidirectional changes, version conflicts, disconnect/reconnect, undo and recovery. Apply must verify its destination and outcome, not merely send a message.
- Measure real large-CC-library cold loading, interaction p95, memory and correctness against the current application. Rust alone is not a performance result.

## Delivery gate
Use existing [acceptance requirements](docs/ACCEPTANCE.md), not a parallel definition of done. Retain local/private fidelity evidence securely; public comparison notes contain methodology and permissible summaries only. Never publish saves, private households, extracted UI atlases or installed assets. This document adds research scope, not a claim that live sync, complete CAS or a qualified release is finished.

## Evidence rules and recurring review
Last source review: **2026-11-12**. This is a living comparison, not a claim that our app beats these tools. Source-described behavior is not hands-on verification. No comparative runtime measurements were performed in this pass.

For each daily review, inspect official release notes, documentation, repositories and relevant issue resolutions. Record publication date separately from review date, version/commit, affected workflow, source link, and what actually changed. An old release page remains historical evidence, not today's release. If unchanged, retain the previous evidence and say no material change. If blocked, mark unavailable; never infer feature absence from a failed fetch.

Maintain each entry as: **reference | workflow | source/version/date | observed strength | reported grievance and resolution | our measured baseline | hypothesis | acceptance evidence | decision/issue**. A complaint is a report, not proof of a defect or its prevalence. Check whether it was fixed before proposing work. Preserve failures and last-good evidence.

Before adopting an idea, compare representative real inputs on the same machine and record cold/warm p50/p95 latency, memory, CPU, correctness and accessibility. Preserve feature parity, crash/restart recovery and durable data. A faster microbenchmark does not establish a faster product. Reuse compatible maintained upstream code only after license/dependency review. New feature proposals require owner review; this research does not silently authorize implementation.

Daily upkeep is a requested cadence, not a claim that a scheduler or uninterrupted worker is active. Add dated results only after an actual review.

## Foundations to build on
| Foundation | Specific reuse or study | License evidence and boundary | Improvement acceptance |
|---|---|---|---|
| Existing native resource, scene, morph, Tray and session components | Preserve recovered implementations and provenance; reuse actual established parsing/math and revision-safe editing before writing parallel versions. | [Native source notices](native/THIRD_PARTY_NOTICES.md) identify GPL-3.0-or-later derived CAS Tools/SimRipper and other format work; MIT session component remains separately licensed. Check pinned provenance and release dependency bundle. | Differential resource/Sim comparisons, round-trip edits, missing-dependency behavior and verified rollback. |
| [egui](https://github.com/emilk/egui/blob/main/Cargo.toml) and [wgpu](https://github.com/gfx-rs/wgpu/blob/trunk/Cargo.toml) | Build on established Rust UI/GPU infrastructure already named in the app README; investigate measured bottlenecks rather than replacing the stack on fashion. | Reviewed upstream workspace manifests declare MIT OR Apache-2.0. Current branch version text is not evidence that a release is stable or that the project should upgrade. | Real large-library interaction, material/morph fidelity, accessibility and device-loss tests against the current pinned app. |
| Authorized installed Sims 4 resources | Read local room, UI, meshes/textures and supported creator overrides using validated resource priority. | Installed resources are not licensed by this repository's source license. Do not bundle proprietary assets or claim all scripts/engine behavior can simply be reused. | Demonstrate actual supported behavior for pinned game/mod versions; unsupported dynamic UI or scripts must remain explicit. |

## User-project interoperability opportunities
- **Hover:** offer a minimal read-only resource/preview contract and common dependency diagnostics after license review, rather than maintaining incompatible interpretations of the same package. Do not export private households or proprietary assets to public services.
- **TD1/Apex:** investigate shared capability negotiation and Sim identity/revision semantics while preserving separate application ownership. External CAS must not hijack an Apex session or assume a live bridge exists.
- **GameSync/mod-management tools:** accept explicitly selected content provenance and installation status, not their authentication tokens. External CAS owns its local drafts and mode transitions; a manager update must invalidate stale resource caches safely.
- **Engineering Atlas:** retain isolated experiments and fidelity/performance results; experimental format/rendering candidates do not become release support without real app qualification.

## Reuse qualification and interoperability contract
**Status: design/reuse candidates, not integrated or benchmark-qualified by this review.** Before adopting code, pin the exact revision; inspect file-level license headers, dependency licenses, notices, assets and redistribution obligations. A repository badge is not a full license audit. Preserve attribution and corresponding-source obligations where applicable. A language port does not erase upstream obligations.

Cross-project integration should use a versioned, documented contract with stable resource IDs, source hashes, schema version, completeness/access state, cancellation and explicit errors. Read-only preview and metadata exchange is the default. Never copy credentials, grant filesystem-wide access or let a preview trigger installation, game mutation or background downloads. Mutations require the owning application's authorization, revision check, journal and rollback. Negotiate capabilities rather than assuming every consumer supports every format. Test disconnected peers, stale revisions, malformed messages and incompatible schema upgrades. These contracts are proposed; no working cross-app link is claimed here.

No private repository contents, personal catalogs, source assets or credentials belong in public examples. Use redistributable fixtures and synthetic IDs; retain real-input evidence privately.
