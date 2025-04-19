/// Capture a texture to a raw RGBA buffer
pub fn capture_texture(
    rs: &super::RenderState,
    texture: &super::Texture,
    layer: u32,
) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    use eframe::wgpu::*;

    // anyhow::ensure!(
    //     texture.handle.dimension() == TextureDimension::D2,
    //     "Texture capture only supports 2D textures right now"
    // );

    let super::RenderState { device, queue, .. } = rs;

    let texture_wgpu = device.create_texture(&TextureDescriptor {
        label: None,
        size: Extent3d {
            width: texture.desc.width,
            height: texture.desc.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[TextureFormat::Rgba8UnormSrgb],
    });

    let texture_view_wgpu = texture_wgpu.create_view(&TextureViewDescriptor {
        label: None,
        format: Some(TextureFormat::Rgba8UnormSrgb),
        dimension: Some(TextureViewDimension::D2),
        aspect: TextureAspect::All,
        base_mip_level: 0,
        mip_level_count: None,
        base_array_layer: 0,
        array_layer_count: None,
    });

    // Create a buffer to hold the result of copying the texture to CPU memory
    let padded_width = (256.0 * (texture.desc.width as f32 / 256.0).ceil()) as u32;
    let padded_height = (256.0 * (texture.desc.height as f32 / 256.0).ceil()) as u32;
    let buffer_size = (padded_width * padded_height * 4) as usize;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Output Buffer"),
        size: buffer_size as BufferAddress,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("Bind Group Layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    // Create a render pipeline to copy the texture to an RGBA8 texture
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let view = if let Some(ref full_cubemap) = texture.full_cubemap_texture {
        &full_cubemap.create_view(&TextureViewDescriptor {
            base_array_layer: layer,
            array_layer_count: Some(1),
            dimension: Some(TextureViewDimension::D2),
            ..Default::default()
        })
    } else {
        &texture.view
    };

    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("Bind Group"),
        layout: &bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(&device.create_sampler(&SamplerDescriptor {
                    label: Some("Sampler"),
                    address_mode_u: AddressMode::ClampToEdge,
                    address_mode_v: AddressMode::ClampToEdge,
                    address_mode_w: AddressMode::ClampToEdge,
                    mag_filter: FilterMode::Nearest,
                    min_filter: FilterMode::Nearest,
                    mipmap_filter: FilterMode::Nearest,
                    ..Default::default()
                })),
            },
        ],
    });

    let copy_shader = device.create_shader_module(include_wgsl!("../gui/shaders/copy.wgsl"));

    let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&pipeline_layout),
        multiview: None,
        vertex: VertexState {
            module: &copy_shader,
            entry_point: "vs_main",
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(FragmentState {
            module: &copy_shader,
            entry_point: "fs_main",
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8UnormSrgb,
                blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: ColorWrites::all(),
            })],
            compilation_options: Default::default(),
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Cw,
            cull_mode: Some(Face::Back),
            polygon_mode: PolygonMode::Fill,
            conservative: false,
            unclipped_depth: false,
        },
        depth_stencil: None,
        multisample: MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
    });

    // Copy the original texture to the RGBA8 texture using the render pipeline
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor { label: None });
    {
        let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &texture_view_wgpu,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        render_pass.set_pipeline(&render_pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        // Draw a full-screen quad to trigger the fragment shader
        render_pass.draw(0..3, 0..1);
    }

    // Submit commands
    queue.submit(Some(encoder.finish()));

    // Copy the texture data to the CPU-accessible buffer
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor { label: None });
    {
        encoder.copy_texture_to_buffer(
            ImageCopyTexture {
                aspect: TextureAspect::All,
                texture: &texture_wgpu,
                mip_level: 0,
                origin: Origin3d::ZERO,
            },
            ImageCopyBuffer {
                buffer: &buffer,
                layout: ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * padded_width),
                    rows_per_image: Some(padded_height),
                },
            },
            Extent3d {
                width: texture.desc.width,
                height: texture.desc.height,
                depth_or_array_layers: 1,
            },
        );
    }

    // Submit commands
    queue.submit(Some(encoder.finish()));

    // Wait for the copy operation to complete
    device.poll(Maintain::Wait);

    let buffer_slice = buffer.slice(..);
    buffer_slice.map_async(MapMode::Read, |_| {});
    device.poll(Maintain::Wait);
    let buffer_view = buffer_slice.get_mapped_range();
    let buffer_data = buffer_view.to_vec();
    // let final_size = (texture.width * texture.height * 4) as usize;
    // buffer_data.truncate(final_size);

    Ok((buffer_data, padded_width, padded_height))
}
