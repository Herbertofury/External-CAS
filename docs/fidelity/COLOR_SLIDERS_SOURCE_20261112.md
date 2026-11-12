# Color slider source boundary

The user supplied the 2026-11-05 tyjokr update as a local compatibility reference
and requested color controls for all CAS items/swatches. The packages were read
without installing anything or executing the DLL or script archive. No extracted
game/mod source, UI artwork, lookup tables or binaries are included in this repo.
Exact local paths, package/resource pins and private research outputs belong in
[ignored evidence](../../.local/fidelity/tyjokr-ui-inspection.json).

## Actual source data and controls

The supplied CAS-specific package contains no GFX movies. The accompanying color
package overrides the game data, CAS customizer and CAS library movies. The
library exposes four active controls in H,O,S,V order. A separate source mode
binds four specular controls in the same order, with the final control labeled
Gloss rather than Brightness. Presence of that state does not establish a working
native specular shader.

Selected-swatch ranges take precedence. The CASP version 44+ resource layout has a
44-byte block: opacity minimum/step, then hue, saturation and brightness
minimum/maximum/step; all are little-endian 32-bit floats. Opacity's maximum is
implicit 1. The existing GPL CASTools/XmodsDataLib format reader independently
identifies this layout. Native assets retain the exact bits, including invalid
floats and signed zero; UI validation must not rewrite them.

The supplied UI uses fallback ranges only when source ranges are invalid or
disabled and the item is modded or its availability lookup is unknown:

- Hue, saturation and brightness: −0.5 through +0.5, step 0.05.
- Opacity: 0.2 through 1, step 0.05.
- Neutral modifiers: H=0, O=1, S=0, V=0. Hue shifts are positive turns, displayed
  as modifier × 360 degrees.

Zero step or zero maximum is the source UI's disabled sentinel. Finite/order/
positive-step validation is also required before a range is safe to use. A
fallback's neutral display value must not silently replace an unsupported stored
modifier; the application preserves the actual packed value until an explicit
edit or reset.

The source availability table uses H,S,V,O order, distinct from control order.
Available/hidden states come from actual per-item data. Missing lookups remain
unknown; a successful native fallback does not prove that every channel affects
the selected material. The supplied UI treats modded swatches/products as
available for all four channels. This metadata boundary does not impose a body
type blacklist on opacity.

The supplied changelog specifically restores opacity for tights, socks and base
layers, and skips fully transparent textures such as bare feet. Detect actual
decoded texture transparency rather than treating a category name as evidence.
Preserved modifiers on a fully transparent texture can correctly have no visible
effect.

## Native API

`external_cas_fidelity::color_ui` provides:

- `SwatchColorInput::from_casp(part, [h,o,s,v], mode)` projects the selected
  resource's actual ranges. The caller supplies observed mod status, texture
  transparency and actual availability table results when known.
- `resolve(&input)` returns four `ChannelControl`s with effective ranges,
  fallback/source status, real control binding names, source visibility,
  preserved value bits, and out-of-range diagnostics. It does not mutate a Sim,
  source package or saved packed modifier.
- `ColorUiMode::SkinSpecularity` selects `slSpec*` source bindings and a Gloss
  label. Its boundary explicitly requires a specular material adapter; a diffuse
  brightness transform must not be relabeled as gloss.
- `custom_actions(current, original, count, selected_is_custom)` exposes the
  source creation/deletion rules. The reference limit is 10 custom variations;
  Create is unavailable at neutral/original values (comparison tolerance 0.001)
  or at the limit. Delete requires an actual custom swatch. These are metadata
  decisions, not a claim that native custom-swatch persistence is implemented.

Source-derived UI facts do not establish color-engine pixel equivalence. The
root-owned material implementation must separately qualify texture composition,
alpha, color space, hue/saturation/value transforms, specular behavior, default
and original resets, saving and reopening. The full all-item requirement remains
an acceptance gate, including missing, transparent and unsupported resources.

## Validation

The fidelity tests cover selected-range precedence, disabled sentinels,
availability order, modded fallback, unknown lookups, invalid float/value
preservation, skin-specular mode, transparent no-op status and custom-action
limits. The assets tests independently qualify the 44-byte parser against real
selected resources. Private script inspections are research oracles only; native
runtime code consumes selected resources and typed metadata.
