// SPDX-License-Identifier: GPL-3.0-or-later
// Based on CmarNYC-Tools/TS4SimRipper TONE.cs. Unknown suffixes are retained.
use std::fmt;
#[derive(Clone, Debug, PartialEq)]
pub enum ToneError {
    UnsupportedVersion(u32),
    Truncated,
    Invalid(&'static str),
}
impl fmt::Display for ToneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ToneError {}
#[derive(Clone, Debug, PartialEq)]
pub struct ToneSkinSet {
    pub texture_instance: u64,
    pub overlay_instance: u64,
    pub overlay_multiplier: f32,
    pub makeup_opacity: f32,
    pub makeup_opacity_2: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ToneOverlay {
    pub age_gender_flags: u32,
    pub texture_instance: u64,
}
#[derive(Clone, Debug)]
pub struct Tone {
    pub version: u32,
    pub skin_sets: Vec<ToneSkinSet>,
    pub overlays: Vec<ToneOverlay>,
    pub saturation: u16,
    pub hue: u16,
    pub opacity: u32,
    pub tags: Vec<(u16, u32)>,
    pub colors: Vec<u32>,
    pub sort_order: f32,
    pub tuning_instance: Option<u64>,
    pub panel: Option<u16>,
    pub slider: Option<[f32; 3]>,
    pub uninterpreted_suffix: Vec<u8>,
    pub raw: Vec<u8>,
}
struct Reader<'a> {
    raw: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ToneError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&e| e <= self.raw.len())
            .ok_or(ToneError::Truncated)?;
        let b = &self.raw[self.at..end];
        self.at = end;
        Ok(b)
    }
    fn u8(&mut self) -> Result<u8, ToneError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, ToneError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, ToneError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, ToneError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, ToneError> {
        let f = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        if !f.is_finite() {
            return Err(ToneError::Invalid("non-finite TONE float"));
        }
        Ok(f)
    }
    fn count(&mut self) -> Result<usize, ToneError> {
        let n = self.u32()?;
        if n > 4096 {
            return Err(ToneError::Invalid("TONE count exceeds bound"));
        }
        Ok(n as usize)
    }
}
impl Tone {
    pub fn parse(raw: &[u8]) -> Result<Self, ToneError> {
        if raw.len() > 1024 * 1024 {
            return Err(ToneError::Invalid("TONE exceeds bound"));
        }
        let mut r = Reader { raw, at: 0 };
        let version = r.u32()?;
        // Actual native reader 00398060 has a distinct historical v9 skin-set
        // layout. Do not parse it as the legacy v6-v8 layout merely because an
        // old reference reader did so. ScenePlan retains the original bytes.
        if !(6..=12).contains(&version) || version == 9 {
            return Err(ToneError::UnsupportedVersion(version));
        }
        let mut skin_sets = Vec::new();
        if version >= 10 {
            let n = r.u8()? as usize;
            for _ in 0..n {
                skin_sets.push(ToneSkinSet {
                    texture_instance: r.u64()?,
                    overlay_instance: r.u64()?,
                    overlay_multiplier: r.f32()?,
                    makeup_opacity: r.f32()?,
                    makeup_opacity_2: r.f32()?,
                });
            }
        } else {
            skin_sets.push(ToneSkinSet {
                texture_instance: r.u64()?,
                overlay_instance: 0,
                overlay_multiplier: 1.0,
                makeup_opacity: 0.0,
                makeup_opacity_2: 0.0,
            });
        }
        let n = r.count()?;
        let mut overlays = Vec::new();
        for _ in 0..n {
            overlays.push(ToneOverlay {
                age_gender_flags: r.u32()?,
                texture_instance: r.u64()?,
            });
        }
        let saturation = r.u16()?;
        let hue = r.u16()?;
        let opacity = r.u32()?;
        let n = r.count()?;
        let mut tags = Vec::new();
        for _ in 0..n {
            let category = r.u16()?;
            let value = if version >= 7 {
                r.u32()?
            } else {
                r.u16()? as u32
            };
            tags.push((category, value));
        }
        if version < 10 {
            skin_sets[0].makeup_opacity = r.f32()?;
        }
        let n = r.u8()? as usize;
        let mut colors = Vec::new();
        for _ in 0..n {
            colors.push(r.u32()?);
        }
        let sort_order = r.f32()?;
        if version < 10 {
            skin_sets[0].makeup_opacity_2 = r.f32()?;
        }
        let tuning_instance = if version >= 8 { Some(r.u64()?) } else { None };
        let (panel, slider) = if version > 10 {
            (Some(r.u16()?), Some([r.f32()?, r.f32()?, r.f32()?]))
        } else {
            (None, None)
        };
        Ok(Self {
            version,
            skin_sets,
            overlays,
            saturation,
            hue,
            opacity,
            tags,
            colors,
            sort_order,
            tuning_instance,
            panel,
            slider,
            uninterpreted_suffix: raw[r.at..].to_vec(),
            raw: raw.to_vec(),
        })
    }
}
