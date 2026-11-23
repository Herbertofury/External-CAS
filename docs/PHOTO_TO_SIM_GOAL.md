# Photo-to-Sim: face and full-body reference goal
Owner-requested 2026-11-19. **Planned, not implemented or accepted.** This extends External CAS, not Sims4Foundry or Apex. Existing [acceptance gates](ACCEPTANCE.md) remain authoritative and unchecked. No user photos, model downloads, installations or runtime experiments were used in this research.

## Outcome
Let a user select a face photograph, a full-body photograph, or both and create an editable Sims 4 likeness in the external CAS room. Offer optional additional front/profile views, manual corrections and an honest confidence/unsupported-region display. Preserve the whole CAS workflow: outfits, CC, posing, undo, project recovery and separately qualified game/Tray export. A generated generic mesh is not a playable Sim.

The target is an impressive likeness within the game's representable shapes, not guaranteed exact reconstruction. A single image cannot reveal hidden surfaces, true scale or body shape beneath clothing. Perspective, expression, pose, hair and illumination confound inference. Generated side views are hypotheses, never additional measured evidence.

## Current primary-source research shortlist
Reviewed 2026-11-19; dates below are review dates, not invented release dates. This is a bounded shortlist, not all 2026 technology or a head-to-head benchmark.

| Candidate | Verified scope and proposed role | Boundary before adoption |
| --- | --- | --- |
| [MediaPipe Face Landmarker](https://developers.google.com/edge/mediapipe/solutions/vision/face_landmarker) | Face landmarks, expression blendshape scores and transformation output. Start with crop/pose/quality checks and editable landmark constraints. | Expression coefficients are not Sims genetic sliders. Check exact code/model licenses, Windows/native inference integration and measured robustness. Do not add a browser runtime to the Rust app. |
| [MediaPipe Pose Landmarker](https://developers.google.com/edge/mediapipe/solutions/vision/pose_landmarker) | Single-image body landmark estimation, including image/world coordinates and optional segmentation. Useful for pose normalization and silhouette constraints. | Predicted world coordinates are not verified personal measurements; no true height or body dimensions inferred without calibration. Clothing silhouettes are not unclothed anatomy. |
| [SAM 3D Body](https://github.com/facebookresearch/sam-3d-body) | First-party inference repository, trained checkpoints and body reconstruction examples. Evaluate as a body-pose/shape proposal source. | Audit exact checkpoint/model/dependency licenses, hardware and runtime before adoption. Its representation is not Sims topology, skeleton or morph parameters. Not tested here. |
| [SMPLify-X / SMPL-X](https://github.com/vchoutas/smplify-x) | Established single-image expressive body fitting baseline and useful optimization concepts. | [Model/software terms](https://github.com/vchoutas/smplify-x/blob/master/LICENSE) are restricted; do not assume commercial redistribution or GPL compatibility. Keep as research comparator until legal/model provenance gates pass. |
| [Reallusion Headshot 3](https://www.reallusion.com/character-creator/headshot/photo-to-3d-head.html) | Commercial photo-to-head workflow with refinement and optional generated reference views. Strong UX benchmark for photo preparation and manual correction. | Vendor claims are not independent accuracy measurements. Proprietary Character Creator pipeline, not Sims export compatibility; no purchases or SDK/license assumptions. Optional cloud AI must not silently receive photos. |
| [Microsoft TRELLIS.2](https://github.com/microsoft/TRELLIS.2) | Image-to-3D asset generation; repository declares MIT code/model. Optional research for reference geometry or accessory concepts. | General generated topology is not a Sims head/body. Audit all dependencies/assets separately; Windows/Rust/GPU costs and reproducibility untested. Do not prioritize this over fitting existing Sims morphs. |
| [TS4MorphMaker](https://github.com/CmarNYC-Tools/TS4MorphMaker) and existing native External CAS components | Inspect established Sims-specific format/morph behavior alongside current native source rather than introducing a parallel package writer. | Pin exact source and file-level licenses; no unsupported direct import claim. Original game assets and third-party CC remain local and retain their own rights. |
| [EMOCA](https://emoca.is.tue.mpg.de/) | Historical CVPR 2022 expression-aware monocular face reconstruction research; reviewed 2026-11-19, not a new 2026 release. Its authors argue common geometric/photometric reconstruction metrics alone miss expression fidelity. Useful motivation for the expression-versus-stable-shape evaluation below, not proof of Sims likeness. | [Official license](https://emoca.is.tue.mpg.de/license.html) limits use to non-commercial scientific research, education or artistic projects, prohibits commercial use, and restricts reproduction/modification/third-party availability and distribution. Research reference only: no production dependency, download, bundling or redistribution approved. Separate permission/license and dependency review required before any use; existing GPL application does not override these terms. |

## Recommended architecture (proposal)
**First milestone: fit existing editable Sims controls, not arbitrary AI mesh replacement.**
1. Local image selection with consent and input validation. Separate source observations from inferred geometry.
2. Normalize orientation/camera/expression; permit manual landmarks, masks and exclusions. Reject ambiguity or ask the user to choose the subject.
3. Fit a bounded, versioned map of supported installed face/body morph controls using landmark reprojection and silhouette error, with camera parameters and shape regularization separated. Start with a small independently verified control subset. Report unreachable shape residuals instead of silently distorting the Sim.
4. Match face/body and appearance using the best suitable authorized installed presets, sliders, skins, hair and costumes, with optional discovery through the mod database as specified below. Keep installed/usable candidates separate from missing recommendations. Do not download, clone copyrighted clothing from the photo or install CC automatically.
5. Preview several editable candidates, not a single asserted identity match. Keep sliders and corrections inspectable; manual edits must not disappear after a later inference result.
6. Commit only an explicitly reviewed proposal to a separate revision-checked project draft. Reuse existing undo, recovery and Tray validation. Game application remains a different explicit qualified boundary.
7. Optional later texture/custom morph generation requires topology, UV, rig, normals, LOD, material, animation and distribution validation. It must never replace unsupported resources or imply generic AI output can be dropped into CAS.

## CC-aware likeness and mod database integration
Owner clarification, 2026-11-19. **Requirements/backlog, not implemented capability or verified creator catalog.** Expand the original installed-only suggestion scope to include optional discovery of suitable missing CC, without making online access or new CC mandatory.

### Match the whole editable result
- Fit the face and body jointly with supported base-game controls and installed **presets/sliders**, then compare compatible **skins/overlays, hair, costumes/outfits and accessories**. Separate geometry/proportions from texture, makeup, lighting, pose and clothing so a skin's shading is not misrepresented as a shape fix.
- User examples to investigate: **Magic Bot** presets/sliders and **ThisIsThem** skins, including a relevant celebrity skin if one is actually verified. These are owner-provided discovery leads, not assertions of a particular file's identity, availability, licensing, installation or compatibility. Do not guess a celebrity or identify a person from their photograph.
- “Best” means best supported fit for the user's chosen likeness/style and constraints within the searched collection, not a universal ranking or exhaustive inventory. Explain coverage, alternatives, uncertainty and tradeoffs. Let the user prefer installed-only, base-game-only, no paid content, specific styles or excluded creators; never silently substitute missing assets.

### Database contract and recommendation evidence
Use the shared mod database's provider-qualified creator, project, release and file identities rather than matching names alone. Cross-site aliases require verified links. Each candidate must retain:
- Canonical creator/project/source URLs and IDs; exact release/file identity, evidence timestamps, license/usage restrictions and available integrity metadata. Distinguish author attribution from uploader identity.
- Installed status and actual local package/resource identity, hash/revision when available, selected provider/override priority and duplicate/conflict evidence. Do not expose personal paths in public logs.
- Required meshes, packs, other CC, presets/sliders/frameworks and their dependency chains, including missing, conflicting and unresolved requirements.
- Declared game-build, age/frame/species/occult/outfit/category restrictions where supported, plus renderer/morph/parser support and actual local validation separately. Provider-declared compatibility is not game-tested compatibility; unknown stays unknown.
- Availability states: installed and validated, installed but unresolved, publicly downloadable, login/member/payment required, removed/unavailable, or unknown. A failed request is not proof of deletion. Record the legitimate acquisition route and last check without promising universal automatic access.
- The proposed contribution (face shape, body proportion, skin detail, hair silhouette, outfit likeness), local preview evidence, confidence/limitations and why it outranks alternatives under the user's preferences. Preserve the full selected CC/dependency manifest and provenance in the portable draft.

### Missing CC and review workflow
Show missing items explicitly with creator, exact project/file or unresolved identity, why each helps, requirements, compatibility uncertainty, cost/access conditions and official source. Ask the user to supply a legitimately obtained local file or choose an authorized source/login workflow, skip it, or select an installed alternative. Do not download, purchase, install, replace, redistribute or bypass a host's restrictions merely because a recommendation exists.

After user-supplied acquisition, rescan and verify the actual file, resource identities and dependency closure before offering it as usable. Never silently enable scripts or overwrite packages. Revalidate changed/deleted sources and reject stale inference/recommendation results if the reference image, Sim draft, installed inventory, chosen CC or database evidence changed. Commit only reviewed, supported changes to a recoverable draft; retain manual region locks and undo.

Keep photo analysis local by default. Database discovery should use ordinary asset/category metadata, not upload the photograph, face embedding or derived identifying representation. Any optional remote photo processing requires separate informed approval; database access is not that approval. Keep reference photos, private inventories and likeness previews out of public catalog/issue data.

### Local evidence checkpoint
`external_cas_catalog_cache::cc_evidence` reconciles already collected
metadata only. It compares full provider/creator/project/release/file IDs,
the effective resource key and exact byte digest, shows every declared
requirement independently, and treats equal-priority conflicting evidence as
ambiguous. Public/restricted/unavailable status is independent of local
evidence; offline or failed checks do not establish availability. An explicit
alternative must still match the current inventory, and a supplied file
returns a rescan request, not permission to copy or install it.

Synthetic catalog tests cover these seams only. The caller still needs to
collect authoritative IDs, evidence freshness, installed byte digests and
loader precedence, scope reports to their exact file, flatten the dependency
closure (or mark it unresolved), and run native UI/game qualification. This
checkpoint does not add provider lookup, acquisition, alias inference, a review
screen, or game-compatibility claims.

### Additional acceptance backlog
- [ ] **CC1: identity and inventory join.** Fixtures with duplicate filenames, similar creator names, different provider IDs, overrides, changed hashes and missing meshes must not join or mark installed incorrectly.
- [ ] **CC2: constrained likeness comparison.** Compare supported preset/slider + skin + hair/outfit combinations against a base-game/manual baseline; explain geometry-versus-texture effects and preserve user locks and unsupported fields.
- [ ] **CC3: missing-CC workflow.** Exercise available, restricted, unavailable and unknown sources; skip, cancel, offline, user-supplied file and installed alternative paths. No automatic acquisition, hidden substitution or manufactured availability.
- [ ] **CC4: compatibility and recovery.** Test dependency chains, conflicts, stale game/build/catalog/package changes and out-of-order results; save/reopen the exact manifest, undo and separately qualify real game/Tray round-trip.
- [ ] **CC5: privacy and performance.** Verify no photos/embeddings/private inventories leave the device through discovery, portable storage after relocation, and measured cold/warm search/preview at large-library scale without hiding candidates or dropping fidelity.

## Milestones and acceptance backlog
- [ ] **M0: capability/rights inventory.** Pin game build, native baseline and existing parser/morph/Tray contracts; enumerate supported parameters and unsupported fields. Review code, weights, model dependencies and output terms separately. Preserve current development and reserved Apex sessions.
- [ ] **M1: private reference intake.** Local-only face/body selection, orientation, multiple-subject refusal/selection, crop and quality guidance; no photo required to use ordinary CAS. Test malformed/truncated/oversized files and cancellation.
- [ ] **M2: face fitting proof.** Rights-cleared synthetic adult fixtures with known morph/camera parameters; compare recovered parameters and rendered landmark errors against simple preset/manual baselines. Distinguish expression from genetics. Unseen yaw, occlusion and unsupported traits must show uncertainty.
- [ ] **M2a: expression-versus-stable-shape evaluation.** Proposed External CAS test, motivated by [EMOCA's expression-focused research](https://emoca.is.tue.mpg.de/), not a protocol claimed from its authors and not dependent on running EMOCA. With separately consented adult reference pairs of the same person in neutral and expressive poses, hold camera/lighting as consistent as practical and compare inferred stable Sim face/body controls separately from expression parameters. Render each fitted candidate in the same neutral pose; measure normalized stable-control drift, neutral-render landmark differences and participant-rated likeness against a neutral-reference/manual baseline. Record pose/lighting/occlusion confounds, failures and uncertainty; establish thresholds before evaluation, not after selecting successes. Do not infer feelings or psychological traits from facial configuration. No photos acquired or experiment performed; any actual EMOCA comparator must first pass its distinct license/permission gate.
- [ ] **M3: body fitting proof.** Separate pose, camera and shape; evaluate clothing/occlusion and optional calibrated scale. Allow user adjustment of inferred proportions. Never infer health, ethnicity or other sensitive traits.
- [ ] **M4: full editable preview.** Multi-view user correction, locks for manually edited regions, undo/redo, seeded repeatability and model/version provenance. Test stale results after changing image, Sim, project or sliders, plus out-of-order responses and dispose/cancel.
- [ ] **M5: preservation and game round-trip.** Verify unrelated Sim identity, outfits, occult/unknown data and CC dependencies remain unchanged. Save/reopen a new draft, export to disposable Tray/profile, load actual pinned Sims 4 and compare. Test unavailable CC, version mismatch, failed write and crash rollback. No live game/session mutation by inference.
- [ ] **M6: real Windows performance and quality.** Record source versions, hardware, latency p50/p95, peak RAM/VRAM, CPU fallback, device loss and cancellation. Evaluate permitted diverse face/body/lighting cases with user-rated likeness and failure rates; no “all demographics” superiority claim from a few examples.
- [ ] **M7: optional advanced reconstruction.** Compare research models only after M2/M3 establishes a useful baseline. Ship only independently accepted improvement, not a fashionable model name.

## Privacy, portability and rights
Default local-only processing, no automatic cloud uploads, telemetry images or training reuse. Ask users to supply their own images or permission from the depicted person; do not implement identity lookup or infer a creator/person identity from a face. No public facial corpus assembled from users. Exclude minor photographs from initial testing; any future minor workflow needs dedicated consent and safeguards.

Keep selected photo copies, derived geometry, models, caches and receipts inside an explicitly selected portable project/app folder; bounded caches and user-visible retention/removal. Strip EXIF/location from derivatives, avoid personal paths in logs, and make clear that deletion from the app is not a guarantee of secure erasure from backups. No hidden AppData dependency. Optional remote processing needs specific provider/data/retention disclosure and user approval before transmission.

Follow [EA's mod policy](https://help.ea.com/en/articles/the-sims/the-sims-4/mods-policy/) and [game-update compatibility guidance](https://help.ea.com/en/articles/the-sims/the-sims-4/mods-and-the-sims-4-game-updates/). EA does not pre-screen or endorse particular CC; new game builds require compatibility checks. Do not redistribute game resources, paid models, creator CC, photos or generated likenesses without applicable rights. No model has been selected for production and no legal acceptance, download or installation occurred.
