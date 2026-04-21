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
    debug_flags: u32,
    _pad: [u32; 1],
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
    max_texture_dimension_2d: u32,
    cached_mesh_key: Option<(u32, u32, usize)>,
    line_pipeline: Option<wgpu::RenderPipeline>,
    show_wireframe: bool,
    show_all_white: bool,
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
        let adapter = Self::request_adapter(&instance, None)?;
        Self::from_adapter(adapter)
    }

    pub fn request_adapter(
        instance: &wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<wgpu::Adapter> {
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface,
        }))
        .ok_or_else(|| anyhow!("no wgpu adapter available"))
    }

    pub fn from_adapter(adapter: wgpu::Adapter) -> Result<Self> {
        tracing::info!("wgpu adapter: {:?}", adapter.get_info().name);
        let adapter_limits = adapter.limits();

        let mut required_features = wgpu::Features::empty();
        if adapter
            .features()
            .contains(wgpu::Features::POLYGON_MODE_LINE)
        {
            required_features |= wgpu::Features::POLYGON_MODE_LINE;
        }

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("emoji_preview"),
                required_features,
                required_limits: adapter_limits,
                ..Default::default()
            },
            None,
        ))?;

        let max_texture_dimension_2d = device.limits().max_texture_dimension_2d;

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
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 32,
                            shader_location: 3,
                        },
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

        let line_pipeline = if required_features.contains(wgpu::Features::POLYGON_MODE_LINE) {
            Some(
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("billboard_line_pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &[
                                wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32x3,
                                    offset: 0,
                                    shader_location: 0,
                                },
                                wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32x3,
                                    offset: 12,
                                    shader_location: 1,
                                },
                                wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32x2,
                                    offset: 24,
                                    shader_location: 2,
                                },
                                wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Uint32,
                                    offset: 32,
                                    shader_location: 3,
                                },
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
                        polygon_mode: wgpu::PolygonMode::Line,
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: wgpu::TextureFormat::Depth32Float,
                        depth_write_enabled: false,
                        depth_compare: wgpu::CompareFunction::LessEqual,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState {
                            constant: -1,
                            slope_scale: -1.0,
                            clamp: 0.0,
                        },
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                }),
            )
        } else {
            None
        };

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

        let (vertices, indices) = billboard_geometry_rect(1.0, 0.1, true);
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
            max_texture_dimension_2d,
            cached_mesh_key: None,
            line_pipeline,
            show_wireframe: false,
            show_all_white: false,
        })
    }

    pub fn render_billboard(
        &mut self,
        texture: &Texture,
        width: usize,
        height: usize,
        tick: u64,
    ) -> Vec<Line<'static>> {
        let px_w = width;
        let px_h = height * 2;
        let fb = self.render_billboard_rgb(texture, px_w, px_h, tick);
        let px_w = width;
        fb_to_lines(&fb, px_w, px_h, height)
    }

    pub fn render_billboard_rgb(
        &mut self,
        texture: &Texture,
        px_width: usize,
        px_height: usize,
        tick: u64,
    ) -> Vec<(u8, u8, u8)> {
        let px_w = px_width as u32;
        let px_h = px_height as u32;

        if px_w == 0 || px_h == 0 || texture.width == 0 || texture.height == 0 {
            return vec![];
        }

        if self.render_to_offscreen(texture, px_w, px_h, tick).is_err() {
            return vec![];
        }

        let rt = self.render_target.as_ref().unwrap();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

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
            wgpu::Extent3d {
                width: px_w,
                height: px_h,
                depth_or_array_layers: 1,
            },
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
        fb
    }

    pub fn render_to_offscreen(
        &mut self,
        texture: &Texture,
        px_w: u32,
        px_h: u32,
        tick: u64,
    ) -> Result<()> {
        let mesh_key = (
            texture.width,
            texture.height,
            texture.pixels.as_ptr() as usize,
        );
        if self.cached_mesh_key != Some(mesh_key) {
            self.update_geometry(texture);
            self.cached_mesh_key = Some(mesh_key);
        }

        let tex_aspect = texture.width as f32 / texture.height as f32;
        self.ensure_texture(texture);
        self.ensure_render_target(px_w, px_h);

        if self.tex_state.is_none() {
            return Err(anyhow!("emoji preview texture state unavailable"));
        }

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
            debug_flags: self.show_all_white as u32,
            _pad: [0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

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
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
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

        if self.show_wireframe {
            if let Some(line_pipeline) = &self.line_pipeline {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("wireframe_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &rt.color_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &rt.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
                pass.set_pipeline(line_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_bind_group(1, &self.tex_state.as_ref().unwrap().bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..self.num_indices, 0, 0..1);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    pub fn max_texture_dimension_2d(&self) -> u32 {
        self.max_texture_dimension_2d
    }

    pub fn offscreen_view(&self) -> Option<&wgpu::TextureView> {
        self.render_target.as_ref().map(|rt| &rt.color_view)
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn set_wireframe(&mut self, enabled: bool) {
        self.show_wireframe = enabled;
    }

    pub fn wireframe(&self) -> bool {
        self.show_wireframe
    }

    pub fn wireframe_supported(&self) -> bool {
        self.line_pipeline.is_some()
    }

    pub fn set_all_white(&mut self, enabled: bool) {
        self.show_all_white = enabled;
    }

    pub fn all_white(&self) -> bool {
        self.show_all_white
    }

    fn update_geometry(&mut self, texture: &Texture) {
        let aspect = texture.width as f32 / texture.height as f32;
        self.cached_aspect = aspect;
        let (vertices, indices) = extruded_billboard_geometry(texture, 0.1, true);
        self.vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        self.index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        self.num_indices = indices.len() as u32;
    }

    fn ensure_texture(&mut self, texture: &Texture) {
        let threshold = 160u8;
        let mut rgba_data: Vec<u8> = texture
            .pixels
            .iter()
            .flat_map(|p| p.iter().copied())
            .collect();
        for chunk in rgba_data.chunks_exact_mut(4) {
            if chunk[3] >= threshold {
                chunk[3] = 255;
            }
        }

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
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
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
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rt_depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let row_bytes = width * 4;
        let padded_row_bytes = (row_bytes + wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1)
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

fn billboard_geometry_rect(
    aspect: f32,
    depth_ratio: f32,
    mirror_back_face: bool,
) -> (Vec<Vertex>, Vec<u16>) {
    let hw = 1.0f32;
    let hh = 1.0 / aspect;
    let hd = hw * depth_ratio;

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    let mut quad =
        |positions: [[f32; 3]; 4], normal: [f32; 3], uvs: [[f32; 2]; 4], face_type: u32| {
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

    // Back face (z = -hd), mirrored so the rear reads like the true back.
    quad(
        [
            [hw, -hh, -hd],
            [-hw, -hh, -hd],
            [-hw, hh, -hd],
            [hw, hh, -hd],
        ],
        [0.0, 0.0, -1.0],
        if mirror_back_face {
            [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]]
        } else {
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]
        },
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
        [
            [-hw, -hh, -hd],
            [-hw, -hh, hd],
            [-hw, hh, hd],
            [-hw, hh, -hd],
        ],
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
        [
            [-hw, -hh, -hd],
            [hw, -hh, -hd],
            [hw, -hh, hd],
            [-hw, -hh, hd],
        ],
        [0.0, -1.0, 0.0],
        [[0.0, 1.0], [1.0, 1.0], [1.0, 1.0], [0.0, 1.0]],
        5,
    );

    (vertices, indices)
}

fn extruded_billboard_geometry(
    texture: &Texture,
    depth_ratio: f32,
    _mirror_back_face: bool,
) -> (Vec<Vertex>, Vec<u16>) {
    let aspect = if texture.height > 0 {
        texture.width as f32 / texture.height as f32
    } else {
        1.0
    };

    if texture.width == 0 || texture.height == 0 {
        return billboard_geometry_rect(aspect, depth_ratio, true);
    }

    let hw = 1.0f32;
    let hh = 1.0 / aspect.max(0.0001);
    let hd = hw * depth_ratio;

    let max_cells = 256usize;
    let (grid_w, grid_h) = if texture.width >= texture.height {
        let gh = ((texture.height as f32 / texture.width as f32) * max_cells as f32)
            .round()
            .clamp(1.0, max_cells as f32) as usize;
        (max_cells, gh)
    } else {
        let gw = ((texture.width as f32 / texture.height as f32) * max_cells as f32)
            .round()
            .clamp(1.0, max_cells as f32) as usize;
        (gw, max_cells)
    };

    let cols = grid_w + 1;
    let rows = grid_h + 1;
    let field = alpha_field(texture, cols, rows);

    let threshold = 160.0 / 255.0;
    let field_f64: Vec<f64> = field.iter().map(|&v| v as f64).collect();

    let builder = contour::ContourBuilder::new(cols, rows, true)
        .x_step(1.0 / grid_w as f64)
        .y_step(1.0 / grid_h as f64);

    let contours = match builder.contours(&field_f64, &[threshold]) {
        Ok(c) => c,
        Err(_) => return billboard_geometry_rect(aspect, depth_ratio, true),
    };

    if contours.is_empty() {
        return billboard_geometry_rect(aspect, depth_ratio, true);
    }

    let multi_polygon = contours.into_iter().next().unwrap().into_inner().0;
    if multi_polygon.0.is_empty() {
        return billboard_geometry_rect(aspect, depth_ratio, true);
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let texel_u = 0.5 / texture.width.max(1) as f32;
    let texel_v = 0.5 / texture.height.max(1) as f32;

    for polygon in &multi_polygon.0 {
        let exterior = polygon.exterior();
        let ext_coords = exterior.coords().collect::<Vec<_>>();
        if ext_coords.len() < 4 {
            continue;
        }

        // Build flat coordinate array for earcutr: exterior + holes
        let mut flat_coords: Vec<f64> = Vec::new();
        let mut hole_indices: Vec<usize> = Vec::new();

        // Exterior ring (skip the closing duplicate point)
        let ext_len = ext_coords.len() - 1;
        for &coord in &ext_coords[..ext_len] {
            flat_coords.push(coord.x);
            flat_coords.push(coord.y);
        }

        // Interior rings (holes)
        for interior in polygon.interiors() {
            let hole_coords: Vec<_> = interior.coords().collect();
            if hole_coords.len() < 4 {
                continue;
            }
            hole_indices.push(flat_coords.len() / 2);
            let hole_len = hole_coords.len() - 1;
            for &coord in &hole_coords[..hole_len] {
                flat_coords.push(coord.x);
                flat_coords.push(coord.y);
            }
        }

        let n_verts = flat_coords.len() / 2;
        if n_verts < 3 {
            continue;
        }

        let tri_indices = match earcutr::earcut(&flat_coords, &hole_indices, 2) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Build UV and position arrays from flat coords
        // contour crate outputs coordinates in [0, 1] UV space (due to x_step/y_step)
        let uv_points: Vec<[f32; 2]> = (0..n_verts)
            .map(|i| {
                [
                    flat_coords[i * 2] as f32,
                    flat_coords[i * 2 + 1] as f32,
                ]
            })
            .collect();

        let pos_points: Vec<[f32; 2]> = uv_points
            .iter()
            .map(|&[u, v]| [-hw + u * 2.0 * hw, hh - v * 2.0 * hh])
            .collect();

        // Front cap (z = +hd, face_type = 0)
        emit_cap(
            &mut vertices,
            &mut indices,
            &uv_points,
            &pos_points,
            &tri_indices,
            texel_u,
            texel_v,
            hd,
            false,
            false,
        );

        // Back cap (z = -hd, face_type = 1, flipped winding)
        // No UV mirror — the Y-axis rotation already mirrors the view naturally
        emit_cap(
            &mut vertices,
            &mut indices,
            &uv_points,
            &pos_points,
            &tri_indices,
            texel_u,
            texel_v,
            -hd,
            true,
            false,
        );

        // Side walls along exterior ring
        emit_side_walls(
            &mut vertices,
            &mut indices,
            &uv_points[..ext_len],
            &pos_points[..ext_len],
            texel_u,
            texel_v,
            hd,
        );

        // Side walls along each hole (wound opposite direction for inward-facing normals)
        let mut hole_start = ext_len;
        for interior in polygon.interiors() {
            let hole_coords: Vec<_> = interior.coords().collect();
            if hole_coords.len() < 4 {
                continue;
            }
            let hole_len = hole_coords.len() - 1;
            let hole_end = hole_start + hole_len;
            emit_side_walls(
                &mut vertices,
                &mut indices,
                &uv_points[hole_start..hole_end],
                &pos_points[hole_start..hole_end],
                texel_u,
                texel_v,
                hd,
            );
            hole_start = hole_end;
        }
    }

    if indices.is_empty() {
        billboard_geometry_rect(aspect, depth_ratio, true)
    } else {
        (vertices, indices)
    }
}

fn push_quad(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    positions: [[f32; 3]; 4],
    normal: [f32; 3],
    uvs: [[f32; 2]; 4],
    face_type: u32,
) {
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
}

fn emit_cap(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    uv_points: &[[f32; 2]],
    pos_points: &[[f32; 2]],
    tri_indices: &[usize],
    texel_u: f32,
    texel_v: f32,
    z: f32,
    flip_winding: bool,
    mirror_u: bool,
) {
    let base = vertices.len() as u16;
    let face_type = if z >= 0.0 { 0u32 } else { 1 };
    let nz = if z >= 0.0 { 1.0f32 } else { -1.0 };

    for (&[u, v], &[px, py]) in uv_points.iter().zip(pos_points.iter()) {
        let u_final = if mirror_u { 1.0 - u } else { u }.clamp(texel_u, 1.0 - texel_u);
        let v_final = v.clamp(texel_v, 1.0 - texel_v);
        vertices.push(Vertex {
            position: [px, py, z],
            normal: [0.0, 0.0, nz],
            uv: [u_final, v_final],
            face_type,
            _pad: 0,
        });
    }

    for tri in tri_indices.chunks_exact(3) {
        let (a, b, c) = (tri[0] as u16, tri[1] as u16, tri[2] as u16);
        if flip_winding {
            indices.extend_from_slice(&[base + a, base + c, base + b]);
        } else {
            indices.extend_from_slice(&[base + a, base + b, base + c]);
        }
    }
}

fn emit_side_walls(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    uv_ring: &[[f32; 2]],
    pos_ring: &[[f32; 2]],
    texel_u: f32,
    texel_v: f32,
    hd: f32,
) {
    let n = uv_ring.len();
    if n < 2 {
        return;
    }
    for i in 0..n {
        let j = (i + 1) % n;
        let [au, av] = uv_ring[i];
        let [bu, bv] = uv_ring[j];
        let [ax, ay] = pos_ring[i];
        let [bx, by] = pos_ring[j];

        let normal = normalize([(by - ay) as f64, -(bx - ax) as f64, 0.0]);

        let au_c = au.clamp(texel_u, 1.0 - texel_u);
        let av_c = av.clamp(texel_v, 1.0 - texel_v);
        let bu_c = bu.clamp(texel_u, 1.0 - texel_u);
        let bv_c = bv.clamp(texel_v, 1.0 - texel_v);

        push_quad(
            vertices,
            indices,
            [[ax, ay, -hd], [ax, ay, hd], [bx, by, hd], [bx, by, -hd]],
            [normal[0] as f32, normal[1] as f32, 0.0],
            [[au_c, av_c], [au_c, av_c], [bu_c, bv_c], [bu_c, bv_c]],
            2,
        );
    }
}

fn alpha_field(texture: &Texture, cols: usize, rows: usize) -> Vec<f32> {
    let mut field = vec![0.0f32; cols * rows];
    for gy in 0..rows {
        let v = gy as f64 / (rows - 1).max(1) as f64;
        for gx in 0..cols {
            let u = gx as f64 / (cols - 1).max(1) as f64;
            field[gy * cols + gx] = texture.sample(u, v)[3] as f32 / 255.0;
        }
    }
    field
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padded_texture() -> Texture<'static> {
        let mut pixels = vec![[0, 0, 0, 0]; 16 * 16];
        for y in 4..12 {
            for x in 4..12 {
                pixels[y * 16 + x] = [240, 100, 40, 255];
            }
        }
        let leaked = Box::leak(pixels.into_boxed_slice());
        Texture {
            pixels: leaked,
            width: 16,
            height: 16,
        }
    }

    #[test]
    fn geometry_trims_to_opaque_content() {
        let texture = padded_texture();
        let (vertices, _) = extruded_billboard_geometry(&texture, 0.1, true);
        let min_x = vertices.iter().map(|v| v.position[0]).fold(f32::INFINITY, f32::min);
        let max_x = vertices.iter().map(|v| v.position[0]).fold(f32::NEG_INFINITY, f32::max);
        let min_y = vertices.iter().map(|v| v.position[1]).fold(f32::INFINITY, f32::min);
        let max_y = vertices.iter().map(|v| v.position[1]).fold(f32::NEG_INFINITY, f32::max);

        assert!(
            min_x > -0.7 && max_x < 0.7,
            "side walls should be trimmed to opaque region, got {min_x}..{max_x}"
        );
        assert!(
            min_y > -0.7 && max_y < 0.7,
            "top/bottom should be trimmed to opaque region, got {min_y}..{max_y}"
        );
    }

    #[test]
    fn extruded_geometry_has_front_back_caps_and_sides() {
        let texture = padded_texture();
        let (vertices, indices) = extruded_billboard_geometry(&texture, 0.1, true);
        assert!(!indices.is_empty(), "should produce geometry");

        let count = |ft: u32| {
            indices
                .chunks(3)
                .filter(|tri| tri.iter().all(|&i| vertices[i as usize].face_type == ft))
                .count()
        };
        let front = count(0);
        let back = count(1);
        let sides = count(2);

        eprintln!("front: {front}, back: {back}, sides: {sides}, total: {}", indices.len() / 3);
        assert!(front > 0, "must have front cap triangles");
        assert!(back > 0, "must have back cap triangles");
        assert!(sides > 0, "must have side wall triangles");
        assert_eq!(front, back, "front and back should have equal triangle count");
    }

    #[test]
    fn front_cap_uvs_in_opaque_region() {
        let texture = padded_texture();
        let (vertices, _) = extruded_billboard_geometry(&texture, 0.1, true);
        let front_verts: Vec<_> = vertices.iter().filter(|v| v.face_type == 0).collect();
        assert!(!front_verts.is_empty());

        let min_u = front_verts.iter().map(|v| v.uv[0]).fold(f32::INFINITY, f32::min);
        let max_u = front_verts.iter().map(|v| v.uv[0]).fold(f32::NEG_INFINITY, f32::max);

        assert!(
            min_u > 0.1 && max_u < 0.9,
            "front cap UVs should be trimmed to opaque region; got u=[{min_u}, {max_u}]"
        );
    }

    #[test]
    fn circle_has_diagonal_side_normals() {
        let mut pixels = vec![[0, 0, 0, 0]; 32 * 32];
        for y in 0..32 {
            for x in 0..32 {
                let dx = x as f32 - 15.5;
                let dy = y as f32 - 15.5;
                if dx * dx + dy * dy < 12.0 * 12.0 {
                    pixels[y * 32 + x] = [255, 0, 0, 255];
                }
            }
        }
        let leaked = Box::leak(pixels.into_boxed_slice());
        let texture = Texture { pixels: leaked, width: 32, height: 32 };
        let (vertices, indices) = extruded_billboard_geometry(&texture, 0.1, true);
        assert!(!indices.is_empty());

        let has_diagonal = vertices
            .iter()
            .filter(|v| v.face_type == 2)
            .any(|v| v.normal[0].abs() > 0.01 && v.normal[1].abs() > 0.01);
        assert!(has_diagonal, "circle should have diagonal side-wall normals");
    }

    #[test]
    fn fully_opaque_falls_back_to_rect() {
        let pixels = vec![[255, 0, 0, 255]; 16 * 16];
        let leaked = Box::leak(pixels.into_boxed_slice());
        let texture = Texture { pixels: leaked, width: 16, height: 16 };
        let (_vertices, indices) = extruded_billboard_geometry(&texture, 0.1, true);
        assert!(!indices.is_empty(), "fully opaque should produce rect fallback geometry");
    }

    #[test]
    fn fully_transparent_falls_back_to_rect() {
        let pixels = vec![[0, 0, 0, 0]; 16 * 16];
        let leaked = Box::leak(pixels.into_boxed_slice());
        let texture = Texture { pixels: leaked, width: 16, height: 16 };
        let (_vertices, indices) = extruded_billboard_geometry(&texture, 0.1, true);
        assert!(!indices.is_empty(), "fully transparent should produce rect fallback geometry");
    }
}
