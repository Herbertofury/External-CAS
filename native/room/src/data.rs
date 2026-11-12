// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded schema-following SimData observation, informed by S4TK models (MIT).
//! Supports the native camera/light scalar/object/vector subset. Unknown cells
//! remain raw bytes rather than being replaced by a default.
use crate::*;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Debug, Clone, Serialize)]
pub struct DataColumn {
    pub name: String,
    pub data_type: u16,
    pub flags: u16,
    pub offset: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct DataSchema {
    pub name: String,
    pub hash: u32,
    pub size: usize,
    pub columns: Vec<DataColumn>,
}
#[derive(Debug, Clone, Serialize)]
pub enum DataValue {
    Boolean(u8),
    FloatBits(u32),
    FloatVectorBits(Vec<u32>),
    Signed64(i64),
    Unsigned64(u64),
    Unsigned32(u32),
    Object {
        schema: String,
        schema_hash: u32,
        fields: Vec<(String, DataValue)>,
    },
    Vector(Vec<DataValue>),
    Opaque {
        data_type: u16,
        source_offset: usize,
        bytes: Vec<u8>,
    },
}
impl DataValue {
    pub fn field(&self, name: &str) -> Result<Option<&DataValue>> {
        match self {
            Self::Object { fields, .. } => {
                let mut found = fields.iter().filter(|(n, _)| n == name);
                let value = found.next().map(|(_, v)| v);
                if found.next().is_some() {
                    return Err(format!("duplicate SimData field {name}"));
                }
                Ok(value)
            }
            _ => Err("SimData field access on non-object".into()),
        }
    }
    pub fn bool_value(&self) -> Result<bool> {
        match self {
            Self::Boolean(0) => Ok(false),
            Self::Boolean(1) => Ok(true),
            _ => Err("invalid SimData Boolean byte".into()),
        }
    }
    pub fn float_bits(&self) -> Result<u32> {
        match self {
            Self::FloatBits(bits) => Ok(*bits),
            _ => Err("expected SimData float".into()),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct DataDocument {
    pub version: u32,
    pub schemas: Vec<DataSchema>,
    pub instances: Vec<(String, DataValue)>,
    pub issues: Vec<String>,
}
#[derive(Debug, Clone)]
struct Table {
    name: Option<String>,
    schema: Option<usize>,
    typ: u16,
    start: usize,
    size: usize,
    count: usize,
}
struct DataReader<'a> {
    raw: &'a [u8],
    schemas: BTreeMap<usize, DataSchema>,
    tables: Vec<Table>,
    cancel: &'a AtomicBool,
    visits: usize,
    issues: Vec<String>,
}
fn take(raw: &[u8], at: usize, n: usize) -> Result<&[u8]> {
    raw.get(at..at.checked_add(n).ok_or("SimData extent overflow")?)
        .ok_or_else(|| "truncated SimData extent".into())
}
fn u16at(raw: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(take(raw, at, 2)?.try_into().unwrap()))
}
fn u32at(raw: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(take(raw, at, 4)?.try_into().unwrap()))
}
fn rel(raw: &[u8], at: usize) -> Result<Option<usize>> {
    let delta = u32at(raw, at)? as i32;
    if delta == i32::MIN {
        return Ok(None);
    }
    let target = (at as i64)
        .checked_add(i64::from(delta))
        .filter(|v| *v >= 0 && *v <= raw.len() as i64)
        .ok_or("SimData relative pointer extent")?;
    Ok(Some(target as usize))
}
fn text(raw: &[u8], at: usize) -> Result<String> {
    let region =
        take(raw, at, 1).and_then(|_| Ok(&raw[at..raw.len().min(at.saturating_add(1025))]))?;
    let n = region
        .iter()
        .position(|b| *b == 0)
        .ok_or("SimData name terminator/budget")?;
    String::from_utf8(region[..n].to_vec()).map_err(|_| "SimData name UTF-8".into())
}
fn named(raw: &[u8], at: usize) -> Result<Option<String>> {
    rel(raw, at)?.map(|p| text(raw, p)).transpose()
}
fn width(typ: u16) -> Option<usize> {
    match typ {
        0..=3 => Some(1),
        4 | 5 => Some(2),
        6 | 7 | 10 | 11 | 13 | 20 => Some(4),
        8 | 9 | 12 | 14 | 15 | 18 | 21 => Some(8),
        16 => Some(12),
        17 | 19 => Some(16),
        _ => None,
    }
}
impl DataReader<'_> {
    fn table(&self, p: usize) -> Result<&Table> {
        let mut found = self.tables.iter().filter(|t| {
            t.size > 0
                && p >= t.start
                && p < t.start + t.size * t.count
                && (p - t.start) % t.size == 0
        });
        let table = found
            .next()
            .ok_or("SimData pointer is not a declared table row")?;
        if found.next().is_some() {
            return Err("overlapping SimData row tables".into());
        }
        Ok(table)
    }
    fn cell(
        &mut self,
        typ: u16,
        p: usize,
        path: &mut BTreeSet<(u16, usize)>,
        depth: usize,
    ) -> Result<DataValue> {
        self.visits += 1;
        if depth > 64 || self.visits > 100000 {
            return Err("SimData recursion/node budget".into());
        }
        if self.visits % 256 == 0 && self.cancel.load(Ordering::Relaxed) {
            return Err("SimData parse cancelled".into());
        }
        if !path.insert((typ, p)) {
            return Err("SimData pointer cycle".into());
        }
        let size = width(typ).ok_or_else(|| format!("unknown SimData cell type {typ}"))?;
        let raw = take(self.raw, p, size)?;
        let result = match typ {
            0 => DataValue::Boolean(raw[0]),
            10 => DataValue::FloatBits(u32at(self.raw, p)?),
            15..=17 => DataValue::FloatVectorBits(
                (0..(typ - 13) as usize)
                    .map(|i| u32at(self.raw, p + i * 4))
                    .collect::<Result<Vec<_>>>()?,
            ),
            8 => DataValue::Signed64(i64::from_le_bytes(raw.try_into().unwrap())),
            9 | 18 => DataValue::Unsigned64(u64::from_le_bytes(raw.try_into().unwrap())),
            7 | 20 => DataValue::Unsigned32(u32at(self.raw, p)?),
            13 => {
                let target = rel(self.raw, p)?.ok_or("null SimData object")?;
                self.object(target, path, depth + 1)?
            }
            14 => {
                let count = u32at(self.raw, p + 4)? as usize;
                if count > 65536 {
                    return Err("SimData vector count budget".into());
                }
                let target = rel(self.raw, p)?;
                let mut values = Vec::new();
                if count > 0 {
                    let target = target.ok_or("null SimData vector with nonzero count")?;
                    let t = self.table(target)?.clone();
                    let start = (target - t.start) / t.size;
                    if start.checked_add(count).is_none_or(|end| end > t.count) {
                        return Err("SimData vector exceeds declared table".into());
                    }
                    for i in 0..count {
                        let at = target + i * t.size;
                        values.push(if t.typ == 13 {
                            self.object(at, path, depth + 1)?
                        } else {
                            self.cell(t.typ, at, path, depth + 1)?
                        });
                    }
                }
                DataValue::Vector(values)
            }
            _ => {
                self.issues
                    .push(format!("SimData cell type {typ} at {p} retained opaque"));
                DataValue::Opaque {
                    data_type: typ,
                    source_offset: p,
                    bytes: raw.to_vec(),
                }
            }
        };
        path.remove(&(typ, p));
        Ok(result)
    }
    fn object(
        &mut self,
        p: usize,
        path: &mut BTreeSet<(u16, usize)>,
        depth: usize,
    ) -> Result<DataValue> {
        if depth > 64 {
            return Err("SimData object depth".into());
        }
        let t = self.table(p)?.clone();
        if t.typ != 13 {
            return Err("SimData object target table type".into());
        }
        let schema = self
            .schemas
            .get(&t.schema.ok_or("SimData object table lacks schema")?)
            .ok_or("SimData object schema absent")?
            .clone();
        if schema.size != t.size {
            return Err("SimData row/schema size mismatch".into());
        }
        let mut fields = Vec::new();
        for c in &schema.columns {
            let v = self.cell(c.data_type, p + c.offset, path, depth + 1)?;
            fields.push((c.name.clone(), v));
        }
        Ok(DataValue::Object {
            schema: schema.name,
            schema_hash: schema.hash,
            fields,
        })
    }
}
pub fn parse_simdata(raw: &[u8], cancel: &AtomicBool) -> Result<DataDocument> {
    if cancel.load(Ordering::Relaxed) {
        return Err("SimData parse cancelled".into());
    }
    if raw.len() > 16 * 1024 * 1024 || raw.get(..4) != Some(b"DATA") {
        return Err("SimData magic/byte budget".into());
    }
    let version = u32at(raw, 4)?;
    if !matches!(version, 0x100 | 0x101) {
        return Err(format!("unsupported SimData version {version:08X}"));
    }
    take(raw, 0, if version == 0x101 { 28 } else { 24 })?;
    let tp = rel(raw, 8)?.ok_or("SimData table pointer absent")?;
    let tc = u32at(raw, 12)? as usize;
    let sp = rel(raw, 16)?.ok_or("SimData schema pointer absent")?;
    let sc = u32at(raw, 20)? as usize;
    if tc > 4096 || sc > 256 {
        return Err("SimData table/schema budget".into());
    }
    take(
        raw,
        tp,
        tc.checked_mul(28).ok_or("SimData table count overflow")?,
    )?;
    take(
        raw,
        sp,
        sc.checked_mul(24).ok_or("SimData schema count overflow")?,
    )?;
    let mut schemas = BTreeMap::new();
    for i in 0..sc {
        let p = sp + i * 24;
        let name = named(raw, p)?.ok_or("unnamed SimData schema")?;
        let hash = u32at(raw, p + 8)?;
        let size = u32at(raw, p + 12)? as usize;
        let cp = rel(raw, p + 16)?.ok_or("SimData column pointer absent")?;
        let cc = u32at(raw, p + 20)? as usize;
        if size == 0 || size > 65536 || cc > 256 {
            return Err("SimData schema extent/budget".into());
        }
        take(raw, cp, cc * 20)?;
        let mut columns = Vec::new();
        let mut names = BTreeSet::new();
        for c in 0..cc {
            let q = cp + c * 20;
            let name = named(raw, q)?.ok_or("unnamed SimData column")?;
            let data_type = u16at(raw, q + 8)?;
            let flags = u16at(raw, q + 10)?;
            let offset = u32at(raw, q + 12)? as usize;
            let w = width(data_type).ok_or("unknown SimData column width")?;
            if offset.checked_add(w).is_none_or(|end| end > size) || !names.insert(name.clone()) {
                return Err("SimData duplicate/out-of-row column".into());
            }
            columns.push(DataColumn {
                name,
                data_type,
                flags,
                offset,
            });
        }
        schemas.insert(
            p,
            DataSchema {
                name,
                hash,
                size,
                columns,
            },
        );
    }
    let mut tables = Vec::new();
    for i in 0..tc {
        let p = tp + i * 28;
        let name = named(raw, p)?;
        let schema = rel(raw, p + 8)?;
        let typ = u32at(raw, p + 12)?;
        if typ > u16::MAX as u32 {
            return Err("SimData table type extent".into());
        }
        let size = u32at(raw, p + 16)? as usize;
        let start = rel(raw, p + 20)?.ok_or("SimData row pointer absent")?;
        let count = u32at(raw, p + 24)? as usize;
        if size == 0 || count > 1000000 {
            return Err("SimData row size/count budget".into());
        }
        take(
            raw,
            start,
            size.checked_mul(count).ok_or("SimData rows overflow")?,
        )?;
        if typ == 13 {
            let s = schema
                .and_then(|p| schemas.get(&p))
                .ok_or("SimData table schema absent")?;
            if s.size != size {
                return Err("SimData row/schema mismatch".into());
            }
        } else if width(typ as u16).is_none_or(|n| n != size) {
            return Err("unsupported SimData primitive stride".into());
        }
        tables.push(Table {
            name,
            schema,
            typ: typ as u16,
            start,
            size,
            count,
        });
    }
    for (i, a) in tables.iter().enumerate() {
        if a.count == 0 {
            continue;
        }
        for b in &tables[i + 1..] {
            if b.count > 0
                && a.start < b.start + b.size * b.count
                && b.start < a.start + a.size * a.count
            {
                return Err("overlapping SimData tables".into());
            }
        }
    }
    let mut reader = DataReader {
        raw,
        schemas,
        tables,
        cancel,
        visits: 0,
        issues: Vec::new(),
    };
    let named: Vec<_> = reader
        .tables
        .iter()
        .filter_map(|t| {
            t.name
                .as_ref()
                .map(|n| (n.clone(), t.start, t.count, t.typ))
        })
        .collect();
    let mut instances = Vec::new();
    for (name, p, count, typ) in named {
        if count != 1 || typ != 13 {
            return Err("named SimData instance must be a single object".into());
        }
        instances.push((name, reader.object(p, &mut BTreeSet::new(), 0)?));
    }
    Ok(DataDocument {
        version,
        schemas: reader.schemas.into_values().collect(),
        instances,
        issues: reader.issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put32(b: &mut [u8], p: usize, v: u32) {
        b[p..p + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn pointer(b: &mut [u8], p: usize, target: usize) {
        put32(b, p, (target as i32 - p as i32) as u32);
    }
    fn fixture() -> Vec<u8> {
        let mut b = vec![0; 384];
        b[..4].copy_from_slice(b"DATA");
        put32(&mut b, 4, 0x101);
        pointer(&mut b, 8, 64);
        put32(&mut b, 12, 1);
        pointer(&mut b, 16, 96);
        put32(&mut b, 20, 1);
        pointer(&mut b, 64, 256);
        pointer(&mut b, 72, 96);
        put32(&mut b, 76, 13);
        put32(&mut b, 80, 8);
        pointer(&mut b, 84, 224);
        put32(&mut b, 88, 1);
        pointer(&mut b, 96, 272);
        put32(&mut b, 104, 0x1234);
        put32(&mut b, 108, 8);
        pointer(&mut b, 112, 144);
        put32(&mut b, 116, 2);
        pointer(&mut b, 144, 288);
        b[152..154].copy_from_slice(&10u16.to_le_bytes());
        put32(&mut b, 156, 0);
        pointer(&mut b, 164, 304);
        b[172..174].copy_from_slice(&0u16.to_le_bytes());
        put32(&mut b, 176, 4);
        b[256..264].copy_from_slice(b"Default\0");
        b[272..280].copy_from_slice(b"Fixture\0");
        b[288..294].copy_from_slice(b"Float\0");
        b[304..312].copy_from_slice(b"Enabled\0");
        put32(&mut b, 224, 0x7fc12345);
        b[228] = 2;
        b
    }
    #[test]
    fn simdata_bits_invalid_boolean_and_unknown_source_are_not_repaired() {
        let b = fixture();
        let d = parse_simdata(&b, &AtomicBool::new(false)).unwrap();
        let n = &d.instances[0].1;
        assert_eq!(
            n.field("Float").unwrap().unwrap().float_bits().unwrap(),
            0x7fc12345
        );
        assert!(n.field("Enabled").unwrap().unwrap().bool_value().is_err());
        assert_eq!(b, fixture());
    }
    #[test]
    fn simdata_truncated_tables_cycles_duplicate_fields_and_cancel_fail() {
        let b = fixture();
        for end in [0, 4, 24, 68, 110, 180, 230] {
            assert!(parse_simdata(&b[..end], &AtomicBool::new(false)).is_err());
        }
        let mut cycle = b.clone();
        cycle[152..154].copy_from_slice(&13u16.to_le_bytes());
        put32(&mut cycle, 224, 0);
        assert!(
            parse_simdata(&cycle, &AtomicBool::new(false))
                .unwrap_err()
                .contains("cycle")
        );
        let mut duplicate = b.clone();
        pointer(&mut duplicate, 164, 288);
        assert!(parse_simdata(&duplicate, &AtomicBool::new(false)).is_err());
        assert!(parse_simdata(&b, &AtomicBool::new(true)).is_err());
    }
}
