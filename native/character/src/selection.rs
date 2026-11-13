// SPDX-License-Identifier: GPL-3.0-or-later
// Explicit source-tool part/region selection from pinned PreviewControl.cs.
use crate::*;
#[derive(Clone, Debug)]
pub enum PartSelection {
    IncludedByReference,
    ExcludedByReference { word: usize, bit: u8 },
    Unqualified,
}
#[derive(Clone, Debug)]
pub struct PartSubstitution {
    pub original: ResourceKey,
    pub replacement: ResourceKey,
    pub reason: String,
}
pub(crate) fn substitution_target(
    casp: &CasPart,
    gender: Option<u32>,
    frame: Option<u32>,
) -> std::result::Result<Option<(ResourceKey, String)>, String> {
    let frame_flag = match casp.disallow_opposite_frame() {
        Some(v) => v,
        None if casp.version < 39 => false,
        None => return Err("CASP frame restriction metadata unavailable".into()),
    };
    if !casp.disallow_opposite_gender() && !frame_flag {
        return Ok(None);
    }
    let gender = gender
        .filter(|v| [0x1000, 0x2000].contains(v))
        .ok_or("Native sex unavailable for CASP restriction")?;
    let part_gender = (casp.gender as u32) << 8;
    let wrong_gender = casp.disallow_opposite_gender() && (gender & part_gender) == 0;
    let wrong_frame = if frame_flag {
        (frame.ok_or("Native frame unavailable for CASP restriction")? & part_gender) == 0
    } else {
        false
    };
    if !wrong_gender && !wrong_frame {
        return Ok(None);
    }
    let fields = casp
        .compatibility
        .as_ref()
        .ok_or("CASP alternate-part metadata unavailable")?;
    let (id, kind) = if let Some(id) = fields.opposite_gender_part.filter(|v| *v > 0) {
        (id, "opposite-gender part")
    } else if let Some(id) = fields.fallback_part.filter(|v| *v > 0) {
        (id, "fallback part")
    } else {
        return Err(
            "Restricted native CASP has no declared replacement; original retained for inspection"
                .into(),
        );
    };
    Ok(Some((
        ResourceKey::new(0x034aeecb, 0, id),
        format!(
            "reference {kind}: gender restriction={wrong_gender}, frame restriction={wrong_frame}"
        ),
    )))
}
pub fn excluded_by_reference_mask(
    body_type: u32,
    words: &[u64],
) -> std::result::Result<Option<(usize, u8)>, String> {
    if body_type >= 128 {
        return Err("BodyType exceeds the source-qualified two-word mask mapping".into());
    }
    let word = (body_type / 64) as usize;
    let bit = (body_type % 64) as u8;
    Ok(words
        .get(word)
        .filter(|v| **v & (1u64 << bit) != 0)
        .map(|_| (word, bit)))
}
pub(crate) fn apply_reference_visibility(parts: &mut [CharacterPart]) {
    let mut words = vec![0u64; 2];
    let mut complete = true;
    for part in parts.iter() {
        if let Some(c) = part.casp.as_ref().and_then(|c| c.compatibility.as_ref()) {
            for (i, w) in c.exclude_part_flags.iter().take(2).enumerate() {
                words[i] |= *w;
            }
        } else {
            complete = false;
        }
    }
    for part in parts.iter_mut() {
        part.reference_selection = match part
            .casp
            .as_ref()
            .map(|c| excluded_by_reference_mask(c.body_type, &words))
        {
            Some(Ok(Some((word, bit)))) => PartSelection::ExcludedByReference { word, bit },
            Some(Ok(None)) if complete && part.substitution_unresolved == false => {
                PartSelection::IncludedByReference
            }
            _ => PartSelection::Unqualified,
        };
    }
    let observed = parts
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            !matches!(
                p.reference_selection,
                PartSelection::ExcludedByReference { .. }
            )
        })
        .flat_map(|(pi, p)| {
            p.meshes.iter().enumerate().map(move |(mi, m)| {
                (
                    pi,
                    mi,
                    m.key,
                    m.regions.clone(),
                    p.region_map
                        .as_ref()
                        .map(|r| r.suffix.is_empty())
                        .unwrap_or(false),
                )
            })
        })
        .collect::<Vec<_>>();
    for part_index in 0..parts.len() {
        for mesh_index in 0..parts[part_index].meshes.len() {
            if matches!(
                parts[part_index].reference_selection,
                PartSelection::ExcludedByReference { .. }
            ) {
                continue;
            }
            let m = &parts[part_index].meshes[mesh_index];
            let own = m.regions.clone();
            let qualified = parts[part_index]
                .region_map
                .as_ref()
                .map(|r| r.suffix.is_empty())
                .unwrap_or(false);
            let mut hidden = None;
            if qualified {
                for r in &own {
                    if r.region == 0 {
                        continue;
                    }
                    for (pi, mi, key, regions, other_qualified) in &observed {
                        if (*pi == part_index && *mi == mesh_index) || !other_qualified {
                            continue;
                        }
                        if let Some(other) = regions
                            .iter()
                            .find(|o| o.region == r.region && o.layer > r.layer)
                        {
                            hidden = Some(MeshVisibility::HiddenByRegion {
                                occluder: *key,
                                region: r.region,
                                layer: other.layer,
                            });
                            break;
                        }
                    }
                    if hidden.is_some() {
                        break;
                    }
                }
            }
            parts[part_index].meshes[mesh_index].visibility = hidden.unwrap_or(if qualified {
                MeshVisibility::VisibleByRegionRule
            } else {
                MeshVisibility::InspectionOnly
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use external_cas_assets::casp::CaspCompatibility;
    fn casp() -> CasPart {
        CasPart {
            version: 52,
            name: String::new(),
            sort_priority: 0.0,
            swatch_order: 0,
            outfit_id: 0,
            material_hash: 0,
            flags1: 0,
            flags2: Some(0),
            layer_id: None,
            tags: vec![],
            price: 0,
            title_key: 0,
            description_key: 0,
            create_description_key: None,
            texture_space: 0,
            shared_uv_space: Some(0),
            region_overrides: Some(Vec::new()),
            body_type: 6,
            body_sub_type: 0,
            age: 56,
            gender: 16,
            age_gender_reserved: 0,
            species: Some(1),
            pack_id: None,
            pack_flags: None,
            colors: vec![],
            occult_mask: None,
            sort_layer: 0,
            composition_method: 0,
            references: vec![],
            roles: vec![],
            diffuse: None,
            normal: None,
            specular: None,
            lod0_meshes: vec![],
            compatibility: Some(CaspCompatibility {
                exclude_part_flags: vec![0, 0],
                exclude_modifier_region_flags: 0,
                opposite_gender_part: Some(10),
                fallback_part: Some(20),
                lod_count: 1,
            }),
            color_sliders: None,
        }
    }
    #[test]
    fn exact_native_sex_frame_restrictions_and_declared_replacement_order() {
        let mut c = casp();
        assert!(substitution_target(&c, None, None).unwrap().is_none());
        c.flags1 = 0x80;
        let (k, _) = substitution_target(&c, Some(0x2000), Some(0x2000))
            .unwrap()
            .unwrap();
        assert_eq!(k, ResourceKey::new(0x034aeecb, 0, 10));
        c.compatibility.as_mut().unwrap().opposite_gender_part = Some(0);
        assert_eq!(
            substitution_target(&c, Some(0x2000), Some(0x2000))
                .unwrap()
                .unwrap()
                .0
                .instance_id,
            20
        );
        assert!(substitution_target(&c, None, Some(0x2000)).is_err());
        c.flags1 = 0;
        c.flags2 = Some(1);
        assert!(substitution_target(&c, Some(0x1000), Some(0x2000))
            .unwrap()
            .is_some());
        assert!(substitution_target(&c, Some(0x1000), None).is_err());
        c.version = 38;
        c.flags2 = None;
        assert!(substitution_target(&c, None, None).unwrap().is_none());
    }
    #[test]
    fn native_mask_bit63_and_word_boundary_preserved() {
        let masks = [1u64 << 63, (1u64 << 0) | (1u64 << 16) | (1u64 << 63)];
        for (body, expected) in [
            (62, None),
            (63, Some((0, 63))),
            (64, Some((1, 0))),
            (65, None),
            (80, Some((1, 16))),
            (127, Some((1, 63))),
        ] {
            assert_eq!(excluded_by_reference_mask(body, &masks).unwrap(), expected);
        }
        assert!(excluded_by_reference_mask(128, &[0, 0, 1]).is_err());
    }
}
