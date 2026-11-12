// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded PackedXml DATA reader. Layout/reference semantics follow maintained
//! S4TK models 4345132 (MIT); no row-size-only schema fallback is permitted.
use crate::{
    check,
    tuning::{inspect_nodes, Node},
    CharacterError, CombinedXmlInspection, Result,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
};
const NULL: i32 = i32::MIN;
#[derive(Clone, Copy)]
struct Table {
    start: usize,
    size: usize,
    count: usize,
}
fn invalid(s: impl Into<String>) -> CharacterError {
    CharacterError::Invalid(s.into())
}
fn unsupported(s: impl Into<String>) -> CharacterError {
    CharacterError::Unsupported(s.into())
}
struct Packed<'a> {
    raw: &'a [u8],
    tables: [Table; 7],
    first: usize,
    root: usize,
    cancel: &'a AtomicBool,
    steps: usize,
    string_bytes: usize,
    strings: BTreeMap<u32, String>,
    nodes: Vec<Node>,
}
impl<'a> Packed<'a> {
    fn bytes(raw: &[u8], at: usize, n: usize) -> Result<&[u8]> {
        raw.get(
            at..at
                .checked_add(n)
                .ok_or_else(|| invalid("DATA extent overflow"))?,
        )
        .ok_or_else(|| invalid("Truncated DATA extent"))
    }
    fn u32(raw: &[u8], at: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(
            Self::bytes(raw, at, 4)?.try_into().unwrap(),
        ))
    }
    fn i32(raw: &[u8], at: usize) -> Result<i32> {
        Ok(Self::u32(raw, at)? as i32)
    }
    fn relative(raw: &[u8], at: usize) -> Result<Option<usize>> {
        let delta = Self::i32(raw, at)?;
        if delta == NULL {
            return Ok(None);
        }
        let p = (at as i64)
            .checked_add(delta as i64)
            .ok_or_else(|| invalid("DATA relative overflow"))?;
        if p < 0 || p as usize > raw.len() {
            return Err(invalid("DATA relative target extent"));
        }
        Ok(Some(p as usize))
    }
    fn cstr(raw: &[u8], at: usize, end: usize, max: usize) -> Result<String> {
        if at >= end || end > raw.len() {
            return Err(invalid("DATA string extent"));
        }
        let region = &raw[at..end.min(at.saturating_add(max + 1))];
        let len = region
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| invalid("DATA string terminator/budget"))?;
        String::from_utf8(region[..len].to_vec()).map_err(|_| invalid("DATA string UTF-8"))
    }
    fn new(raw: &'a [u8], cancel: &'a AtomicBool) -> Result<Self> {
        check(cancel)?;
        if raw.len() > 32 * 1024 * 1024 || raw.get(..4) != Some(b"DATA") {
            return Err(unsupported("Combined DATA magic/byte budget"));
        }
        let version = Self::u32(raw, 4)?;
        let header = match version {
            0x100 => 24,
            0x101 => 28,
            _ => return Err(unsupported(format!("DATA version {version:08X}"))),
        };
        Self::bytes(raw, 0, header)?;
        let to = Self::relative(raw, 8)?.ok_or_else(|| invalid("DATA null table offset"))?;
        let so = Self::relative(raw, 16)?.ok_or_else(|| invalid("DATA null schema offset"))?;
        if Self::u32(raw, 12)? != 7 || Self::u32(raw, 20)? != 3 {
            return Err(unsupported(
                "PackedXml requires seven tables and three declared schemas",
            ));
        }
        if to < header || so < header {
            return Err(invalid("DATA header/table overlap"));
        }
        Self::bytes(raw, to, 7 * 28)?;
        Self::bytes(raw, so, 3 * 24)?;
        let expected: [&[(&str, u16, usize)]; 3] = [
            &[
                ("first_element", 13, 0),
                ("top_element", 13, 4),
                ("element_count", 7, 8),
                ("string_table", 14, 12),
            ],
            &[("text", 7, 0), ("attrs", 13, 4), ("children", 13, 8)],
            &[("name", 7, 0), ("value", 7, 4)],
        ];
        let sizes = [20, 12, 8, 4, 4, 4, 1];
        let types = [13, 13, 13, 13, 13, 11, 1];
        let mut schemas = BTreeMap::new();
        for i in 0..3 {
            let p = so + i * 24;
            let size = Self::u32(raw, p + 12)? as usize;
            let cols =
                Self::relative(raw, p + 16)?.ok_or_else(|| invalid("DATA null schema columns"))?;
            let count = Self::u32(raw, p + 20)? as usize;
            if count > 64 {
                return Err(unsupported("DATA schema column budget"));
            }
            Self::bytes(raw, cols, count * 20)?;
            let mut entries = BTreeMap::new();
            for c in (0..count).map(|j| cols + j * 20) {
                let np = Self::relative(raw, c)?.ok_or_else(|| invalid("DATA null column name"))?;
                let name = Self::cstr(raw, np, raw.len(), 128)?;
                let typ = u16::from_le_bytes(Self::bytes(raw, c + 8, 2)?.try_into().unwrap());
                let offset = Self::u32(raw, c + 12)? as usize;
                if offset >= size || entries.insert(name, (typ, offset)).is_some() {
                    return Err(invalid("DATA duplicate/invalid column"));
                }
            }
            schemas.insert(p, (size, entries));
        }
        let mut tables = [Table {
            start: 0,
            size: 0,
            count: 0,
        }; 7];
        let mut previous = 0;
        for i in 0..7 {
            let p = to + i * 28;
            let schema = Self::relative(raw, p + 8)?;
            let size = Self::u32(raw, p + 16)? as usize;
            if size != sizes[i] || Self::u32(raw, p + 12)? != types[i] {
                return Err(unsupported(format!("PackedXml table {i} layout")));
            }
            let unaligned =
                Self::relative(raw, p + 20)?.ok_or_else(|| invalid("DATA null rows"))?;
            let start = unaligned
                .checked_add(15)
                .ok_or_else(|| invalid("DATA alignment overflow"))?
                & !15;
            let count = Self::u32(raw, p + 24)? as usize;
            let bytes = size
                .checked_mul(count)
                .ok_or_else(|| invalid("DATA table extent overflow"))?;
            Self::bytes(raw, start, bytes)?;
            if start < to + 7 * 28 || (i > 0 && start < previous) {
                return Err(invalid("DATA overlapping/out-of-order table rows"));
            }
            previous = start + bytes;
            if i < 3 {
                let (ss, columns) = schema.and_then(|s| schemas.get(&s)).ok_or_else(|| {
                    unsupported(format!("PackedXml table {i} references no declared schema"))
                })?;
                if *ss != size
                    || columns.len() != expected[i].len()
                    || expected[i]
                        .iter()
                        .any(|(n, t, o)| columns.get(*n) != Some(&(*t, *o)))
                {
                    return Err(unsupported(format!("PackedXml table {i} column schema")));
                }
            } else if schema.is_some() {
                return Err(unsupported("PackedXml primitive table has schema"));
            }
            tables[i] = Table { start, size, count };
        }
        if tables[0].count != 1 {
            return Err(unsupported("PackedXml document count"));
        }
        let mut out = Self {
            raw,
            tables,
            first: 0,
            root: 0,
            cancel,
            steps: 0,
            string_bytes: 0,
            strings: BTreeMap::new(),
            nodes: vec![],
        };
        out.first = out
            .pointer(tables[0].start, 1)?
            .ok_or_else(|| invalid("DATA first element absent"))?;
        out.root = out
            .pointer(tables[0].start + 4, 1)?
            .ok_or_else(|| invalid("DATA root absent"))?;
        Ok(out)
    }
    fn step(&mut self) -> Result<()> {
        self.steps += 1;
        if self.steps > 6_000_000 {
            return Err(invalid("PackedXml reference budget"));
        }
        if self.steps % 1024 == 0 {
            check(self.cancel)?
        }
        Ok(())
    }
    fn row(&self, p: usize, table: usize) -> Result<usize> {
        let t = self.tables[table];
        if p < t.start || (p - t.start) % t.size != 0 || (p - t.start) / t.size >= t.count {
            return Err(invalid(format!(
                "Invalid DATA row reference in table {table}"
            )));
        }
        Ok(p)
    }
    fn pointer(&self, p: usize, table: usize) -> Result<Option<usize>> {
        Self::relative(self.raw, p)?
            .map(|p| self.row(p, table))
            .transpose()
    }
    fn refs(
        &mut self,
        start: Option<usize>,
        table: usize,
        target: usize,
        max: usize,
    ) -> Result<Vec<usize>> {
        let Some(mut p) = start else {
            return Ok(vec![]);
        };
        self.row(p, table)?;
        let mut out = vec![];
        let t = self.tables[table];
        while p < t.start + t.count * t.size {
            self.step()?;
            let Some(v) = self.pointer(p, target)? else {
                return Ok(out);
            };
            if out.len() >= max {
                return Err(invalid("PackedXml list budget"));
            }
            out.push(v);
            p += 4;
        }
        Err(invalid("DATA unterminated reference list"))
    }
    fn text(&mut self, id: u32) -> Result<String> {
        self.step()?;
        if let Some(s) = self.strings.get(&id) {
            return Ok(s.clone());
        }
        let t = self.tables[5];
        if id as usize >= t.count {
            return Err(invalid("DATA string index"));
        }
        let p = Self::relative(self.raw, t.start + id as usize * 4)?
            .ok_or_else(|| invalid("DATA null string"))?;
        let chars = self.tables[6];
        if p < chars.start {
            return Err(invalid("DATA string outside character table"));
        }
        let s = Self::cstr(self.raw, p, chars.start + chars.count, 65536)?;
        self.string_bytes += s.len();
        if self.string_bytes > 128 * 1024 * 1024 {
            return Err(invalid("DATA string byte budget"));
        }
        if self.strings.len() < 32768 {
            self.strings.insert(id, s.clone());
        }
        Ok(s)
    }
    fn attr(&mut self, p: usize) -> Result<BTreeMap<String, String>> {
        let start = self.pointer(p + 4, 4)?;
        let mut out = BTreeMap::new();
        for a in self.refs(start, 4, 2, 128)? {
            let name = self.text(Self::u32(self.raw, a)?)?;
            let value = self.text(Self::u32(self.raw, a + 4)?)?;
            if out.insert(name, value).is_some() {
                return Err(invalid("PackedXml duplicate attribute"));
            }
        }
        Ok(out)
    }
    fn children(&mut self, p: usize) -> Result<Vec<usize>> {
        let start = self.pointer(p + 8, 3)?;
        self.refs(start, 3, 1, 1_500_000)
    }
    fn tag(&mut self, p: usize) -> Result<String> {
        self.row(p, 1)?;
        if p < self.first {
            return Err(invalid("Expected PackedXml element"));
        }
        self.text(Self::u32(self.raw, p)?)
    }
    fn capture(&mut self, p: usize, path: &mut BTreeSet<usize>, depth: usize) -> Result<usize> {
        self.step()?;
        if depth > 64 || self.nodes.len() >= 500_000 {
            return Err(invalid("Selective PackedXml depth/node budget"));
        }
        if !path.insert(p) {
            return Err(invalid("PackedXml node reference cycle"));
        }
        let tag = self.tag(p)?;
        let attrs = self.attr(p)?;
        let id = self.nodes.len();
        self.nodes.push(Node {
            tag,
            attrs,
            text: String::new(),
            children: vec![],
        });
        for child in self.children(p)? {
            if child < self.first {
                if self.pointer(child + 4, 4)?.is_some() || self.pointer(child + 8, 3)?.is_some() {
                    return Err(invalid("PackedXml text node has element fields"));
                }
                let text = self.text(Self::u32(self.raw, child)?)?;
                if self.nodes[id].text.len() + text.len() > 65536 {
                    return Err(invalid("PackedXml node text budget"));
                }
                self.nodes[id].text.push_str(&text);
            } else {
                let c = self.capture(child, path, depth + 1)?;
                self.nodes[id].children.push(c);
            }
        }
        path.remove(&p);
        Ok(id)
    }
    fn capture_nodes(mut self, classes: &[&str]) -> Result<crate::tuning::NodeInput> {
        if self.tag(self.root)? != "combined" {
            return Err(unsupported("DATA root is not combined"));
        }
        let mut roots = vec![];
        let mut groups = vec![];
        for p in self.children(self.root)? {
            match self.tag(p)?.as_str() {
                "g" => groups.push(self.capture(p, &mut BTreeSet::new(), 0)?),
                "R" => {
                    let namespace = self.attr(p)?.get("n").cloned().unwrap_or_default();
                    for r in self.children(p)? {
                        if self.tag(r)? == "I"
                            && self
                                .attr(r)?
                                .get("c")
                                .is_some_and(|c| classes.contains(&c.as_str()))
                        {
                            if roots.len() >= 4096 {
                                return Err(invalid("PackedXml modifier record budget"));
                            }
                            roots.push((
                                self.capture(r, &mut BTreeSet::new(), 0)?,
                                namespace.clone(),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok((self.nodes, roots, groups,false))
    }
}
/// Reads only the proven seven-table PackedXml layout and retains exact
/// unknown context tokens. Native record applicability/precedence is separate.
pub fn inspect_combined_data(bytes: &[u8], cancel: &AtomicBool) -> Result<CombinedXmlInspection> {
    let (nodes, roots, groups, _) =
        read_combined_data_nodes(bytes, &["Client_CASModifierTuning"], cancel)?;
    inspect_nodes(nodes, roots, groups, cancel)
}
pub(crate) fn read_combined_data_nodes(
    bytes: &[u8],
    classes: &[&str],
    cancel: &AtomicBool,
) -> Result<crate::tuning::NodeInput> {
    Packed::new(bytes, cancel)?.capture_nodes(classes)
}
pub fn inspect_combined_tuning(bytes: &[u8], cancel: &AtomicBool) -> Result<CombinedXmlInspection> {
    if bytes.starts_with(b"DATA") {
        inspect_combined_data(bytes, cancel)
    } else {
        crate::inspect_combined_xml(bytes, cancel)
    }
}
/// Retains a schema-checked PackedXml document (such as an installed ASM)
/// without assigning combined-tuning shared-reference semantics to its tags.
pub fn inspect_packed_xml_document(bytes:&[u8],cancel:&AtomicBool)->Result<crate::ResolvedTuningNode>{
    let mut reader=Packed::new(bytes,cancel)?;
    let root=reader.capture(reader.root,&mut BTreeSet::new(),0)?;
    fn copy(nodes:&[Node],id:usize)->crate::ResolvedTuningNode{
        let n=&nodes[id];crate::ResolvedTuningNode{tag:n.tag.clone(),attrs:n.attrs.clone(),text:n.text.clone(),children:n.children.iter().map(|i|copy(nodes,*i)).collect()}
    }
    check(cancel)?;Ok(copy(&reader.nodes,root))
}
