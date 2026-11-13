//! Source-named set/program metadata in PRECOMPv6. Unknown record words and
//! the original metadata prefix stay retained; later parameter/heap metadata
//! is explicitly unparsed and remains in the pinned ShaderLibrary source.
use crate::{FeatureLimits, FeatureProgram, Result, ShaderLibrary, check, relative, table, u32_at};
use sha2::{Digest, Sha256};
use std::sync::atomic::AtomicBool;

#[derive(Clone, Debug, serde::Serialize)]
pub struct VariantField {
    pub hash: u32,
    pub name: String,
    pub name_bytes: Vec<u8>,
    pub name_hash_matches: bool,
    pub multiplier: u32,
    pub start: u32,
    pub range: u32,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct NamedShaderSet {
    pub ordinal: usize,
    pub source_file_sha256: String,
    pub source_record: usize,
    pub raw_record_words: [u32; 2],
    pub source_payload: usize,
    pub name: String,
    pub name_bytes: Vec<u8>,
    /// Exact lowercase-ASCII FNV1 name hash used by this source metadata.
    pub name_hash: u32,
    pub modes: Vec<[u32; 2]>,
    pub program_words: Vec<u32>,
    pub program_sha256: String,
    pub variant_fields: Vec<VariantField>,
    pub raw_metadata_prefix: Vec<u8>,
    /// Parameter/state/heap data following this prefix has NOT been interpreted.
    pub unparsed_source_offset: usize,
}
fn name_hash(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5u32, |h, b| {
        h.wrapping_mul(0x1000193) ^ b.to_ascii_lowercase() as u32
    })
}
impl NamedShaderSet {
    pub fn feature_program(
        &self,
        limits: FeatureLimits,
        cancel: &AtomicBool,
    ) -> Result<FeatureProgram<'_>> {
        check(cancel)?;
        let mut digest = Sha256::new();
        for (i, word) in self.program_words.iter().enumerate() {
            if i & 255 == 0 {
                check(cancel)?;
            }
            digest.update(word.to_le_bytes());
        }
        if format!("{:x}", digest.finalize()) != self.program_sha256 {
            return Err("Retained source feature program changed after validation".into());
        }
        FeatureProgram::parse(&self.program_words, limits, cancel)
    }
}
struct Cursor<'a> {
    raw: &'a [u8],
    at: usize,
    count_limit: usize,
}
impl Cursor<'_> {
    fn word(&mut self) -> Result<u32> {
        let value = u32_at(self.raw, self.at)?;
        self.at = self
            .at
            .checked_add(4)
            .ok_or("Set metadata offset overflow")?;
        Ok(value)
    }
    fn count(&mut self) -> Result<usize> {
        let n = self.word()? as usize;
        if n > self.count_limit {
            Err("Set metadata count exceeds budget".into())
        } else {
            Ok(n)
        }
    }
    fn take(&mut self, n: usize) -> Result<Vec<u8>> {
        let end = self
            .at
            .checked_add(n)
            .ok_or("Set metadata extent overflow")?;
        let bytes = self
            .raw
            .get(self.at..end)
            .ok_or("Truncated named shader set")?
            .to_vec();
        self.at = end;
        Ok(bytes)
    }
    fn name(&mut self) -> Result<(String, Vec<u8>)> {
        let n = self.count()?;
        let bytes = self.take(n)?;
        let text = String::from_utf8(bytes.clone())
            .map_err(|_| "Unsupported non-UTF8 source shader name")?;
        Ok((text, bytes))
    }
}
impl ShaderLibrary {
    pub fn named_set_count(&self) -> Result<usize> {
        Ok(table(&self.bytes, self.payload_offset + 44, 8, self.limits)?.count)
    }
    pub fn named_set(&self, ordinal: usize, cancel: &AtomicBool) -> Result<NamedShaderSet> {
        check(cancel)?;
        let sets = table(&self.bytes, self.payload_offset + 44, 8, self.limits)?;
        if ordinal >= sets.count {
            return Err("Named shader set ordinal outside source table".into());
        }
        let record = sets.offset + ordinal * 8;
        let raw_record_words = [
            u32_at(&self.bytes, record)?,
            u32_at(&self.bytes, record + 4)?,
        ];
        let payload = relative(&self.bytes, record)?;
        let mut cur = Cursor {
            raw: &self.bytes,
            at: payload,
            count_limit: self.limits.table_records,
        };
        let (name, name_bytes) = cur.name()?;
        // Only the SET name is padded to a4-byte boundary; VariantField names
        // below are explicitly unaligned in the actual source/EA declaration.
        cur.take((4 - name_bytes.len() % 4) % 4)?;
        let mut modes = Vec::new();
        let count = cur.count()?;
        for _ in 0..count {
            check(cancel)?;
            modes.push([cur.word()?, cur.word()?]);
        }
        let count = cur.count()?;
        if count
            .checked_mul(4)
            .is_none_or(|n| n > self.limits.decoded_shader_bytes)
        {
            return Err("Set program exceeds byte budget".into());
        }
        let program_start = cur.at;
        let mut program_words = Vec::new();
        for _ in 0..count {
            check(cancel)?;
            program_words.push(cur.word()?);
        }
        let program_sha256 = format!("{:x}", Sha256::digest(&self.bytes[program_start..cur.at]));
        let count = cur.count()?;
        let mut variant_fields = Vec::new();
        for _ in 0..count {
            check(cancel)?;
            let hash = cur.word()?;
            let (name, name_bytes) = cur.name()?;
            variant_fields.push(VariantField {
                hash,
                name,
                name_hash_matches: name_hash(&name_bytes) == hash,
                name_bytes,
                multiplier: cur.word()?,
                start: cur.word()?,
                range: cur.word()?,
            });
        }
        check(cancel)?;
        Ok(NamedShaderSet {
            ordinal,
            source_file_sha256: self.pin.sha256.clone(),
            source_record: record,
            raw_record_words,
            source_payload: payload,
            name,
            name_hash: name_hash(&name_bytes),
            name_bytes,
            modes,
            program_words,
            program_sha256,
            variant_fields,
            raw_metadata_prefix: self.bytes[payload..cur.at].to_vec(),
            unparsed_source_offset: cur.at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put(raw: &mut [u8], at: usize, value: u32) {
        raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn fixture() -> Vec<u8> {
        let mut raw = vec![0; 512];
        raw[..4].copy_from_slice(b"DATA");
        put(&mut raw, 8, 24);
        put(&mut raw, 52, 12);
        put(&mut raw, 64, 6);
        put(&mut raw, 108, 100);
        put(&mut raw, 112, 1);
        put(&mut raw, 208, 48);
        put(&mut raw, 212, 0xabcde);
        put(&mut raw, 256, 4);
        raw[260..264].copy_from_slice(b"Test");
        put(&mut raw, 264, 1);
        put(&mut raw, 268, 12);
        put(&mut raw, 272, 13);
        put(&mut raw, 276, 3);
        put(&mut raw, 280, 0);
        put(&mut raw, 284, 0);
        put(&mut raw, 288, 0);
        put(&mut raw, 292, 1);
        put(&mut raw, 296, name_hash(b"AbC"));
        put(&mut raw, 300, 3);
        raw[304..307].copy_from_slice(b"AbC");
        put(&mut raw, 307, 128);
        put(&mut raw, 311, 0);
        put(&mut raw, 315, 2);
        put(&mut raw, 319, 0xdecafbad);
        raw
    }
    #[test]
    fn source_names_unaligned_fields_raw_unknown_and_pins_are_retained() {
        let raw = fixture();
        let lib = ShaderLibrary::from_bytes(
            "fixture".into(),
            raw.clone(),
            crate::Limits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(lib.named_set_count().unwrap(), 1);
        let set = lib.named_set(0, &AtomicBool::new(false)).unwrap();
        assert_eq!(set.source_file_sha256, lib.pin.sha256);
        assert_eq!(set.name, "Test");
        assert_eq!(set.modes, [[12, 13]]);
        assert_eq!(set.raw_record_words, [48, 0xabcde]);
        assert_eq!(set.variant_fields[0].name, "AbC");
        assert!(set.variant_fields[0].name_hash_matches);
        assert_eq!(set.variant_fields[0].multiplier, 128);
        assert_eq!(set.raw_metadata_prefix, &raw[256..319]);
        assert_eq!(set.unparsed_source_offset, 319);
        assert_eq!(
            set.feature_program(FeatureLimits::default(), &AtomicBool::new(false))
                .unwrap()
                .raw_words(),
            [0, 0, 0]
        );
        let mut changed = set.clone();
        changed.program_words[2] = 8;
        assert!(
            changed
                .feature_program(FeatureLimits::default(), &AtomicBool::new(false))
                .is_err()
        );
        assert!(lib.named_set(1, &AtomicBool::new(false)).is_err());
        assert!(lib.named_set(0, &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn malformed_names_counts_relative_targets_and_prefixes_are_refused() {
        for (at, value) in [
            (108, u32::MAX),
            (256, u32::MAX),
            (264, u32::MAX),
            (276, u32::MAX),
            (300, u32::MAX),
        ] {
            let mut raw = fixture();
            put(&mut raw, at, value);
            let lib = ShaderLibrary::from_bytes(
                "fixture".into(),
                raw,
                crate::Limits::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
            assert!(
                lib.named_set(0, &AtomicBool::new(false)).is_err(),
                "offset{at}"
            );
        }
    }
}
