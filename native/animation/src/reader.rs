// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{AnimationError, Result, invalid};

pub(crate) struct Reader<'a> {
    pub data: &'a [u8],
    pub at: usize,
}
impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| invalid(self.at, "extent overflow"))?;
        let value = self
            .data
            .get(self.at..end)
            .ok_or(AnimationError::Truncated {
                offset: self.at,
                needed: n,
            })?;
        self.at = end;
        Ok(value)
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
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32> {
        let at = self.at;
        let value = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        if !value.is_finite() {
            return Err(invalid(at, "non-finite float"));
        }
        Ok(value)
    }
    pub fn vector<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut out = [0.0; N];
        for value in &mut out {
            *value = self.f32()?;
        }
        Ok(out)
    }
    pub fn text(&mut self) -> Result<String> {
        let at = self.at;
        let n = self.u32()? as usize;
        if n > 4096 {
            return Err(invalid(at, "string length exceeds budget"));
        }
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| invalid(at, "invalid UTF-8 string"))
    }
}
