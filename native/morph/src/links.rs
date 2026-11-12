// SPDX-License-Identifier: GPL-3.0-or-later
// Native RCOL SCULPT/SMOD/BOND field layouts from the pinned SimRipper source.
use super::bytes::*;
use super::*;
use external_cas_assets::ResourceKey;
#[derive(Clone, Debug)]
pub struct Sculpt {
    pub version: u32,
    pub age_gender: u32,
    pub region: u32,
    pub subregion: Option<u32>,
    pub link_tag: u32,
    pub bgeo: Vec<ResourceKey>,
    pub texture: ResourceKey,
    pub specular: Option<ResourceKey>,
    pub bump: Option<ResourceKey>,
    pub shape: ResourceKey,
    pub normal: ResourceKey,
    pub bond: Option<ResourceKey>,
    pub raw: Vec<u8>,
    pub unparsed_suffix: Vec<u8>,
}
impl Sculpt {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let mut r = Reader::new(raw);
        let h = rcol(&mut r)?;
        let version = r.u32()?;
        if ![0x60, 0x80, 0x90, 0xa0, 0xb0].contains(&version) {
            return Err(MorphError::Unsupported(format!(
                "SCULPT version {version:X}"
            )));
        }
        let age_gender = r.u32()?;
        let region = r.u32()?;
        let subregion = if version > 0x60 { Some(r.u32()?) } else { None };
        let link_tag = r.u32()?;
        let texture = r.key()?;
        let (specular, bump) = if version > 0x60 {
            (Some(r.key()?), Some(r.key()?))
        } else {
            (None, None)
        };
        r.u8()?;
        let shape = r.key()?;
        let normal = r.key()?;
        let bond = if version > 0x60 {
            let key = r.key()?;
            r.u32()?;
            Some(key)
        } else {
            None
        };
        Ok(Self {
            version,
            age_gender,
            region,
            subregion,
            link_tag,
            bgeo: h.delayed,
            texture,
            specular,
            bump,
            shape,
            normal,
            bond,
            raw: raw.to_vec(),
            unparsed_suffix: raw[r.at..].to_vec(),
        })
    }
}
#[derive(Clone, Debug)]
pub struct SimModifier {
    pub version: u32,
    pub age_gender: u32,
    pub region: u32,
    pub subregion: Option<u32>,
    pub link_tag: u32,
    pub bgeo: Vec<ResourceKey>,
    pub bond: ResourceKey,
    pub shape: ResourceKey,
    pub normal: ResourceKey,
    pub bones: Vec<(u32, f32)>,
    pub raw: Vec<u8>,
    pub unparsed_suffix: Vec<u8>,
}
impl SimModifier {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let mut r = Reader::new(raw);
        let h = rcol(&mut r)?;
        let version = r.u32()?;
        if ![0x70, 0x80, 0x90].contains(&version) {
            return Err(MorphError::Unsupported(format!("SMOD version {version:X}")));
        }
        let age_gender = r.u32()?;
        let region = r.u32()?;
        let subregion = if version >= 144 { Some(r.u32()?) } else { None };
        let link_tag = r.u32()?;
        let bond = r.key()?;
        let shape = r.key()?;
        let normal = r.key()?;
        let count = r.count(8)?;
        let mut bones = Vec::new();
        for _ in 0..count {
            bones.push((r.u32()?, r.f32()?));
        }
        Ok(Self {
            version,
            age_gender,
            region,
            subregion,
            link_tag,
            bgeo: h.delayed,
            bond,
            shape,
            normal,
            bones,
            raw: raw.to_vec(),
            unparsed_suffix: raw[r.at..].to_vec(),
        })
    }
}
#[derive(Clone, Debug)]
pub struct BoneAdjustment {
    pub slot_hash: u32,
    pub offset: [f32; 3],
    pub scale: [f32; 3],
    pub quaternion: [f32; 4],
}
#[derive(Clone, Debug)]
pub struct Bond {
    pub version: u32,
    pub adjustments: Vec<BoneAdjustment>,
    pub raw: Vec<u8>,
    pub unparsed_suffix: Vec<u8>,
}
impl Bond {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let mut r = Reader::new(raw);
        rcol_private(&mut r, true)?;
        let version = r.u32()?;
        if version != 1 {
            return Err(MorphError::Unsupported(format!("BOND version {version}")));
        }
        let count = r.count(44)?;
        let mut adjustments = Vec::new();
        for _ in 0..count {
            adjustments.push(BoneAdjustment {
                slot_hash: r.u32()?,
                offset: r.vec3()?,
                scale: r.vec3()?,
                quaternion: [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
            });
        }
        Ok(Self {
            version,
            adjustments,
            raw: raw.to_vec(),
            unparsed_suffix: raw[r.at..].to_vec(),
        })
    }
}
