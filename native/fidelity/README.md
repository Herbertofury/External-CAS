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
- `timeline::SymbolLibrary` selects an actual exported symbol/state frame and
  evaluates source placement, move/remove, affine and color transforms. Its
  initial visible geometry subset is bitmap-filled rectangles; masks, filters,
  strokes, nonrectangular paths and unsupported bindings fail explicitly.
  `symbol_raster::rasterize` resolves actual sub-images and requires an explicit
  channel presentation for BC5 UI artwork. No source ActionScript executes.
- `color_ui` projects selected-swatch CASP ranges and reference control bindings,
  flags unavailable/unknown/fallback states, retains unsupported modifier bits,
  and distinguishes diffuse and specular modes. See the
  [color boundary](../../docs/fidelity/COLOR_SLIDERS_SOURCE_20261112.md).

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
Sim headline text. Three original CAS/Component/Navigation atlases decode and
upload. Atlas R/G channels are retained as literal BC5 UNORM data. Navigation
symbol rendering explicitly selects red luminance plus green alpha and has been
compared against independent static rendering.

Six source navigation symbols replace temporary artwork in the actual native
controls: rotation left/right, undo/redo, cancel/close, and household-save artwork
used for the project action. Their up/over/down/disabled states are selected from
the original movie. Existing native handlers remain attached. Source hover
animation, exact global layout and the other UI components are still open.

Source nine-slice scaling, full animation/state semantics, dynamic image assignment, masks, clipping,
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
- Both original CAS/Component atlas R/G pixel buffers exactly match the independent MIT
  `texture2ddecoder` BC5 decoder.
- 28 source navigation states were compared with the independent FFDec static
  renderer, using its own sub-image parser and an independently decoded atlas
  bound in memory because its DDS reader does not support ATI2. The six adopted
  controls have maximum premultiplied RGB error below 2/255 and alpha error at
  most 2/255; most states' alpha matches exactly. Reset artwork remains unused
  because its source semantics and higher pixel discrepancy need investigation.
- An actual hidden native-app launch loads nine original fonts and three atlases
  plus 24 cached original navigation states
  and renders real catalog content without UI resource errors.

The generic examples `inspect_installed`, `enumerate_installed` and `render_symbol`
require explicit providers. They print
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

The additive `runtime`, `runtime_assets` and `ui_layout` modules support the
[original source UI runtime qualification track](../../docs/fidelity/SOURCE_UI_RUNTIME_20261112.md).
They retain original movie/manifest bytes and pins, provide source layout and
font/bitmap data, and separate declared AS3/GFx requirements from demonstrated
support. No Ruffle fork, original ActionScript or proprietary movie/artwork is
bundled. The current interpreter experiment remains private and unqualified for
an application framework switch.
