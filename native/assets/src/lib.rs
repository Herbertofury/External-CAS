// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only Sims 4 asset decoding. Original bytes remain the authority for saves.
pub mod casp;
pub mod dbpf;
pub mod geom;
pub mod gpu_vertex;
pub mod lrle;
pub mod rig;
pub mod scan;
pub mod thumbnail;
pub mod thumbnail_decode;

pub use casp::CasPart;
pub use dbpf::{PackageIndex, PackageReader, ReadLimits, ResourceEntry, ResourceKey};
pub use geom::{Geometry, Vertex};
pub use rig::Rig;
pub use scan::{Catalog, ScanProgress};
use std::{fmt, io};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Invalid(String),
    Unsupported(String),
    Changed,
    Cancelled,
    Budget { requested: u64, limit: u64 },
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
    pub(crate) fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported(message.into())
    }
}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Invalid(e) => write!(f, "Invalid asset: {e}"),
            Self::Unsupported(e) => write!(f, "Unsupported asset: {e}"),
            Self::Changed => write!(f, "Source changed; refresh the library before reading it"),
            Self::Cancelled => write!(f, "Asset operation cancelled"),
            Self::Budget { requested, limit } => write!(
                f,
                "Resource requires {requested} bytes; configured budget is {limit}"
            ),
        }
    }
}
impl std::error::Error for Error {}

pub(crate) struct Bytes<'a> {
    pub data: &'a [u8],
    pub at: usize,
    pub end: usize,
}
impl<'a> Bytes<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            at: 0,
            end: data.len(),
        }
    }
    pub fn remaining(&self) -> usize {
        self.end.saturating_sub(self.at)
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|n| *n <= self.end)
            .ok_or_else(|| Error::invalid("truncated fields"))?;
        let out = &self.data[self.at..end];
        self.at = end;
        Ok(out)
    }
    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
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
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32> {
        let v = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        if !v.is_finite() {
            return Err(Error::invalid("nonfinite float"));
        }
        Ok(v)
    }
    pub fn count(&mut self, minimum_bytes_per_item: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        if minimum_bytes_per_item > 0 && n > self.remaining() / minimum_bytes_per_item {
            return Err(Error::invalid("count exceeds payload extent"));
        }
        Ok(n)
    }
}
