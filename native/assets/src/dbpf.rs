// SPDX-License-Identifier: GPL-3.0-or-later
//! Streaming index access; each payload read is tied to an observed file/index.
use crate::{Bytes, Error, Result};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    fs::{File, Metadata},
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct ResourceKey {
    pub type_id: u32,
    pub group_id: u32,
    pub instance_id: u64,
}
impl ResourceKey {
    pub const fn new(type_id: u32, group_id: u32, instance_id: u64) -> Self {
        Self {
            type_id,
            group_id,
            instance_id,
        }
    }
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
}
impl fmt::Display for ResourceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:08X}:{:08X}:{:016X}",
            self.type_id, self.group_id, self.instance_id
        )
    }
}
impl std::str::FromStr for ResourceKey {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let mut p = s.split(':');
        let mut next = |len| -> Result<&str> {
            p.next()
                .filter(|s| s.len() == len && s.bytes().all(|c| c.is_ascii_hexdigit()))
                .ok_or_else(|| Error::invalid("TGI key"))
        };
        let ty = u32::from_str_radix(next(8)?, 16).map_err(|_| Error::invalid("TGI type"))?;
        let gr = u32::from_str_radix(next(8)?, 16).map_err(|_| Error::invalid("TGI group"))?;
        let instance =
            u64::from_str_radix(next(16)?, 16).map_err(|_| Error::invalid("TGI instance"))?;
        if p.next().is_some() {
            return Err(Error::invalid("TGI trailing fields"));
        }
        Ok(Self::new(ty, gr, instance))
    }
}
pub const CASP: u32 = 0x034a_eecb;
pub const GEOM: u32 = 0x015a_1849;
pub const RIG: u32 = 0x8eaf_13de;
pub const CLIP: u32 = 0x6b20_c4f3;
pub const THUMBNAIL: u32 = 0x3c1a_f1f2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceEntry {
    pub key: ResourceKey,
    pub ordinal: u32,
    pub offset: u64,
    pub stored_size: u32,
    pub memory_size: u32,
    pub compression: u16,
    /// Trailing DBPF index word (s4pi calls this Unknown2). The legacy public
    /// name is retained; zero is used by real packages as well as one.
    pub committed: u16,
    pub extended: bool,
}
impl ResourceEntry {
    pub fn deleted(&self) -> bool {
        self.compression == 0xffe0
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
}
impl FileStamp {
    fn from(m: &Metadata) -> Self {
        Self {
            len: m.len(),
            modified: m.modified().ok(),
            created: m.created().ok(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ReadLimits {
    pub max_resource_bytes: u64,
}
impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_resource_bytes: 512 * 1024 * 1024,
        }
    }
}
impl ReadLimits {
    pub fn check(&self, n: u64) -> Result<()> {
        if n > self.max_resource_bytes {
            Err(Error::Budget {
                requested: n,
                limit: self.max_resource_bytes,
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug)]
pub struct PackageIndex {
    pub path: PathBuf,
    pub entries: Vec<ResourceEntry>,
    pub index_sha256: [u8; 32],
    pub file_size: u64,
    stamp: FileStamp,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageFingerprint {
    pub path: PathBuf,
    pub file_size: u64,
    pub index_sha256: [u8; 32],
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub casp_count: usize,
}
impl PackageIndex {
    pub fn fingerprint(&self) -> PackageFingerprint {
        PackageFingerprint {
            path: self.path.clone(),
            file_size: self.file_size,
            index_sha256: self.index_sha256,
            modified: self.stamp.modified,
            created: self.stamp.created,
            casp_count: self
                .entries
                .iter()
                .filter(|e| e.key.type_id == CASP && !e.deleted())
                .count(),
        }
    }
    pub fn revalidate(&self, cancel: &AtomicBool) -> Result<()> {
        self.reader_cancellable(cancel).map(|_| ())
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_cancellable(path, &AtomicBool::new(false))
    }
    pub fn open_cancellable(path: impl AsRef<Path>, cancel: &AtomicBool) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut f = File::open(&path)?;
        Self::from_file(path, &mut f, cancel)
    }
    fn from_file(path: PathBuf, f: &mut File, cancel: &AtomicBool) -> Result<Self> {
        let before = f.metadata()?;
        if !before.is_file() || before.len() < 96 {
            return Err(Error::invalid("DBPF file extent"));
        }
        let stamp = FileStamp::from(&before);
        let mut header = [0u8; 96];
        f.rewind()?;
        f.read_exact(&mut header)?;
        let mut r = Bytes::new(&header);
        if r.take(4)? != b"DBPF" {
            return Err(Error::invalid("DBPF magic"));
        }
        if r.u32()? != 2 || r.u32()? != 1 {
            return Err(Error::unsupported("DBPF version (requires 2.1)"));
        }
        let word = |at| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
        let count = word(36);
        let index_size = word(44) as u64;
        let index_offset = if word(64) == 0 { word(40) } else { word(64) } as u64;
        let mut digest = Sha256::new();
        digest.update(header);
        if index_size == 0 && count == 0 {
            return Ok(Self {
                path,
                entries: Vec::new(),
                index_sha256: digest.finalize().into(),
                file_size: before.len(),
                stamp,
            });
        }
        if index_offset < 96
            || index_size < 4
            || index_offset
                .checked_add(index_size)
                .is_none_or(|e| e > before.len())
        {
            return Err(Error::invalid("DBPF index extent"));
        }
        // Index bytes are streamed, never allocate from a declared index length.
        f.seek(SeekFrom::Start(index_offset))?;
        let mut stream = BufReader::with_capacity(256 * 1024, &mut *f);
        let mut remaining = index_size;
        let mut get = |n: usize| -> Result<[u8; 4]> {
            if remaining < n as u64 {
                return Err(Error::invalid("truncated DBPF index"));
            }
            let mut b = [0; 4];
            stream.read_exact(&mut b[..n])?;
            digest.update(&b[..n]);
            remaining -= n as u64;
            Ok(b)
        };
        let flags = u32::from_le_bytes(get(4)?);
        if flags & !7 != 0 {
            return Err(Error::unsupported(format!("DBPF index flags {flags:#X}")));
        }
        let mut shared = [None; 3];
        for i in 0..3 {
            if flags & (1 << i) != 0 {
                shared[i] = Some(u32::from_le_bytes(get(4)?));
            }
        }
        // The actual count must fit the smallest possible entries in this file.
        let minimum = 16 + 4 * (3 - flags.count_ones()) as u64;
        if count as u64 > (index_size - 4 - 4 * flags.count_ones() as u64) / minimum {
            return Err(Error::invalid("DBPF entry count exceeds index"));
        }
        let mut entries = Vec::new();
        entries
            .try_reserve(count as usize)
            .map_err(|_| Error::invalid("DBPF entry allocation"))?;
        for ordinal in 0..count {
            if ordinal % 1024 == 0 && cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let ty = match shared[0] {
                Some(x) => x,
                None => u32::from_le_bytes(get(4)?),
            };
            let group = match shared[1] {
                Some(x) => x,
                None => u32::from_le_bytes(get(4)?),
            };
            let hi = match shared[2] {
                Some(x) => x,
                None => u32::from_le_bytes(get(4)?),
            };
            let lo = u32::from_le_bytes(get(4)?);
            let offset = u32::from_le_bytes(get(4)?) as u64;
            let size = u32::from_le_bytes(get(4)?);
            let memory_size = u32::from_le_bytes(get(4)?);
            let extended = size & 0x80000000 != 0;
            let stored_size = size & 0x7fffffff;
            let (compression, committed) = if extended {
                (
                    u16::from_le_bytes(get(2)?[..2].try_into().unwrap()),
                    u16::from_le_bytes(get(2)?[..2].try_into().unwrap()),
                )
            } else {
                (0, 1)
            };
            if compression != 0xffe0
                && offset
                    .checked_add(stored_size as u64)
                    .is_none_or(|e| e > before.len())
            {
                return Err(Error::invalid("DBPF payload outside file"));
            }
            entries.push(ResourceEntry {
                key: ResourceKey::new(ty, group, ((hi as u64) << 32) | lo as u64),
                ordinal,
                offset,
                stored_size,
                memory_size,
                compression,
                committed,
                extended,
            });
        }
        drop(get);
        drop(stream);
        if remaining != 0 {
            return Err(Error::unsupported("trailing DBPF index bytes"));
        }
        if FileStamp::from(&f.metadata()?) != stamp {
            return Err(Error::Changed);
        }
        Ok(Self {
            path,
            entries,
            index_sha256: digest.finalize().into(),
            file_size: before.len(),
            stamp,
        })
    }
    pub fn reader(&self) -> Result<PackageReader<'_>> {
        self.reader_cancellable(&AtomicBool::new(false))
    }
    pub fn reader_cancellable(&self, cancel: &AtomicBool) -> Result<PackageReader<'_>> {
        let mut file = File::open(&self.path)?;
        let current = Self::from_file(self.path.clone(), &mut file, cancel)?;
        if current.stamp != self.stamp
            || current.index_sha256 != self.index_sha256
            || current.entries != self.entries
        {
            return Err(Error::Changed);
        }
        Ok(PackageReader { index: self, file })
    }
    pub fn read_resource(&self, entry: &ResourceEntry, limits: ReadLimits) -> Result<Vec<u8>> {
        self.reader()?.read_resource(entry, limits)
    }
    pub fn matching(&self, key: ResourceKey) -> impl Iterator<Item = &ResourceEntry> {
        self.entries.iter().filter(move |e| e.key == key)
    }
}
pub struct PackageReader<'a> {
    index: &'a PackageIndex,
    file: File,
}
impl PackageReader<'_> {
    /// Reuse an independently retained payload SHA256 to detect even a source
    /// rewrite whose file timestamps and index metadata were deliberately kept.
    pub fn read_pinned_resource(
        &mut self,
        entry: &ResourceEntry,
        expected_sha256: [u8; 32],
        limits: ReadLimits,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>> {
        let bytes = self.read_resource_cancellable(entry, limits, cancel)?;
        let mut digest = Sha256::new();
        for chunk in bytes.chunks(1024 * 1024) {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            digest.update(chunk);
        }
        let actual: [u8; 32] = digest.finalize().into();
        if actual != expected_sha256 || FileStamp::from(&self.file.metadata()?) != self.index.stamp
        {
            return Err(Error::Changed);
        }
        Ok(bytes)
    }
    pub fn read_resource(&mut self, entry: &ResourceEntry, limits: ReadLimits) -> Result<Vec<u8>> {
        self.read_resource_cancellable(entry, limits, &AtomicBool::new(false))
    }
    pub fn read_resource_cancellable(
        &mut self,
        entry: &ResourceEntry,
        limits: ReadLimits,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>> {
        if self.index.entries.get(entry.ordinal as usize) != Some(entry) {
            return Err(Error::invalid("foreign resource locator"));
        }
        if entry.deleted() {
            return Err(Error::unsupported("deleted resource"));
        }
        if ![0, 1].contains(&entry.committed) {
            return Err(Error::unsupported(format!(
                "storage marker {:#06X}",
                entry.committed
            )));
        }
        if ![0, 0x5a42, 0xffff].contains(&entry.compression) {
            return Err(Error::unsupported(format!(
                "compression {:#06X}",
                entry.compression
            )));
        }
        limits.check(entry.stored_size as u64)?;
        limits.check(entry.memory_size as u64)?;
        if FileStamp::from(&self.file.metadata()?) != self.index.stamp {
            return Err(Error::Changed);
        }
        self.file.seek(SeekFrom::Start(entry.offset))?;
        let mut bytes = vec![0; entry.stored_size as usize];
        for part in bytes.chunks_mut(1024 * 1024) {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            self.file.read_exact(part)?;
        }
        let decoded = match entry.compression {
            0 => bytes,
            0xffff => decode_refpack(&bytes, entry.memory_size as usize, limits, cancel)?,
            0x5a42 => {
                let mut decoder = flate2::bufread::ZlibDecoder::new(bytes.as_slice());
                let mut output = Vec::new();
                let mut buf = [0u8; 64 * 1024];
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(Error::Cancelled);
                    }
                    let n = decoder.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    if output
                        .len()
                        .checked_add(n)
                        .is_none_or(|n| n > entry.memory_size as usize)
                    {
                        return Err(Error::invalid("zlib output exceeds declared size"));
                    }
                    output.extend_from_slice(&buf[..n]);
                }
                if decoder.total_in() != bytes.len() as u64 {
                    return Err(Error::invalid("trailing zlib bytes"));
                }
                output
            }
            _ => unreachable!(),
        };
        if decoded.len() != entry.memory_size as usize {
            return Err(Error::invalid("decoded resource size mismatch"));
        }
        if FileStamp::from(&self.file.metadata()?) != self.index.stamp {
            return Err(Error::Changed);
        }
        Ok(decoded)
    }
}

pub fn decode_refpack(
    input: &[u8],
    expected: usize,
    limits: ReadLimits,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    limits.check(expected as u64)?;
    let mut r = Bytes::new(input);
    let flags = r.u8()?;
    if flags & 0x7e != 0x10 || r.u8()? != 0xfb {
        return Err(Error::unsupported("RefPack header"));
    }
    let mut declared = 0u64;
    for _ in 0..if flags & 0x80 != 0 { 4 } else { 3 } {
        declared = (declared << 8) | r.u8()? as u64;
    }
    if declared != expected as u64 {
        return Err(Error::invalid("RefPack declared size"));
    }
    let mut output = Vec::new();
    output
        .try_reserve(expected)
        .map_err(|_| Error::invalid("RefPack allocation"))?;
    let mut ended = false;
    while r.remaining() > 0 && !ended {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let op = r.u8()?;
        let literals: usize;
        let mut len = 0usize;
        let mut distance = 0usize;
        if op <= 0x7f {
            let b = r.u8()?;
            literals = (op & 3) as usize;
            len = ((op & 0x1c) >> 2) as usize + 3;
            distance = (((op & 0x60) as usize) << 3) + b as usize + 1;
        } else if op <= 0xbf {
            let b = r.u8()?;
            let c = r.u8()?;
            literals = (b >> 6) as usize;
            len = (op & 0x3f) as usize + 4;
            distance = (((b & 0x3f) as usize) << 8) + c as usize + 1;
        } else if op <= 0xdf {
            let b = r.u8()?;
            let c = r.u8()?;
            let d = r.u8()?;
            literals = (op & 3) as usize;
            len = (((op & 0x0c) as usize) << 6) + d as usize + 5;
            distance = (((op & 0x10) as usize) << 12) + ((b as usize) << 8) + c as usize + 1;
        } else if op <= 0xfb {
            literals = (((op & 0x1f) as usize) << 2) + 4;
        } else {
            literals = (op & 3) as usize;
            ended = true;
        }
        if output
            .len()
            .checked_add(literals + len)
            .is_none_or(|n| n > expected)
        {
            return Err(Error::invalid("RefPack output extent"));
        }
        output.extend_from_slice(r.take(literals)?);
        if len > 0 && distance > output.len() {
            return Err(Error::invalid("RefPack backreference"));
        }
        for _ in 0..len {
            let b = output[output.len() - distance];
            output.push(b);
        }
    }
    if !ended || r.remaining() != 0 || output.len() != expected {
        return Err(Error::invalid("incomplete RefPack stream"));
    }
    Ok(output)
}
