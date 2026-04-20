use anyhow::{Result, anyhow};
use ratatui::text::Line;
use std::cmp::Ordering;
use std::collections::HashMap;
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
    mirror_back_face: bool,
) -> (Vec<Vertex>, Vec<u16>) {
    let aspect = if texture.height > 0 {
        texture.width as f32 / texture.height as f32
    } else {
        1.0
    };
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let hw = 1.0f32;
    let hh = 1.0 / aspect.max(0.0001);
    let hd = hw * depth_ratio;

    let max_cells = 256usize;
    let (grid_w, grid_h) = match (
        texture.width.cmp(&texture.height),
        texture.width,
        texture.height,
    ) {
        (_, 0, _) | (_, _, 0) => return (vertices, indices),
        (Ordering::Greater | Ordering::Equal, w, h) => (
            max_cells,
            ((h as f32 / w as f32) * max_cells as f32)
                .round()
                .clamp(1.0, max_cells as f32) as usize,
        ),
        (Ordering::Less, w, h) => (
            ((w as f32 / h as f32) * max_cells as f32)
                .round()
                .clamp(1.0, max_cells as f32) as usize,
            max_cells,
        ),
    };

    let occupied = alpha_occupancy_grid(texture, grid_w, grid_h);
    if !occupied.iter().any(|&filled| filled) {
        return billboard_geometry_rect(aspect, depth_ratio, mirror_back_face);
    }

    let loops = extract_boundary_loops(&occupied, grid_w, grid_h);
    if loops.is_empty() {
        return billboard_geometry_rect(aspect, depth_ratio, mirror_back_face);
    }

    let x_for = |gx: usize| -hw + (gx as f32 / grid_w as f32) * (2.0 * hw);
    let y_for = |gy: usize| hh - (gy as f32 / grid_h as f32) * (2.0 * hh);
    let texel_u = 0.5 / texture.width.max(1) as f32;
    let texel_v = 0.5 / texture.height.max(1) as f32;

    for loop_points in loops {
        let simplified = simplify_collinear(&loop_points);
        if simplified.len() < 3 {
            continue;
        }

        let contour2d: Vec<[f32; 2]> = simplified
            .iter()
            .map(|p| [x_for(p.x as usize), y_for(p.y as usize)])
            .collect();
        let mut contour = simplified.clone();
        if signed_area(&contour2d) < 0.0 {
            contour.reverse();
        }
        let contour2d: Vec<[f32; 2]> = contour
            .iter()
            .map(|p| [x_for(p.x as usize), y_for(p.y as usize)])
            .collect();

        emit_cap(
            &mut vertices,
            &mut indices,
            &contour,
            &contour2d,
            grid_w,
            grid_h,
            hd,
            false,
            false,
        );
        emit_cap(
            &mut vertices,
            &mut indices,
            &contour,
            &contour2d,
            grid_w,
            grid_h,
            -hd,
            true,
            false,
        );

        for i in 0..contour.len() {
            let a = contour[i];
            let b = contour[(i + 1) % contour.len()];
            let ax = x_for(a.x as usize);
            let ay = y_for(a.y as usize);
            let bx = x_for(b.x as usize);
            let by = y_for(b.y as usize);

            let normal = normalize([(by - ay) as f64, -(bx - ax) as f64, 0.0]);

            let au = (a.x as f32 / grid_w as f32).clamp(texel_u, 1.0 - texel_u);
            let av = (a.y as f32 / grid_h as f32).clamp(texel_v, 1.0 - texel_v);
            let bu = (b.x as f32 / grid_w as f32).clamp(texel_u, 1.0 - texel_u);
            let bv = (b.y as f32 / grid_h as f32).clamp(texel_v, 1.0 - texel_v);

            push_quad(
                &mut vertices,
                &mut indices,
                [[ax, ay, -hd], [ax, ay, hd], [bx, by, hd], [bx, by, -hd]],
                [normal[0] as f32, normal[1] as f32, 0.0],
                [[au, av], [au, av], [bu, bv], [bu, bv]],
                2,
            );
        }
    }

    if indices.is_empty() {
        billboard_geometry_rect(aspect, depth_ratio, mirror_back_face)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct GridPoint {
    x: u16,
    y: u16,
}

fn extract_boundary_loops(occupied: &[bool], grid_w: usize, grid_h: usize) -> Vec<Vec<GridPoint>> {
    let mut next: HashMap<GridPoint, GridPoint> = HashMap::new();
    let gp = |x: usize, y: usize| GridPoint {
        x: x as u16,
        y: y as u16,
    };

    for y in 0..grid_h {
        for x in 0..grid_w {
            if !occupied[y * grid_w + x] {
                continue;
            }
            let top_empty = y == 0 || !occupied[(y - 1) * grid_w + x];
            if top_empty {
                next.insert(gp(x, y), gp(x + 1, y));
            }
            let right_empty = x + 1 == grid_w || !occupied[y * grid_w + (x + 1)];
            if right_empty {
                next.insert(gp(x + 1, y), gp(x + 1, y + 1));
            }
            let bottom_empty = y + 1 == grid_h || !occupied[(y + 1) * grid_w + x];
            if bottom_empty {
                next.insert(gp(x + 1, y + 1), gp(x, y + 1));
            }
            let left_empty = x == 0 || !occupied[y * grid_w + (x - 1)];
            if left_empty {
                next.insert(gp(x, y + 1), gp(x, y));
            }
        }
    }

    let mut visited: HashMap<GridPoint, bool> = HashMap::new();
    let mut loops = Vec::new();
    for &start in next.keys() {
        if visited.get(&start).copied().unwrap_or(false) {
            continue;
        }
        let mut loop_points = Vec::new();
        let mut current = start;
        loop {
            if visited.get(&current).copied().unwrap_or(false) {
                break;
            }
            visited.insert(current, true);
            loop_points.push(current);
            let Some(&next_point) = next.get(&current) else {
                break;
            };
            current = next_point;
            if current == start {
                break;
            }
        }
        if loop_points.len() >= 3 {
            loops.push(loop_points);
        }
    }
    loops
}

fn simplify_collinear(points: &[GridPoint]) -> Vec<GridPoint> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = Vec::new();
    for i in 0..points.len() {
        let prev = points[(i + points.len() - 1) % points.len()];
        let curr = points[i];
        let next = points[(i + 1) % points.len()];
        let dx1 = curr.x as i32 - prev.x as i32;
        let dy1 = curr.y as i32 - prev.y as i32;
        let dx2 = next.x as i32 - curr.x as i32;
        let dy2 = next.y as i32 - curr.y as i32;
        if dx1 * dy2 != dy1 * dx2 {
            out.push(curr);
        }
    }
    out
}

fn signed_area(points: &[[f32; 2]]) -> f32 {
    let mut area = 0.0f32;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        area += a[0] * b[1] - b[0] * a[1];
    }
    area * 0.5
}

fn emit_cap(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    contour: &[GridPoint],
    contour2d: &[[f32; 2]],
    grid_w: usize,
    grid_h: usize,
    z: f32,
    flip_winding: bool,
    mirror_u: bool,
) {
    let base = vertices.len() as u16;
    for (grid, pos2) in contour.iter().zip(contour2d.iter()) {
        let mut u = grid.x as f32 / grid_w as f32;
        if mirror_u {
            u = 1.0 - u;
        }
        let v = grid.y as f32 / grid_h as f32;
        vertices.push(Vertex {
            position: [pos2[0], pos2[1], z],
            normal: [0.0, 0.0, if z >= 0.0 { 1.0 } else { -1.0 }],
            uv: [u, v],
            face_type: if z >= 0.0 { 0 } else { 1 },
            _pad: 0,
        });
    }

    for tri in triangulate_polygon(contour2d) {
        if flip_winding {
            indices.extend_from_slice(&[
                base + tri[0] as u16,
                base + tri[2] as u16,
                base + tri[1] as u16,
            ]);
        } else {
            indices.extend_from_slice(&[
                base + tri[0] as u16,
                base + tri[1] as u16,
                base + tri[2] as u16,
            ]);
        }
    }
}

fn triangulate_polygon(points: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let mut remaining: Vec<usize> = (0..points.len()).collect();
    let mut tris = Vec::new();
    while remaining.len() > 2 {
        let len = remaining.len();
        let mut ear_found = false;
        for i in 0..len {
            let prev = remaining[(i + len - 1) % len];
            let curr = remaining[i];
            let next = remaining[(i + 1) % len];
            let a = points[prev];
            let b = points[curr];
            let c = points[next];
            if cross2(a, b, c) <= 0.0 {
                continue;
            }
            let mut contains = false;
            for &idx in &remaining {
                if idx == prev || idx == curr || idx == next {
                    continue;
                }
                if point_in_triangle(points[idx], a, b, c) {
                    contains = true;
                    break;
                }
            }
            if contains {
                continue;
            }
            tris.push([prev, curr, next]);
            remaining.remove(i);
            ear_found = true;
            break;
        }
        if !ear_found {
            break;
        }
    }
    tris
}

fn cross2(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn point_in_triangle(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let c1 = cross2(a, b, p);
    let c2 = cross2(b, c, p);
    let c3 = cross2(c, a, p);
    (c1 >= 0.0 && c2 >= 0.0 && c3 >= 0.0) || (c1 <= 0.0 && c2 <= 0.0 && c3 <= 0.0)
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
    fn extruded_geometry_avoids_outer_rect_for_padded_alpha() {
        let texture = padded_texture();
        let (vertices, _) = extruded_billboard_geometry(&texture, 0.1, true);
        let min_x = vertices
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = vertices
            .iter()
            .map(|v| v.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = vertices
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        let max_y = vertices
            .iter()
            .map(|v| v.position[1])
            .fold(f32::NEG_INFINITY, f32::max);

        assert!(
            min_x > -0.7 && max_x < 0.7,
            "outer rectangular side walls should be gone"
        );
        assert!(
            min_y > -0.7 && max_y < 0.7,
            "outer rectangular side walls should be gone"
        );
    }
}

fn alpha_occupancy_grid(texture: &Texture, grid_w: usize, grid_h: usize) -> Vec<bool> {
    let mut grid = vec![false; grid_w * grid_h];
    let samples_per_axis = 3usize;
    let coverage_threshold = 0.35f32;
    for y in 0..grid_h {
        for x in 0..grid_w {
            let mut covered = 0usize;
            let mut total = 0usize;
            for sy in 0..samples_per_axis {
                for sx in 0..samples_per_axis {
                    let u =
                        (x as f64 + (sx as f64 + 0.5) / samples_per_axis as f64) / grid_w as f64;
                    let v =
                        (y as f64 + (sy as f64 + 0.5) / samples_per_axis as f64) / grid_h as f64;
                    let alpha = texture.sample(u, v)[3];
                    if alpha >= ALPHA_SHAPE_THRESHOLD {
                        covered += 1;
                    }
                    total += 1;
                }
            }
            let coverage = covered as f32 / total as f32;
            grid[y * grid_w + x] = coverage >= coverage_threshold;
        }
    }
    grid
}
