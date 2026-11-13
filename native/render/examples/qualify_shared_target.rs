//! Opt-in actual-device qualification, not a fake application viewport.
//! Diagnostic GPU readback exists only here; production interop has none.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use external_cas_render::{NativeVertexElement, OriginalShaderBridge};
    use external_cas_shader::{Limits, ShaderLibrary, Stage, VertexSemantic};
    use std::{
        path::PathBuf,
        sync::{atomic::AtomicBool, mpsc},
        time::Duration,
    };
    let path = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Explicit installed PRECOMP path required")?,
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let mut bridge = OriginalShaderBridge::new(&device, &queue)?;
    let library = ShaderLibrary::open(&path, Limits::default(), &AtomicBool::new(false))?;
    let original = bridge.create_pixel_shader(&library.shader(
        Stage::Pixel,
        3038,
        &AtomicBool::new(false),
    )?)?;
    let vertex_code = library.shader(Stage::Vertex, 924, &AtomicBool::new(false))?;
    let vertex_bindings = library.vertex_bindings(924, &AtomicBool::new(false))?;
    let original_vertex = bridge.create_vertex_shader(&vertex_code)?;
    let paired_pixel = bridge.create_pixel_shader(&library.shader(
        Stage::Pixel,
        3051,
        &AtomicBool::new(false),
    )?)?;
    // Explicit float storage is only a controlled input-layout qualification.
    // This does not claim the native game's quantized GEOM buffer production.
    let elements = [
        (VertexSemantic::Position, 0, 3, 0),
        (VertexSemantic::Normal, 0, 2, 16),
        (VertexSemantic::TexCoord, 0, 1, 28),
        (VertexSemantic::TexCoord, 1, 1, 36),
        (VertexSemantic::Color, 1, 3, 44),
        (VertexSemantic::BlendIndices, 0, 4, 60),
        (VertexSemantic::BlendWeight, 0, 3, 64),
        (VertexSemantic::Tangent, 0, 3, 80),
    ]
    .map(
        |(semantic, semantic_index, native_format, byte_offset)| NativeVertexElement {
            semantic,
            semantic_index,
            native_format,
            slot: 0,
            byte_offset,
        },
    );
    let original_layout = bridge.create_input_layout(&vertex_code, &vertex_bindings, &elements)?;
    if original_layout.decode_scales != [1.; 4] {
        return Err("Float declaration acquired nonnative decode scales".into());
    }
    if bridge
        .create_input_layout(&vertex_code, &vertex_bindings, &elements[..7])
        .is_ok()
    {
        return Err("Missing original tangent input was silently substituted".into());
    }
    let mut foreign_bindings = vertex_bindings.clone();
    foreign_bindings.source_file_sha256.push('0');
    if bridge
        .create_input_layout(&vertex_code, &foreign_bindings, &elements)
        .is_ok()
    {
        return Err("Foreign vertex binding source was accepted".into());
    }
    let mut observations = Vec::new();
    let mut other_bridge = OriginalShaderBridge::new(&device, &queue)?;
    let mut rejected_foreign_contexts = 0;
    let mut recovered_callback_errors = 0;
    for format in [
        wgpu::TextureFormat::Rgba32Float,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        for (width, height) in [(8, 8), (16, 8), (8, 16)] {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Qualified shared original-shader target"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let target = bridge.wrap_target(&texture)?;
            if unsafe { other_bridge.clear_shared_target(&target, [1.; 4]) }.is_ok() {
                return Err("A foreign D3D11 context acquired another bridge target".into());
            }
            rejected_foreign_contexts += 1;
            let interrupted = unsafe {
                bridge.draw_shared_target(&target, |_, _, _| {
                    Err("Expected diagnostic callback cancellation".into())
                })
            };
            if interrupted
                .as_ref()
                .err()
                .is_none_or(|e| !e.contains("Expected diagnostic"))
            {
                return Err("Interrupted frame did not preserve callback error".into());
            }
            recovered_callback_errors += 1;
            for color in [[0.125, 0.25, 0.5, 1.], [0.75, 0.5, 0.25, 0.]] {
                // This example owns the texture exclusively and never gives the
                // context or resource to another thread, queue or retained closure.
                unsafe { bridge.clear_shared_target(&target, color) }?;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Diagnostic shared target readback"),
                    size: 512 * height as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let mut encoder = device.create_command_encoder(&Default::default());
                let reference = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Independent wgpu clear reference"),
                    size: texture.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let reference_view = reference.create_view(&Default::default());
                {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Reference native DX12 format conversion"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &reference_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: color[0] as f64,
                                    g: color[1] as f64,
                                    b: color[2] as f64,
                                    a: color[3] as f64,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                }
                encoder.copy_texture_to_buffer(
                    texture.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(256),
                            rows_per_image: Some(height),
                        },
                    },
                    texture.size(),
                );
                encoder.copy_texture_to_buffer(
                    reference.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 256 * height as u64,
                            bytes_per_row: Some(256),
                            rows_per_image: Some(height),
                        },
                    },
                    reference.size(),
                );
                let submitted = queue.submit([encoder.finish()]);
                let (tx, rx) = mpsc::channel();
                buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        let _ = tx.send(result);
                    });
                device.poll(wgpu::PollType::Wait {
                    submission_index: Some(submitted),
                    timeout: Some(Duration::from_secs(10)),
                })?;
                rx.recv_timeout(Duration::from_secs(1))??;
                let mapped = buffer.slice(..).get_mapped_range()?;
                let mut exact = 0usize;
                for y in 0..height as usize {
                    for x in 0..width as usize {
                        let bytes_per_pixel = if format == wgpu::TextureFormat::Rgba32Float {
                            16
                        } else {
                            4
                        };
                        let at = y * 256 + x * bytes_per_pixel;
                        let reference_at = 256 * height as usize + at;
                        if mapped[at..at + bytes_per_pixel]
                            != mapped[reference_at..reference_at + bytes_per_pixel]
                        {
                            return Err(format!("Native On12 versus independent wgpu target format differs in {format:?} at {x},{y}").into());
                        }
                        let exact_pixel = if format == wgpu::TextureFormat::Rgba32Float {
                            let at = y * 256 + x * 16;
                            let pixel: [f32; 4] = std::array::from_fn(|i| {
                                f32::from_le_bytes(
                                    mapped[at + i * 4..at + i * 4 + 4].try_into().unwrap(),
                                )
                            });
                            pixel == color
                        } else {
                            // Native UNORM conversion may choose either equally
                            // close representation at a half-byte tie. Require the
                            // nearest encoded value AND byte-exact native wgpu
                            // reference above; do not assume CPU ties-even behavior.
                            let at = y * 256 + x * 4;
                            (0..4).all(|i| {
                                let v = color[i];
                                let encoded =
                                    if i < 3 && format == wgpu::TextureFormat::Rgba8UnormSrgb {
                                        if v <= 0.0031308 {
                                            12.92 * v
                                        } else {
                                            1.055 * v.powf(1. / 2.4) - 0.055
                                        }
                                    } else {
                                        v
                                    };
                                (mapped[at + i] as f32 - encoded * 255.).abs() <= 0.5001
                            })
                        };
                        if !exact_pixel {
                            return Err(format!(
                            "Original shader shared target readback differs in {format:?} at {x},{y} for {color:?}; bytes {:?}",&mapped[y*256+x*4..y*256+x*4+4]
                        )
                        .into());
                        }
                        exact += 1;
                    }
                }
                drop(mapped);
                buffer.unmap();
                observations.push(serde_json::json!({"format":format!("{format:?}"),"width":width,"height":height,"color":color,"byte_exact_wgpu_reference_pixels":exact,"native_format_quantization_qualified":true}));
            }
            drop(target);
            drop(texture);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"adapter":adapter.get_info().name,"backend":"D3D12 + D3D11On12","original_pixel_shader_reference":original.source_reference,"original_pixel_shader_sha256":original.source_sha256,"original_vertex_reference":original_vertex.source_reference,"original_vertex_sha256":original_vertex.source_sha256,"paired_pixel_reference":paired_pixel.source_reference,"paired_pixel_sha256":paired_pixel.source_sha256,"original_input_layout_created":true,"input_decode_scales":original_layout.decode_scales,"missing_vertex_input_refused":true,"foreign_vertex_bindings_refused":true,"native_geom_packing_qualified":false,"source_pin":library.pin,"shared_target_observations":observations,"foreign_contexts_refused":rejected_foreign_contexts,"interrupted_frames_recovered":recovered_callback_errors,"cpu_readback_in_production":false,"game_contacted":false,"complete_cas_render_qualified":false})
        )?
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("This original shader interop boundary requires Windows");
    std::process::exit(2);
}
