# Installed CAS UI resources

This Rust package reads the player's installed UI resources through the native,
read-only DBPF reader. It does not ship game artwork, fonts, binaries, proprietary
source, extracted packages, or an ActionScript runtime. These assets remain on the
player's computer. The native app can accept an explicit `--ui-assets UI.package`
provider or select it in the installed UI source picker.

## Runtime API

- `GfxMovie::inspect(bytes)` validates an uncompressed GFX/FWS movie and exposes
  stage dimensions, frame rate/count, imports, exported symbol classes, compacted
  font headers, external image names and sub-image rectangles. `raw()` retains the
  exact movie; every tag has its original payload and unparsed ranges. Compressed
  movie bodies are explicitly unsupported; DBPF-level compression is supported by
  the asset reader before this boundary.
- `installed::load_movie(index, bare_name)` resolves the actual imported library
  name using lower-case FNV-1. The caller explicitly supplies a package provider.
  Duplicate live matching ordinals are rejected. Returned source pins identify
  the full TGI, provider, ordinal, index digest and selected payload digest.
- `installed::load_atlas(index, external_image)` resolves the source filename's
  DDS resources across texture resolution groups. It requires exactly one with
  dimensions equal to the movie's actual target dimensions. It preserves the
  selected DDS and pins, decodes unsigned BC5 channels natively, and bounds both
  stored bytes and decoded pixels. Other supported DDS formats use `image`.
- `font::CompactedFont::open(movie, header)` reads source character codes,
  advances, kerning, contours, shared contour references and every supported
  compacted line/quadratic encoding. Requested glyph decoding accepts an explicit
  edge budget. Source coordinates remain Y-down.
- `truetype::convert(font)` converts those source curves into an in-memory,
  hint-free TrueType face for egui. It retains every source character and mapped
  horizontal kerning pair, converts the Y basis explicitly, and rejects unknown
  extra FontTypes, missing kerning targets, duplicate codes and unsupported
  metrics. It does not infer redistribution or embedding permissions.

`cas_ui::InstalledUiState` owns asynchronous app loading, generation cancellation,
font/texture publication, accepted pins and visible compatibility messages. Named
font families remain bound during initial load, deferred egui font publication,
reload and failure. A failed new source does not inherit an old provider label.
The application loader writes no extracted resource files.

Session restore calls `InstalledUiState::load_pinned(ctx, provider, expected_pins)`.
Every dependency's provider, TGI, ordinal, index hash and payload hash must match
the newly observed reads before fonts/textures are published. Missing, extra or
duplicate pins fail closed. Explicit source reselection uses `load` to accept
current resources. All file reads, decoding and validation stay on the worker.

## Current supported boundary

Nine installed font faces are consumed by the actual app, including its body and
Sim headline text. Two actual high-resolution CAS/Component atlases decode and
upload. Atlas R/G channels are literal BC5 UNORM data; assigning luminance, color
and alpha is a separate, explicitly selected presentation operation. Uploading
these pixels does **not** reproduce the source UI display list.

Source sprite timelines, bitmap fill matrices, color transforms, nine-slice
scaling, symbol/state selection, dynamic image assignment, masks, clipping,
filters, imported components and engine service behavior still require native
adapters and golden comparisons. The current authored control marks and room
gradient are temporary scaffolding. Their existence is not a fidelity or
performance exception, and they are not the final visual source.

The room's native model/material/camera/light loader and the full assembled Sim
remain separate acceptance gates. This package does not claim CAS parity.

## Validation and provenance

`cargo test -p external-cas-fidelity --offline` exercises every compacted edge
encoding, signed/unsigned packed values, truncation, tag extents, unknown-byte
retention, explicit BC5 swizzling, both interpolation branches and curve basis
conversion. The app's installed-font tests render active egui passes through
initial, completed, reload and failed states.

Private installed-asset validation currently confirms:

- All nine compacted faces convert without omitted glyphs or kerning; all nine
  resulting faces independently open with FreeType.
- 104 real glyphs across those faces, including all 72 symbol glyphs and 4,777
  edges, exactly match the independent installed FFDec reader.
- Both complete atlas R/G pixel buffers exactly match the independent MIT
  `texture2ddecoder` BC5 decoder.
- An actual hidden native-app launch loads nine original fonts and two atlases
  and renders real catalog content without UI resource errors.

The generic example `inspect_installed` requires an explicit provider. It prints
private source pins to stdout. Its optional `--private-output-dir` is an explicit
diagnostic extraction boundary; direct such output to an ignored local directory
and never publish it with the application. No private evidence or assets belong
in Git.

GFX extension structure references were checked against the installed GPL v3
[FFDec](https://github.com/jindrapetrik/jpexs-decompiler) reader:
`DefineCompactedFont`, `FontType`, `GlyphInfoType`, `GlyphType`, `ContourType`,
`EdgeType`, `GFxInputStream`, `DefineExternalImage{,2}` and `DefineSubImage`.
This package is GPL-3.0-or-later; the repository's existing license applies.
Native BC5 decoding implements the published block representation independently.
The independent validation tools are development oracles, not runtime
dependencies. Exact local tool hashes and asset/provider pins are in ignored
private evidence referenced by [the fidelity checklist](../../docs/fidelity/CAS_FIDELITY_20261112.md).
