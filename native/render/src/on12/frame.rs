//! Explicit original-stage resources. Caller-selected constants, planes and
//! pass states are never repaired or replaced with invented material defaults.
use super::*;
use external_cas_shader::PixelBindings;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;

pub struct OriginalProgram {
    pub vertex: OriginalVertexShader,
    pub pixel: OriginalPixelShader,
    vertex_code: ShaderCode,
    vertex_bindings: VertexBindings,
    pixel_bindings: PixelBindings,
    bridge_id: u64,
}
pub struct OriginalMesh {
    vertices: ID3D11Buffer,
    indices: ID3D11Buffer,
    layout: OriginalInputLayout,
    stride: u32,
    count: u32,
    bridge_id: u64,
}
impl OriginalMesh {
    pub fn decode_scales(&self) -> [f32; 4] {
        self.layout.decode_scales
    }
}
pub struct OriginalConstants {
    buffer: ID3D11Buffer,
    pub slot: u32,
    pub bytes: usize,
    bridge_id: u64,
}
pub struct OriginalTexture {
    view: ID3D11ShaderResourceView,
    bridge_id: u64,
}
pub struct OriginalSampler {
    state: ID3D11SamplerState,
    bridge_id: u64,
}
pub struct OriginalPassState {
    rasterizer: ID3D11RasterizerState,
    blend: ID3D11BlendState,
    depth: ID3D11DepthStencilState,
    bridge_id: u64,
}
pub struct OriginalDepth {
    view: ID3D11DepthStencilView,
    size: [u32; 2],
    has_stencil: bool,
    bridge_id: u64,
}
/// Source channel storage and SRV color space are separate explicit decisions.
#[derive(Clone, Copy, Debug)]
pub enum PlaneFormat {
    Rgba8,
    Rgba8Srgb,
    Bgra8,
    Bgra8Srgb,
    Bc1,
    Bc1Srgb,
    Bc3,
    Bc3Srgb,
}
impl PlaneFormat {
    fn storage(self) -> (DXGI_FORMAT, u32, u32) {
        match self {
            Self::Rgba8 => (DXGI_FORMAT_R8G8B8A8_UNORM, 1, 4),
            Self::Rgba8Srgb => (DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, 1, 4),
            Self::Bgra8 => (DXGI_FORMAT_B8G8R8A8_UNORM, 1, 4),
            Self::Bgra8Srgb => (DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, 1, 4),
            Self::Bc1 => (DXGI_FORMAT_BC1_UNORM, 4, 8),
            Self::Bc1Srgb => (DXGI_FORMAT_BC1_UNORM_SRGB, 4, 8),
            Self::Bc3 => (DXGI_FORMAT_BC3_UNORM, 4, 16),
            Self::Bc3Srgb => (DXGI_FORMAT_BC3_UNORM_SRGB, 4, 16),
        }
    }
}
pub struct OriginalDraw<'a> {
    pub program: &'a OriginalProgram,
    pub mesh: &'a OriginalMesh,
    pub vertex_constants: &'a [&'a OriginalConstants],
    pub pixel_constants: &'a [&'a OriginalConstants],
    pub textures: &'a [(u32, &'a OriginalTexture)],
    pub samplers: &'a [(u32, &'a OriginalSampler)],
}
#[derive(Clone, Copy, Debug, Default)]
pub struct OriginalClear {
    pub color: Option<[f32; 4]>,
    pub depth: Option<f32>,
    pub stencil: Option<u8>,
}
fn immutable_buffer(
    device: &ID3D11Device,
    bytes: &[u8],
    flags: D3D11_BIND_FLAG,
) -> Result<ID3D11Buffer> {
    let description = D3D11_BUFFER_DESC {
        ByteWidth: u32::try_from(bytes.len()).map_err(|_| "Native buffer byte count overflow")?,
        Usage: D3D11_USAGE_IMMUTABLE,
        BindFlags: flags.0 as u32,
        ..Default::default()
    };
    let initial = D3D11_SUBRESOURCE_DATA {
        pSysMem: bytes.as_ptr().cast(),
        ..Default::default()
    };
    let mut buffer = None;
    unsafe { device.CreateBuffer(&description, Some(&initial), Some(&mut buffer)) }
        .map_err(error)?;
    buffer.ok_or("D3D11 returned no immutable buffer".into())
}
impl OriginalShaderBridge {
    pub fn create_program(
        &self,
        vertex: &ShaderCode,
        vertex_bindings: &VertexBindings,
        pixel: &ShaderCode,
        pixel_bindings: &PixelBindings,
    ) -> Result<OriginalProgram> {
        if vertex.source_file_sha256 != pixel.source_file_sha256
            || vertex.source_file_sha256 != vertex_bindings.source_file_sha256
            || pixel.source_file_sha256 != pixel_bindings.source_file_sha256
            || vertex.reference != vertex_bindings.vertex_ref
            || pixel.reference != pixel_bindings.pixel_ref
        {
            return Err(
                "Original program stages/bindings belong to different source revisions".into(),
            );
        }
        Ok(OriginalProgram {
            vertex: self.create_vertex_shader(vertex)?,
            pixel: self.create_pixel_shader(pixel)?,
            vertex_code: vertex.clone(),
            vertex_bindings: vertex_bindings.clone(),
            pixel_bindings: pixel_bindings.clone(),
            bridge_id: self.bridge_id,
        })
    }
    pub fn create_mesh(
        &self,
        program: &OriginalProgram,
        elements: &[NativeVertexElement],
        stride: u32,
        vertices: &[u8],
        indices: &[u32],
    ) -> Result<OriginalMesh> {
        if program.bridge_id != self.bridge_id {
            return Err("Foreign original program".into());
        }
        if stride == 0
            || stride > 2048
            || vertices.is_empty()
            || vertices.len() % stride as usize != 0
            || vertices.len() > 256 * 1024 * 1024
            || indices.is_empty()
            || indices.len() > 16 * 1024 * 1024
            || indices.len() % 3 != 0
        {
            return Err("Original mesh extent/triangle/budget validation failed".into());
        }
        if indices
            .iter()
            .any(|&i| i as usize >= vertices.len() / stride as usize)
        {
            return Err("Original mesh index exceeds supplied vertices".into());
        }
        for e in elements {
            if e.slot != 0
                || e.byte_offset
                    .checked_add(native_vertex_format(e.native_format)?.1)
                    .is_none_or(|n| n > stride)
            {
                return Err(
                    "Original mesh requires explicit single-stream in-stride attributes".into(),
                );
            }
        }
        let layout =
            self.create_input_layout(&program.vertex_code, &program.vertex_bindings, elements)?;
        let index_bytes: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        Ok(OriginalMesh {
            vertices: immutable_buffer(&self.device, vertices, D3D11_BIND_VERTEX_BUFFER)?,
            indices: immutable_buffer(&self.device, &index_bytes, D3D11_BIND_INDEX_BUFFER)?,
            layout,
            stride,
            count: indices.len() as u32,
            bridge_id: self.bridge_id,
        })
    }
    pub fn create_constants(&self, slot: u32, bytes: &[u8]) -> Result<OriginalConstants> {
        if slot > 13 || bytes.is_empty() || bytes.len() > 65536 || bytes.len() % 16 != 0 {
            return Err("Original constant-buffer slot/alignment/extent invalid".into());
        }
        let description = D3D11_BUFFER_DESC {
            ByteWidth: bytes.len() as u32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        let initial = D3D11_SUBRESOURCE_DATA {
            pSysMem: bytes.as_ptr().cast(),
            ..Default::default()
        };
        let mut buffer = None;
        unsafe {
            self.device
                .CreateBuffer(&description, Some(&initial), Some(&mut buffer))
        }
        .map_err(error)?;
        Ok(OriginalConstants {
            buffer: buffer.ok_or("No original constant buffer")?,
            slot,
            bytes: bytes.len(),
            bridge_id: self.bridge_id,
        })
    }
    pub fn update_constants(&mut self, buffer: &OriginalConstants, bytes: &[u8]) -> Result<()> {
        if buffer.bridge_id != self.bridge_id || bytes.len() != buffer.bytes {
            return Err("Original constant update changed owner/extent".into());
        }
        unsafe {
            self.context
                .UpdateSubresource(&buffer.buffer, 0, None, bytes.as_ptr().cast(), 0, 0)
        };
        Ok(())
    }
    pub fn create_plane(
        &self,
        size: [u32; 2],
        format: PlaneFormat,
        bytes: &[u8],
    ) -> Result<OriginalTexture> {
        if size.iter().any(|&v| v == 0 || v > 16384) {
            return Err("Original plane extent invalid".into());
        }
        let (format, block, block_bytes) = format.storage();
        let pitch = size[0].div_ceil(block) * block_bytes;
        if bytes.len() != pitch as usize * size[1].div_ceil(block) as usize {
            return Err("Original plane storage extent differs from explicit format".into());
        }
        let description = D3D11_TEXTURE2D_DESC {
            Width: size[0],
            Height: size[1],
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            ..Default::default()
        };
        let initial = D3D11_SUBRESOURCE_DATA {
            pSysMem: bytes.as_ptr().cast(),
            SysMemPitch: pitch,
            ..Default::default()
        };
        let mut texture = None;
        unsafe {
            self.device
                .CreateTexture2D(&description, Some(&initial), Some(&mut texture))
        }
        .map_err(error)?;
        let texture = texture.ok_or("No original material texture")?;
        let mut view = None;
        unsafe {
            self.device
                .CreateShaderResourceView(&texture, None, Some(&mut view))
        }
        .map_err(error)?;
        Ok(OriginalTexture {
            view: view.ok_or("No original plane SRV")?,
            bridge_id: self.bridge_id,
        })
    }
    pub fn create_sampler(&self, description: &D3D11_SAMPLER_DESC) -> Result<OriginalSampler> {
        let mut state = None;
        unsafe {
            self.device
                .CreateSamplerState(description, Some(&mut state))
        }
        .map_err(error)?;
        Ok(OriginalSampler {
            state: state.ok_or("No original sampler")?,
            bridge_id: self.bridge_id,
        })
    }
    pub fn create_pass_state(
        &self,
        rasterizer: &D3D11_RASTERIZER_DESC,
        blend: &D3D11_BLEND_DESC,
        depth: &D3D11_DEPTH_STENCIL_DESC,
    ) -> Result<OriginalPassState> {
        let mut rasterizer_state = None;
        let mut blend_state = None;
        let mut depth_state = None;
        unsafe {
            self.device
                .CreateRasterizerState(rasterizer, Some(&mut rasterizer_state))
                .map_err(error)?;
            self.device
                .CreateBlendState(blend, Some(&mut blend_state))
                .map_err(error)?;
            self.device
                .CreateDepthStencilState(depth, Some(&mut depth_state))
                .map_err(error)?;
        }
        Ok(OriginalPassState {
            rasterizer: rasterizer_state.ok_or("No original rasterizer")?,
            blend: blend_state.ok_or("No original blend state")?,
            depth: depth_state.ok_or("No original depth state")?,
            bridge_id: self.bridge_id,
        })
    }
    pub fn create_depth(&self, size: [u32; 2], format: DXGI_FORMAT) -> Result<OriginalDepth> {
        if size.iter().any(|&v| v == 0 || v > 16384)
            || !matches!(
                format,
                DXGI_FORMAT_D32_FLOAT | DXGI_FORMAT_D24_UNORM_S8_UINT
            )
        {
            return Err("Original depth extent/format invalid".into());
        }
        let description = D3D11_TEXTURE2D_DESC {
            Width: size[0],
            Height: size[1],
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BindFlags: D3D11_BIND_DEPTH_STENCIL.0 as u32,
            ..Default::default()
        };
        let mut texture = None;
        unsafe {
            self.device
                .CreateTexture2D(&description, None, Some(&mut texture))
        }
        .map_err(error)?;
        let mut view = None;
        unsafe {
            self.device.CreateDepthStencilView(
                &texture.ok_or("No original depth texture")?,
                None,
                Some(&mut view),
            )
        }
        .map_err(error)?;
        Ok(OriginalDepth {
            view: view.ok_or("No original depth view")?,
            size,
            has_stencil: format == DXGI_FORMAT_D24_UNORM_S8_UINT,
            bridge_id: self.bridge_id,
        })
    }
    /// Executes supplied original stages without a CPU copy or readback.
    ///
    /// # Safety
    /// The caller owns exclusive target/queue frame access as required by
    /// `draw_shared_target`. Source feature choice and every supplied native
    /// resource/state must already be qualified for the intended scene.
    pub unsafe fn draw_scene(
        &mut self,
        target: &SharedTarget,
        state: &OriginalPassState,
        depth: Option<&OriginalDepth>,
        draws: &[OriginalDraw<'_>],
        clear: OriginalClear,
        blend_factor: [f32; 4],
        stencil_reference: u32,
    ) -> Result<()> {
        let size = [target.texture.width(), target.texture.height()];
        if state.bridge_id != self.bridge_id
            || depth.is_some_and(|d| d.bridge_id != self.bridge_id || d.size != size)
            || clear
                .color
                .is_some_and(|v| v.iter().any(|n| !n.is_finite()))
            || clear
                .depth
                .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            || ((clear.depth.is_some() || clear.stencil.is_some()) && depth.is_none())
            || (clear.stencil.is_some() && depth.is_some_and(|d| !d.has_stencil))
            || blend_factor.iter().any(|n| !n.is_finite())
        {
            return Err("Original frame owner/extent/value validation failed".into());
        }
        for draw in draws {
            validate_draw(self.bridge_id, draw)?;
        }
        unsafe {
            self.draw_shared_target(target, |_, context, view| {
                context.ClearState();
                if let Some(color) = clear.color {
                    context.ClearRenderTargetView(view, &color);
                }
                if let Some(depth) = depth {
                    let flags = if clear.depth.is_some() {
                        D3D11_CLEAR_DEPTH.0 as u32
                    } else {
                        0
                    } | if clear.stencil.is_some() {
                        D3D11_CLEAR_STENCIL.0 as u32
                    } else {
                        0
                    };
                    if flags != 0 {
                        context.ClearDepthStencilView(
                            &depth.view,
                            flags,
                            clear.depth.unwrap_or(1.),
                            clear.stencil.unwrap_or(0),
                        );
                    }
                }
                context.OMSetRenderTargets(Some(&[Some(view.clone())]), depth.map(|d| &d.view));
                context.OMSetBlendState(&state.blend, Some(&blend_factor), u32::MAX);
                context.OMSetDepthStencilState(&state.depth, stencil_reference);
                context.RSSetState(&state.rasterizer);
                context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                    Width: size[0] as f32,
                    Height: size[1] as f32,
                    MaxDepth: 1.,
                    ..Default::default()
                }]));
                context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                for draw in draws {
                    context.IASetInputLayout(&draw.mesh.layout.layout);
                    context.IASetVertexBuffers(
                        0,
                        1,
                        Some([Some(draw.mesh.vertices.clone())].as_ptr()),
                        Some(&draw.mesh.stride),
                        Some(&0),
                    );
                    context.IASetIndexBuffer(&draw.mesh.indices, DXGI_FORMAT_R32_UINT, 0);
                    context.VSSetShader(&draw.program.vertex.shader, None);
                    context.PSSetShader(&draw.program.pixel.shader, None);
                    for cb in draw.vertex_constants {
                        context.VSSetConstantBuffers(cb.slot, Some(&[Some(cb.buffer.clone())]));
                    }
                    for cb in draw.pixel_constants {
                        context.PSSetConstantBuffers(cb.slot, Some(&[Some(cb.buffer.clone())]));
                    }
                    for (slot, plane) in draw.textures {
                        context.PSSetShaderResources(*slot, Some(&[Some(plane.view.clone())]));
                    }
                    for (slot, sampler) in draw.samplers {
                        context.PSSetSamplers(*slot, Some(&[Some(sampler.state.clone())]));
                    }
                    context.DrawIndexed(draw.mesh.count, 0, 0);
                }
                Ok(())
            })
        }
    }
}
fn validate_draw(owner: u64, draw: &OriginalDraw<'_>) -> Result<()> {
    if draw.program.bridge_id != owner
        || draw.mesh.bridge_id != owner
        || draw.mesh.layout.source_sha256 != draw.program.vertex.source_sha256
    {
        return Err("Original draw program/mesh owner or source mismatch".into());
    }
    for (provided, required) in [
        (
            draw.vertex_constants,
            &draw.program.vertex_bindings.constant_buffers,
        ),
        (
            draw.pixel_constants,
            &draw.program.pixel_bindings.constant_buffers,
        ),
    ] {
        let mut seen = std::collections::HashSet::new();
        for cb in provided {
            if cb.bridge_id != owner || !seen.insert(cb.slot) {
                return Err("Foreign or duplicate original constant binding".into());
            }
        }
        for cb in required {
            if !provided
                .iter()
                .any(|c| c.slot == cb.slot && c.bytes == cb.default_words.len() * 4)
            {
                return Err(format!(
                    "Missing or wrong-sized original constant buffer at slot{}",
                    cb.slot
                ));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    for (slot, plane) in draw.textures {
        if *slot > 127 || plane.bridge_id != owner || !seen.insert(*slot) {
            return Err("Foreign/duplicate/out-of-range original texture".into());
        }
    }
    for texture in &draw.program.pixel_bindings.textures {
        if !seen.contains(&(texture.slot as u32)) {
            return Err(format!(
                "Missing original material texture at slot{}",
                texture.slot
            ));
        }
    }
    seen.clear();
    for (slot, sampler) in draw.samplers {
        if *slot > 15 || sampler.bridge_id != owner || !seen.insert(*slot) {
            return Err("Foreign/duplicate/out-of-range original sampler".into());
        }
    }
    for sampler in draw.program.pixel_bindings.sampler_references() {
        if !seen.contains(&(sampler.slot as u32)) {
            return Err(format!(
                "Missing original pixel sampler at slot{}",
                sampler.slot
            ));
        }
    }
    // Preparation metadata does not declare every sampler or special buffer
    // (e.g. native vertex-decode CB1). Their explicit caller contract remains.
    Ok(())
}
