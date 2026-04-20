use anyhow::{Result, anyhow};
use ratatui::text::Line;
use wgpu::util::DeviceExt;

use super::common::*;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    face_type: u32,
    _pad: u32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    cos_rot: f32,
    sin_rot: f32,
    bob: f32,
    light_angle: f32,
    scale_x: f32,
    scale_y: f32,
    _pad: [f32; 2],
}

pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    tex_bind_group_layout: wgpu::BindGroupLayout,
    edge_color_buffer: wgpu::Buffer,
    tex_state: Option<TexState>,
    render_target: Option<RenderTargetState>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    cached_aspect: f32,
}

struct TexState {
    gpu_texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    tex_w: u32,
    tex_h: u32,
}

struct RenderTargetState {
    color_texture: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    staging_buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    padded_row_bytes: u32,
}

impl GpuRenderer {
    pub fn try_new() -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok_or_else(|| anyhow!("no wgpu adapter available"))?;

        tracing::info!("wgpu adapter: {:?}", adapter.get_info().name);

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("emoji_preview"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                ..Default::default()
            },
            None,
        ))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("billboard_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("uniform_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let tex_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("tex_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("billboard_pipeline_layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &tex_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("billboard_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 2 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 32, shader_location: 3 },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniform_bg"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let edge_color_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("edge_color"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let (vertices, indices) = billboard_geometry(1.0, 0.1);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let num_indices = indices.len() as u32;
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            tex_bind_group_layout,
            edge_color_buffer,
            tex_state: None,
            render_target: None,
            vertex_buffer,
            index_buffer,
            num_indices,
            cached_aspect: 1.0,
        })
    }

    pub fn render_billboard(
        &mut self,
        texture: &Texture,
        width: usize,
        height: usize,
        tick: u64,
    ) -> Vec<Line<'static>> {
        let px_w = width as u32;
        let px_h = (height * 2) as u32;

        if px_w == 0 || px_h == 0 || texture.width == 0 || texture.height == 0 {
            return vec![];
        }

        let tex_aspect = texture.width as f32 / texture.height as f32;
        if (tex_aspect - self.cached_aspect).abs() > 0.01 {
            self.update_geometry(tex_aspect);
        }

        self.ensure_texture(texture);
        self.ensure_render_target(px_w, px_h);

        if self.tex_state.is_none() {
            return vec![];
        }

        // Compute uniforms
        let vp_aspect = px_w as f32 / px_h as f32;
        let fill = 0.65f32;
        let (scale_x, scale_y) = if tex_aspect > vp_aspect {
            (fill, fill * vp_aspect / tex_aspect)
        } else {
            (fill * tex_aspect / vp_aspect, fill)
        };

        let spin = tick as f64 * 0.04;
        let bob_pixels = (tick as f64 * 0.035).sin() * px_h as f64 * 0.03;
        let bob_ndc = bob_pixels as f32 / px_h as f32 * 2.0;

        let uniforms = Uniforms {
            cos_rot: spin.cos() as f32,
            sin_rot: spin.sin() as f32,
            bob: bob_ndc,
            light_angle: (tick as f64 * 0.015) as f32,
            scale_x,
            scale_y,
            _pad: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // Render pass
        let rt = self.render_target.as_ref().unwrap();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &rt.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &rt.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_bind_group(1, &self.tex_state.as_ref().unwrap().bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        // Copy render target to staging buffer
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &rt.color_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &rt.staging_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(rt.padded_row_bytes),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d { width: px_w, height: px_h, depth_or_array_layers: 1 },
        );

        self.queue.submit(Some(encoder.finish()));

        // Readback
        let rt = self.render_target.as_ref().unwrap();
        let buffer_slice = rt.staging_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);

        if rx.recv().ok().and_then(|r| r.ok()).is_none() {
            return vec![];
        }

        let data = buffer_slice.get_mapped_range();
        let row_bytes = px_w as usize * 4;
        let padded = rt.padded_row_bytes as usize;

        // Composite GPU output over background gradient
        let mut fb = vec![(0u8, 0u8, 0u8); (px_w * px_h) as usize];
        let mut hit_mask = vec![false; (px_w * px_h) as usize];

        background_gradient(&mut fb, px_w as usize, px_h as usize);

        for y in 0..px_h as usize {
            let row_start = y * padded;
            let row = &data[row_start..row_start + row_bytes];
            for x in 0..px_w as usize {
                let i = x * 4;
                let (r, g, b, a) = (row[i], row[i + 1], row[i + 2], row[i + 3]);
                if a < 2 {
                    continue;
                }
                let idx = y * px_w as usize + x;
                hit_mask[idx] = true;
                let alpha = a as f64 / 255.0;
                let bg = fb[idx];
                let inv = 1.0 - alpha;
                fb[idx] = (
                    (r as f64 * alpha + bg.0 as f64 * inv) as u8,
                    (g as f64 * alpha + bg.1 as f64 * inv) as u8,
                    (b as f64 * alpha + bg.2 as f64 * inv) as u8,
                );
            }
        }

        drop(data);
        rt.staging_buffer.unmap();

        shadow_pass(&mut fb, &hit_mask, px_w as usize, px_h as usize);
        fb_to_lines(&fb, px_w as usize, px_h as usize, height)
    }

    fn update_geometry(&mut self, aspect: f32) {
        self.cached_aspect = aspect;
        let (vertices, _indices) = billboard_geometry(aspect, 0.1);
        self.queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&vertices),
        );
    }

    fn ensure_texture(&mut self, texture: &Texture) {
        let rgba_data: Vec<u8> = texture
            .pixels
            .iter()
            .flat_map(|p| p.iter().copied())
            .collect();

        let same_size = self
            .tex_state
            .as_ref()
            .is_some_and(|ts| ts.tex_w == texture.width && ts.tex_h == texture.height);

        if same_size {
            let ts = self.tex_state.as_ref().unwrap();
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &ts.gpu_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &rgba_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(texture.width * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: texture.width,
                    height: texture.height,
                    depth_or_array_layers: 1,
                },
            );
            return;
        }

        let w = texture.width.max(1);
        let h = texture.height.max(1);

        let gpu_texture = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some("emoji_tex"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &rgba_data,
        );

        let view = gpu_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let edge = texture.edge_color();
        let edge_data: [f32; 4] = [
            edge[0] as f32 / 255.0,
            edge[1] as f32 / 255.0,
            edge[2] as f32 / 255.0,
            1.0,
        ];
        self.queue
            .write_buffer(&self.edge_color_buffer, 0, bytemuck::bytes_of(&edge_data));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tex_bg"),
            layout: &self.tex_bind_group_layout,
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
                    resource: self.edge_color_buffer.as_entire_binding(),
                },
            ],
        });

        self.tex_state = Some(TexState {
            gpu_texture,
            bind_group,
            tex_w: texture.width,
            tex_h: texture.height,
        });
    }

    fn ensure_render_target(&mut self, width: u32, height: u32) {
        let needs_update = match &self.render_target {
            Some(rt) => rt.width != width || rt.height != height,
            None => true,
        };
        if !needs_update {
            return;
        }

        let color_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rt_color"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rt_depth"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let row_bytes = width * 4;
        let padded_row_bytes =
            (row_bytes + wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1)
                / wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
                * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

        let staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: padded_row_bytes as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        self.render_target = Some(RenderTargetState {
            color_texture,
            color_view,
            depth_view,
            staging_buffer,
            width,
            height,
            padded_row_bytes,
        });
    }
}

fn billboard_geometry(aspect: f32, depth_ratio: f32) -> (Vec<Vertex>, Vec<u16>) {
    let hw = 1.0f32;
    let hh = 1.0 / aspect;
    let hd = hw * depth_ratio;

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    let mut quad = |positions: [[f32; 3]; 4], normal: [f32; 3], uvs: [[f32; 2]; 4], face_type: u32| {
        let base = vertices.len() as u16;
        for i in 0..4 {
            vertices.push(Vertex {
                position: positions[i],
                normal,
                uv: uvs[i],
                face_type,
                _pad: 0,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    // Front face (z = +hd)
    quad(
        [[-hw, -hh, hd], [hw, -hh, hd], [hw, hh, hd], [-hw, hh, hd]],
        [0.0, 0.0, 1.0],
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        0,
    );

    // Back face (z = -hd), horizontally mirrored UVs
    quad(
        [[hw, -hh, -hd], [-hw, -hh, -hd], [-hw, hh, -hd], [hw, hh, -hd]],
        [0.0, 0.0, -1.0],
        [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
        1,
    );

    // Right face (x = +hw)
    quad(
        [[hw, -hh, hd], [hw, -hh, -hd], [hw, hh, -hd], [hw, hh, hd]],
        [1.0, 0.0, 0.0],
        [[1.0, 1.0], [1.0, 1.0], [1.0, 0.0], [1.0, 0.0]],
        2,
    );

    // Left face (x = -hw)
    quad(
        [[-hw, -hh, -hd], [-hw, -hh, hd], [-hw, hh, hd], [-hw, hh, -hd]],
        [-1.0, 0.0, 0.0],
        [[0.0, 1.0], [0.0, 1.0], [0.0, 0.0], [0.0, 0.0]],
        3,
    );

    // Top face (y = +hh)
    quad(
        [[-hw, hh, hd], [hw, hh, hd], [hw, hh, -hd], [-hw, hh, -hd]],
        [0.0, 1.0, 0.0],
        [[0.0, 0.0], [1.0, 0.0], [1.0, 0.0], [0.0, 0.0]],
        4,
    );

    // Bottom face (y = -hh)
    quad(
        [[-hw, -hh, -hd], [hw, -hh, -hd], [hw, -hh, hd], [-hw, -hh, hd]],
        [0.0, -1.0, 0.0],
        [[0.0, 1.0], [1.0, 1.0], [1.0, 1.0], [0.0, 1.0]],
        5,
    );

    (vertices, indices)
}
