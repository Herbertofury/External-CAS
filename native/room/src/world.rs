// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Bytes, Result};
use external_cas_assets::ResourceKey;
use serde::Serialize;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize)]
pub struct WorldObject {
    pub object_id: Option<u64>,
    pub definition_instance: Option<u64>,
    pub model_key: Option<ResourceKey>,
    pub position_bits: Option<[u32; 3]>,
    pub rotation_bits: Option<[u32; 4]>,
    pub scale_bits: Option<u32>,
    pub opaque_fields: Vec<([u8; 4], Vec<u8>)>,
}
impl WorldObject {
    pub fn position(&self) -> Option<[f32; 3]> {
        self.position_bits.map(|v| v.map(f32::from_bits))
    }
    pub fn rotation(&self) -> Option<[f32; 4]> {
        self.rotation_bits.map(|v| v.map(f32::from_bits))
    }
    pub fn scale(&self) -> Option<f32> {
        self.scale_bits.map(f32::from_bits)
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct WorldData {
    pub objects: Vec<WorldObject>,
    pub opaque_chunks: Vec<([u8; 4], Vec<u8>)>,
}
fn chunks<'a>(data: &'a [u8]) -> Result<Vec<([u8; 4], &'a [u8])>> {
    let mut r = Bytes::new(data);
    let mut out = Vec::new();
    while r.remaining() > 0 {
        let tag = r.take(4)?.try_into().unwrap();
        let n = r.u32()? as usize;
        out.push((tag, r.take(n)?));
    }
    Ok(out)
}
pub fn parse_world_data(data: &[u8]) -> Result<WorldData> {
    if data.len() > 16 * 1024 * 1024 {
        return Err("world data budget".into());
    }
    let top = chunks(data)?;
    if top.len() != 1 || &top[0].0 != b"OMGS" {
        return Err("unsupported world data root".into());
    }
    let mut out = WorldData {
        objects: Vec::new(),
        opaque_chunks: Vec::new(),
    };
    for (tag, manager) in chunks(top[0].1)? {
        if &tag != b"OMGR" {
            out.opaque_chunks.push((tag, manager.to_vec()));
            continue;
        }
        for (tag, object) in chunks(manager)? {
            if &tag != b"OBJ " {
                out.opaque_chunks.push((tag, object.to_vec()));
                continue;
            }
            let mut obj = WorldObject {
                object_id: None,
                definition_instance: None,
                model_key: None,
                position_bits: None,
                rotation_bits: None,
                scale_bits: None,
                opaque_fields: Vec::new(),
            };
            for (tag, body) in chunks(object)? {
                let mut b = Bytes::new(body);
                match &tag {
                    b"ID  " if body.len() == 8 => {
                        if obj.object_id.replace(b.u64()?).is_some() {
                            return Err("duplicate world object id".into());
                        }
                    }
                    b"DGUD" if body.len() == 8 => {
                        if obj.definition_instance.replace(b.u64()?).is_some() {
                            return Err("duplicate world definition".into());
                        }
                    }
                    b"POS " if body.len() == 12 => {
                        let values = [b.u32()?, b.u32()?, b.u32()?];
                        if obj.position_bits.replace(values).is_some() {
                            return Err("duplicate world position".into());
                        }
                    }
                    b"ROT " if body.len() == 16 => {
                        let values = [b.u32()?, b.u32()?, b.u32()?, b.u32()?];
                        if obj.rotation_bits.replace(values).is_some() {
                            return Err("duplicate world rotation".into());
                        }
                    }
                    b"SCAL" if body.len() == 4 => {
                        if obj.scale_bits.replace(b.u32()?).is_some() {
                            return Err("duplicate world scale".into());
                        }
                    }
                    b"MODL" if body.len() == 16 => {
                        let instance = b.u64()?;
                        let ty = b.u32()?;
                        let group = b.u32()?;
                        if obj
                            .model_key
                            .replace(ResourceKey::new(ty, group, instance))
                            .is_some()
                        {
                            return Err("duplicate world model".into());
                        }
                    }
                    _ => obj.opaque_fields.push((tag, body.to_vec())),
                }
            }
            out.objects.push(obj);
        }
    }
    Ok(out)
}
#[derive(Debug, Clone, Serialize)]
pub struct ObjectDefinition {
    pub version: u16,
    pub name: Option<String>,
    pub links: BTreeMap<u32, Vec<ResourceKey>>,
    pub properties: Vec<(u32, u32)>,
}
impl ObjectDefinition {
    pub fn models(&self) -> Option<&[ResourceKey]> {
        self.links.get(&0x8d20acc6).map(Vec::as_slice)
    }
}
pub fn parse_object_definition(data: &[u8]) -> Result<ObjectDefinition> {
    if data.len() > 1024 * 1024 {
        return Err("object definition budget".into());
    }
    let mut r = Bytes::new(data);
    let version = r.u16()?;
    if version != 2 {
        return Err(format!("unsupported object definition version {version}"));
    }
    let table = r.u32()? as usize;
    if table < 6 {
        return Err("object table intersects header".into());
    }
    let mut t = Bytes::region(data, table..data.len())?;
    let count = t.u16()? as usize;
    if count > t.remaining() / 8 {
        return Err("object property count outside source".into());
    }
    let mut name = None;
    let mut links = BTreeMap::new();
    let mut properties = Vec::new();
    for _ in 0..count {
        let ty = t.u32()?;
        let offset = t.u32()?;
        let mut field = Bytes::region(data, offset as usize..data.len())?;
        properties.push((ty, offset));
        if ty == 0xe7f07786 {
            if name.is_some() {
                return Err("duplicate room object name".into());
            }
            let n = field.u32()? as usize;
            name = Some(
                std::str::from_utf8(field.take(n)?)
                    .map_err(|_| "non-UTF8 room object name")?
                    .to_owned(),
            );
        }
        if matches!(
            ty,
            0x8d20acc6 | 0x6c737ad8 | 0xcaded888 | 0xe206ae4f | 0x8a85aff3
        ) {
            if links.contains_key(&ty) {
                return Err("duplicate room object links".into());
            }
            let size = field.i32()?;
            if size < 0 || size % 4 != 0 {
                return Err("invalid room object link count".into());
            }
            let n = size as usize / 4;
            if n > field.remaining() / 16 {
                return Err("room object links outside source".into());
            }
            let mut keys = Vec::new();
            for _ in 0..n {
                let hi = field.u32()?;
                let lo = field.u32()?;
                let kind = field.u32()?;
                let group = field.u32()?;
                keys.push(ResourceKey::new(
                    kind,
                    group,
                    (u64::from(hi) << 32) | u64::from(lo),
                ));
            }
            links.insert(ty, keys);
        }
    }
    Ok(ObjectDefinition {
        version,
        name,
        links,
        properties,
    })
}
