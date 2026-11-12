// SPDX-License-Identifier: GPL-3.0-or-later
// Port of recovered casp.mjs; upstream format basis: C. Marinetti TS4CAS Tools.
use crate::{Bytes, Error, ResourceKey, Result};

#[derive(Clone, Debug, serde::Serialize)]
pub struct CaspReference {
    pub role: &'static str,
    pub table_index: u8,
    pub lod: Option<u8>,
    pub key: ResourceKey,
}
impl<'de> serde::Deserialize<'de> for CaspReference {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Owned {
            role: String,
            table_index: u8,
            lod: Option<u8>,
            key: ResourceKey,
        }
        let owned = Owned::deserialize(deserializer)?;
        const ROLES: &[&str] = &[
            "buff",
            "swatch",
            "hair-color",
            "naked",
            "parent",
            "mesh",
            "slot",
            "diffuse",
            "shadow",
            "region-map",
            "normal",
            "specular",
            "emission",
            "blend-geometry",
            "color-shift-mask",
            "new-image",
        ];
        let role = ROLES
            .iter()
            .copied()
            .find(|r| *r == owned.role)
            .ok_or_else(|| serde::de::Error::custom("unknown cached CASP role; rescan source"))?;
        Ok(Self {
            role,
            table_index: owned.table_index,
            lod: owned.lod,
            key: owned.key,
        })
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CaspCompatibility {
    /// Ordered raw words exactly as stored (v51+ can have any bounded count).
    pub exclude_part_flags: Vec<u64>,
    pub exclude_modifier_region_flags: u64,
    /// Present zero means the field exists but declares no alternative.
    pub opposite_gender_part: Option<u64>,
    pub fallback_part: Option<u64>,
    /// SimRipper HasMesh is lod_count>0, not a test of LOD0 references.
    pub lod_count: u8,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CasPart {
    pub version: u32,
    pub name: String,
    pub sort_priority: f32,
    pub swatch_order: u16,
    pub outfit_id: u32,
    pub material_hash: u32,
    pub flags1: u8,
    pub flags2: Option<u8>,
    pub layer_id: Option<u16>,
    pub tags: Vec<(u16, u32)>,
    pub price: u32,
    pub title_key: u32,
    pub description_key: u32,
    pub create_description_key: Option<u32>,
    pub texture_space: u8,
    pub body_type: u32,
    pub body_sub_type: u32,
    pub age: u8,
    pub gender: u8,
    pub age_gender_reserved: u16,
    pub species: Option<u32>,
    pub pack_id: Option<u16>,
    pub pack_flags: Option<u8>,
    pub colors: Vec<u32>,
    pub occult_mask: Option<u32>,
    pub sort_layer: i32,
    pub composition_method: u8,
    pub references: Vec<ResourceKey>,
    pub roles: Vec<CaspReference>,
    pub diffuse: Option<ResourceKey>,
    pub normal: Option<ResourceKey>,
    pub specular: Option<ResourceKey>,
    pub lod0_meshes: Vec<ResourceKey>,
    /// None is explicit unknown metadata from an older cache, never zero flags.
    #[serde(default)]
    pub compatibility: Option<CaspCompatibility>,
}
impl CasPart {
    pub fn disallow_opposite_gender(&self) -> bool {
        self.flags1 & 0x80 != 0
    }
    pub fn disallow_opposite_frame(&self) -> Option<bool> {
        self.flags2.map(|f| f & 1 != 0)
    }
    pub fn has_mesh(&self) -> Option<bool> {
        self.compatibility.as_ref().map(|c| c.lod_count > 0)
    }
    pub fn region_map(&self) -> Option<ResourceKey> {
        self.roles
            .iter()
            .find(|r| r.role == "region-map" && !r.key.is_empty())
            .map(|r| r.key)
    }
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Bytes::new(data);
        let version = r.u32()?;
        if !(26..=52).contains(&version) {
            return Err(Error::unsupported(format!("CASP version {version}")));
        }
        let table_at = (r.u32()? as usize)
            .checked_add(8)
            .ok_or_else(|| Error::invalid("CASP table offset"))?;
        if table_at < 12 || table_at >= data.len() {
            return Err(Error::invalid("CASP reference table extent"));
        }
        let mut table = Bytes {
            data,
            at: table_at,
            end: data.len(),
        };
        let count = table.u8()? as usize;
        if count.checked_mul(16) != Some(table.remaining()) {
            return Err(Error::invalid("CASP reference table length"));
        }
        let mut references = Vec::with_capacity(count);
        for _ in 0..count {
            let instance = table.u64()?;
            let group = table.u32()?;
            let ty = table.u32()?;
            references.push(ResourceKey::new(ty, group, instance));
        }
        r.end = table_at;
        if r.i32()? != 0 {
            return Err(Error::unsupported("CASP preset data"));
        }
        let mut name_len = 0u32;
        let mut terminated = false;
        for i in 0..5 {
            let b = r.u8()?;
            if i == 4 && b & 0xf0 != 0 {
                return Err(Error::invalid("CASP name length overflow"));
            }
            name_len |= ((b & 0x7f) as u32) << (7 * i);
            if b & 0x80 == 0 {
                terminated = true;
                break;
            }
        }
        if !terminated || name_len % 2 != 0 {
            return Err(Error::invalid("CASP UTF16 name length"));
        }
        let words = r
            .take(name_len as usize)?
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes(b.try_into().unwrap()));
        let name = char::decode_utf16(words)
            .collect::<std::result::Result<String, _>>()
            .map_err(|_| Error::invalid("CASP UTF16 name"))?;
        let sort_priority = r.f32()?;
        let swatch_order = r.u16()?;
        let outfit_id = r.u32()?;
        let material_hash = r.u32()?;
        let flags1 = r.u8()?;
        let flags2 = if version >= 39 { Some(r.u8()?) } else { None };
        let layer_id = if version >= 50 { Some(r.u16()?) } else { None };
        let mut exclude_part_flags = Vec::new();
        if version >= 51 {
            let n = r.count(8)?;
            for _ in 0..n {
                exclude_part_flags.push(r.u64()?);
            }
        } else {
            exclude_part_flags.push(r.u64()?);
            if version >= 41 {
                exclude_part_flags.push(r.u64()?);
            }
        }
        let exclude_modifier_region_flags = if version > 36 {
            r.u64()?
        } else {
            r.u32()? as u64
        };
        let n = r.count(if version >= 37 { 6 } else { 4 })?;
        let mut tags = Vec::with_capacity(n);
        for _ in 0..n {
            tags.push((
                r.u16()?,
                if version >= 37 {
                    r.u32()?
                } else {
                    r.u16()? as u32
                },
            ));
        }
        let price = r.u32()?;
        let title_key = r.u32()?;
        let description_key = r.u32()?;
        let create_description_key = if version > 42 { Some(r.u32()?) } else { None };
        let texture_space = r.u8()?;
        let body_type = r.u32()?;
        let body_sub_type = r.u32()?;
        let age = r.u8()?;
        let gender = r.u8()?;
        let age_gender_reserved = r.u16()?;
        let species = if version >= 32 { Some(r.u32()?) } else { None };
        let (pack_id, pack_flags) = if version >= 34 {
            let id = r.u16()?;
            let flags = r.u8()?;
            r.skip(9)?;
            (Some(id), Some(flags))
        } else {
            if r.u8()? != 0 {
                r.skip(1)?;
            }
            (None, None)
        };
        let count = r.u8()?;
        let mut colors = Vec::with_capacity(count as usize);
        for _ in 0..count {
            colors.push(r.u32()?);
        }
        let mut pending = Vec::new();
        pending.push(("buff", r.u8()?, None));
        pending.push(("swatch", r.u8()?, None));
        if version >= 28 {
            r.skip(8)?;
        }
        if version >= 30 && r.u8()? > 0 {
            r.skip(12)?;
        }
        let occult_mask = if version >= 31 { Some(r.u32()?) } else { None };
        if version >= 46 {
            r.skip(8)?;
        }
        let opposite_gender_part = if version >= 38 { Some(r.u64()?) } else { None };
        let fallback_part = if version >= 39 { Some(r.u64()?) } else { None };
        if version >= 44 {
            r.skip(44)?;
        }
        if version >= 46 {
            let n = r.u8()?;
            for _ in 0..n {
                pending.push(("hair-color", r.u8()?, None));
            }
        }
        pending.push(("naked", r.u8()?, None));
        pending.push(("parent", r.u8()?, None));
        let sort_layer = r.i32()?;
        let lod_count = r.u8()?;
        for _ in 0..lod_count {
            let lod = r.u8()?;
            r.skip(4)?;
            let assets = r.u8()?;
            r.skip(assets as usize * 12)?;
            let meshes = r.u8()?;
            for _ in 0..meshes {
                pending.push(("mesh", r.u8()?, Some(lod)));
            }
        }
        let slots = r.u8()?;
        for _ in 0..slots {
            pending.push(("slot", r.u8()?, None));
        }
        pending.push(("diffuse", r.u8()?, None));
        pending.push(("shadow", r.u8()?, None));
        let composition_method = r.u8()?;
        pending.push(("region-map", r.u8()?, None));
        let overrides = r.u8()?;
        r.skip(overrides as usize * 5)?;
        pending.push(("normal", r.u8()?, None));
        pending.push(("specular", r.u8()?, None));
        if version >= 27 {
            r.skip(4)?;
        }
        if version >= 29 {
            pending.push(("emission", r.u8()?, None));
        }
        if version >= 42 {
            pending.push(("blend-geometry", r.u8()?, None));
        }
        if version >= 49 {
            pending.push(("color-shift-mask", r.u8()?, None));
        }
        if version >= 52 {
            pending.push(("new-image", r.u8()?, None));
        }
        if r.at != table_at {
            return Err(Error::unsupported(
                "CASP field layout leaves unparsed bytes",
            ));
        }
        let roles = pending
            .into_iter()
            .map(|(role, table_index, lod)| {
                references
                    .get(table_index as usize)
                    .copied()
                    .map(|key| CaspReference {
                        role,
                        table_index,
                        lod,
                        key,
                    })
                    .ok_or_else(|| Error::invalid("CASP role index outside table"))
            })
            .collect::<Result<Vec<_>>>()?;
        let get = |role| {
            roles
                .iter()
                .find(|r| r.role == role && !r.key.is_empty())
                .map(|r| r.key)
        };
        let diffuse = get("diffuse");
        let normal = get("normal");
        let specular = get("specular");
        let lod0_meshes = roles
            .iter()
            .filter(|r| r.role == "mesh" && r.lod == Some(0) && !r.key.is_empty())
            .map(|r| r.key)
            .collect();
        Ok(Self {
            version,
            name,
            sort_priority,
            swatch_order,
            outfit_id,
            material_hash,
            flags1,
            flags2,
            layer_id,
            tags,
            price,
            title_key,
            description_key,
            create_description_key,
            texture_space,
            body_type,
            body_sub_type,
            age,
            gender,
            age_gender_reserved,
            species,
            pack_id,
            pack_flags,
            colors,
            occult_mask,
            sort_layer,
            composition_method,
            references,
            roles,
            diffuse,
            normal,
            specular,
            lod0_meshes,
            compatibility: Some(CaspCompatibility {
                exclude_part_flags,
                exclude_modifier_region_flags,
                opposite_gender_part,
                fallback_part,
                lod_count,
            }),
        })
    }
}
