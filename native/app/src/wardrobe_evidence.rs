//! Wardrobe CASP metadata is selection evidence, never validation of a genetic control.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceSlot {
    Hair,
    FullBodyOutfit,
    Top,
    Bottom,
    Shoes,
    Unclassified(u32),
}

impl SourceSlot {
    pub(crate) fn from_native_body_type(raw: u32) -> Self {
        // These are the appearance slots already named by the native Wardrobe.
        match raw {
            2 => Self::Hair,
            5 => Self::FullBodyOutfit,
            6 => Self::Top,
            7 => Self::Bottom,
            8 => Self::Shoes,
            raw => Self::Unclassified(raw),
        }
    }

    pub(crate) fn label(self) -> String {
        match self {
            Self::Hair => "Hair".into(),
            Self::FullBodyOutfit => "Full body".into(),
            Self::Top => "Tops".into(),
            Self::Bottom => "Bottoms".into(),
            Self::Shoes => "Shoes".into(),
            Self::Unclassified(raw) => format!("Body slot {raw}"),
        }
    }

    pub(crate) fn review_label(self) -> String {
        match self {
            Self::Unclassified(raw) => {
                format!("Appearance purpose unclassified (body slot {raw}).")
            }
            Self::FullBodyOutfit => "Appearance slot: full-body outfit.".into(),
            known => format!("Appearance slot: {}.", known.label()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreviewRequirements {
    BothReferencesListed,
    NoSupportedLod0Reference,
    NoDiffuseReference,
    NeitherReferenceListed,
}

impl PreviewRequirements {
    pub(crate) fn review_label(self) -> &'static str {
        match self {
            Self::BothReferencesListed => {
                "LOD 0 mesh and diffuse references are listed. This does not verify their files or preview."
            }
            Self::NoSupportedLod0Reference => {
                "Diffuse is listed; no supported LOD 0 mesh reference for the standalone preview."
            }
            Self::NoDiffuseReference => {
                "LOD 0 mesh is listed; no diffuse reference for the standalone preview."
            }
            Self::NeitherReferenceListed => {
                "No supported LOD 0 mesh or diffuse reference for the standalone preview."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IndexedAppearanceEvidence {
    pub(crate) slot: SourceSlot,
    pub(crate) preview: PreviewRequirements,
}

impl IndexedAppearanceEvidence {
    pub(crate) fn from_casp_index(body: u32, lod0_listed: bool, diffuse_listed: bool) -> Self {
        let preview = match (lod0_listed, diffuse_listed) {
            (true, true) => PreviewRequirements::BothReferencesListed,
            (false, true) => PreviewRequirements::NoSupportedLod0Reference,
            (true, false) => PreviewRequirements::NoDiffuseReference,
            (false, false) => PreviewRequirements::NeitherReferenceListed,
        };
        Self {
            slot: SourceSlot::from_native_body_type(body),
            preview,
        }
    }

    pub(crate) fn morph_review_label(self) -> &'static str {
        // No CASP-only branch may promote references, outfits or an unclassified
        // slot to a verified editable face or body control.
        "Editable face/body controls are not verified by this metadata."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn established_slot_labels_are_unchanged_and_full_body_means_outfit_here() {
        for (body, slot, label) in [
            (2, SourceSlot::Hair, "Hair"),
            (5, SourceSlot::FullBodyOutfit, "Full body"),
            (6, SourceSlot::Top, "Tops"),
            (7, SourceSlot::Bottom, "Bottoms"),
            (8, SourceSlot::Shoes, "Shoes"),
        ] {
            let evidence = IndexedAppearanceEvidence::from_casp_index(body, true, true);
            assert_eq!(evidence.slot, slot);
            assert_eq!(evidence.slot.label(), label);
            assert!(
                evidence
                    .slot
                    .review_label()
                    .starts_with("Appearance slot: ")
            );
        }
        assert_eq!(
            SourceSlot::FullBodyOutfit.review_label(),
            "Appearance slot: full-body outfit."
        );
    }

    #[test]
    fn unknown_body_types_are_not_guessed_from_geometry_or_texture() {
        for body in [0, 1, 19, u32::MAX] {
            let evidence = IndexedAppearanceEvidence::from_casp_index(body, true, true);
            assert_eq!(evidence.slot, SourceSlot::Unclassified(body));
            assert!(evidence.slot.review_label().contains("unclassified"));
        }
    }

    #[test]
    fn unresolved_preview_geometry_is_not_absence_of_a_valid_appearance_item() {
        for (geom, diffuse, expected) in [
            (true, true, PreviewRequirements::BothReferencesListed),
            (false, true, PreviewRequirements::NoSupportedLod0Reference),
            (true, false, PreviewRequirements::NoDiffuseReference),
            (false, false, PreviewRequirements::NeitherReferenceListed),
        ] {
            let evidence = IndexedAppearanceEvidence::from_casp_index(2, geom, diffuse);
            assert_eq!(evidence.slot, SourceSlot::Hair);
            assert_eq!(evidence.preview, expected);
            assert!(!evidence.preview.review_label().contains("invalid"));
        }
    }

    #[test]
    fn a_mesh_or_full_body_outfit_never_establishes_an_editable_morph() {
        for body in [2, 5, 6, u32::MAX] {
            for geometry in [false, true] {
                for texture in [false, true] {
                    let evidence =
                        IndexedAppearanceEvidence::from_casp_index(body, geometry, texture);
                    assert_eq!(
                        evidence.morph_review_label(),
                        "Editable face/body controls are not verified by this metadata."
                    );
                }
            }
        }
    }
}
