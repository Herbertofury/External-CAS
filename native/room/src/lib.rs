// SPDX-License-Identifier: GPL-3.0-or-later
//! Native, source-preserving installed CAS room loading. Original selected bytes
//! remain authoritative; coordinate/material/camera qualification is explicit.
mod data;
mod load;
mod mesh;
mod mods;
mod projection;
mod rcol;
mod skin;
mod texture;
mod tuning;
mod world;
pub use data::*;
pub use load::*;
pub use mesh::*;
pub use mods::*;
pub use projection::*;
pub use rcol::*;
pub use skin::*;
use std::ops::Range;
pub use texture::*;
pub use tuning::*;
pub use world::*;
pub type Result<T> = std::result::Result<T, String>;
pub(crate) struct Bytes<'a> {
    pub data: &'a [u8],
    pub at: usize,
    pub base: usize,
}
#[cfg(test)]
mod tests;
impl<'a> Bytes<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            at: 0,
            base: 0,
        }
    }
    pub fn region(data: &'a [u8], range: Range<usize>) -> Result<Self> {
        let selected = data
            .get(range.clone())
            .ok_or("room chunk outside payload")?;
        Ok(Self {
            data: selected,
            at: 0,
            base: range.start,
        })
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.data.len())
            .ok_or("truncated room fields")?;
        let bytes = &self.data[self.at..end];
        self.at = end;
        Ok(bytes)
    }
    pub fn remaining(&self) -> usize {
        self.data.len() - self.at
    }
    pub fn absolute(&self) -> usize {
        self.base + self.at
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
    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32> {
        let value = f32::from_bits(self.u32()?);
        if !value.is_finite() {
            return Err("nonfinite required room coordinate".into());
        }
        Ok(value)
    }
    pub fn count(&mut self, min_bytes: usize) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 || min_bytes > 0 && n as usize > self.remaining() / min_bytes {
            return Err("room count exceeds source extent".into());
        }
        Ok(n as usize)
    }
}
