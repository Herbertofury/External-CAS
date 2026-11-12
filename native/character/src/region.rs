// SPDX-License-Identifier: GPL-3.0-or-later
// Layout/query semantics from pinned SimRipper RegionMap.cs.
use crate::{CharacterError, Result};
use external_cas_assets::ResourceKey;
#[derive(Clone, Debug)]
pub struct RegionBlock {
    pub region: u32,
    pub layer: f32,
    pub is_replacement: u8,
    pub meshes: Vec<ResourceKey>,
}
#[derive(Clone, Debug)]
pub struct RegionMembership {
    pub region: u32,
    pub layer: f32,
    pub is_replacement: u8,
}
#[derive(Clone, Debug)]
pub struct RegionMap {
    pub context_version: u32,
    pub version: u32,
    pub public_keys: Vec<ResourceKey>,
    pub external_keys: Vec<ResourceKey>,
    pub delayed_keys: Vec<ResourceKey>,
    pub objects: Vec<(u32, u32)>,
    pub blocks: Vec<RegionBlock>,
    pub raw: Vec<u8>,
    pub suffix: Vec<u8>,
}
struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&v| v <= self.b.len())
            .ok_or_else(|| CharacterError::Invalid("Truncated RMAP".into()))?;
        let v = &self.b[self.at..end];
        self.at = end;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn count(&mut self, stride: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        if n > self.b.len().saturating_sub(self.at) / stride {
            return Err(CharacterError::Invalid("RMAP count extent".into()));
        }
        Ok(n)
    }
    fn key(&mut self) -> Result<ResourceKey> {
        let instance = u64::from_le_bytes(self.take(8)?.try_into().unwrap());
        let ty = self.u32()?;
        let group = self.u32()?;
        Ok(ResourceKey::new(ty, group, instance))
    }
}
impl RegionMap {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader { b: bytes, at: 0 };
        let context_version = r.u32()?;
        let counts = [
            r.u32()? as usize,
            r.u32()? as usize,
            r.u32()? as usize,
            r.u32()? as usize,
        ];
        if context_version != 3 || counts[3] != 1 {
            return Err(CharacterError::Unsupported(
                "RMAP requires context3/one object".into(),
            ));
        }
        let mut lists = vec![];
        for n in &counts[..3] {
            if *n > r.b.len().saturating_sub(r.at) / 16 {
                return Err(CharacterError::Invalid("RMAP key extent".into()));
            }
            let mut keys = vec![];
            for _ in 0..*n {
                keys.push(r.key()?);
            }
            lists.push(keys);
        }
        let mut objects = vec![];
        for _ in 0..counts[3] {
            objects.push((r.u32()?, r.u32()?));
        }
        if objects[0].0 as usize != r.at
            || (objects[0].0 as usize)
                .checked_add(objects[0].1 as usize)
                .is_none_or(|v| v > bytes.len())
        {
            return Err(CharacterError::Invalid("RMAP object extent".into()));
        }
        let version = r.u32()?;
        if version != 1 {
            return Err(CharacterError::Unsupported(format!(
                "RMAP version {version}"
            )));
        }
        let n = r.count(13)?;
        let mut blocks = vec![];
        for _ in 0..n {
            let region = r.u32()?;
            let layer = f32::from_le_bytes(r.take(4)?.try_into().unwrap());
            if !layer.is_finite() {
                return Err(CharacterError::Invalid("Nonfinite RMAP layer".into()));
            }
            let is_replacement = r.take(1)?[0];
            let n = r.count(16)?;
            let mut meshes = vec![];
            for _ in 0..n {
                meshes.push(r.key()?);
            }
            blocks.push(RegionBlock {
                region,
                layer,
                is_replacement,
                meshes,
            });
        }
        Ok(Self {
            context_version,
            version,
            public_keys: lists.remove(0),
            external_keys: lists.remove(0),
            delayed_keys: lists.remove(0),
            objects,
            blocks,
            raw: bytes.to_vec(),
            suffix: bytes[r.at..].to_vec(),
        })
    }
    /// Source returns the Base region alone as soon as it finds one. Other repeated
    /// memberships remain repeated and preserve source wire order.
    pub fn regions_for(&self, key: ResourceKey) -> Vec<RegionMembership> {
        let mut out = vec![];
        for b in &self.blocks {
            for mesh in &b.meshes {
                if *mesh == key {
                    if b.region == 0 {
                        return vec![RegionMembership {
                            region: 0,
                            layer: 0.0,
                            is_replacement: b.is_replacement,
                        }];
                    }
                    out.push(RegionMembership {
                        region: b.region,
                        layer: b.layer,
                        is_replacement: b.is_replacement,
                    });
                }
            }
        }
        out
    }
    pub fn max_layer(&self, region: u32) -> f32 {
        self.blocks
            .iter()
            .filter(|b| b.region == region)
            .map(|b| b.layer)
            .fold(-1.0, f32::max)
    }
}
