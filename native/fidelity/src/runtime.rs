// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-preserving GFx metadata preparation for an isolated SWF interpreter.
//! This does not implement Scaleform extensions or the game's command backend.
use crate::{GfxMovie, NamedCharacter};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeRequirements {
    pub source_signature: String,
    pub version: u8,
    pub stage_twips: [i32; 4],
    pub frame_rate: f64,
    pub frame_count: u16,
    pub attributes_flags: Option<u32>,
    pub uses_avm2: Option<bool>,
    pub abc_blocks: usize,
    pub avm1_action_blocks: usize,
    pub source_extensions: BTreeMap<u16, usize>,
    pub imports: Vec<String>,
    pub symbol_classes: Vec<NamedCharacter>,
    pub compacted_fonts: usize,
    pub external_images: usize,
    pub sub_images: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct RuntimePreparationReceipt {
    pub source_sha256: String,
    pub runtime_sha256: String,
    pub changes: Vec<String>,
    pub requirements: RuntimeRequirements,
    pub complete_scaleform_runtime: bool,
}
#[derive(Debug)]
pub struct PreparedRuntimeMovie {
    pub runtime_bytes: Vec<u8>,
    pub receipt: RuntimePreparationReceipt,
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
pub fn requirements(raw: &[u8]) -> Result<RuntimeRequirements, String> {
    let m = GfxMovie::inspect(raw).map_err(|e| e.to_string())?;
    let attributes = m.tags.iter().filter(|t| t.code == 69).collect::<Vec<_>>();
    if attributes.len() > 1 {
        return Err("duplicate source FileAttributes".into());
    }
    let flags = attributes
        .first()
        .map(|t| {
            let b = m.payload(t);
            if b.len() != 4 {
                return Err("invalid FileAttributes extent".to_string());
            }
            Ok(u32::from_le_bytes(b.try_into().unwrap()))
        })
        .transpose()?;
    let mut source_extensions = BTreeMap::new();
    for t in &m.tags {
        if t.code >= 1000 {
            *source_extensions.entry(t.code).or_default() += 1;
        }
    }
    Ok(RuntimeRequirements {
        source_signature: m.signature.clone(),
        version: m.version,
        stage_twips: m.stage_twips,
        frame_rate: m.frame_rate,
        frame_count: m.frame_count,
        attributes_flags: flags,
        uses_avm2: flags.map(|f| f & 8 != 0),
        abc_blocks: m.tags.iter().filter(|t| matches!(t.code, 72 | 82)).count(),
        avm1_action_blocks: m.tags.iter().filter(|t| matches!(t.code, 12 | 59)).count(),
        source_extensions,
        imports: m.imports.iter().map(|i| i.movie.clone()).collect(),
        symbol_classes: m.symbol_classes.clone(),
        compacted_fonts: m.compacted_font_headers.len(),
        external_images: m.external_images.len(),
        sub_images: m.sub_images.len(),
    })
}
/// Scaleform writes its exporter/image metadata before FileAttributes. Stock
/// Ruffle honors that SWF tag only in first position, misclassifying actual AS3
/// GFx as AVM1. Move the original tag body to first position in an in-memory copy;
/// every other tag body and the complete original source remain authoritative.
/// No ABC, artwork, font, frame, coordinate or command payload is synthesized.
pub fn prepare_swf_movie(raw: &[u8]) -> Result<PreparedRuntimeMovie, String> {
    let m = GfxMovie::inspect(raw).map_err(|e| e.to_string())?;
    let req = requirements(raw)?;
    if req.abc_blocks > 0 && req.uses_avm2 != Some(true) {
        return Err("AS3 ABC exists without an explicit source AVM2 FileAttributes bit".into());
    }
    let mut runtime_bytes = raw.to_vec();
    let mut changes = Vec::new();
    if m.signature == "GFX" {
        if let Some(index) = m.tags.iter().position(|t| t.code == 69) {
            if index > 0 {
                let mut bits = crate::Bits { raw, at: 8 * 8 };
                let width = bits.unsigned(5).map_err(|e| e.to_string())? as u8;
                for _ in 0..4 {
                    bits.signed(width).map_err(|e| e.to_string())?;
                }
                let header = bits
                    .at
                    .div_ceil(8)
                    .checked_add(4)
                    .filter(|end| *end <= raw.len())
                    .ok_or("source frame header extent")?;
                runtime_bytes = raw[..header].to_vec();
                let append = |out: &mut Vec<u8>, code: u16, payload: &[u8]| {
                    let n = payload.len();
                    out.extend_from_slice(&((code << 6) | n.min(63) as u16).to_le_bytes());
                    if n >= 63 {
                        out.extend_from_slice(&(n as u32).to_le_bytes());
                    }
                    out.extend_from_slice(payload);
                };
                append(&mut runtime_bytes, 69, m.payload(&m.tags[index]));
                for (i, t) in m.tags.iter().enumerate() {
                    if i != index {
                        append(&mut runtime_bytes, t.code, m.payload(t));
                    }
                }
                runtime_bytes.extend_from_slice(&raw[m.trailing_bytes.clone()]);
                let len =
                    u32::try_from(runtime_bytes.len()).map_err(|_| "runtime SWF byte extent")?;
                runtime_bytes[4..8].copy_from_slice(&len.to_le_bytes());
                changes.push("Moved original GFx FileAttributes body before exporter/image tags for SWF header classification".into());
            }
        }
    }
    let prepared = GfxMovie::inspect(&runtime_bytes).map_err(|e| e.to_string())?;
    if prepared.stage_twips != m.stage_twips
        || prepared.frame_rate != m.frame_rate
        || prepared.frame_count != m.frame_count
    {
        return Err("runtime preparation changed source timing/coordinates".into());
    }
    Ok(PreparedRuntimeMovie {
        receipt: RuntimePreparationReceipt {
            source_sha256: hash(raw),
            runtime_sha256: hash(&runtime_bytes),
            changes,
            requirements: req,
            complete_scaleform_runtime: false,
        },
        runtime_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tag(out: &mut Vec<u8>, code: u16, body: &[u8]) {
        out.extend_from_slice(&((code << 6) | 63).to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
    }
    fn fixture(flags: Option<u32>) -> Vec<u8> {
        let mut b = b"GFX\x0f\0\0\0\0".to_vec();
        b.extend_from_slice(&[8, 0, 0, 30, 1, 0]);
        tag(&mut b, 999, &[42; 63]);
        if let Some(f) = flags {
            tag(&mut b, 69, &f.to_le_bytes());
        }
        tag(&mut b, 82, &[1, 2, 3, 4]);
        tag(&mut b, 1, &[]);
        tag(&mut b, 0, &[]);
        b.extend_from_slice(&[77, 88]);
        let n = b.len() as u32;
        b[4..8].copy_from_slice(&n.to_le_bytes());
        b
    }
    #[test]
    fn reorders_only_metadata_keeps_abc_unknown_tag_timing_and_trailing_bytes() {
        let b = fixture(Some(8));
        let copy = b.clone();
        let p = prepare_swf_movie(&b).unwrap();
        let before = GfxMovie::inspect(&b).unwrap();
        let after = GfxMovie::inspect(&p.runtime_bytes).unwrap();
        assert_eq!(after.tags[0].code, 69);
        assert_eq!(after.stage_twips, before.stage_twips);
        assert_eq!(after.frame_count, before.frame_count);
        let canonical = |m: &GfxMovie<'_>| {
            m.tags
                .iter()
                .filter(|t| t.code != 69)
                .map(|t| (t.code, m.payload(t).to_vec()))
                .collect::<Vec<_>>()
        };
        assert_eq!(canonical(&before), canonical(&after));
        assert_eq!(
            &p.runtime_bytes[after.trailing_bytes],
            &b[before.trailing_bytes]
        );
        assert_eq!(b, copy);
        assert!(!p.receipt.complete_scaleform_runtime);
        assert_eq!(p.receipt.changes.len(), 1);
    }
    #[test]
    fn missing_as3_flag_or_truncated_inputs_do_not_guess_engine() {
        assert!(prepare_swf_movie(&fixture(None)).is_err());
        assert!(prepare_swf_movie(&fixture(Some(0))).is_err());
        let b = fixture(Some(8));
        for n in 0..b.len() {
            assert!(prepare_swf_movie(&b[..n]).is_err());
        }
    }
}
