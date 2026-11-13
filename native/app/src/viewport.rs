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
    pub rgba: std::sync::Arc<[u8]>,
    pub texture_size: [u32; 2],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RoomVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}
pub struct RoomPart {
    pub vertices: Vec<RoomVertex>,
    pub indices: Vec<u32>,
    pub rgba: Vec<u8>,
    pub texture_size: [u32; 2],
    pub tint: [f32; 4],
}
struct GpuRoomPart {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    material: wgpu::BindGroup,
    _parameters: wgpu::Buffer,
}

struct GpuPart {
    color_buffer: wgpu::Buffer,
    color_shift: [f32; 4],
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
    pub fov_degrees: f32,
    /// Original CAS orbital/FOV semantics. Explicit preview clipping and room
    /// origin remain separate from the qualified source camera equations.
    pub source_cas: bool,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.08,
            distance: 3.6,
            target: [0.0, 0.9, 0.0],
            fov_degrees: 36.,
            source_cas: false,
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
    fn matrix(&self, aspect: f32) -> Result<[[f32; 4]; 4], String> {
        if self.source_cas {
            use external_cas_scene::cas_camera::*;
            return source_cas_camera_frame(
                SourceCasCameraProfile {
                    target: self.target,
                    fov_degrees: self.fov_degrees,
                    distance: self.distance,
                    orbital_rotation_degrees: self.yaw.to_degrees(),
                    pitch_degrees: self.pitch.to_degrees(),
                },
                SourceCasCameraProjection {
                    aspect_ratio: aspect,
                    near_clip: 0.01,
                    far_clip: 200.,
                },
            )
            .map(|frame| frame.view_projection_rows);
        }
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
        let t = 1. / (0.5 * self.fov_degrees.clamp(1., 120.).to_radians()).tan();
        let (near, far) = (0.01, 200.0);
        let projection = [
            [t / aspect, 0., 0., 0.],
            [0., t, 0., 0.],
            [0., 0., far / (near - far), -1.],
            [0., 0., far * near / (near - far), 0.],
        ];
        Ok(mul(projection, view))
    }
    pub fn from_source_profile(
        profile: external_cas_scene::cas_camera::SourceCasCameraProfile,
    ) -> Result<Self, String> {
        let camera = Self {
            yaw: profile.orbital_rotation_degrees.to_radians(),
            pitch: profile.pitch_degrees.to_radians(),
            distance: profile.distance,
            target: profile.target,
            fov_degrees: profile.fov_degrees,
            source_cas: true,
        };
        camera.matrix(1.)?;
        Ok(camera)
    }
    pub fn frame(&mut self, minimum: [f32; 3], maximum: [f32; 3]) {
        self.source_cas = false;
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
    translucent_pipeline: wgpu::RenderPipeline,
    room_pipeline: wgpu::RenderPipeline,
    source_room_pipeline: wgpu::RenderPipeline,
    room_parts: Vec<GpuRoomPart>,
    camera_buffer: wgpu::Buffer,
    palette_buffer: wgpu::Buffer,
    model_buffer: wgpu::Buffer,
    lighting_buffer: wgpu::Buffer,
    scene_binding: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    parts: Vec<GpuPart>,
    color: wgpu::TextureView,
    ui_color: wgpu::TextureView,
    depth: wgpu::TextureView,
    size: [u32; 2],
    pub texture_id: egui::TextureId,
    pub camera: Camera,
    pub model_yaw: f32,
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
    pub fn material_shifts(&self) -> Vec<[f32; 4]> {
        self.parts.iter().map(|p| p.color_shift).collect()
    }
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
        let model_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("CAS appearance transform"),
            contents: bytemuck::cast_slice(&IDENTITY),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let lighting_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("CAS installed lighting"),
            contents: bytemuck::cast_slice(&[[0f32; 4]; 9]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: model_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: lighting_buffer.as_entire_binding(),
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
        let translucent_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("CAS explicit opacity preview"),
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
                depth_write_enabled: Some(false),
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
        let room_attributes =
            wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Float32x4];
        let source_room_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Original installed CAS room"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_room_mesh"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RoomVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &room_attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_room_mesh"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            translucent_pipeline,
            room_pipeline,
            source_room_pipeline,
            room_parts: Vec::new(),
            camera_buffer,
            palette_buffer,
            model_buffer,
            lighting_buffer,
            scene_binding,
            material_layout,
            parts: Vec::new(),
            color,
            ui_color,
            depth,
            size,
            texture_id,
            camera: Camera::default(),
            model_yaw: 0.,
            rendered_frames: 0,
            framing_offset: 0.,
        }
    }

    /// Applies exact observed values. Direction/hemisphere/rim response is a
    /// renderer candidate until compared with the installed shader equations.
    pub fn set_installed_lighting(
        &mut self,
        source: &external_cas_room::LightingTuning,
    ) -> Result<(), String> {
        let number = |v: &Option<external_cas_room::TuningFloat>| {
            v.as_ref().ok_or("Installed light value absent")?.finite()
        };
        let rgb = |v: &Option<[external_cas_room::TuningFloat; 3]>| -> Result<[f32; 3], String> {
            let v = v.as_ref().ok_or("Installed light color absent")?;
            Ok([v[0].finite()?, v[1].finite()?, v[2].finite()?])
        };
        let mut packed = [[0.; 4]; 9];
        packed[0] = [
            number(&source.ambient_min)?,
            number(&source.ambient_max)?,
            1.,
            0.,
        ];
        for (i, light) in source.key_lights.iter().enumerate() {
            let a = number(&light.angle)?.to_radians();
            let h = number(&light.height)? + number(&source.light_base_height)?;
            let d = unit([-a.sin(), h, a.cos()]);
            let c = rgb(&light.color)?;
            let intensity = if light.enable.ok_or("Installed light enable absent")? {
                number(&light.intensity)?
            } else {
                0.
            };
            packed[1 + i] = [d[0], d[1], d[2], number(&light.specular_intensity)?];
            packed[3 + i] = [c[0] * intensity, c[1] * intensity, c[2] * intensity, 0.];
        }
        for (i, light) in source.rim_lights.iter().enumerate() {
            let c = rgb(&light.color)?;
            let intensity = if light.enable.ok_or("Installed rim enable absent")? {
                number(&light.intensity)?
            } else {
                0.
            };
            packed[5 + i] = [
                c[0] * intensity,
                c[1] * intensity,
                c[2] * intensity,
                number(&light.falloff)?,
            ];
        }
        if packed.iter().flatten().any(|v| !v.is_finite()) {
            return Err("Nonfinite installed lighting".into());
        }
        self.state
            .queue
            .write_buffer(&self.lighting_buffer, 0, bytemuck::cast_slice(&packed));
        Ok(())
    }
    pub fn clear_installed_lighting(&mut self) {
        self.state.queue.write_buffer(
            &self.lighting_buffer,
            0,
            bytemuck::cast_slice(&[[0f32; 4]; 9]),
        );
    }
    pub fn set_room(&mut self, parts: Vec<RoomPart>) -> Result<(), String> {
        for part in &parts {
            if part.vertices.is_empty()
                || part.indices.is_empty()
                || part.indices.len() % 3 != 0
                || part
                    .indices
                    .iter()
                    .any(|&i| i as usize >= part.vertices.len())
                || part.vertices.iter().any(|v| {
                    v.position
                        .iter()
                        .chain(v.normal.iter())
                        .chain(v.uv.iter())
                        .chain(v.color.iter())
                        .any(|f| !f.is_finite())
                })
            {
                return Err("Original room geometry is invalid".into());
            }
            let [w, h] = part.texture_size;
            if w == 0
                || h == 0
                || w > self.state.device.limits().max_texture_dimension_2d
                || h > self.state.device.limits().max_texture_dimension_2d
                || w as u64 * h as u64 * 4 != part.rgba.len() as u64
                || part.tint.iter().any(|v| !v.is_finite())
            {
                return Err("Original room texture/material extent is invalid".into());
            }
        }
        let device = &self.state.device;
        let mut uploaded = Vec::new();
        for part in parts {
            let texture = device.create_texture_with_data(
                &self.state.queue,
                &wgpu::TextureDescriptor {
                    label: Some("Original CAS room texture"),
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
                label: Some("Native room sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Native room diffuse/opacity"),
                contents: bytemuck::cast_slice(&part.tint),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let material = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Native room material"),
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
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: parameters.as_entire_binding(),
                    },
                ],
            });
            uploaded.push(GpuRoomPart {
                vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Native room vertices"),
                    contents: bytemuck::cast_slice(&part.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Native room indices"),
                    contents: bytemuck::cast_slice(&part.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                count: part.indices.len() as u32,
                material,
                _parameters: parameters,
            });
        }
        self.room_parts = uploaded;
        Ok(())
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
        let mut textures = std::collections::HashMap::new();
        for part in parts {
            for v in &part.vertices {
                for k in 0..3 {
                    minimum[k] = minimum[k].min(v.position[k]);
                    maximum[k] = maximum[k].max(v.position[k]);
                }
            }
            let key = (part.rgba.as_ptr() as usize, part.texture_size);
            let view = textures.entry(key).or_insert_with(|| {
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
                texture.create_view(&Default::default())
            });
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("CAS material sampler"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let color_shift = [0f32, 0., 0., 1.];
            let color_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("CAS source color shifts"),
                contents: bytemuck::cast_slice(&color_shift),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: color_buffer.as_entire_binding(),
                    },
                ],
            });
            uploaded.push(GpuPart {
                color_buffer,
                color_shift,
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

    pub fn set_part_color_shift(
        &mut self,
        index: usize,
        shift: external_cas_material::color::PackedColorShift,
    ) -> Result<(), String> {
        if !shift.preview_supported() {
            return Err(
                "Stored color shift has an unqualified preview range; original bits are preserved"
                    .into(),
            );
        }
        let part = self
            .parts
            .get_mut(index)
            .ok_or("Color target mesh is unavailable")?;
        part.color_shift = shift.hue_saturation_value_opacity();
        self.state.queue.write_buffer(
            &part.color_buffer,
            0,
            bytemuck::cast_slice(&part.color_shift),
        );
        Ok(())
    }
    pub fn set_item_color_shift(
        &mut self,
        shift: external_cas_material::color::PackedColorShift,
    ) -> Result<(), String> {
        if !shift.preview_supported() {
            return Err(
                "Stored color shift has an unqualified preview range; original bits are preserved"
                    .into(),
            );
        }
        for index in 0..self.parts.len() {
            self.set_part_color_shift(index, shift)?;
        }
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

    pub fn render(&mut self, size: [u32; 2]) -> Result<(), String> {
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
        let mut matrix = self.camera.matrix(size[0] as f32 / size[1] as f32)?;
        for column in &mut matrix {
            column[0] += self.framing_offset * column[3];
        }
        self.state
            .queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&matrix));
        let (s, c) = self.model_yaw.sin_cos();
        let model = [
            [c, 0., -s, 0.],
            [0., 1., 0., 0.],
            [s, 0., c, 0.],
            [0., 0., 0., 1.],
        ];
        self.state
            .queue
            .write_buffer(&self.model_buffer, 0, bytemuck::cast_slice(&model));
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
            if self.room_parts.is_empty() {
                pass.set_pipeline(&self.room_pipeline);
                pass.draw(0..3, 0..1);
            } else {
                pass.set_pipeline(&self.source_room_pipeline);
                pass.set_bind_group(0, &self.scene_binding, &[]);
                for part in &self.room_parts {
                    pass.set_bind_group(1, &part.material, &[]);
                    pass.set_vertex_buffer(0, part.vertices.slice(..));
                    pass.set_index_buffer(part.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..part.count, 0, 0..1);
                }
            }
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.scene_binding, &[]);
            for part in &self.parts {
                if part.color_shift[3] < 1. {
                    continue;
                }
                pass.set_bind_group(1, &part.material, &[]);
                pass.set_vertex_buffer(0, part.vertices.slice(..));
                pass.set_index_buffer(part.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..part.count, 0, 0..1);
            }
            // Explicit opacity preview. Full source material-family and
            // per-triangle transparency ordering remain separate fidelity gates.
            pass.set_pipeline(&self.translucent_pipeline);
            for part in &self.parts {
                if part.color_shift[3] >= 1. {
                    continue;
                }
                pass.set_bind_group(1, &part.material, &[]);
                pass.set_vertex_buffer(0, part.vertices.slice(..));
                pass.set_index_buffer(part.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..part.count, 0, 0..1);
            }
        }
        self.state.queue.submit([encoder.finish()]);
        self.rendered_frames += 1;
        Ok(())
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
            let matrix = camera.matrix(aspect).unwrap();
            let point = [camera.target[0], camera.target[1], camera.target[2], 1.];
            let clip: [f32; 4] =
                std::array::from_fn(|r| (0..4).map(|c| matrix[c][r] * point[c]).sum());
            assert!(clip[3] > 0.);
            assert!((clip[0] / clip[3]).abs() < 0.0001);
            assert!((clip[1] / clip[3]).abs() < 0.0001);
            assert!((0.0..1.0).contains(&(clip[2] / clip[3])));
        }
    }
    #[test]
    fn original_camera_rows_reach_wgsl_column_layout_and_invalid_source_is_refused() {
        use external_cas_scene::cas_camera::*;
        let profile = SourceCasCameraProfile {
            target: [0.04, 0.9, 0.12],
            fov_degrees: 44.,
            distance: 3.95,
            orbital_rotation_degrees: 177.,
            pitch_degrees: 5.,
        };
        let camera = Camera::from_source_profile(profile).unwrap();
        assert!(camera.source_cas);
        for aspect in [0.3, 1., 16. / 9., 3.] {
            let matrix = camera.matrix(aspect).unwrap();
            let clip: [f32; 4] = std::array::from_fn(|r| {
                (0..4)
                    .map(|c| {
                        matrix[c][r] * [camera.target[0], camera.target[1], camera.target[2], 1.][c]
                    })
                    .sum()
            });
            assert!(clip[3] > 0.);
            assert!((clip[0] / clip[3]).abs() < 1e-5);
            assert!((clip[1] / clip[3]).abs() < 1e-5);
            assert!((0.0..1.0).contains(&(clip[2] / clip[3])));
        }
        assert!(
            Camera::from_source_profile(SourceCasCameraProfile {
                distance: 0.,
                ..profile
            })
            .is_err()
        );
        let mut invalid = camera;
        invalid.fov_degrees = f32::NAN;
        assert!(invalid.matrix(1.).is_err());
    }
}
