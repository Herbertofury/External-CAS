//! Bounded native access to the original installed DX11 shader library.
//! No game executable is loaded. Unknown effect words and pass states stay
//! uninterpreted; this reader never guesses which runtime permutation to use.
use sha2::{Digest, Sha256};
mod feature;
mod set;
mod vertex;
pub use feature::{FeatureEvaluation, FeatureInputs, FeatureLimits, FeatureProgram};
pub use set::{NamedShaderSet, VariantField};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
pub use vertex::{VertexBindings, VertexInput, VertexSemantic};
pub type Result<T> = std::result::Result<T, String>;
#[cfg(test)]
mod tests;
fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("Shader load cancelled".into())
    } else {
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Stage {
    Vertex,
    Pixel,
    Compute,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct SourcePin {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub file_bytes: usize,
    pub decoded_shader_bytes: usize,
    pub table_records: usize,
    pub passes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            file_bytes: 256 * 1024 * 1024,
            decoded_shader_bytes: 16 * 1024 * 1024,
            table_records: 2_000_000,
            passes: 1024,
        }
    }
}
#[derive(Clone, Copy, Debug)]
struct Table {
    offset: usize,
    count: usize,
    stride: usize,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Effect {
    pub ordinal: usize,
    pub source_file_sha256: String,
    pub shader_hash: u32,
    pub raw_words: [u32; 7],
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Pass {
    pub vertex_ref: u32,
    pub pixel_ref: u32,
    pub compute_ref: u32,
    pub states: Vec<[u32; 2]>,
    pub raw_words: [u32; 6],
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Technique {
    pub raw_words: [u32; 3],
    pub passes: Vec<Pass>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Parameter {
    pub index: u16,
    pub hash: u32,
    pub kind: u32,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct TextureBinding {
    pub parameter: Parameter,
    pub slot: u16,
    pub raw_words: [u16; 4],
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct ConstantBinding {
    pub parameter: Parameter,
    pub byte_offset: u16,
    pub byte_size: u16,
    pub type_word: u16,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct ConstantBuffer {
    pub preparation_index: u16,
    pub slot: u32,
    pub raw_words: [u32; 7],
    pub bindings: Vec<ConstantBinding>,
    pub default_words: Vec<u32>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct PixelBindings {
    pub source_file_sha256: String,
    pub pixel_ref: u32,
    pub textures: Vec<TextureBinding>,
    pub raw_sampler_references: Vec<u16>,
    pub constant_buffers: Vec<ConstantBuffer>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct SamplerRecord {
    pub ordinal: usize,
    pub source_file_sha256: String,
    /// Packed border color, raw MipLODBias bits and native setting flags.
    /// Resolution requires the actual native sampler/settings contract.
    pub raw_words: [u32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SamplerReference {
    pub raw: u16,
    pub slot: u16,
    pub state_ordinal: u16,
}
impl SamplerReference {
    pub fn from_raw(raw: u16) -> Self {
        Self {
            raw,
            slot: raw >> 12,
            state_ordinal: raw & 0x0fff,
        }
    }
}
impl PixelBindings {
    pub fn sampler_references(&self) -> impl Iterator<Item = SamplerReference> + '_ {
        self.raw_sampler_references
            .iter()
            .copied()
            .map(SamplerReference::from_raw)
    }
}
#[derive(Clone, Debug)]
pub struct ShaderCode {
    pub stage: Stage,
    pub reference: u32,
    pub source_file_sha256: String,
    pub source_record: usize,
    pub raw_record: Vec<u32>,
    pub sha256: String,
    pub dxbc: Vec<u8>,
}
impl ShaderCode {
    pub fn verify(&self) -> Result<()> {
        validate_dxbc(&self.dxbc, self.stage)?;
        if format!("{:x}", Sha256::digest(&self.dxbc)) != self.sha256 {
            return Err("Shader program changed after source validation".into());
        }
        Ok(())
    }
}
pub struct ShaderLibrary {
    pub pin: SourcePin,
    pub payload_version: u32,
    pub payload_offset: usize,
    pub descriptor_words: Vec<[u32; 7]>,
    bytes: Arc<[u8]>,
    effects: Table,
    vs: Table,
    ps: Table,
    cs: Table,
    states: Table,
    parameters: Table,
    preparations: Table,
    samplers: Table,
    limits: Limits,
}
fn u16_at(raw: &[u8], offset: usize) -> Result<u16> {
    let b = raw
        .get(offset..offset.checked_add(2).ok_or("Shader word offset overflow")?)
        .ok_or("Truncated shader binding")?;
    Ok(u16::from_le_bytes(b.try_into().unwrap()))
}
fn u32_at(raw: &[u8], offset: usize) -> Result<u32> {
    let bytes = raw
        .get(offset..offset.checked_add(4).ok_or("Shader offset overflow")?)
        .ok_or("Truncated shader library")?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}
fn relative(raw: &[u8], field: usize) -> Result<usize> {
    let delta = u32_at(raw, field)? as i32;
    if delta == i32::MIN {
        return Err("Absent required shader pointer".into());
    }
    let value = (field as i128) + (delta as i128);
    if value < 0 || value >= raw.len() as i128 {
        return Err("Shader relative pointer outside source".into());
    }
    Ok(value as usize)
}
fn words<const N: usize>(raw: &[u8], offset: usize) -> Result<[u32; N]> {
    let mut out = [0; N];
    for (i, v) in out.iter_mut().enumerate() {
        *v = u32_at(raw, offset + i * 4)?;
    }
    Ok(out)
}
fn table(raw: &[u8], field: usize, stride: usize, limits: Limits) -> Result<Table> {
    let count = u32_at(raw, field + 4)? as usize;
    if count > limits.table_records {
        return Err("Shader table count exceeds budget".into());
    }
    let offset = if count == 0 { 0 } else { relative(raw, field)? };
    let end = count
        .checked_mul(stride)
        .and_then(|n| offset.checked_add(n))
        .ok_or("Shader table extent overflow")?;
    if end > raw.len() {
        return Err("Shader table outside source".into());
    }
    Ok(Table {
        offset,
        count,
        stride,
    })
}
impl ShaderLibrary {
    pub fn open(path: &Path, limits: Limits, cancel: &AtomicBool) -> Result<Self> {
        use std::io::Read;
        check(cancel)?;
        if !path.is_absolute() {
            return Err("Explicit absolute shader source required".into());
        }
        let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let before = file.metadata().map_err(|e| e.to_string())?;
        if !before.is_file() || before.len() > limits.file_bytes as u64 {
            return Err("Installed shader source exceeds file budget".into());
        }
        let mut bytes = Vec::with_capacity(before.len() as usize);
        let mut buffer = [0; 64 * 1024];
        loop {
            check(cancel)?;
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            if bytes
                .len()
                .checked_add(n)
                .is_none_or(|n| n > limits.file_bytes)
            {
                return Err("Shader source grew past budget".into());
            }
            bytes.extend_from_slice(&buffer[..n]);
        }
        let after = file.metadata().map_err(|e| e.to_string())?;
        if before.len() != after.len()
            || before.modified().ok() != after.modified().ok()
            || bytes.len() as u64 != before.len()
        {
            return Err("Shader source changed during read".into());
        }
        Self::from_bytes(path.to_path_buf(), bytes, limits, cancel)
    }
    pub fn from_bytes(
        path: PathBuf,
        bytes: Vec<u8>,
        limits: Limits,
        cancel: &AtomicBool,
    ) -> Result<Self> {
        check(cancel)?;
        if bytes.len() > limits.file_bytes
            || limits.decoded_shader_bytes == 0
            || limits.table_records == 0
            || limits.passes == 0
        {
            return Err("Shader source/limits budget".into());
        }
        if bytes.get(..4) != Some(b"DATA") {
            return Err("Shader DATA signature missing".into());
        }
        let count = u32_at(&bytes, 12)? as usize;
        if count > limits.table_records {
            return Err("Shader descriptor count exceeds budget".into());
        }
        let end = count
            .checked_mul(28)
            .and_then(|n| 32usize.checked_add(n))
            .ok_or("Shader descriptor extent overflow")?;
        if end > bytes.len() {
            return Err("Truncated shader descriptors".into());
        }
        let mut descriptor_words = Vec::with_capacity(count);
        for i in 0..count {
            descriptor_words.push(words(&bytes, 32 + i * 28)?);
        }
        let root = relative(&bytes, 8)?;
        let payload = relative(&bytes, root.checked_add(20).ok_or("Shader root overflow")?)?;
        let version = u32_at(&bytes, payload)?;
        if version != 6 {
            return Err(format!(
                "Unsupported installed DX11 shader payload {version}; source retained"
            ));
        }
        let effects = table(&bytes, payload + 4, 28, limits)?;
        let vs = table(&bytes, payload + 20, 52, limits)?;
        let ps = table(&bytes, payload + 28, 44, limits)?;
        let cs = table(&bytes, payload + 36, 44, limits)?;
        let states = table(&bytes, payload + 76, 8, limits)?;
        let parameters = table(&bytes, payload + 60, 8, limits)?;
        let preparations = table(&bytes, payload + 84, 28, limits)?;
        let samplers = table(&bytes, payload + 92, 12, limits)?;
        check(cancel)?;
        let pin = SourcePin {
            path,
            bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        };
        Ok(Self {
            pin,
            payload_version: version,
            payload_offset: payload,
            descriptor_words,
            bytes: bytes.into(),
            effects,
            vs,
            ps,
            cs,
            states,
            parameters,
            preparations,
            samplers,
            limits,
        })
    }
    pub fn stage_count(&self, stage: Stage) -> usize {
        self.stage_table(stage).count
    }
    pub fn effect_count(&self) -> usize {
        self.effects.count
    }
    pub fn sampler_count(&self) -> usize {
        self.samplers.count
    }
    pub fn sampler_record(&self, ordinal: usize) -> Result<SamplerRecord> {
        if ordinal >= self.samplers.count {
            return Err("Sampler record outside source table".into());
        }
        Ok(SamplerRecord {
            ordinal,
            source_file_sha256: self.pin.sha256.clone(),
            raw_words: words(&self.bytes, self.samplers.offset + ordinal * 12)?,
        })
    }
    fn stage_table(&self, stage: Stage) -> Table {
        match stage {
            Stage::Vertex => self.vs,
            Stage::Pixel => self.ps,
            Stage::Compute => self.cs,
        }
    }
    fn parameter(&self, index: u16) -> Result<Parameter> {
        if index as usize >= self.parameters.count {
            return Err("Shader binding parameter outside global table".into());
        }
        let at = self.parameters.offset + index as usize * 8;
        Ok(Parameter {
            index,
            hash: u32_at(&self.bytes, at)?,
            kind: u32_at(&self.bytes, at + 4)?,
        })
    }
    pub fn pixel_bindings(&self, reference: u32, cancel: &AtomicBool) -> Result<PixelBindings> {
        check(cancel)?;
        if reference == 0 || reference as usize > self.ps.count {
            return Err("Pixel shader binding reference outside source table".into());
        }
        let at = self.ps.offset + (reference as usize - 1) * self.ps.stride;
        let texture_table = table(&self.bytes, at + 12, 8, self.limits)?;
        if texture_table
            .count
            .checked_mul(std::mem::size_of::<TextureBinding>())
            .is_none_or(|n| n > self.limits.decoded_shader_bytes)
        {
            return Err("Shader texture-binding metadata exceeds decoded budget".into());
        }
        let mut textures = Vec::new();
        for i in 0..texture_table.count {
            check(cancel)?;
            let at = texture_table.offset + i * 8;
            let raw_words = [
                u16_at(&self.bytes, at)?,
                u16_at(&self.bytes, at + 2)?,
                u16_at(&self.bytes, at + 4)?,
                u16_at(&self.bytes, at + 6)?,
            ];
            textures.push(TextureBinding {
                parameter: self.parameter(raw_words[0])?,
                slot: raw_words[1],
                raw_words,
            });
        }
        let constant_buffers = self.constant_buffers(at + 36, cancel)?;
        let sampler_table = table(&self.bytes, at + 28, 2, self.limits)?;
        let mut raw_sampler_references = Vec::with_capacity(sampler_table.count);
        for i in 0..sampler_table.count {
            check(cancel)?;
            let raw = u16_at(&self.bytes, sampler_table.offset + i * 2)?;
            if SamplerReference::from_raw(raw).state_ordinal as usize >= self.samplers.count {
                return Err("Shader sampler reference outside source state table".into());
            }
            raw_sampler_references.push(raw);
        }
        Ok(PixelBindings {
            source_file_sha256: self.pin.sha256.clone(),
            pixel_ref: reference,
            textures,
            raw_sampler_references,
            constant_buffers,
        })
    }
    fn constant_buffers(&self, field: usize, cancel: &AtomicBool) -> Result<Vec<ConstantBuffer>> {
        let indices = table(&self.bytes, field, 2, self.limits)?;
        // Repeated references can amplify a tiny source table into very large
        // output. Bound the whole decoded observation, not only each buffer.
        let mut decoded_bytes = indices
            .count
            .checked_mul(std::mem::size_of::<ConstantBuffer>())
            .ok_or("Shader constant metadata extent overflow")?;
        if decoded_bytes > self.limits.decoded_shader_bytes {
            return Err("Shader constant metadata exceeds decoded budget".into());
        }
        let mut constant_buffers = Vec::new();
        for i in 0..indices.count {
            check(cancel)?;
            let index = u16_at(&self.bytes, indices.offset + i * 2)?;
            if index as usize >= self.preparations.count {
                return Err("Shader preparation index outside source table".into());
            }
            let at = self.preparations.offset + index as usize * 28;
            let raw_words = words::<7>(&self.bytes, at)?;
            let members = table(&self.bytes, at + 12, 8, self.limits)?;
            let defaults = table(&self.bytes, at + 20, 4, self.limits)?;
            decoded_bytes = defaults
                .count
                .checked_mul(4)
                .and_then(|n| {
                    members
                        .count
                        .checked_mul(std::mem::size_of::<ConstantBinding>())
                        .and_then(|m| n.checked_add(m))
                })
                .and_then(|n| decoded_bytes.checked_add(n))
                .ok_or("Shader decoded metadata extent overflow")?;
            if decoded_bytes > self.limits.decoded_shader_bytes {
                return Err("Shader constant metadata exceeds aggregate decoded budget".into());
            }
            if defaults
                .count
                .checked_mul(4)
                .is_none_or(|n| n > self.limits.decoded_shader_bytes)
            {
                return Err("Shader constant default byte budget".into());
            }
            let mut default_words = Vec::with_capacity(defaults.count);
            for i in 0..defaults.count {
                default_words.push(u32_at(&self.bytes, defaults.offset + i * 4)?);
            }
            let mut bindings = Vec::new();
            for i in 0..members.count {
                let at = members.offset + i * 8;
                let index = u16_at(&self.bytes, at)?;
                let byte_offset = u16_at(&self.bytes, at + 2)?;
                let byte_size = u16_at(&self.bytes, at + 4)?;
                if byte_offset as usize + byte_size as usize > defaults.count * 4 {
                    return Err("Shader constant binding exceeds its default buffer extent".into());
                }
                bindings.push(ConstantBinding {
                    parameter: self.parameter(index)?,
                    byte_offset,
                    byte_size,
                    type_word: u16_at(&self.bytes, at + 6)?,
                });
            }
            constant_buffers.push(ConstantBuffer {
                preparation_index: index,
                slot: raw_words[2],
                raw_words,
                bindings,
                default_words,
            });
        }
        Ok(constant_buffers)
    }
    pub fn effect(&self, ordinal: usize) -> Result<Effect> {
        if ordinal >= self.effects.count {
            return Err("Shader effect ordinal outside source table".into());
        }
        let raw_words = words(&self.bytes, self.effects.offset + ordinal * 28)?;
        Ok(Effect {
            ordinal,
            source_file_sha256: self.pin.sha256.clone(),
            shader_hash: raw_words[0],
            raw_words,
        })
    }
    pub fn techniques(&self, effect: &Effect, cancel: &AtomicBool) -> Result<Vec<Technique>> {
        check(cancel)?;
        if effect.source_file_sha256 != self.pin.sha256
            || self.effect(effect.ordinal)?.raw_words != effect.raw_words
        {
            return Err("Shader effect belongs to another source/revision".into());
        }
        let at = self.effects.offset + effect.ordinal * 28;
        let t = table(&self.bytes, at + 20, 12, self.limits)?;
        if t.count > self.limits.passes {
            return Err("Shader technique budget".into());
        }
        let mut output = Vec::new();
        let mut total = 0usize;
        for i in 0..t.count {
            check(cancel)?;
            let at = t.offset + i * 12;
            let raw_words = words(&self.bytes, at)?;
            let p = table(&self.bytes, at + 4, 24, self.limits)?;
            total = total
                .checked_add(p.count)
                .ok_or("Shader pass count overflow")?;
            if total > self.limits.passes {
                return Err("Shader pass budget".into());
            }
            let mut passes = Vec::new();
            for j in 0..p.count {
                let raw = words::<6>(&self.bytes, p.offset + j * 24)?;
                if raw[0] as usize > self.vs.count
                    || raw[1] as usize > self.ps.count
                    || raw[2] as usize > self.cs.count
                {
                    return Err("Shader pass references absent stage".into());
                }
                let end = (raw[3] as usize)
                    .checked_add(raw[4] as usize)
                    .ok_or("Shader state extent overflow")?;
                if end > self.states.count {
                    return Err("Shader pass states outside source bound".into());
                }
                let mut states = Vec::new();
                for k in raw[3] as usize..end {
                    states.push(words(&self.bytes, self.states.offset + k * 8)?);
                }
                passes.push(Pass {
                    vertex_ref: raw[0],
                    pixel_ref: raw[1],
                    compute_ref: raw[2],
                    states,
                    raw_words: raw,
                });
            }
            output.push(Technique { raw_words, passes });
        }
        Ok(output)
    }
    pub fn shader(&self, stage: Stage, reference: u32, cancel: &AtomicBool) -> Result<ShaderCode> {
        check(cancel)?;
        let table = self.stage_table(stage);
        if reference == 0 || reference as usize > table.count {
            return Err("Shader reference must be a present one-based stage index".into());
        }
        let at = table.offset + (reference as usize - 1) * table.stride;
        let compact = relative(&self.bytes, at)?;
        let length = u32_at(&self.bytes, at + 8)? as usize;
        let raw = self
            .bytes
            .get(
                compact
                    ..compact
                        .checked_add(length)
                        .ok_or("Compact shader extent overflow")?,
            )
            .ok_or("Compact shader outside source")?;
        let dxbc = decode_raw_snappy(raw, self.limits.decoded_shader_bytes, cancel)?;
        validate_dxbc(&dxbc, stage)?;
        let mut raw_record = Vec::new();
        for offset in (0..table.stride).step_by(4) {
            raw_record.push(u32_at(&self.bytes, at + offset)?);
        }
        Ok(ShaderCode {
            stage,
            reference,
            source_file_sha256: self.pin.sha256.clone(),
            source_record: at,
            raw_record,
            sha256: format!("{:x}", Sha256::digest(&dxbc)),
            dxbc,
        })
    }
}
pub fn decode_raw_snappy(raw: &[u8], limit: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
    check(cancel)?;
    let mut at = 0;
    let mut size = 0u64;
    let mut ended = false;
    for shift in (0..35).step_by(7) {
        let b = *raw.get(at).ok_or("Truncated Snappy size")?;
        at += 1;
        size |= ((b & 127) as u64) << shift;
        if b < 128 {
            ended = true;
            break;
        }
    }
    if !ended || size > limit as u64 || size > u32::MAX as u64 {
        return Err("Snappy decoded shader exceeds budget/size domain".into());
    }
    let size = size as usize;
    let mut out = Vec::with_capacity(size);
    while out.len() < size {
        check(cancel)?;
        let tag = *raw.get(at).ok_or("Truncated Snappy operation")?;
        at += 1;
        let (length, offset) = match tag & 3 {
            0 => {
                let n = tag >> 2;
                let length = if n < 60 {
                    n as usize + 1
                } else {
                    let width = (n - 59) as usize;
                    let mut value = 0u32;
                    for i in 0..width {
                        value |= (*raw.get(at + i).ok_or("Truncated Snappy literal size")? as u32)
                            << (i * 8);
                    }
                    at += width;
                    (value as usize)
                        .checked_add(1)
                        .ok_or("Snappy literal size overflow")?
                };
                (length, 0)
            }
            1 => {
                let low = *raw.get(at).ok_or("Truncated Snappy copy")?;
                at += 1;
                (
                    4 + ((tag >> 2) & 7) as usize,
                    (((tag as usize) & 224) << 3) | low as usize,
                )
            }
            kind => {
                let width = if kind == 2 { 2 } else { 4 };
                let mut value = 0u32;
                for i in 0..width {
                    value |=
                        (*raw.get(at + i).ok_or("Truncated Snappy copy offset")? as u32) << (i * 8);
                }
                at += width;
                (1 + (tag >> 2) as usize, value as usize)
            }
        };
        if length > size - out.len() {
            return Err("Snappy operation exceeds decoded shader extent".into());
        }
        if offset == 0 {
            if tag & 3 != 0 {
                return Err("Zero Snappy back-reference".into());
            }
            let end = at.checked_add(length).ok_or("Snappy literal overflow")?;
            out.extend_from_slice(raw.get(at..end).ok_or("Truncated Snappy literal")?);
            at = end;
        } else {
            if offset > out.len() {
                return Err("Snappy back-reference precedes output".into());
            }
            for i in 0..length {
                if i % 4096 == 0 {
                    check(cancel)?;
                }
                let b = out[out.len() - offset];
                out.push(b);
            }
        }
    }
    if at != raw.len() {
        return Err("Trailing compact shader bytes outside encoded stream".into());
    }
    Ok(out)
}
pub fn validate_dxbc(raw: &[u8], stage: Stage) -> Result<()> {
    if raw.get(..4) != Some(b"DXBC") || u32_at(raw, 24)? as usize != raw.len() {
        return Err("DXBC signature/extent mismatch".into());
    }
    let count = u32_at(raw, 28)? as usize;
    if count == 0 || count > 1024 || 32 + count * 4 > raw.len() {
        return Err("DXBC chunk table extent".into());
    }
    let mut program = None;
    let mut spans = Vec::new();
    for i in 0..count {
        let at = u32_at(raw, 32 + i * 4)? as usize;
        if at < 32 + count * 4 {
            return Err("DXBC chunk overlaps header".into());
        }
        let n = u32_at(raw, at + 4)? as usize;
        let end = at
            .checked_add(8)
            .and_then(|n0| n0.checked_add(n))
            .ok_or("DXBC chunk overflow")?;
        if end > raw.len() || spans.iter().any(|&(a, b)| at < b && end > a) {
            return Err("DXBC chunks overlap or exceed source".into());
        }
        spans.push((at, end));
        let kind = &raw[at..at + 4];
        if kind == b"SHDR" || kind == b"SHEX" {
            if program.is_some() || n < 8 {
                return Err("DXBC ambiguous/truncated shader program".into());
            }
            let token = u32_at(raw, at + 8)?;
            let expected = match stage {
                Stage::Pixel => 0,
                Stage::Vertex => 1,
                Stage::Compute => 5,
            };
            if token >> 16 != expected {
                return Err("DXBC program stage differs from source table".into());
            }
            if (u32_at(raw, at + 12)? as usize).checked_mul(4) != Some(n) {
                return Err("DXBC program word count mismatch".into());
            }
            program = Some(token);
        }
    }
    if program.is_none() {
        return Err("DXBC contains no supported program chunk".into());
    }
    Ok(())
}
