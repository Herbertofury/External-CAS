use super::*;

/// Names from the installed engine's input-layout semantic table. This domain
/// is separate from the feature interpreter's renderer ID conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub enum VertexSemantic {
    Position,
    Normal,
    TexCoord,
    BlendIndices,
    BlendWeight,
    Tangent,
    Color,
}
impl VertexSemantic {
    pub fn name(self) -> &'static str {
        match self {
            Self::Position => "POSITION",
            Self::Normal => "NORMAL",
            Self::TexCoord => "TEXCOORD",
            Self::BlendIndices => "BLENDINDICES",
            Self::BlendWeight => "BLENDWEIGHT",
            Self::Tangent => "TANGENT",
            Self::Color => "COLOR",
        }
    }
    pub fn from_native_usage(usage: u8) -> Result<Self> {
        match usage & 0x0f {
            0 => Ok(Self::Position),
            1 => Ok(Self::Normal),
            2 => Ok(Self::TexCoord),
            3 => Ok(Self::BlendIndices),
            4 => Ok(Self::BlendWeight),
            5 => Ok(Self::Tangent),
            6 => Ok(Self::Color),
            value => Err(format!(
                "Unsupported original vertex input semantic {value}"
            )),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct VertexInput {
    pub register: u8,
    pub semantic_index: u8,
    pub raw_usage: u8,
}
impl VertexInput {
    pub fn semantic(&self) -> Result<VertexSemantic> {
        VertexSemantic::from_native_usage(self.raw_usage)
    }
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct VertexBindings {
    pub source_file_sha256: String,
    pub vertex_ref: u32,
    /// Exact packed triples, including flag bits whose meanings are not guessed.
    pub inputs: Vec<VertexInput>,
    pub constant_buffers: Vec<ConstantBuffer>,
}
impl ShaderLibrary {
    pub fn vertex_bindings(&self, reference: u32, cancel: &AtomicBool) -> Result<VertexBindings> {
        check(cancel)?;
        if reference == 0 || reference as usize > self.vs.count {
            return Err("Vertex shader binding reference outside source table".into());
        }
        let at = self.vs.offset + (reference as usize - 1) * self.vs.stride;
        // The count at +0x30 is BYTES, not a number of triples.
        let packed = table(&self.bytes, at + 44, 1, self.limits)?;
        if packed.count % 3 != 0 {
            return Err("Original vertex input metadata has an incomplete triple".into());
        }
        let mut inputs = Vec::with_capacity(packed.count / 3);
        for i in 0..packed.count / 3 {
            check(cancel)?;
            let p = packed.offset + i * 3;
            inputs.push(VertexInput {
                register: self.bytes[p],
                semantic_index: self.bytes[p + 1],
                raw_usage: self.bytes[p + 2],
            });
        }
        Ok(VertexBindings {
            source_file_sha256: self.pin.sha256.clone(),
            vertex_ref: reference,
            inputs,
            constant_buffers: self.constant_buffers(at + 36, cancel)?,
        })
    }
}
