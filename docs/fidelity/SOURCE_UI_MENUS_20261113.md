# Original CAS menu host data

The native adapter now reads the installed `cas.cas_tuning.CASTuning` SimData
and projects the original `CasGetAddSimMenuData`, `CasGetCopyFiltersData`, and
`CasGetRandomizationFiltersData` replies. It retains original bytes, the existing
Room parser's full document and unknown cells, and exact provider/index/payload
pins. No source movie, script, icon, localization text, user data or game binary
is included in this repository.

The observed source contains 17 Add-Sim entries, eight copy entries and 26
randomization entries. The context filters follow the original native
membership, first-active-occult-bit priority, exclusion and early-exit order.
Fields use the original ABI: signed enum decimal strings, unsigned icon instance
decimal strings, UInt32 masks, signed sort indices, and localization hash
objects. Copy order counts admitted entries from one; randomization retains its
source key. Unknown criteria and missing contextual observations are errors,
not empty lists or invented defaults.

`menus::CasMenuSnapshot::load(explicit_provider, cancel)` selects the exact
module key `545AC67A:006CA304:5E2C8D43E4F3DF6C` within one explicitly resolved
provider. Duplicate live rows fail. This function does not infer game-wide
Full/Delta/Mods priority or account entitlements. The caller's configured source
resolver must supply the winner. The source reader remains read-only.

`CriterionContext` contains optional native AgeGender bits, species, occult
flags, active occult form, named observed Boolean context, and an optional
explicit pack-capability set. `None` and a known empty set differ. The current
qualified Boolean predicates are `is_new_sim`, `is_mannequin`, and
`is_full_edit_mode`. Standalone editor workflow facts may supply these where
the application actually knows them; account or gameplay facts are not inferred.

The actor's `HostConfig.menu_provider` loads this data off the UI thread.
`EditorSnapshot.menu_context` participates in revision identity and is latched
with the same snapshot as Sim data and action capabilities. The three original
service names resolve synchronously from that immutable snapshot. Required
first-frame replies are constructed before original initialization; incomplete
context returns visible `WaitingForEditor` reasons. A competing static/editor
reply cannot shadow the source menu provider.

All three packets match the unchanged independent S4TK 0.6.14 reader. They also
match execution of the original three native marshalling methods across 12
explicit qualification contexts: 132,694 original instructions, all comparisons
exact. Those contexts exercise new/existing, human/pet, age/gender, occult and
mixed-occult priority, mannequin, pack subsets and edit modes. The original
native eligibility instructions execute; the context-builder output, typed-value
operations and explicit pack-membership calls are documented hooks. This is a
marshaller/data-boundary qualification, not a claim that the game, GFx VM, live
profile or entitlement services were executed. No game process was contacted.

Sanitized machine-readable evidence is in
[`native/source_ui/evidence/static-menu-qualification.json`](../../native/source_ui/evidence/static-menu-qualification.json).
Private original-code receipts and extracted data remain in ignored local
research storage.

The original widget host normally supplies global class libraries. Standalone
movie preparation now carries UIContainer's actual zero-symbol ImportAssets
library declarations into its in-memory derivative. It retains the UIContainer
pin and dependency pins; the root's ABC bytes and all installed originals are
unchanged. Own-source tests verify this and avoid duplicate import insertion.
This fixes the missing CASRandomizeFilter definition. It does not supply the
original LayoutManager or make a standalone widget a complete CAS host.

The full original container now passes the previous copy/menu typed-array
initialization failures in the private, explicitly opted-in GFx experiment.
Actual Sim/family/tag services, authorized `img://` resource loading, input/layout
and remaining runtime argument semantics are still open. No usable-layout or
full CAS parity claim follows from these packet comparisons.
