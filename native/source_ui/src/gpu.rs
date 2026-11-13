// SPDX-License-Identifier: GPL-3.0-or-later
use crate::wgpu;
use ruffle_render_wgpu::{
    backend::WgpuRenderBackend,
    descriptors::Descriptors,
    target::{RenderTarget, RenderTargetFrame},
};
use std::sync::Arc;
#[derive(Clone)]
pub struct SharedGpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub target_format: wgpu::TextureFormat,
}
pub struct UiFrame {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub dimensions: [u32; 2],
    pub format: wgpu::TextureFormat,
    pub premultiplied_alpha: bool,
}
#[derive(Debug)]
pub(crate) struct OverlayTarget {
    pub texture: wgpu::Texture,
    pub dimensions: [u32; 2],
    pub format: wgpu::TextureFormat,
}
#[derive(Debug)]
pub(crate) struct Frame(wgpu::TextureView);
impl RenderTargetFrame for Frame {
    fn into_view(self) -> wgpu::TextureView {
        self.0
    }
    fn view(&self) -> &wgpu::TextureView {
        &self.0
    }
}
impl OverlayTarget {
    fn new(device: &wgpu::Device, size: [u32; 2], format: wgpu::TextureFormat) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Original CAS UI transparent overlay"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            view_formats: &[],
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        });
        Self {
            texture,
            dimensions: size,
            format,
        }
    }
}
impl RenderTarget for OverlayTarget {
    type Frame = Frame;
    fn resize(&mut self, d: &wgpu::Device, w: u32, h: u32) {
        *self = Self::new(d, [w, h], self.format);
    }
    fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
    fn width(&self) -> u32 {
        self.dimensions[0]
    }
    fn height(&self) -> u32 {
        self.dimensions[1]
    }
    fn get_next_texture(&mut self) -> Option<Frame> {
        Some(Frame(self.texture.create_view(&Default::default())))
    }
    fn submit<I: IntoIterator<Item = wgpu::CommandBuffer>>(
        &self,
        _: &wgpu::Device,
        q: &wgpu::Queue,
        c: I,
        _: Frame,
    ) -> wgpu::SubmissionIndex {
        q.submit(c)
    }
}
pub(crate) fn validate_dimensions(
    d: &wgpu::Device,
    size: [u32; 2],
    dpi: f64,
) -> Result<(), String> {
    if size
        .iter()
        .any(|v| *v == 0 || *v > d.limits().max_texture_dimension_2d)
        || !dpi.is_finite()
        || dpi <= 0.
    {
        Err("source UI viewport dimensions/scale are outside device limits".into())
    } else {
        Ok(())
    }
}
pub(crate) fn renderer(
    gpu: SharedGpu,
    size: [u32; 2],
) -> Result<WgpuRenderBackend<OverlayTarget>, String> {
    // Ruffle's actual Flash display values currently qualify in a non-sRGB
    // target. Do not silently apply an additional transfer curve.
    if !matches!(
        gpu.target_format,
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
    ) {
        return Err("source UI overlay requires an explicit non-sRGB RGBA8/BGRA8 intermediate; sample/composite it once in the host".into());
    }
    let target = OverlayTarget::new(&gpu.device, size, gpu.target_format);
    let d = Descriptors::new(gpu.instance, gpu.adapter, gpu.device, gpu.queue);
    WgpuRenderBackend::new(Arc::new(d), target).map_err(|e| e.to_string())
}
pub(crate) fn get_renderer(
    p: &ruffle_core::Player,
) -> Result<&WgpuRenderBackend<OverlayTarget>, String> {
    <dyn std::any::Any>::downcast_ref(p.renderer())
        .ok_or_else(|| "source UI renderer type changed".into())
}
