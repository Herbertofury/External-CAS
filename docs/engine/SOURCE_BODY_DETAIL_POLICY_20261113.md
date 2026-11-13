# Source body-detail selector inputs

The adult Breast detail prefix is selected from the model's resolved context
byte `+0x249`. It must not be inferred from a displayed gender alone. These
contracts were traced read-only against installed client SHA256
`95e40b75f76711c66b9f36a974ec04d43304a6f591d409d3d4019818f366237b`.
Original instructions and source payloads remain private.

Native model-copy function `019EFF30` finishes by calling `0033EEC0` with the
full Sim description and storing its low return byte into the context. The
selector reads that description's ordered 64-bit trait vector. Its resolved
`Client_CASTuning` input comes from `0032A2F0`, called with descriptor word 4
and descriptor word 2; this document does not substitute a guessed selector.

The policy is exact, with checks in this order:

1. If the trait vector contains `SpecialTraits.ForceBreastsTrait`, return true.
2. Otherwise, if it contains `SpecialTraits.ForceBreastlessTrait`, return false.
3. Otherwise, return false when both `(word2 & 0x20) != 0` and
   `(word0 & 0x78) != 0`.
4. Otherwise, return `(word0 & 0x2000) != 0`.

If both traits exist, the first rule wins independently of their vector order.
The original helper's false paths do not clear every high return-register bit;
its caller consumes only `AL`. A reference oracle must use that same boundary.

The tuning-field join is source-backed: loader `0032C720` creates a tree node
whose selected data begins at node `+0x28`. Selected data `+0x1C8` and `+0x1D0`
therefore correspond to node `+0x1F0` and `+0x1F8`. These are loaded from
SpecialTraits offsets `+0x10` and `+0x18`, the third and fourth 8-byte fields.
Registration `0032B320` names those fields `ForceBreastsTrait` and
`ForceBreastlessTrait` in the same order. The independently retained installed
Human SimData schema agrees with the order and type. Their values are selected
source data, not globally hardcoded constants.

Earlier descriptor-update function `019F01E0` writes only rules 3 and 4. That
initial update does not replace the final full-description trait policy. A port
that stops at the earlier write loses explicit trait behavior.

The material consumer `01A1AAB0` uses this resolved byte to choose the adult
Breast-prefix character. Ordinary body-prefix selection follows its own
descriptor path; this trait policy must not replace every gender/form field.

## Qualification

An isolated emulator executed the original selector and its descriptor helper
`003347B0` for 632 cases. It intercepted only the already-resolved tuning pointer.
Cases include all individual age flags, controlled sex/frame words, each trait,
both trait orders, duplicate traits, unrelated traits and full-width random
64-bit trait IDs. All results match the functional policy above. The reference
executed 20,366 original instructions at 57 addresses with no native I/O or game
contact. The private receipt is retained under the ignored local evidence tree;
[sanitized evidence](evidence/body-detail-trait-policy.json) contains no original
code, assets or Sim data.

This establishes the selector's mathematical policy and its model write. Active
form/profile selection, persisted trait decoding and actual complete material
construction remain separately qualified caller responsibilities.
