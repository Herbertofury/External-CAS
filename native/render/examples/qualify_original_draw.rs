//! Opt-in original paired-stage GPU qualification using explicit controlled
//! inputs. It exercises the production draw API; no game or Sim is modified.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use external_cas_render::*;
    use external_cas_shader::*;
    use std::{
        path::PathBuf,
        sync::{atomic::AtomicBool, mpsc},
        time::Duration,
    };
    use windows::Win32::Graphics::Direct3D11::*;
    let source = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Explicit PRECOMP path required")?,
    );
    let cancel = AtomicBool::new(false);
    let library = ShaderLibrary::open(&source, Limits::default(), &cancel)?;
    let vs = library.shader(Stage::Vertex, 924, &cancel)?;
    let ps = library.shader(Stage::Pixel, 3051, &cancel)?;
    if vs.sha256 != "0c09f08016b8b2fd008624108d6c7b986e9ed798542b6ab89d08247d220a7b1e"
        || ps.sha256 != "7fe86f137295a1cef91003a59c0440f28d01524cabe12c5d80717f2a3a164f47"
    {
        return Err(
            "This controlled case requires the inspected source pair; no permutation guessed"
                .into(),
        );
    }
    let vb = library.vertex_bindings(924, &cancel)?;
    let pb = library.pixel_bindings(3051, &cancel)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let mut bridge = OriginalShaderBridge::new(&device, &queue)?;
    let program = bridge.create_program(&vs, &vb, &ps, &pb)?;
    let vertices = [(-1., -1.), (-1., 3.), (3., -1.)]
        .map(|(x, y)| external_cas_assets::geom::Vertex {
            position: [x, y, 0.],
            normal: [0., 0., 1.],
            uv: [0.5; 2],
            joints: [0; 4],
            weights: [1., 0., 0., 0.],
            raw_weights: [255, 0, 0, 0],
            uvs: vec![[0.5; 2], [0.5; 2]],
            has_skin: true,
            vertex_id: None,
            tag: Some(0xff000000),
            tangent: Some([1., 0., 0.]),
        })
        .to_vec();
    let geometry = external_cas_assets::Geometry {
        version: 12,
        vertices,
        indices: vec![0, 1, 2],
        bone_palette: vec![0x1234],
        palette_error: None,
        shader: 0x548394b9,
        merge_group: 0,
        sort_order: 0,
        references: vec![],
        material: vec![],
        unparsed_vertex_usages: vec![],
        uv_stitches: None,
        seam_stitches: None,
        original: vec![],
    };
    let rig = external_cas_animation::Rig {
        version: 3,
        minor_version: 1,
        bones: vec![external_cas_animation::Bone {
            name: "own-test-root".into(),
            name_bytes: b"own-test-root".to_vec(),
            opposing_index: -1,
            parent_index: -1,
            hash: 0x1234,
            flags: 0,
            position: [0.; 3],
            orientation: [0., 0., 0., 1.],
            scale: [1.; 3],
        }],
        chains: vec![],
        trailing: vec![],
        original: vec![],
        sha256: [0; 32],
        limitations: vec![],
    };
    let mut packet = encode_cas_normal_skinning_mesh(
        &geometry,
        &rig,
        5,
        CasVertexPacking {
            position: PositionPacking::Initial,
            normal: NormalPacking::Initial,
            tangent: TangentPacking::Initial,
        },
    )?;
    let mesh = bridge.create_encoded_cas_mesh(&program, &packet)?;
    packet.indices[2] = 3;
    if bridge.create_encoded_cas_mesh(&program, &packet).is_ok() {
        return Err("Out-of-range geometry index accepted".into());
    }
    packet.indices[2] = 2;
    let mut vertex_buffers = Vec::new();
    let mut pixel_buffers = Vec::new();
    let write = |bytes: &mut [u8], offset: usize, values: &[f32]| {
        for (i, f) in values.iter().enumerate() {
            bytes[offset + i * 4..offset + i * 4 + 4].copy_from_slice(&f.to_le_bytes());
        }
    };
    let identity = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    for cb in &vb.constant_buffers {
        let mut bytes: Vec<u8> = cb
            .default_words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        match cb.slot {
            0 => {
                write(&mut bytes, 0, &identity);
                write(&mut bytes, 64, &identity);
                write(&mut bytes, 128, &identity[..12]);
                write(&mut bytes, 3008, &[0., 0., 1., 1.]);
            }
            2 => write(&mut bytes, 0, &[0., 0., 5., 1.]),
            _ => {}
        }
        vertex_buffers.push(bridge.create_constants(cb.slot, &bytes)?);
    }
    vertex_buffers.push(
        bridge.create_constants(
            1,
            &mesh
                .decode_scales()
                .into_iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        )?,
    );
    for cb in &pb.constant_buffers {
        let mut bytes: Vec<u8> = cb
            .default_words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        if cb.slot == 0 {
            write(
                &mut bytes,
                0,
                &[
                    0., 1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0.,
                ],
            );
            write(&mut bytes, 64, &[0.; 16]);
            write(&mut bytes, 3008, &[0.; 4]);
            write(&mut bytes, 3024, &[0.; 4]);
            write(&mut bytes, 3040, &[1.; 4]);
            write(&mut bytes, 3056, &[1.; 4]);
        }
        pixel_buffers.push(bridge.create_constants(cb.slot, &bytes)?);
    }
    let planes = [
        [128, 128, 128, 255],
        [64, 128, 192, 255],
        [10, 20, 30, 255],
        [0, 0, 0, 0],
        [0, 0, 132, 132],
        [0, 0, 0, 0],
    ]
    .map(|p| bridge.create_plane([1, 1], PlaneFormat::Rgba8, &p))
    .into_iter()
    .collect::<std::result::Result<Vec<_>, _>>()?;
    let sampler_references = pb.sampler_references().collect::<Vec<_>>();
    let source_samplers = sampler_references
        .iter()
        .map(|reference| {
            bridge.create_source_sampler(
                &library.sampler_record(reference.state_ordinal as usize)?,
                NativeSamplerContext {
                    knob_a: false,
                    knob_b: false,
                    anisotropy_floor: 1,
                },
            )
        })
        .collect::<std::result::Result<Vec<_>, String>>()?;
    let mut blend = D3D11_BLEND_DESC::default();
    blend.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
        SrcBlend: D3D11_BLEND_ONE,
        DestBlend: D3D11_BLEND_ZERO,
        BlendOp: D3D11_BLEND_OP_ADD,
        SrcBlendAlpha: D3D11_BLEND_ONE,
        DestBlendAlpha: D3D11_BLEND_ZERO,
        BlendOpAlpha: D3D11_BLEND_OP_ADD,
        RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
        ..Default::default()
    };
    let state = bridge.create_pass_state(
        &D3D11_RASTERIZER_DESC {
            FillMode: D3D11_FILL_SOLID,
            CullMode: D3D11_CULL_NONE,
            DepthClipEnable: true.into(),
            ..Default::default()
        },
        &blend,
        &D3D11_DEPTH_STENCIL_DESC {
            DepthEnable: true.into(),
            DepthFunc: D3D11_COMPARISON_LESS,
            DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ALL,
            FrontFace: D3D11_DEPTH_STENCILOP_DESC {
                StencilFailOp: D3D11_STENCIL_OP_KEEP,
                StencilDepthFailOp: D3D11_STENCIL_OP_KEEP,
                StencilPassOp: D3D11_STENCIL_OP_KEEP,
                StencilFunc: D3D11_COMPARISON_ALWAYS,
            },
            BackFace: D3D11_DEPTH_STENCILOP_DESC {
                StencilFailOp: D3D11_STENCIL_OP_KEEP,
                StencilDepthFailOp: D3D11_STENCIL_OP_KEEP,
                StencilPassOp: D3D11_STENCIL_OP_KEEP,
                StencilFunc: D3D11_COMPARISON_ALWAYS,
            },
            ..Default::default()
        },
    )?;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Original source paired-stage qualification"),
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target = bridge.wrap_target(&texture)?;
    let depth = bridge.create_depth(
        [16, 16],
        windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_D32_FLOAT,
    )?;
    let vertex_refs = vertex_buffers.iter().collect::<Vec<_>>();
    let pixel_refs = pixel_buffers.iter().collect::<Vec<_>>();
    let textures = planes
        .iter()
        .enumerate()
        .map(|(i, p)| (i as u32, p))
        .collect::<Vec<_>>();
    let samplers = sampler_references
        .iter()
        .zip(&source_samplers)
        .map(|(reference, sampler)| (reference.slot as u32, sampler))
        .collect::<Vec<_>>();
    for missing_texture in [false, true] {
        let incomplete = OriginalDraw {
            program: &program,
            mesh: &mesh,
            vertex_constants: &vertex_refs,
            pixel_constants: if missing_texture { &pixel_refs } else { &[] },
            textures: if missing_texture {
                &textures[..5]
            } else {
                &textures
            },
            samplers: &samplers,
        };
        if unsafe {
            bridge.draw_scene(
                &target,
                &state,
                Some(&depth),
                &[incomplete],
                OriginalClear::default(),
                [0.; 4],
                0,
            )
        }
        .is_ok()
        {
            return Err("Incomplete source material bindings reached a draw".into());
        }
    }
    let draw = OriginalDraw {
        program: &program,
        mesh: &mesh,
        vertex_constants: &vertex_refs,
        pixel_constants: &pixel_refs,
        textures: &textures,
        samplers: &samplers,
    };
    unsafe {
        bridge.draw_scene(
            &target,
            &state,
            Some(&depth),
            &[draw],
            OriginalClear {
                color: Some([0.3, 0.3, 0.3, 1.]),
                depth: Some(1.),
                ..Default::default()
            },
            [0.; 4],
            0,
        )
    }?;
    // A second source draw is farther away. Explicit load semantics must keep
    // the first pass's depth; a hidden per-pass clear would expose this color.
    let mut farther: Vec<u8> = vb
        .constant_buffers
        .iter()
        .find(|cb| cb.slot == 0)
        .unwrap()
        .default_words
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    write(&mut farther, 0, &identity);
    write(&mut farther, 64, &identity);
    write(&mut farther, 108, &[0.5]);
    write(&mut farther, 128, &identity[..12]);
    write(&mut farther, 3008, &[0., 0., 1., 1.]);
    bridge.update_constants(
        vertex_buffers.iter().find(|cb| cb.slot == 0).unwrap(),
        &farther,
    )?;
    let other_cloth = bridge.create_plane([1, 1], PlaneFormat::Rgba8, &[200, 180, 160, 255])?;
    let farther_textures = textures
        .iter()
        .map(|&(slot, plane)| {
            if slot == 2 {
                (slot, &other_cloth)
            } else {
                (slot, plane)
            }
        })
        .collect::<Vec<_>>();
    let farther_draw = OriginalDraw {
        program: &program,
        mesh: &mesh,
        vertex_constants: &vertex_refs,
        pixel_constants: &pixel_refs,
        textures: &farther_textures,
        samplers: &samplers,
    };
    unsafe {
        bridge.draw_scene(
            &target,
            &state,
            Some(&depth),
            &[farther_draw],
            OriginalClear::default(),
            [0.; 4],
            0,
        )
    }?;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Qualification only readback"),
        size: 4096,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(16),
            },
        },
        texture.size(),
    );
    let submission = queue.submit([encoder.finish()]);
    let (tx, rx) = mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::Wait {
        submission_index: Some(submission),
        timeout: Some(Duration::from_secs(10)),
    })?;
    rx.recv_timeout(Duration::from_secs(10))??;
    let mapped = readback.slice(..).get_mapped_range()?;
    let expected = [10. / 255., 20. / 255., 30. / 255., 0.];
    let mut maximum = 0.0f32;
    for (i, raw) in mapped.chunks_exact(4).enumerate() {
        let value = f32::from_le_bytes(raw.try_into()?);
        let error = (value - expected[i % 4]).abs();
        if !value.is_finite() || error > 1e-6 {
            return Err(format!(
                "Original paired stage pixel differs at channel{i}: {value}, expected{}",
                expected[i % 4]
            )
            .into());
        }
        maximum = maximum.max(error);
    }
    drop(mapped);
    readback.unmap();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({"adapter":adapter.get_info().name,
        "original_vertex_reference":924,"original_vertex_sha256":vs.sha256,"original_pixel_reference":3051,"original_pixel_sha256":ps.sha256,
        "production_draw_api_executed":true,"controlled_pixels":256,"maximum_absolute_error":maximum,
        "invalid_indices_refused":true,"incomplete_constant_and_texture_bindings_refused":true,
        "ordered_pass_depth_preserved":true,"native_attribute_packet_executed":true,"source_decode_scales":mesh.decode_scales(),
        "source_sampler_references":sampler_references,"controlled_sampler_context":{"knob_a":false,"knob_b":false,"anisotropy_floor":1},
        "actual_renderer_setting_selection_qualified":false,
        "source_alpha_retained":0,"native_geom_packing_qualified":false,"native_material_construction_qualified":false,
        "game_contacted":false,"complete_cas_render_qualified":false}))?
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Original stages require Windows");
    std::process::exit(2);
}
