use external_cas_shader::{ShaderCode, Stage, VertexBindings, VertexSemantic};
use std::{
    marker::PhantomData,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use windows::{
    Win32::Graphics::{
        Direct3D::D3D_FEATURE_LEVEL_11_0, Direct3D11::*, Direct3D11on12::*, Direct3D12::*,
        Dxgi::Common::*,
    },
    core::{Interface, PCSTR},
};
mod frame;
pub use frame::*;
mod sampler;
pub use sampler::*;
mod geometry;
pub use geometry::*;
pub type Result<T> = std::result::Result<T, String>;
fn error(e: windows::core::Error) -> String {
    e.to_string()
}
static NEXT_BRIDGE: AtomicU64 = AtomicU64::new(1);

/// UI-thread-owned D3D11 context over the exact wgpu DX12 device/queue. Holding
/// both wgpu handles keeps their allocator and submission infrastructure alive.
pub struct OriginalShaderBridge {
    _wgpu_device: wgpu::Device,
    _wgpu_queue: wgpu::Queue,
    raw_device: ID3D12Device,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    on12: ID3D11On12Device,
    _ui_thread: PhantomData<Rc<()>>,
    bridge_id: u64,
}
pub struct OriginalPixelShader {
    pub shader: ID3D11PixelShader,
    pub source_sha256: String,
    pub source_reference: u32,
}
pub struct OriginalVertexShader {
    pub shader: ID3D11VertexShader,
    pub source_sha256: String,
    pub source_reference: u32,
}
pub struct OriginalInputLayout {
    pub layout: ID3D11InputLayout,
    pub source_sha256: String,
    pub source_reference: u32,
    pub decode_scales: [f32; 4],
}
/// An explicitly supplied native declaration. Missing semantics are refused;
/// the engine-owned fallback/default vertex stream is not reproduced here.
#[derive(Clone, Copy, Debug)]
pub struct NativeVertexElement {
    pub semantic: VertexSemantic,
    pub semantic_index: u32,
    pub native_format: u8,
    pub slot: u32,
    pub byte_offset: u32,
}
/// Exact current native declaration table; zero/unknown formats remain errors.
/// Byte sizes describe storage, not a promise that arbitrary GEOM floats have
/// already been encoded for the original shader's decode instructions.
pub fn native_vertex_format(format: u8) -> Result<(DXGI_FORMAT, u32)> {
    let (dxgi, bytes) = match format {
        0 => (41, 4),
        1 => (16, 8),
        2 => (6, 12),
        3 => (2, 16),
        4 => (30, 4),
        5 | 8 => (28, 4),
        6 | 9 => (37, 4),
        7 | 10 => (13, 8),
        11 => (35, 4),
        12 => (11, 8),
        15 => (34, 4),
        16 => (10, 8),
        _ => return Err(format!("Unsupported native vertex format {format}")),
    };
    Ok((DXGI_FORMAT(dxgi), bytes))
}
fn semantic_name(semantic: VertexSemantic) -> PCSTR {
    PCSTR(match semantic {
        VertexSemantic::Position => c"POSITION".as_ptr().cast(),
        VertexSemantic::Normal => c"NORMAL".as_ptr().cast(),
        VertexSemantic::TexCoord => c"TEXCOORD".as_ptr().cast(),
        VertexSemantic::BlendIndices => c"BLENDINDICES".as_ptr().cast(),
        VertexSemantic::BlendWeight => c"BLENDWEIGHT".as_ptr().cast(),
        VertexSemantic::Tangent => c"TANGENT".as_ptr().cast(),
        VertexSemantic::Color => c"COLOR".as_ptr().cast(),
    })
}
fn input_declaration(
    elements: &[NativeVertexElement],
    bindings: &VertexBindings,
) -> Result<(Vec<D3D11_INPUT_ELEMENT_DESC>, [f32; 4])> {
    if elements.len() > 32 {
        return Err("Native vertex element count exceeds D3D11 limit".into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut scales = [1.; 4];
    let mut descriptions = Vec::with_capacity(elements.len());
    for element in elements {
        if !seen.insert((element.semantic, element.semantic_index)) {
            return Err("Duplicate native vertex semantic".into());
        }
        let (format, bytes) = native_vertex_format(element.native_format)?;
        if element.slot > 31
            || element
                .byte_offset
                .checked_add(bytes)
                .is_none_or(|end| end > 2048)
        {
            return Err("Native vertex input slot/extent exceeds D3D11 limit".into());
        }
        // The source constructor changes only these four roles for Short2/4.
        if matches!(element.native_format, 6 | 7) {
            let component = match element.semantic {
                VertexSemantic::Position => Some(0),
                VertexSemantic::Normal => Some(1),
                VertexSemantic::Tangent => Some(2),
                VertexSemantic::TexCoord => Some(3),
                _ => None,
            };
            if let Some(component) = component {
                scales[component] = 32767.;
            }
        }
        descriptions.push(D3D11_INPUT_ELEMENT_DESC {
            SemanticName: semantic_name(element.semantic),
            SemanticIndex: element.semantic_index,
            Format: format,
            InputSlot: element.slot,
            AlignedByteOffset: element.byte_offset,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        });
    }
    for input in &bindings.inputs {
        if !seen.contains(&(input.semantic()?, input.semantic_index as u32)) {
            return Err(format!(
                "Missing original vertex input {}{}; no implicit default stream",
                input.semantic()?.name(),
                input.semantic_index
            ));
        }
    }
    Ok((descriptions, scales))
}
pub struct SharedTarget {
    // A COM reference alone does not pin wgpu's suballocation. Keep the wgpu
    // texture alive for the entire lifetime of the external view/wrapper.
    resource: ID3D11Resource,
    view: ID3D11RenderTargetView,
    raw_device: ID3D12Device,
    _ui_thread: PhantomData<Rc<()>>,
    bridge_id: u64,
    // Drop this LAST, after the native views/resources release their references.
    texture: wgpu::Texture,
}
impl OriginalShaderBridge {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self> {
        // Guards are borrowed only long enough to clone COM ownership. No HAL
        // object is destroyed or mutated through its borrowed internal handle.
        let hal_device = unsafe { device.as_hal::<wgpu::hal::api::Dx12>() }
            .ok_or("Original shader bridge requires the DX12 backend")?;
        let hal_queue = unsafe { queue.as_hal::<wgpu::hal::api::Dx12>() }
            .ok_or("Original shader bridge requires the matching DX12 queue")?;
        let raw_device = hal_device.raw_device().clone();
        let raw_queue = hal_queue.as_raw().clone();
        let mut queue_device: Option<ID3D12Device> = None;
        unsafe { raw_queue.GetDevice(&mut queue_device) }.map_err(error)?;
        if queue_device.as_ref().map(Interface::as_raw) != Some(raw_device.as_raw()) {
            return Err("D3D12 queue and device have different owners".into());
        }
        let mut native_device = None;
        let mut context = None;
        unsafe {
            D3D11On12CreateDevice(
                &raw_device,
                0,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                Some(&[Some(raw_queue.cast().map_err(error)?)]),
                0,
                Some(&mut native_device),
                Some(&mut context),
                None,
            )
        }
        .map_err(error)?;
        let native_device: ID3D11Device =
            native_device.ok_or("D3D11On12 did not return a device")?;
        let on12 = native_device.cast().map_err(error)?;
        let bridge_id = NEXT_BRIDGE.fetch_add(1, Ordering::Relaxed);
        if bridge_id == 0 {
            return Err("Original shader bridge identity budget exhausted".into());
        }
        Ok(Self {
            _wgpu_device: device.clone(),
            _wgpu_queue: queue.clone(),
            raw_device,
            device: native_device,
            context: context.ok_or("D3D11On12 did not return an immediate context")?,
            on12,
            _ui_thread: PhantomData,
            bridge_id,
        })
    }
    pub fn create_pixel_shader(&self, code: &ShaderCode) -> Result<OriginalPixelShader> {
        if code.stage != Stage::Pixel {
            return Err("A nonpixel shader cannot be bound as a pixel stage".into());
        }
        code.verify()?;
        let mut shader = None;
        unsafe {
            self.device
                .CreatePixelShader(&code.dxbc, None, Some(&mut shader))
        }
        .map_err(error)?;
        Ok(OriginalPixelShader {
            shader: shader.ok_or("D3D11 returned no original pixel shader")?,
            source_sha256: code.sha256.clone(),
            source_reference: code.reference,
        })
    }
    pub fn create_vertex_shader(&self, code: &ShaderCode) -> Result<OriginalVertexShader> {
        if code.stage != Stage::Vertex {
            return Err("A nonvertex shader cannot be bound as a vertex stage".into());
        }
        code.verify()?;
        let mut shader = None;
        unsafe {
            self.device
                .CreateVertexShader(&code.dxbc, None, Some(&mut shader))
        }
        .map_err(error)?;
        Ok(OriginalVertexShader {
            shader: shader.ok_or("D3D11 returned no original vertex shader")?,
            source_sha256: code.sha256.clone(),
            source_reference: code.reference,
        })
    }
    pub fn create_input_layout(
        &self,
        code: &ShaderCode,
        bindings: &VertexBindings,
        elements: &[NativeVertexElement],
    ) -> Result<OriginalInputLayout> {
        if code.stage != Stage::Vertex
            || code.reference != bindings.vertex_ref
            || code.source_file_sha256 != bindings.source_file_sha256
        {
            return Err(
                "Original vertex bindings and program belong to different sources/stages".into(),
            );
        }
        code.verify()?;
        let (descriptions, decode_scales) = input_declaration(elements, bindings)?;
        let mut layout = None;
        unsafe {
            self.device
                .CreateInputLayout(&descriptions, &code.dxbc, Some(&mut layout))
        }
        .map_err(error)?;
        Ok(OriginalInputLayout {
            layout: layout.ok_or("D3D11 returned no original input layout")?,
            source_sha256: code.sha256.clone(),
            source_reference: code.reference,
            decode_scales,
        })
    }
    pub fn wrap_target(&self, texture: &wgpu::Texture) -> Result<SharedTarget> {
        if !texture
            .usage()
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            || texture.sample_count() != 1
            || texture.mip_level_count() != 1
            || texture.dimension() != wgpu::TextureDimension::D2
            || texture.size().depth_or_array_layers != 1
        {
            return Err(
                "Shared shader target must be a single-sample, single-mip 2D render attachment"
                    .into(),
            );
        }
        let format = match texture.format() {
            wgpu::TextureFormat::Rgba8Unorm => DXGI_FORMAT_R8G8B8A8_UNORM,
            wgpu::TextureFormat::Rgba8UnormSrgb => DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            wgpu::TextureFormat::Rgba32Float => DXGI_FORMAT_R32G32B32A32_FLOAT,
            _ => return Err("Shared target format is not qualified".into()),
        };
        let hal = unsafe { texture.as_hal::<wgpu::hal::api::Dx12>() }
            .ok_or("Shared texture is not from the DX12 backend")?;
        let raw = unsafe { hal.raw_resource() };
        let mut resource_device: Option<ID3D12Device> = None;
        unsafe { raw.GetDevice(&mut resource_device) }.map_err(error)?;
        if resource_device.as_ref().map(Interface::as_raw) != Some(self.raw_device.as_raw()) {
            return Err("Shared texture belongs to a different D3D12 device".into());
        }
        let flags = D3D11_RESOURCE_FLAGS {
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            ..Default::default()
        };
        let mut resource: Option<ID3D11Resource> = None;
        unsafe {
            self.on12.CreateWrappedResource(
                raw,
                &flags,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                &mut resource,
            )
        }
        .map_err(error)?;
        let resource = resource.ok_or("D3D11 returned no wrapped target")?;
        let description = D3D11_RENDER_TARGET_VIEW_DESC {
            Format: format,
            ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_RENDER_TARGET_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_RTV { MipSlice: 0 },
            },
        };
        let mut view = None;
        unsafe {
            self.device
                .CreateRenderTargetView(&resource, Some(&description), Some(&mut view))
        }
        .map_err(error)?;
        Ok(SharedTarget {
            texture: texture.clone(),
            resource,
            view: view.ok_or("D3D11 returned no render target view")?,
            raw_device: self.raw_device.clone(),
            _ui_thread: PhantomData,
            bridge_id: self.bridge_id,
        })
    }
    /// Submits an otherwise empty wgpu attachment pass to establish the exact
    /// tracked/driver RENDER_TARGET state, then acquires and releases the same
    /// resource on the same queue. There is no CPU readback or per-frame wait.
    ///
    /// # Safety
    /// The caller owns exclusive frame access to this target until this method
    /// returns, including other threads and foreign queues. `draw` must not keep
    /// the immediate context/target for later use, change wrapped ownership, or
    /// submit work on another queue. It must release no borrowed native object.
    pub unsafe fn draw_shared_target(
        &mut self,
        target: &SharedTarget,
        draw: impl FnOnce(&ID3D11Device, &ID3D11DeviceContext, &ID3D11RenderTargetView) -> Result<()>,
    ) -> Result<()> {
        if self.bridge_id != target.bridge_id
            || self.raw_device.as_raw() != target.raw_device.as_raw()
        {
            return Err("Shared target belongs to another bridge/context".into());
        }
        unsafe { self.device.GetDeviceRemovedReason() }.map_err(error)?;
        let view = target.texture.create_view(&Default::default());
        let mut encoder =
            self._wgpu_device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Original CAS shader ownership boundary"),
                });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Establish original CAS attachment state"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        self._wgpu_queue.submit([encoder.finish()]);
        struct Acquired<'a> {
            bridge: &'a OriginalShaderBridge,
            resource: ID3D11Resource,
        }
        impl Drop for Acquired<'_> {
            fn drop(&mut self) {
                unsafe {
                    self.bridge.context.OMSetRenderTargets(None, None);
                    self.bridge
                        .on12
                        .ReleaseWrappedResources(&[Some(self.resource.clone())]);
                    self.bridge.context.Flush();
                }
            }
        }
        unsafe {
            self.on12
                .AcquireWrappedResources(&[Some(target.resource.clone())]);
        }
        let acquired = Acquired {
            bridge: self,
            resource: target.resource.clone(),
        };
        let result = draw(&self.device, &self.context, &target.view);
        drop(acquired);
        result?;
        unsafe { self.device.GetDeviceRemovedReason() }.map_err(error)
    }
    /// Used by the offscreen ownership qualification; application rendering
    /// supplies actual installed programs, geometry and validated bindings.
    ///
    /// # Safety
    /// Requires the same exclusive frame/queue ownership as `draw_shared_target`.
    pub unsafe fn clear_shared_target(
        &mut self,
        target: &SharedTarget,
        rgba: [f32; 4],
    ) -> Result<()> {
        if rgba.iter().any(|v| !v.is_finite()) {
            return Err("Nonfinite target value".into());
        }
        unsafe {
            self.draw_shared_target(target, |_, ctx, view| {
                ctx.ClearRenderTargetView(view, &rgba);
                Ok(())
            })
        }
    }
}
