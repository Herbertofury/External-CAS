// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only STBL v5 lookup for an explicitly selected locale/provider.
//! Token expansion is deliberately separate from exact source string lookup.
use crate::installed::SourcePin;
use external_cas_assets::{PackageIndex, ReadLimits};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};

pub const STBL_TYPE: u32 = 0x2205_57da;
const MAX_TABLE: usize = 128 * 1024 * 1024;
const MAX_BANK: usize = 512 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct StringEntry {
    pub hash: u32,
    pub flags: u8,
    pub bytes: Range<usize>,
}
#[derive(Clone, Debug)]
pub struct StringTable {
    pub source_bytes: Vec<u8>,
    pub reserved: [u8; 2],
    pub declared_string_length: u32,
    pub entries: Vec<StringEntry>,
    pub trailing: Range<usize>,
    pub issues: Vec<String>,
}
impl StringTable {
    pub fn parse(source: Vec<u8>) -> Result<Self, String> {
        if source.len() > MAX_TABLE {
            return Err("STBL byte budget exceeded".into());
        }
        let mut reader = Reader {
            bytes: &source,
            at: 0,
        };
        if reader.take(4)? != b"STBL" {
            return Err("STBL signature differs".into());
        }
        if reader.u16()? != 5 {
            return Err("unsupported STBL version; source retained by caller".into());
        }
        if reader.u8()? != 0 {
            return Err("compressed STBL is unsupported".into());
        }
        let count = usize::try_from(reader.u64()?).map_err(|_| "STBL count overflow")?;
        let reserved = reader.take(2)?.try_into().unwrap();
        let declared_string_length = reader.u32()?;
        // Every entry requires seven bytes even when its text is empty.
        if count > (source.len() - reader.at) / 7 {
            return Err("STBL entry count exceeds remaining bytes".into());
        }
        let mut entries = Vec::with_capacity(count);
        let mut observed_length = 0u64;
        for _ in 0..count {
            let hash = reader.u32()?;
            let flags = reader.u8()?;
            let length = usize::from(reader.u16()?);
            let start = reader.at;
            reader.take(length)?;
            observed_length += length as u64 + 1;
            entries.push(StringEntry {
                hash,
                flags,
                bytes: start..reader.at,
            });
        }
        let trailing = reader.at..source.len();
        let mut issues = Vec::new();
        if observed_length != u64::from(declared_string_length) {
            issues.push(format!("declared string length {declared_string_length} differs from observed UTF-8 length plus terminators {observed_length}"));
        }
        if !trailing.is_empty() {
            issues.push("uninterpreted trailing STBL bytes retained".into());
        }
        Ok(Self {
            source_bytes: source,
            reserved,
            declared_string_length,
            entries,
            trailing,
            issues,
        })
    }
    pub fn text(&self, entry: &StringEntry) -> Result<&str, String> {
        std::str::from_utf8(&self.source_bytes[entry.bytes.clone()])
            .map_err(|_| "STBL entry contains invalid UTF-8; original bytes retained".into())
    }
}
#[derive(Clone, Debug)]
pub struct PinnedStringTable {
    pub pin: SourcePin,
    pub table: StringTable,
}
#[derive(Clone, Debug)]
pub struct StringBank {
    pub tables: Vec<PinnedStringTable>,
    by_hash: BTreeMap<u32, Vec<(usize, usize)>>,
}
impl StringBank {
    /// Explicit package selection is the locale/override boundary. All live
    /// STBL ordinals participate; conflicting strings are never first-wins.
    pub fn load(index: &PackageIndex, cancel: &AtomicBool) -> Result<Self, String> {
        let mut tables = Vec::new();
        let mut total = 0usize;
        for entry in index
            .entries
            .iter()
            .filter(|e| e.key.type_id == STBL_TYPE && !e.deleted())
        {
            if cancel.load(Ordering::Relaxed) {
                return Err("string-table loading cancelled".into());
            }
            let bytes = index
                .read_resource(
                    entry,
                    ReadLimits {
                        max_resource_bytes: MAX_TABLE as u64,
                    },
                )
                .map_err(|e| e.to_string())?;
            total = total
                .checked_add(bytes.len())
                .ok_or("STBL aggregate size overflow")?;
            if total > MAX_BANK {
                return Err("STBL aggregate byte budget exceeded".into());
            }
            let pin = SourcePin {
                provider_path: index.path.clone(),
                key: entry.key,
                ordinal: entry.ordinal,
                index_sha256: hex(&index.index_sha256),
                payload_sha256: hex(&Sha256::digest(&bytes)),
            };
            tables.push(PinnedStringTable {
                pin,
                table: StringTable::parse(bytes)?,
            });
        }
        if tables.is_empty() {
            return Err("selected provider contains no live STBL resources".into());
        }
        Ok(Self::from_tables(tables))
    }
    pub fn from_tables(tables: Vec<PinnedStringTable>) -> Self {
        let mut by_hash: BTreeMap<u32, Vec<(usize, usize)>> = BTreeMap::new();
        for (ti, table) in tables.iter().enumerate() {
            for (ei, entry) in table.table.entries.iter().enumerate() {
                by_hash.entry(entry.hash).or_default().push((ti, ei));
            }
        }
        Self { tables, by_hash }
    }
    pub fn source_pins(&self) -> Vec<SourcePin> {
        self.tables.iter().map(|t| t.pin.clone()).collect()
    }
    pub fn distinct_hashes(&self) -> usize {
        self.by_hash.len()
    }
    pub fn lookup(&self, hash: u32) -> Result<&str, String> {
        let locations = self
            .by_hash
            .get(&hash)
            .ok_or_else(|| format!("localized hash {hash:08X} is absent from selected provider"))?;
        let mut result: Option<&str> = None;
        for &(ti, ei) in locations {
            let table = &self.tables[ti].table;
            let entry = &table.entries[ei];
            if entry.flags != 0 {
                return Err(format!(
                    "localized hash {hash:08X} has unsupported flags {}; raw bytes retained",
                    entry.flags
                ));
            }
            let text = table.text(entry)?;
            if result.is_some_and(|old| old != text) {
                return Err(format!(
                    "localized hash {hash:08X} has conflicting source entries; select providers explicitly"
                ));
            }
            result = Some(text);
        }
        result.ok_or_else(|| "localized hash has no source entry".into())
    }
    /// Safe initial subset for the source LocKey native Function. Real tokens,
    /// grammar/variant directives and filename-safe flags require their own
    /// qualified host formatter; they are never replaced with invented text.
    pub fn resolve_literal(
        &self,
        hash: u32,
        token_count: usize,
        flags: u32,
    ) -> Result<&str, String> {
        if token_count != 0 || flags != 0 {
            return Err("localized token/flag formatter is not yet qualified".into());
        }
        let text = self.lookup(hash)?;
        if text.contains(['{', '}']) {
            return Err("localized grammar/token directives require a qualified formatter".into());
        }
        Ok(text)
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or("STBL offset overflow")?;
        let result = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| format!("truncated STBL at {}", self.at))?;
        self.at = end;
        Ok(result)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source(entries: &[(u32, u8, &[u8])]) -> Vec<u8> {
        let mut b = b"STBL\x05\0\0".to_vec();
        b.extend((entries.len() as u64).to_le_bytes());
        b.extend([0, 0]);
        b.extend(
            entries
                .iter()
                .map(|(_, _, s)| s.len() as u32 + 1)
                .sum::<u32>()
                .to_le_bytes(),
        );
        for (hash, flags, text) in entries {
            b.extend(hash.to_le_bytes());
            b.push(*flags);
            b.extend((text.len() as u16).to_le_bytes());
            b.extend(*text);
        }
        b
    }
    fn pin() -> SourcePin {
        SourcePin {
            provider_path: "explicit-locale.package".into(),
            key: external_cas_assets::ResourceKey::new(STBL_TYPE, 0, 1),
            ordinal: 0,
            index_sha256: "00".repeat(32),
            payload_sha256: "11".repeat(32),
        }
    }
    fn bank(entries: &[(u32, u8, &[u8])]) -> StringBank {
        StringBank::from_tables(vec![PinnedStringTable {
            pin: pin(),
            table: StringTable::parse(source(entries)).unwrap(),
        }])
    }
    #[test]
    fn all_truncations_and_excessive_count_are_rejected() {
        let b = source(&[(4, 0, b"Undo"), (5, 0, b"")]);
        for cut in 0..b.len() {
            assert!(StringTable::parse(b[..cut].to_vec()).is_err(), "{cut}");
        }
        let mut b = b;
        b[7..15].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(StringTable::parse(b).is_err());
    }
    #[test]
    fn unknown_bytes_invalid_utf8_and_declared_length_are_preserved() {
        let mut b = source(&[(4, 2, &[255])]);
        b[15..17].copy_from_slice(&[9, 8]);
        b[17..21].copy_from_slice(&99u32.to_le_bytes());
        b.extend([1, 2]);
        let t = StringTable::parse(b.clone()).unwrap();
        assert_eq!(t.source_bytes, b);
        assert_eq!(t.reserved, [9, 8]);
        assert_eq!(t.entries[0].flags, 2);
        assert!(t.text(&t.entries[0]).is_err());
        assert_eq!(t.issues.len(), 2);
    }
    #[test]
    fn duplicate_conflicts_and_unqualified_formatting_are_explicit() {
        let a = bank(&[
            (1, 0, b"Undo"),
            (1, 0, b"Undo"),
            (2, 0, b"Hello {0.SimName}"),
        ]);
        assert_eq!(a.resolve_literal(1, 0, 0).unwrap(), "Undo");
        assert!(a.resolve_literal(2, 0, 0).is_err());
        assert!(a.resolve_literal(1, 1, 0).is_err());
        assert!(a.resolve_literal(1, 0, 4).is_err());
        assert!(a.lookup(3).is_err());
        assert!(bank(&[(1, 0, b"Undo"), (1, 0, b"Redo")]).lookup(1).is_err());
        assert!(bank(&[(1, 1, b"Undo")]).lookup(1).is_err());
    }
}
