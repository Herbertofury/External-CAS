use eframe::{egui, egui_wgpu::RenderState};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

pub struct MeshPart {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub rgba: Vec<u8>,
    pub texture_size: [u32; 2],
}

struct GpuPart {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    material: wgpu::BindGroup,
    vertex_count: usize,
}

#[derive(Clone, Copy)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: [f32; 3],
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.08,
            distance: 3.6,
            target: [0.0, 0.9, 0.0],
        }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: [f32; 3]) -> [f32; 3] {
    let n = dot(a, a).sqrt().max(0.00001);
    a.map(|v| v / n)
}
fn mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    std::array::from_fn(|c| std::array::from_fn(|r| (0..4).map(|k| a[k][r] * b[c][k]).sum()))
}
pub const IDENTITY: [[f32; 4]; 4] = [
    [1., 0., 0., 0.],
    [0., 1., 0., 0.],
    [0., 0., 1., 0.],
    [0., 0., 0., 1.],
];
impl Camera {
    fn matrix(&self, aspect: f32) -> [[f32; 4]; 4] {
        let cp = self.pitch.cos();
        let eye = [
            self.target[0] + self.distance * cp * self.yaw.sin(),
            self.target[1] + self.distance * self.pitch.sin(),
            self.target[2] + self.distance * cp * self.yaw.cos(),
        ];
        let f = unit(sub(self.target, eye));
        let s = unit(cross(f, [0., 1., 0.]));
        let u = cross(s, f);
        let view = [
            [s[0], u[0], -f[0], 0.],
            [s[1], u[1], -f[1], 0.],
            [s[2], u[2], -f[2], 0.],
            [-dot(s, eye), -dot(u, eye), dot(f, eye), 1.],
        ];
        let t = 1. / (0.5 * 36f32.to_radians()).tan();
        let (near, far) = (0.01, 200.0);
        let projection = [
            [t / aspect, 0., 0., 0.],
            [0., t, 0., 0.],
            [0., 0., far / (near - far), -1.],
            [0., 0., far * near / (near - far), 0.],
        ];
        mul(projection, view)
    }
    pub fn frame(&mut self, minimum: [f32; 3], maximum: [f32; 3]) {
        self.target = std::array::from_fn(|i| (minimum[i] + maximum[i]) * 0.5);
        let diameter = dot(sub(maximum, minimum), sub(maximum, minimum))
            .sqrt()
            .max(0.01);
        self.distance = diameter * 1.9;
    }
}

pub struct Viewport {
    state: RenderState,
    pipeline: wgpu::RenderPipeline,
    room_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    palette_buffer: wgpu::Buffer,
    scene_binding: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    parts: Vec<GpuPart>,
    color: wgpu::TextureView,
    ui_color: wgpu::TextureView,
    depth: wgpu::TextureView,
    size: [u32; 2],
    pub texture_id: egui::TextureId,
    pub camera: Camera,
    pub rendered_frames: u64,
    pub framing_offset: f32,
}

fn target(
    device: &wgpu::Device,
    size: [u32; 2],
    depth: bool,
) -> (wgpu::TextureView, Option<wgpu::TextureView>) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(if depth { "CAS depth" } else { "CAS viewport" }),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if depth {
            wgpu::TextureFormat::Depth32Float
        } else {
            wgpu::TextureFormat::Rgba8UnormSrgb
        },
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | if depth {
                wgpu::TextureUsages::empty()
            } else {
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC
            },
        view_formats: if depth {
            &[]
        } else {
            &[wgpu::TextureFormat::Rgba8Unorm]
        },
    });
    let render = texture.create_view(&Default::default());
    // egui samples user textures as gamma-encoded bytes. Render in linear light
    // through the sRGB view, then present the stored bytes through an UNORM view.
    // Registering the sRGB view would decode them a second time and darken colors.
    let ui = if depth {
        None
    } else {
        Some(texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        }))
    };
    (render, ui)
}

impl Viewport {
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.state.adapter.get_info()
    }

    pub fn new(state: RenderState) -> Self {
        let device = &state.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("External CAS material"),
            source: wgpu::ShaderSource::Wgsl(include_str!("viewport.wgsl").into()),
        });
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera"),
            contents: bytemuck::cast_slice(&IDENTITY),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let palette_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skin palette"),
            contents: bytemuck::cast_slice(&[IDENTITY; 256]),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let scene_binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene bindings"),
            layout: &scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: palette_buffer.as_entire_binding(),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("CAS pipeline layout"),
            bind_group_layouts: &[Some(&scene_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0=>Float32x3, 1=>Float32x3, 2=>Float32x2, 3=>Uint16x4, 4=>Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("CAS skinned material"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let room_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("CAS room backdrop"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_room"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_room"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let size = [64, 64];
        let (color, ui_color) = target(device, size, false);
        let ui_color = ui_color.expect("color target has a presentation view");
        let depth = target(device, size, true).0;
        let texture_id = state.renderer.write().register_native_texture(
            device,
            &ui_color,
            wgpu::FilterMode::Linear,
        );
        Self {
            state,
            pipeline,
            room_pipeline,
            camera_buffer,
            palette_buffer,
            scene_binding,
            material_layout,
            parts: Vec::new(),
            color,
            ui_color,
            depth,
            size,
            texture_id,
            camera: Camera::default(),
            rendered_frames: 0,
            framing_offset: 0.,
        }
    }

    pub fn set_parts(&mut self, parts: Vec<MeshPart>) -> Result<(), String> {
        // Validate the complete replacement before changing the currently visible scene.
        for part in &parts {
            let [w, h] = part.texture_size;
            if part.vertices.is_empty()
                || part.indices.is_empty()
                || part.indices.len() % 3 != 0
                || part
                    .indices
                    .iter()
                    .any(|&i| i as usize >= part.vertices.len())
            {
                return Err("Invalid triangle topology".into());
            }
            if w == 0
                || h == 0
                || w > self.state.device.limits().max_texture_dimension_2d
                || h > self.state.device.limits().max_texture_dimension_2d
                || u64::from(w) * u64::from(h) * 4 != part.rgba.len() as u64
            {
                return Err("Invalid texture dimensions or extent".into());
            }
            if part.vertices.iter().any(|v| {
                v.position
                    .iter()
                    .chain(v.normal.iter())
                    .chain(v.uv.iter())
                    .chain(v.weights.iter())
                    .any(|x| !x.is_finite())
                    || v.joints.iter().any(|&j| j >= 256)
                    || v.weights.iter().any(|&w| !(0.0..=1.0).contains(&w))
                    || v.weights.iter().sum::<f32>() <= 0.
            }) {
                return Err("Invalid vertex or skin data".into());
            }
        }
        let device = &self.state.device;
        let mut uploaded = Vec::with_capacity(parts.len());
        let mut minimum = [f32::INFINITY; 3];
        let mut maximum = [f32::NEG_INFINITY; 3];
        for part in parts {
            for v in &part.vertices {
                for k in 0..3 {
                    minimum[k] = minimum[k].min(v.position[k]);
                    maximum[k] = maximum[k].max(v.position[k]);
                }
            }
            let texture = device.create_texture_with_data(
                &self.state.queue,
                &wgpu::TextureDescriptor {
                    label: Some("CAS diffuse sRGB"),
                    size: wgpu::Extent3d {
                        width: part.texture_size[0],
                        height: part.texture_size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &part.rgba,
            );
            let view = texture.create_view(&Default::default());
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("CAS material sampler"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let material = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("CAS diffuse"),
                layout: &self.material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
            uploaded.push(GpuPart {
                vertex_count: part.vertices.len(),
                vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("CAS vertices"),
                    contents: bytemuck::cast_slice(&part.vertices),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                }),
                indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("CAS indices"),
                    contents: bytemuck::cast_slice(&part.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                count: part.indices.len() as u32,
                material,
            });
        }
        if minimum[0].is_finite() {
            self.camera.frame(minimum, maximum);
        }
        self.parts = uploaded;
        self.state.queue.write_buffer(
            &self.palette_buffer,
            0,
            bytemuck::cast_slice(&[IDENTITY; 256]),
        );
        Ok(())
    }

    pub fn set_skin_vertices(&mut self, parts: &[Vec<Vertex>]) -> Result<(), String> {
        if parts.len() != self.parts.len()
            || parts
                .iter()
                .zip(&self.parts)
                .any(|(v, p)| v.len() != p.vertex_count)
        {
            return Err("Skin binding does not match the visible geometry".into());
        }
        if parts.iter().flatten().any(|v| {
            v.joints.iter().any(|&j| j >= 256)
                || v.weights
                    .iter()
                    .any(|w| !w.is_finite() || !(0.0..=1.0).contains(w))
                || v.weights.iter().sum::<f32>() <= 0.
        }) {
            return Err("Invalid skin binding".into());
        }
        for (vertices, part) in parts.iter().zip(&self.parts) {
            self.state
                .queue
                .write_buffer(&part.vertices, 0, bytemuck::cast_slice(vertices));
        }
        Ok(())
    }
    pub fn set_palette(&mut self, matrices: &[[f32; 16]]) -> Result<(), String> {
        if matrices.is_empty()
            || matrices.len() > 256
            || matrices.iter().flatten().any(|v| !v.is_finite())
        {
            return Err("Invalid bone palette".into());
        }
        let mut palette = [IDENTITY; 256];
        for (dst, src) in palette.iter_mut().zip(matrices) {
            *dst = std::array::from_fn(|c| std::array::from_fn(|r| src[c * 4 + r]));
        }
        self.state
            .queue
            .write_buffer(&self.palette_buffer, 0, bytemuck::cast_slice(&palette));
        Ok(())
    }
    pub fn reset_pose(&mut self) {
        self.state.queue.write_buffer(
            &self.palette_buffer,
            0,
            bytemuck::cast_slice(&[IDENTITY; 256]),
        );
    }

    pub fn render(&mut self, size: [u32; 2]) {
        let limit = self.state.device.limits().max_texture_dimension_2d;
        let size = size.map(|n| n.clamp(1, limit));
        if size != self.size {
            self.size = size;
            let (color, ui_color) = target(&self.state.device, size, false);
            self.color = color;
            self.ui_color = ui_color.expect("color target has a presentation view");
            self.depth = target(&self.state.device, size, true).0;
            self.state
                .renderer
                .write()
                .update_egui_texture_from_wgpu_texture(
                    &self.state.device,
                    &self.ui_color,
                    wgpu::FilterMode::Linear,
                    self.texture_id,
                );
        }
        let mut matrix = self.camera.matrix(size[0] as f32 / size[1] as f32);
        for column in &mut matrix {
            column[0] += self.framing_offset * column[3];
        }
        self.state
            .queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&matrix));
        let mut encoder =
            self.state
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("CAS viewport commands"),
                });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("CAS viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.105,
                            g: 0.117,
                            b: 0.124,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.room_pipeline);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.scene_binding, &[]);
            for part in &self.parts {
                pass.set_bind_group(1, &part.material, &[]);
                pass.set_vertex_buffer(0, part.vertices.slice(..));
                pass.set_index_buffer(part.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..part.count, 0, 0..1);
            }
        }
        self.state.queue.submit([encoder.finish()]);
        self.rendered_frames += 1;
    }
}
impl Drop for Viewport {
    fn drop(&mut self) {
        self.state.renderer.write().free_texture(&self.texture_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_projects_center_inside_frustum_and_preserves_depth_order() {
        let mut camera = Camera::default();
        camera.frame([-0.5, -0.2, 0.], [0.5, 0.2, 1.9]);
        for aspect in [0.3, 1., 3.] {
            let matrix = camera.matrix(aspect);
            let point = [camera.target[0], camera.target[1], camera.target[2], 1.];
            let clip: [f32; 4] =
                std::array::from_fn(|r| (0..4).map(|c| matrix[c][r] * point[c]).sum());
            assert!(clip[3] > 0.);
            assert!((clip[0] / clip[3]).abs() < 0.0001);
            assert!((clip[1] / clip[3]).abs() < 0.0001);
            assert!((0.0..1.0).contains(&(clip[2] / clip[3])));
        }
    }
}
