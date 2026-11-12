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
