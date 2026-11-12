use super::*;
use external_cas_assets::ResourceKey;
pub struct Reader<'a> {
    pub raw: &'a [u8],
    pub at: usize,
}
impl<'a> Reader<'a> {
    pub fn new(raw: &'a [u8]) -> Self {
        Self { raw, at: 0 }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&n| n <= self.raw.len())
            .ok_or(MorphError::Truncated)?;
        let b = &self.raw[self.at..end];
        self.at = end;
        Ok(b)
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32> {
        let f = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        if !f.is_finite() {
            return Err(MorphError::Invalid("Non-finite morph field".into()));
        }
        Ok(f)
    }
    pub fn vec3(&mut self) -> Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
    pub fn key(&mut self) -> Result<ResourceKey> {
        let id = self.u64()?;
        let ty = self.u32()?;
        let group = self.u32()?;
        Ok(ResourceKey::new(ty, group, id))
    }
    pub fn count(&mut self, stride: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        if stride > 0 && n > self.raw.len().saturating_sub(self.at) / stride {
            return Err(MorphError::Truncated);
        }
        Ok(n)
    }
}
#[derive(Clone, Debug)]
#[allow(dead_code)] // Full RCOL observations retained for format diagnostics.
pub struct RcolHeader {
    pub context_version: u32,
    pub public: Vec<ResourceKey>,
    pub external: Vec<ResourceKey>,
    pub delayed: Vec<ResourceKey>,
    pub objects: Vec<(u32, u32)>,
}
pub fn rcol(r: &mut Reader<'_>) -> Result<RcolHeader> {
    rcol_private(r, false)
}
pub fn rcol_private(r: &mut Reader<'_>, has_private: bool) -> Result<RcolHeader> {
    let context_version = r.u32()?;
    let counts = [
        r.u32()? as usize,
        r.u32()? as usize,
        r.u32()? as usize,
        r.u32()? as usize,
    ];
    let private = if has_private {
        counts[3]
            .checked_sub(counts[0])
            .ok_or_else(|| MorphError::Invalid("RCOL public count exceeds objects".into()))?
    } else {
        0
    };
    let keys = counts[0]
        .checked_add(private)
        .and_then(|n| n.checked_add(counts[1]))
        .and_then(|n| n.checked_add(counts[2]))
        .and_then(|n| n.checked_mul(16))
        .and_then(|n| n.checked_add(counts[3].checked_mul(8)?))
        .ok_or_else(|| MorphError::Invalid("RCOL count overflow".into()))?;
    if keys > r.raw.len().saturating_sub(r.at) {
        return Err(MorphError::Truncated);
    }
    let mut lists = Vec::new();
    for (i, count) in counts[..3].iter().enumerate() {
        let mut v = Vec::new();
        for _ in 0..*count {
            v.push(r.key()?);
        }
        lists.push(v);
        if i == 0 {
            for _ in 0..private {
                r.key()?;
            }
        }
    }
    let mut objects = Vec::new();
    for _ in 0..counts[3] {
        objects.push((r.u32()?, r.u32()?));
    }
    if context_version != 3 || objects.len() != 1 {
        return Err(MorphError::Unsupported(
            "Morph requires RCOL context3 and one object".into(),
        ));
    }
    if objects[0].0 as usize != r.at || objects[0].0 as usize + objects[0].1 as usize > r.raw.len()
    {
        return Err(MorphError::Invalid("RCOL object extent differs".into()));
    }
    Ok(RcolHeader {
        context_version,
        public: lists.remove(0),
        external: lists.remove(0),
        delayed: lists.remove(0),
        objects,
    })
}
