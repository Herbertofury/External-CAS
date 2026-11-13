//! Native, read-only Tray inspection and lossless disposable project snapshots.
//!
//! Original wire bytes, wrappers and portraits are retained verbatim. This crate
//! does not regenerate native households or claim native game acceptance.
//! See PROVENANCE.md for the schema evidence and remaining compatibility limits.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

mod draft;
pub use draft::{
    ColorShiftStorage, MissingColorShiftPolicy, OutfitColorShift, OutfitSlot, ProjectDraft,
    COLOR_SHIFT_SCHEMA_EVIDENCE_ID, DRAFT_SCHEMA_EVIDENCE_ID,
};
mod appearance;
pub use appearance::{
    AppearanceForm, AppearanceModifier, FacialCustomization, GeneticAppearance, GeneticPart,
    NativeField, NativeMessage, NativeTraitIds, OutfitAppearance, OutfitLayerIds, OutfitPartAppearance, SimAppearance,
};

pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PROJECT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_FIELDS: usize = 1_000_000;
const PROJECT_MANIFEST: &str = "ExternalCasProject.json";

#[derive(Debug)]
pub enum TrayError {
    Io(std::io::Error),
    Invalid(String),
    Conflict(String),
    Cancelled,
}
impl fmt::Display for TrayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Invalid(s) => write!(f, "{s}"),
            Self::Conflict(s) => write!(f, "{s}"),
            Self::Cancelled => write!(f, "Tray operation cancelled"),
        }
    }
}
impl std::error::Error for TrayError {}
impl From<std::io::Error> for TrayError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
pub type Result<T> = std::result::Result<T, TrayError>;
fn invalid(s: impl Into<String>) -> TrayError {
    TrayError::Invalid(s.into())
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayKind {
    Metadata,
    Household,
    HouseholdPortrait,
    SimPortrait,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayName {
    pub group: u32,
    pub instance: u64,
    pub kind: TrayKind,
}
impl TrayName {
    pub fn parse(name: &str) -> Result<Self> {
        let lower = name.to_ascii_lowercase();
        let (stem, ext) = lower
            .rsplit_once('.')
            .ok_or_else(|| invalid("Tray filename has no extension"))?;
        if !stem.is_ascii() || stem.len() != 29 || !stem.starts_with("0x") || &stem[10..13] != "!0x"
        {
            return Err(invalid(
                "Expected exact native Tray filename 0xGGGGGGGG!0xIIIIIIIIIIIIIIII.ext",
            ));
        }
        let group =
            u32::from_str_radix(&stem[2..10], 16).map_err(|_| invalid("Invalid Tray group"))?;
        let instance =
            u64::from_str_radix(&stem[13..], 16).map_err(|_| invalid("Invalid Tray instance"))?;
        let kind = match ext {
            "trayitem" => TrayKind::Metadata,
            "householdbinary" => TrayKind::Household,
            "hhi" => TrayKind::HouseholdPortrait,
            "sgi" => TrayKind::SimPortrait,
            _ => return Err(invalid("Unsupported Tray file type")),
        };
        Ok(Self {
            group,
            instance,
            kind,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimMetadata {
    pub sim_id: Option<u64>,
    pub household_id: Option<u64>,
    pub first_name: String,
    pub last_name: String,
    pub gender: Option<u64>,
    pub age: Option<u64>,
    pub skin_tone: Option<u64>,
    /// Hash of the complete original SimData message, including unknown fields.
    pub native_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddedImage {
    pub id: u64,
    pub offset: usize,
    pub size: usize,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HouseholdSummary {
    pub version: u32,
    pub account_id: u64,
    pub family_id: Option<u64>,
    pub family_name: String,
    pub last_played_sim_id: Option<u64>,
    pub sims: Vec<SimMetadata>,
    pub payload_range: Range<usize>,
    pub suffix_range: Range<usize>,
    /// None means an opaque suffix was retained but its structure is unknown.
    pub embedded_images: Option<Vec<EmbeddedImage>>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataSummary {
    pub header_type: u32,
    pub tray_id: Option<u64>,
    pub item_type: Option<u64>,
    pub name: String,
    pub declared_family_size: Option<u64>,
    /// These metadata IDs are deliberately separate from SimData.sim_id.
    pub sims: Vec<SimMetadata>,
    pub has_household_metadata: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum DocumentSummary {
    Household(HouseholdSummary),
    Metadata(MetadataSummary),
    Portrait { container: String, decoded: bool },
}
#[derive(Clone, Debug)]
pub struct TrayDocument {
    pub filename: String,
    pub sha256: String,
    pub summary: DocumentSummary,
    bytes: Vec<u8>,
}
impl TrayDocument {
    pub fn from_bytes(filename: &str, bytes: Vec<u8>) -> Result<Self> {
        if bytes.is_empty() || bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(invalid("Empty or oversized Tray file"));
        }
        let name = TrayName::parse(filename)?;
        let summary = match name.kind {
            TrayKind::Household => DocumentSummary::Household(parse_household(&bytes)?),
            TrayKind::Metadata => DocumentSummary::Metadata(parse_metadata(&bytes)?),
            TrayKind::HouseholdPortrait | TrayKind::SimPortrait => {
                let container = if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
                    "jpeg"
                } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                    "png"
                } else if bytes.len() >= 24 && u32_at(&bytes, 0)? as usize == bytes.len() - 24 {
                    "wrapped_portrait"
                } else {
                    "opaque_portrait"
                };
                DocumentSummary::Portrait {
                    container: container.to_string(),
                    decoded: false,
                }
            }
        };
        Ok(Self {
            filename: filename.to_string(),
            sha256: sha256(&bytes),
            summary,
            bytes,
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Raw SimData is exposed without reconstructing or losing unsupported state.
    pub fn native_sims(&self) -> Result<Vec<&[u8]>> {
        match &self.summary {
            DocumentSummary::Household(h) => {
                let family = fields(&self.bytes[h.payload_range.clone()])?;
                let account = required_blob(&family, 1)?;
                Ok(fields(account)?
                    .iter()
                    .filter(|f| f.number == 6)
                    .map(|f| f.bytes())
                    .collect::<Result<Vec<_>>>()?)
            }
            _ => Err(invalid("This document is not a household")),
        }
    }
}

#[derive(Clone, Debug)]
struct Field<'a> {
    number: u32,
    wire: u8,
    value: &'a [u8],
    integer: Option<u64>,
    raw: &'a [u8],
}
impl<'a> Field<'a> {
    fn bytes(&self) -> Result<&'a [u8]> {
        if self.wire == 2 {
            Ok(self.value)
        } else {
            Err(invalid(
                "Known message/string field has a changed wire type",
            ))
        }
    }
}
fn varint(raw: &[u8], at: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    for i in 0..10 {
        let b = *raw
            .get(*at)
            .ok_or_else(|| invalid("Truncated protobuf varint"))?;
        *at += 1;
        if i == 9 && b > 1 {
            return Err(invalid("Overflowing protobuf varint"));
        }
        value |= ((b & 127) as u64) << (i * 7);
        if b & 128 == 0 {
            return Ok(value);
        }
    }
    Err(invalid("Overlong protobuf varint"))
}
fn advance(raw: &[u8], at: &mut usize, count: usize) -> Result<Range<usize>> {
    let end = at
        .checked_add(count)
        .ok_or_else(|| invalid("Extent overflow"))?;
    if end > raw.len() {
        return Err(invalid("Truncated protobuf field"));
    }
    let range = *at..end;
    *at = end;
    Ok(range)
}
fn skip_group(
    raw: &[u8],
    at: &mut usize,
    owner: u32,
    depth: usize,
    budget: &mut usize,
) -> Result<()> {
    if depth > 32 {
        return Err(invalid("Protobuf group nesting exceeds limit"));
    }
    while *at < raw.len() {
        *budget += 1;
        if *budget > MAX_FIELDS {
            return Err(invalid("Protobuf field count exceeds limit"));
        }
        let tag = varint(raw, at)?;
        let number = (tag >> 3) as u32;
        let wire = (tag & 7) as u8;
        if number == 0 || tag >> 3 >= 1 << 29 {
            return Err(invalid("Invalid protobuf tag"));
        }
        match wire {
            4 => {
                return if number == owner {
                    Ok(())
                } else {
                    Err(invalid("Mismatched protobuf end group"))
                }
            }
            0 => {
                varint(raw, at)?;
            }
            1 => {
                advance(raw, at, 8)?;
            }
            2 => {
                let n =
                    usize::try_from(varint(raw, at)?).map_err(|_| invalid("Extent overflow"))?;
                advance(raw, at, n)?;
            }
            3 => skip_group(raw, at, number, depth + 1, budget)?,
            5 => {
                advance(raw, at, 4)?;
            }
            _ => return Err(invalid("Invalid protobuf wire type")),
        }
    }
    Err(invalid("Unterminated protobuf group"))
}
fn fields(raw: &[u8]) -> Result<Vec<Field<'_>>> {
    let mut result = Vec::new();
    let mut at = 0;
    let mut budget = 0;
    while at < raw.len() {
        let start = at;
        budget += 1;
        if budget > MAX_FIELDS {
            return Err(invalid("Protobuf field count exceeds limit"));
        }
        let tag = varint(raw, &mut at)?;
        let number = (tag >> 3) as u32;
        let wire = (tag & 7) as u8;
        if number == 0 || tag >> 3 >= 1 << 29 {
            return Err(invalid("Invalid protobuf tag"));
        }
        let mut integer = None;
        let range = match wire {
            0 => {
                let start = at;
                integer = Some(varint(raw, &mut at)?);
                start..at
            }
            1 => {
                let r = advance(raw, &mut at, 8)?;
                integer = Some(u64::from_le_bytes(raw[r.clone()].try_into().unwrap()));
                r
            }
            2 => {
                let n = usize::try_from(varint(raw, &mut at)?)
                    .map_err(|_| invalid("Extent overflow"))?;
                advance(raw, &mut at, n)?
            }
            3 => {
                let start = at;
                skip_group(raw, &mut at, number, 1, &mut budget)?;
                start..at
            }
            5 => advance(raw, &mut at, 4)?,
            _ => return Err(invalid("Invalid protobuf wire type")),
        };
        result.push(Field {
            number,
            wire,
            value: &raw[range],
            integer,
            raw: &raw[start..at],
        });
    }
    Ok(result)
}
fn unique<'a, 'b>(fields: &'b [Field<'a>], number: u32) -> Result<Option<&'b Field<'a>>> {
    let mut selected = None;
    for field in fields.iter().filter(|f| f.number == number) {
        if selected.is_some() {
            return Err(invalid(format!("Ambiguous singular field {number}")));
        }
        selected = Some(field);
    }
    Ok(selected)
}
fn required_blob<'a>(fields: &[Field<'a>], n: u32) -> Result<&'a [u8]> {
    unique(fields, n)?
        .ok_or_else(|| invalid(format!("Required message field {n} is absent")))?
        .bytes()
}
fn number(fields: &[Field<'_>], n: u32, wire: u8) -> Result<Option<u64>> {
    unique(fields, n)?
        .map(|f| {
            if f.wire == wire {
                f.integer.ok_or_else(|| invalid("Invalid integer wire"))
            } else {
                Err(invalid(format!(
                    "Known integer field {n} has a changed wire type"
                )))
            }
        })
        .transpose()
}
fn text(fields: &[Field<'_>], n: u32) -> Result<String> {
    let Some(f) = unique(fields, n)? else {
        return Ok(String::new());
    };
    let s = std::str::from_utf8(f.bytes()?).map_err(|_| invalid("Invalid UTF-8 name"))?;
    Ok(s.to_string())
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| invalid("Truncated native header"))?
            .try_into()
            .unwrap(),
    ))
}

fn parse_sim(raw: &[u8], metadata: bool) -> Result<SimMetadata> {
    let f = fields(raw)?;
    if metadata {
        Ok(SimMetadata {
            sim_id: number(&f, 5, 0)?,
            first_name: text(&f, 3)?,
            last_name: text(&f, 4)?,
            gender: number(&f, 6, 0)?,
            age: number(&f, 9, 0)?,
            native_sha256: sha256(raw),
            ..Default::default()
        })
    } else {
        let id = number(&f, 1, 1)?
            .ok_or_else(|| invalid("Required fixed64 SimData.sim_id is absent"))?;
        Ok(SimMetadata {
            sim_id: Some(id),
            household_id: number(&f, 4, 1)?,
            first_name: text(&f, 5)?,
            last_name: text(&f, 6)?,
            gender: number(&f, 7, 0)?,
            age: number(&f, 8, 0)?,
            skin_tone: number(&f, 10, 0)?,
            native_sha256: sha256(raw),
        })
    }
}
fn parse_images(bytes: &[u8], base: usize) -> Result<Vec<EmbeddedImage>> {
    if bytes.len() < 5 {
        return Err(invalid("Opaque or truncated embedded-image section"));
    }
    let count = u32_at(bytes, 1)? as usize;
    if count > 100_000 {
        return Err(invalid("Embedded image count exceeds limit"));
    }
    let mut at = 5;
    let mut images = Vec::new();
    for _ in 0..count {
        let r = advance(bytes, &mut at, 12)?;
        let id = u64::from_le_bytes(bytes[r.start..r.start + 8].try_into().unwrap());
        let size = u32_at(bytes, r.start + 8)? as usize;
        let payload = advance(bytes, &mut at, size)?;
        images.push(EmbeddedImage {
            id,
            offset: base + payload.start,
            size,
            sha256: sha256(&bytes[payload]),
        });
    }
    if at != bytes.len() {
        return Err(invalid("Unknown trailing bytes after embedded images"));
    }
    Ok(images)
}
fn parse_household(bytes: &[u8]) -> Result<HouseholdSummary> {
    let version = u32_at(bytes, 0)?;
    let size = u32_at(bytes, 4)? as usize;
    if size != bytes.len() - 4 {
        return Err(invalid("Household header size differs from actual bytes"));
    }
    let payload_range = match version {
        1 => 8..bytes.len(),
        2 => {
            if u32_at(bytes, 8)? != 0 {
                return Err(invalid("Unsupported nonzero household v2 reserved word"));
            }
            let end = 16usize
                .checked_add(u32_at(bytes, 12)? as usize)
                .ok_or_else(|| invalid("Household extent overflow"))?;
            if end > bytes.len() {
                return Err(invalid("Truncated v2 household payload"));
            }
            16..end
        }
        _ => {
            return Err(invalid(format!(
                "Unsupported household version {version}; bytes must remain opaque"
            )))
        }
    };
    let suffix_range = payload_range.end..bytes.len();
    let f = fields(&bytes[payload_range.clone()])?;
    let account = fields(required_blob(&f, 1)?)?;
    let account_id = number(&account, 1, 0)?
        .ok_or_else(|| invalid("Required AccountFamilyData.accountid is absent"))?;
    let mut sims = Vec::new();
    let mut ids = BTreeSet::new();
    for field in account.iter().filter(|f| f.number == 6) {
        let sim = parse_sim(field.bytes()?, false)?;
        if !ids.insert(sim.sim_id) {
            return Err(invalid("Duplicate SimData.sim_id in household"));
        }
        sims.push(sim);
    }
    let mut warnings = Vec::new();
    let embedded_images = if version == 1 {
        None
    } else {
        match parse_images(&bytes[suffix_range.clone()], suffix_range.start) {
            Ok(images) => Some(images),
            Err(e) => {
                warnings.push(format!("Embedded image suffix retained opaque: {e}"));
                None
            }
        }
    };
    Ok(HouseholdSummary {
        version,
        account_id,
        family_id: number(&account, 2, 0)?,
        family_name: text(&account, 3)?,
        last_played_sim_id: number(&f, 3, 0)?,
        sims,
        payload_range,
        suffix_range,
        embedded_images,
        warnings,
    })
}
fn parse_metadata(bytes: &[u8]) -> Result<MetadataSummary> {
    let header_type = u32_at(bytes, 0)?;
    if u32_at(bytes, 4)? as usize != bytes.len() - 8 {
        return Err(invalid(
            "Tray metadata header size differs from actual bytes",
        ));
    }
    let f = fields(&bytes[8..])?;
    let mut sims = Vec::new();
    let mut declared_family_size = None;
    let mut has_household_metadata = false;
    if let Some(specific) = unique(&f, 10)? {
        let specific = fields(specific.bytes()?)?;
        if let Some(hh) = unique(&specific, 2)? {
            has_household_metadata = true;
            let hh = fields(hh.bytes()?)?;
            declared_family_size = number(&hh, 1, 0)?;
            for field in hh.iter().filter(|f| f.number == 2) {
                sims.push(parse_sim(field.bytes()?, true)?);
            }
        }
    }
    Ok(MetadataSummary {
        header_type,
        tray_id: number(&f, 1, 0)?,
        item_type: number(&f, 2, 0)?,
        name: text(&f, 4)?,
        declared_family_size,
        sims,
        has_household_metadata,
    })
}

/// Refuse symbolic links, junctions and other Windows reparse points along a path.
fn plain_path(path: &Path) -> Result<()> {
    for part in path.ancestors() {
        if part.as_os_str().is_empty() {
            continue;
        }
        let md = fs::symlink_metadata(part)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if md.file_attributes() & 0x400 != 0 {
                return Err(invalid(
                    "Reparse paths are not supported for native snapshots",
                ));
            }
        }
        if md.file_type().is_symlink() {
            return Err(invalid(
                "Symbolic link paths are not supported for native snapshots",
            ));
        }
    }
    Ok(())
}
fn read_stable(path: &Path) -> Result<Vec<u8>> {
    plain_path(path)?;
    let before = fs::metadata(path)?;
    if !before.is_file() || before.len() == 0 || before.len() > MAX_FILE_BYTES {
        return Err(invalid("Empty, non-file or oversized Tray input"));
    }
    let mut file = File::open(path)?;
    let opened = file.metadata()?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    let named_after = fs::metadata(path)?;
    if bytes.len() as u64 != before.len()
        || opened.len() != after.len()
        || before.modified().ok() != named_after.modified().ok()
        || before.len() != named_after.len()
        || after.modified().ok() != opened.modified().ok()
    {
        return Err(TrayError::Conflict(
            "Source changed while being read".into(),
        ));
    }
    // A second complete read checks bytes rather than relying only on timestamps.
    let mut second = Vec::new();
    File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut second)?;
    if second != bytes {
        return Err(TrayError::Conflict(
            "Source bytes changed during snapshot".into(),
        ));
    }
    Ok(bytes)
}
pub fn read_document(path: &Path) -> Result<TrayDocument> {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid("Tray filename is not UTF-8"))?;
    TrayDocument::from_bytes(filename, read_stable(path)?)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrayGroup {
    pub metadata_path: PathBuf,
    pub metadata: MetadataSummary,
    pub household_path: Option<PathBuf>,
    pub portrait_paths: Vec<PathBuf>,
    pub missing: Vec<String>,
}
impl TrayGroup {
    pub fn paths(&self) -> Vec<PathBuf> {
        let mut result = vec![self.metadata_path.clone()];
        if let Some(p) = &self.household_path {
            result.push(p.clone());
        }
        result.extend(self.portrait_paths.iter().cloned());
        result
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryIssue {
    pub path: PathBuf,
    pub message: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TrayLibrary {
    pub groups: Vec<TrayGroup>,
    pub issues: Vec<LibraryIssue>,
    pub files_observed: usize,
}
pub fn scan_library(directory: &Path) -> Result<TrayLibrary> {
    scan_library_with_cancel(directory, &AtomicBool::new(false))
}
pub fn scan_library_with_cancel(directory: &Path, cancel: &AtomicBool) -> Result<TrayLibrary> {
    plain_path(directory)?;
    let directory = fs::canonicalize(directory)?;
    let mut names = BTreeMap::new();
    let mut result = TrayLibrary::default();
    for entry in fs::read_dir(&directory)? {
        if cancel.load(Ordering::Relaxed) {
            return Err(TrayError::Cancelled);
        }
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if let Ok(parsed) = TrayName::parse(&name) {
            result.files_observed += 1;
            names.insert(name.to_ascii_lowercase(), (entry.path(), parsed));
        }
    }
    for (path, name) in names.values().filter(|(_, n)| n.kind == TrayKind::Metadata) {
        if cancel.load(Ordering::Relaxed) {
            return Err(TrayError::Cancelled);
        }
        let doc = match read_document(path) {
            Ok(doc) => doc,
            Err(e) => {
                result.issues.push(LibraryIssue {
                    path: path.clone(),
                    message: e.to_string(),
                });
                continue;
            }
        };
        let DocumentSummary::Metadata(metadata) = doc.summary else {
            unreachable!()
        };
        let Some(id) = metadata.tray_id else {
            result.issues.push(LibraryIssue {
                path: path.clone(),
                message: "Metadata has no Tray ID; family cannot be associated safely".into(),
            });
            continue;
        };
        if id != name.instance {
            result.issues.push(LibraryIssue {
                path: path.clone(),
                message: "Filename and metadata Tray IDs differ; not grouped automatically".into(),
            });
            continue;
        }
        let household = names
            .values()
            .filter(|(_, n)| n.kind == TrayKind::Household && n.instance == id)
            .collect::<Vec<_>>();
        if household.len() > 1 {
            result.issues.push(LibraryIssue {
                path: path.clone(),
                message: "Multiple household providers; family remains ambiguous".into(),
            });
            continue;
        }
        if !metadata.has_household_metadata && household.is_empty() {
            continue;
        }
        let household_path = household.first().map(|(p, _)| p.clone());
        let mut missing = Vec::new();
        if household_path.is_none() {
            missing.push(format!("0x00000000!0x{id:016x}.householdbinary"));
        }
        let ids = metadata
            .sims
            .iter()
            .filter_map(|s| s.sim_id)
            .collect::<BTreeSet<_>>();
        let portraits = names
            .values()
            .filter(|(_, n)| {
                (n.kind == TrayKind::HouseholdPortrait && n.instance == id)
                    || (n.kind == TrayKind::SimPortrait && ids.contains(&n.instance))
            })
            .map(|(p, _)| p.clone())
            .collect::<Vec<_>>();
        result.groups.push(TrayGroup {
            metadata_path: path.clone(),
            metadata,
            household_path,
            portrait_paths: portraits,
            missing,
        });
    }
    result.groups.sort_by(|a, b| {
        a.metadata
            .name
            .to_lowercase()
            .cmp(&b.metadata.name.to_lowercase())
            .then(a.metadata_path.cmp(&b.metadata_path))
    });
    Ok(result)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    pub filename: String,
    pub original_path: PathBuf,
    pub sha256: String,
    pub size: u64,
    /// Original user source pin, separate from this project's current bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_size: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    pub schema: u32,
    pub metadata_filename: String,
    pub files: Vec<ProjectFile>,
    pub native_editing_qualified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<DraftProvenance>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftProvenance {
    pub schema_evidence_id: String,
    pub revision: u64,
    pub parent_manifest_sha256: String,
    pub native_game_acceptance_verified: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operation_schema_evidence_ids: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct TrayProject {
    pub directory: PathBuf,
    pub manifest: ProjectManifest,
    pub documents: Vec<TrayDocument>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceConflict {
    pub path: PathBuf,
    pub expected_sha256: String,
    pub observed_sha256: Option<String>,
    pub reason: String,
}
impl TrayProject {
    pub fn household(&self) -> Option<&HouseholdSummary> {
        self.documents.iter().find_map(|d| {
            if let DocumentSummary::Household(h) = &d.summary {
                Some(h)
            } else {
                None
            }
        })
    }
    pub fn check_source_conflicts(&self) -> Vec<SourceConflict> {
        self.manifest
            .files
            .iter()
            .filter_map(|f| match read_stable(&f.original_path) {
                Ok(bytes) => {
                    let observed = sha256(&bytes);
                    let baseline = f.original_sha256.as_ref().unwrap_or(&f.sha256);
                    if &observed == baseline {
                        None
                    } else {
                        Some(SourceConflict {
                            path: f.original_path.clone(),
                            expected_sha256: baseline.clone(),
                            observed_sha256: Some(observed),
                            reason: "Original source has changed since import".into(),
                        })
                    }
                }
                Err(e) => Some(SourceConflict {
                    path: f.original_path.clone(),
                    expected_sha256: f.original_sha256.as_ref().unwrap_or(&f.sha256).clone(),
                    observed_sha256: None,
                    reason: e.to_string(),
                }),
            })
            .collect()
    }
}
fn external_destination(destination: &Path) -> Result<PathBuf> {
    if !destination.is_absolute() {
        return Err(invalid("Project destination must be absolute"));
    }
    for c in destination.components() {
        match c {
            Component::ParentDir => {
                return Err(invalid(
                    "Project destination cannot contain parent traversal",
                ))
            }
            Component::Normal(s) => {
                if s.to_string_lossy().eq_ignore_ascii_case("the sims 4") {
                    return Err(invalid(
                        "Project snapshots must be outside game, Mods, Tray and save directories",
                    ));
                }
            }
            _ => {}
        }
    }
    if destination
        .file_name()
        .map(|s| {
            ["tray", "mods", "saves"]
                .iter()
                .any(|n| s.to_string_lossy().eq_ignore_ascii_case(n))
        })
        .unwrap_or(true)
    {
        return Err(invalid(
            "Choose a new project folder rather than a Tray, Mods or saves folder",
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("Project destination has no parent"))?;
    plain_path(parent)?;
    let parent = fs::canonicalize(parent)?;
    let name = destination
        .file_name()
        .ok_or_else(|| invalid("Project name is absent"))?;
    Ok(parent.join(name))
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}
/// Copy every associated native file into a new isolated project. No existing
/// path is replaced. Incomplete snapshots retain a recovery marker and fail.
pub fn create_project(group: &TrayGroup, destination: &Path) -> Result<TrayProject> {
    create_project_with_progress(group, destination, |_| true)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotStage {
    ReadingOriginal,
    SnapshotPrepared,
    CopyWritten,
    SourceRevalidated,
    ReadyToCommit,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotProgress {
    pub stage: SnapshotStage,
    pub files_completed: usize,
    pub total_files: usize,
    pub bytes_copied: u64,
    pub total_bytes: Option<u64>,
}

/// A false progress callback cancels before commitment. If copying has begun,
/// partial copies remain explicitly marked incomplete for inspection/recovery.
/// Run this operation off the UI thread; callbacks can forward progress to it.
pub fn create_project_with_progress(
    group: &TrayGroup,
    destination: &Path,
    mut progress: impl FnMut(SnapshotProgress) -> bool,
) -> Result<TrayProject> {
    let destination = external_destination(destination)?;
    let mut entries = Vec::new();
    let mut documents = Vec::new();
    let mut names = BTreeSet::new();
    let mut total = 0;
    let sources = group.paths();
    let total_files = sources.len();
    for source in sources {
        if !progress(SnapshotProgress {
            stage: SnapshotStage::ReadingOriginal,
            files_completed: documents.len(),
            total_files,
            bytes_copied: 0,
            total_bytes: None,
        }) {
            return Err(TrayError::Cancelled);
        }
        if destination.starts_with(source.parent().unwrap_or(&source)) {
            return Err(invalid("Project must not be inside its source library"));
        }
        let doc = read_document(&source)?;
        total += doc.bytes().len() as u64;
        if total > MAX_PROJECT_BYTES {
            return Err(invalid("Project native files exceed snapshot limit"));
        }
        if !names.insert(doc.filename.to_ascii_lowercase()) {
            return Err(invalid("Duplicate snapshot filename"));
        }
        entries.push(ProjectFile {
            filename: doc.filename.clone(),
            original_path: fs::canonicalize(source)?,
            sha256: doc.sha256.clone(),
            size: doc.bytes().len() as u64,
            original_sha256: None,
            original_size: None,
        });
        documents.push(doc);
    }
    // Check that the scanned metadata still describes this exact group before copying.
    let metadata_filename = group
        .metadata_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid("Invalid metadata filename"))?
        .to_string();
    let observed = documents
        .iter()
        .find(|d| d.filename == metadata_filename)
        .ok_or_else(|| invalid("Group metadata missing"))?;
    if let DocumentSummary::Metadata(m) = &observed.summary {
        if m != &group.metadata {
            return Err(TrayError::Conflict(
                "Metadata changed after library scan; rescan before import".into(),
            ));
        }
    }
    fs::create_dir(&destination)?; // exclusive ownership; never remove another caller's path
    write_new(
        &destination.join("SNAPSHOT_INCOMPLETE"),
        b"Snapshot is not committed. Original files have not been changed.\n",
    )?;
    fs::create_dir(destination.join("native"))?;
    let manifest = ProjectManifest {
        schema: 1,
        metadata_filename,
        files: entries,
        native_editing_qualified: false,
        draft: None,
    };
    if !progress(SnapshotProgress {
        stage: SnapshotStage::SnapshotPrepared,
        files_completed: 0,
        total_files,
        bytes_copied: 0,
        total_bytes: Some(total),
    }) {
        return Err(TrayError::Cancelled);
    }
    let mut bytes_copied = 0;
    for (index, doc) in documents.iter().enumerate() {
        write_new(&destination.join("native").join(&doc.filename), doc.bytes())?;
        bytes_copied += doc.bytes().len() as u64;
        if !progress(SnapshotProgress {
            stage: SnapshotStage::CopyWritten,
            files_completed: index + 1,
            total_files,
            bytes_copied,
            total_bytes: Some(total),
        }) {
            return Err(TrayError::Cancelled);
        }
    }
    for (index, entry) in manifest.files.iter().enumerate() {
        if sha256(&read_stable(&entry.original_path)?) != entry.sha256 {
            return Err(TrayError::Conflict(
                "Source changed before snapshot commit; incomplete recovery copy retained".into(),
            ));
        }
        if !progress(SnapshotProgress {
            stage: SnapshotStage::SourceRevalidated,
            files_completed: index + 1,
            total_files,
            bytes_copied,
            total_bytes: Some(total),
        }) {
            return Err(TrayError::Cancelled);
        }
    }
    if !progress(SnapshotProgress {
        stage: SnapshotStage::ReadyToCommit,
        files_completed: total_files,
        total_files,
        bytes_copied,
        total_bytes: Some(total),
    }) {
        return Err(TrayError::Cancelled);
    }
    let serialized = serde_json::to_vec_pretty(&manifest).map_err(|e| invalid(e.to_string()))?;
    write_new(&destination.join(PROJECT_MANIFEST), &serialized)?;
    // Validate exact on-disk copies before removing the recovery marker.
    let project = load_project(&destination, true)?;
    fs::remove_file(destination.join("SNAPSHOT_INCOMPLETE"))?;
    Ok(project)
}
pub fn reopen_project(directory: &Path) -> Result<TrayProject> {
    load_project(directory, false)
}
fn load_project(directory: &Path, allow_incomplete: bool) -> Result<TrayProject> {
    plain_path(directory)?;
    let directory = fs::canonicalize(directory)?;
    if !allow_incomplete && directory.join("SNAPSHOT_INCOMPLETE").exists() {
        return Err(invalid(
            "Snapshot is incomplete; original files remain untouched",
        ));
    }
    let manifest_bytes = read_stable(&directory.join(PROJECT_MANIFEST))?;
    if manifest_bytes.len() > 4 * 1024 * 1024 {
        return Err(invalid("Project manifest exceeds limit"));
    }
    let manifest: ProjectManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| invalid(format!("Invalid project manifest: {e}")))?;
    if !(manifest.schema == 1 || manifest.schema == 2)
        || manifest.native_editing_qualified
        || manifest.files.is_empty()
        || manifest.files.len() > 100_000
    {
        return Err(invalid("Unsupported project schema or qualification claim"));
    }
    if manifest.schema == 2
        && !manifest
            .draft
            .as_ref()
            .map(|d| {
                d.schema_evidence_id == DRAFT_SCHEMA_EVIDENCE_ID
                    && !d.native_game_acceptance_verified
            })
            .unwrap_or(false)
    {
        return Err(invalid(
            "Draft project schema evidence is absent or changed",
        ));
    }
    let mut documents = Vec::new();
    let mut names = BTreeSet::new();
    let mut total = 0;
    for f in &manifest.files {
        TrayName::parse(&f.filename)?;
        if !names.insert(f.filename.to_ascii_lowercase())
            || !f.original_path.is_absolute()
            || f.sha256.len() != 64
            || !f
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(invalid("Invalid or duplicate project file identity"));
        }
        if f.original_sha256.is_some() != f.original_size.is_some()
            || f.original_sha256
                .as_ref()
                .map(|s| {
                    s.len() != 64
                        || !s
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                })
                .unwrap_or(false)
            || (manifest.schema == 2 && f.original_sha256.is_none())
            || f.original_size
                .map(|n| n == 0 || n > MAX_FILE_BYTES)
                .unwrap_or(false)
        {
            return Err(invalid(
                "Original baseline source pins are invalid or absent",
            ));
        }
        let doc = read_document(&directory.join("native").join(&f.filename))?;
        if doc.sha256 != f.sha256 || doc.bytes().len() as u64 != f.size {
            return Err(TrayError::Conflict(format!(
                "Project file {} no longer matches its snapshot hash",
                f.filename
            )));
        }
        total += f.size;
        if total > MAX_PROJECT_BYTES {
            return Err(invalid("Project native files exceed snapshot limit"));
        }
        documents.push(doc);
    }
    let metadata = documents
        .iter()
        .filter(|d| matches!(d.summary, DocumentSummary::Metadata(_)))
        .collect::<Vec<_>>();
    if metadata.len() != 1 || metadata[0].filename != manifest.metadata_filename {
        return Err(invalid("Project metadata identity is ambiguous"));
    }
    Ok(TrayProject {
        directory,
        manifest,
        documents,
    })
}
