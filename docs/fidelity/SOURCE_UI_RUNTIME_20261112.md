# Original CAS UI runtime qualification

The installed UI contains an AS3 application, not only artwork. Replacing its
panels or icons individually cannot establish the requested complete CAS UI
behavior. The native runtime track therefore evaluates executing the original
movies with Rust-owned AVM, rendering, and an explicit External CAS data/service
backend. Existing application controls remain until this route qualifies.

## Source layout and assets

`external-cas-fidelity::ui_layout::load_manifest` reads the original widgetlist
from an explicit package provider, retains its exact bytes and source pin, and
parses all unknown XML fields as well as understood layout fields. The observed
installed manifest has 201 widgets and 14 game states; the PC CAS state has 56
entries. An independent XML implementation agrees with all 56 native records.
Selection does not infer that every entry is currently visible or supported.

`widgets_for_state("cas", UiPlatform::Pc)` supplies source layers, on-demand and
initial visibility, position/origin fractions, and top/right/bottom/left logical
pixel margins. `UiPlacement::place(stage_pixels, helper_pixels)` requires the
actual movie's `mcPositionHelper` bounds. Source coordinates are logical pixels;
movie shape/display-list coordinates are twips (20 per logical pixel). No fitted
helper size, UI scale, or screen margin is substituted. Original state-specific
placement overrides and the complete XML tree remain available.

`runtime::requirements` records original AS version flags, stage, frame rate,
imports, linked classes and unsupported extension counts. `prepare_swf_movie`
moves an existing GFx FileAttributes tag before exporter metadata in an in-memory
copy so a standard SWF reader sees the declared AVM2 flag. All other tag bodies,
ABC, artwork, source coordinates/timing and trailing bytes remain unchanged.
The original and derived hashes are distinct; originals are never rewritten.

`runtime_assets::load` supplies original compacted-font outlines, metrics,
kerning and nominal units, and sub-image pixels cropped from exact pinned atlas
resources. It preserves source character IDs and rejects unresolved/cross-movie
bindings, duplicate IDs, unknown font suffixes and unsupported image encodings.
Two-channel UI atlases require an explicit presentation policy; the previously
qualified red-luminance/green-alpha policy remains separate from literal BC5
channel decoding. This adapter is useful to a VM host; it is not a complete GFx
runtime. No installed game/creator asset is included in the repository.

## Rust interpreter experiment

The private experiment uses official
[Ruffle source at 1d7cf3b68281a3b857bdac8e247d346e76aba685](https://github.com/ruffle-rs/ruffle/tree/1d7cf3b68281a3b857bdac8e247d346e76aba685),
version 0.7.1. Its declared MIT-or-Apache-2.0 license is compatible with this GPL
application; full dependency notices still need review before distribution.
The inspected renderer uses wgpu 30.0.1, matching the current native app.

Stock source accepts GFX/CFX signatures but skips the relevant Scaleform tags
(1000 exporter metadata, 1005 compacted font, 1008 sub-image, 1009 external image).
Its AS3 ImportAssets2 implementation is explicitly incomplete. Stock loading of
the observed import graph fails; the isolated fork's experimental import loader
preloads dependencies, shares the parent application domain, and initializes
imported bytecode/linkage before publishing imports. This is an experiment and
needs targeted Flash/Scaleform lifecycle tests before production adoption.

With that private loader change and source header preparation, the original CAS
Navigation movie and its six imported libraries execute AS3 and issue the real
`CasGetCASEditMode` request through `CallGameService`. This proves an interpreter
entry point; it does not prove visual fidelity or functional CAS. All calls are
recorded and return Undefined. The experiment cannot call the game, browse the
network, open arbitrary local files, navigate, or open sockets: only explicitly
pinned in-memory source resources are fetchable. The game and installed assets
are never modified.

## Remaining acceptance gates

- Qualify original font/bitmap adapters through the actual GPU movie renderer,
  then compare independent static goldens and source timeline states.
- Implement required native Scaleform behavior: original `Extensions` and
  `TextFieldEx` AS stubs are not working implementations. Quietly running these
  stubs would lose clipping, text, focus, input and bound UI behavior.
- Boot the original UI container and source widget manifest; preserve original
  layers, clipping, helper bounds, labels, event routing and context changes.
- Define typed real host services/messages, localization and data objects. The
  original bridge uses `CallGameService`, `SendGameMessage`, `ObjectSendMessage`
  and inbound `SendMessage`/`CallService`. Record-only calls are not working
  controls. Mutations must use verified project transactions, stale-source checks
  and the existing session/recovery pipeline.
- Bind original 3D UI to the qualified source camera/Sim transforms, and verify
  selection/category/outfit/undo/redo/accept/cancel states against originals.
- Package a pinned, tested runtime with license notices before switching the
  application's framework or removing currently working editing controls.

Private provider paths, resource TGIs, exact source hashes, logs, source XML and
proprietary ActionScript/assets stay in ignored `.local/fidelity`. The public
source/checklist intentionally contains only adapter code and sanitized facts.
