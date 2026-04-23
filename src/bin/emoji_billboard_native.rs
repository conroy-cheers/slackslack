use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use egui::{Align2, ComboBox};
use egui_wgpu::{Renderer as EguiRenderer, ScreenDescriptor};
use egui_winit::State as EguiWinitState;
use emoji_renderer::gpu::{GpuRenderer, emoji_preview_scene_params};
use pollster::block_on;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopBuilder};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

mod terminal_renderer {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/emoji-web/src/terminal_renderer.rs"
    ));
}

mod gallery {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/emoji-web/src/gallery.rs"
    ));
}

use terminal_renderer::{TERM_COLS, TERM_ROWS, TerminalGrid, TerminalRenderer};

const COMPOSITE_SHADER: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/crates/emoji-web/src/composite.wgsl"));

#[derive(Clone, Copy, PartialEq)]
struct TransferTuning {
    linear_gain: f32,
    gamma: f32,
    lift: f32,
    saturation: f32,
}

impl Default for TransferTuning {
    fn default() -> Self {
        Self {
            linear_gain: 1.15,
            gamma: 1.0,
            lift: -0.05,
            saturation: 1.25,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct PerfToggles {
    crt: bool,
    transfer: bool,
    overlay_filter: bool,
    billboard: bool,
}

impl Default for PerfToggles {
    fn default() -> Self {
        Self {
            crt: true,
            transfer: true,
            overlay_filter: true,
            billboard: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct RenderConfig {
    gallery_canvas_scale: f32,
    preview_canvas_scale: f32,
    preview_max_dim: u32,
    preview_render_scale: f32,
    display_pixelated: bool,
    overlay_filter: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            gallery_canvas_scale: 1.0,
            preview_canvas_scale: 1.0,
            preview_max_dim: 180,
            preview_render_scale: 2.0,
            display_pixelated: false,
            overlay_filter: true,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CompositeUniforms {
    output_size: [f32; 2],
    time_secs: f32,
    preview_mix: f32,
    terminal_rect: [f32; 4],
    billboard_rect: [f32; 4],
    terminal_grid: [f32; 4],
    transfer_tuning: [f32; 4],
    perf_toggles: [f32; 4],
}

#[derive(Clone, Copy, Debug, Default)]
struct NativePerfSnapshot {
    smoothed_fps: f32,
    smoothed_frame_cpu_ms: f32,
    smoothed_frame_interval_ms: f32,
    smoothed_surface_acquire_ms: f32,
    smoothed_terminal_ms: f32,
    smoothed_screen_ms: f32,
    smoothed_scene_ms: f32,
    smoothed_egui_ms: f32,
    smoothed_composite_ms: f32,
    window_width: u32,
    window_height: u32,
    surface_width: u32,
    surface_height: u32,
    terminal_width: u32,
    terminal_height: u32,
    scale_factor: f32,
    preview_mix: f32,
    egui_paint_jobs: u32,
    egui_textures_delta: u32,
    last_screen_redrew: bool,
    last_previewing: bool,
    last_uses_billboard: bool,
    offscreen_stats: Option<emoji_renderer::gpu::OffscreenPerfStats>,
}

struct NativeBillboardApp {
    window: Option<Arc<Window>>,
    renderer: Option<RendererState>,
    exit_error: Option<anyhow::Error>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BackendChoice {
    Auto,
    X11,
    Wayland,
}

struct RendererState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: GpuRenderer,
    surface_format: wgpu::TextureFormat,

    screen_pipeline: wgpu::RenderPipeline,
    screen_bind_group_layout: wgpu::BindGroupLayout,
    screen_bind_group: wgpu::BindGroup,
    screen_overlay_filter: bool,
    screen_uniform_buffer: wgpu::Buffer,
    screen_texture: wgpu::Texture,
    screen_texture_view: wgpu::TextureView,
    screen_dirty: bool,
    last_screen_transfer: TransferTuning,
    last_screen_perf_toggles: PerfToggles,
    last_screen_render_config: RenderConfig,
    last_screen_preview_mix: f32,

    composite_pipeline: wgpu::RenderPipeline,
    composite_bind_group_layout: wgpu::BindGroupLayout,
    composite_bind_group: wgpu::BindGroup,
    composite_uses_billboard: bool,
    composite_display_pixelated: bool,
    composite_billboard_generation: u64,
    composite_uniform_buffer: wgpu::Buffer,

    overlay_sampler_linear: wgpu::Sampler,
    overlay_sampler_nearest: wgpu::Sampler,
    billboard_sampler_linear: wgpu::Sampler,
    billboard_sampler_nearest: wgpu::Sampler,
    placeholder_billboard_view: wgpu::TextureView,

    terminal_renderer: TerminalRenderer,
    terminal_grid: TerminalGrid,
    terminal_dirty: bool,
    gallery: gallery::Gallery,
    demo_pixels: Vec<[u8; 4]>,
    demo_w: u32,
    demo_h: u32,

    egui_ctx: egui::Context,
    egui_state: EguiWinitState,
    egui_renderer: EguiRenderer,

    transfer: TransferTuning,
    perf_toggles: PerfToggles,
    render_config: RenderConfig,

    start_time: Instant,
    last_time_secs: f64,
    last_blink_on: bool,
    last_preview_overlay_visible: bool,
    perf: NativePerfSnapshot,
}

impl NativeBillboardApp {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            exit_error: None,
        }
    }
}

impl ApplicationHandler for NativeBillboardApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }

        let window = match event_loop.create_window(
            WindowAttributes::default()
                .with_title("Emoji Billboard Native")
                .with_inner_size(PhysicalSize::new(1280, 960)),
        ) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                self.exit_error = Some(anyhow!(err));
                event_loop.exit();
                return;
            }
        };

        match block_on(RendererState::new(window.clone())) {
            Ok(renderer) => {
                self.window = Some(window);
                self.renderer = Some(renderer);
            }
            Err(err) => {
                self.exit_error = Some(err);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        if renderer.window.id() != window_id {
            return;
        }

        if renderer.egui_state.on_window_event(&renderer.window, &event).consumed {
            renderer.window.request_redraw();
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => renderer.resize(size),
            WindowEvent::ScaleFactorChanged { .. } => {
                renderer.resize(renderer.window.inner_size());
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if !renderer.egui_ctx.wants_keyboard_input() {
                    renderer.handle_key_event(&event);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(err) = renderer.frame() {
                    self.exit_error = Some(err);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
        if let Some(renderer) = self.renderer.as_ref() {
            renderer.window.request_redraw();
        }
    }
}

impl RendererState {
    async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| anyhow!("no Vulkan/WebGPU adapter for native billboard viewer"))?;

        let adapter_limits = adapter.limits();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("emoji_billboard_native"),
                    required_features: wgpu::Features::empty(),
                    required_limits: adapter_limits,
                    ..Default::default()
                },
                None,
            )
            .await?;

        let linear_depth_format =
            if adapter
                .get_texture_format_features(wgpu::TextureFormat::R32Float)
                .allowed_usages
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            {
                wgpu::TextureFormat::R32Float
            } else {
                wgpu::TextureFormat::R16Float
            };

        let renderer =
            GpuRenderer::from_device_queue(device, queue, wgpu::Features::empty(), linear_depth_format)?;

        let caps = surface.get_capabilities(&adapter);
        let surface_format = preferred_surface_format(&caps.formats);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(renderer.device(), &config);

        let terminal_renderer = TerminalRenderer::new(renderer.device(), renderer.queue())?;
        let terminal_grid = TerminalGrid::new();
        let render_config = RenderConfig::default();
        let transfer = TransferTuning::default();
        let perf_toggles = PerfToggles::default();

        let shader = renderer
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("emoji_billboard_native_shader"),
                source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
            });

        let screen_bind_group_layout =
            renderer
                .device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("screen_bgl"),
                    entries: &[
                        bgl_texture(0),
                        bgl_sampler(1),
                        uniform_bgl_entry(2),
                        bgl_texture(3),
                        bgl_sampler(4),
                    ],
                });
        let composite_bind_group_layout =
            renderer
                .device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("composite_bgl"),
                    entries: &[
                        bgl_texture(0),
                        bgl_sampler(1),
                        uniform_bgl_entry(2),
                        bgl_texture(3),
                        bgl_sampler(4),
                    ],
                });

        let screen_uniform_buffer = renderer.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen_uniforms"),
            size: std::mem::size_of::<CompositeUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let composite_uniform_buffer = renderer.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("composite_uniforms"),
            size: std::mem::size_of::<CompositeUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let screen_pipeline_layout =
            renderer
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("screen_pipeline_layout"),
                    bind_group_layouts: &[&screen_bind_group_layout],
                    push_constant_ranges: &[],
                });
        let screen_pipeline =
            renderer
                .device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("screen_pipeline"),
                    layout: Some(&screen_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_screen"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

        let composite_pipeline_layout =
            renderer
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("composite_pipeline_layout"),
                    bind_group_layouts: &[&composite_bind_group_layout],
                    push_constant_ranges: &[],
                });
        let composite_pipeline =
            renderer
                .device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("composite_pipeline"),
                    layout: Some(&composite_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_composite"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: config.format,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

        let overlay_sampler_linear = create_sampler(renderer.device(), wgpu::FilterMode::Linear);
        let overlay_sampler_nearest = create_sampler(renderer.device(), wgpu::FilterMode::Nearest);
        let billboard_sampler_linear = create_sampler(renderer.device(), wgpu::FilterMode::Linear);
        let billboard_sampler_nearest = create_sampler(renderer.device(), wgpu::FilterMode::Nearest);

        let (placeholder_billboard_tex, placeholder_billboard_view) =
            create_rgba_texture(renderer.device(), 1, 1);
        let _keep_placeholder = placeholder_billboard_tex;
        let (screen_texture, screen_texture_view) = create_render_target_texture(
            renderer.device(),
            terminal_renderer.pixel_width(),
            terminal_renderer.pixel_height(),
            "screen_effect_texture",
        );

        let screen_bind_group =
            renderer
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("screen_bg"),
                    layout: &screen_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                terminal_renderer.texture_view(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&overlay_sampler_linear),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: screen_uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(&placeholder_billboard_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Sampler(&overlay_sampler_linear),
                        },
                    ],
                });
        let composite_bind_group =
            renderer
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("composite_bg"),
                    layout: &composite_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&screen_texture_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&overlay_sampler_linear),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: composite_uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(&placeholder_billboard_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Sampler(&billboard_sampler_linear),
                        },
                    ],
                });

        let egui_ctx = egui::Context::default();
        let egui_state = EguiWinitState::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            window.theme(),
            None,
        );
        let egui_renderer = EguiRenderer::new(
            renderer.device(),
            surface_format,
            None,
            1,
            false,
        );

        let (demo_pixels, demo_w, demo_h) = demo_texture();
        let initial_surface_width = config.width;
        let initial_surface_height = config.height;
        let initial_terminal_width = terminal_renderer.pixel_width();
        let initial_terminal_height = terminal_renderer.pixel_height();
        let initial_scale_factor = window.scale_factor() as f32;

        Ok(Self {
            window,
            surface,
            config,
            renderer,
            surface_format,
            screen_pipeline,
            screen_bind_group_layout,
            screen_bind_group,
            screen_overlay_filter: true,
            screen_uniform_buffer,
            screen_texture,
            screen_texture_view,
            screen_dirty: true,
            last_screen_transfer: transfer,
            last_screen_perf_toggles: perf_toggles,
            last_screen_render_config: render_config,
            last_screen_preview_mix: -1.0,
            composite_pipeline,
            composite_bind_group_layout,
            composite_bind_group,
            composite_uses_billboard: false,
            composite_display_pixelated: false,
            composite_billboard_generation: 0,
            composite_uniform_buffer,
            overlay_sampler_linear,
            overlay_sampler_nearest,
            billboard_sampler_linear,
            billboard_sampler_nearest,
            placeholder_billboard_view,
            terminal_renderer,
            terminal_grid,
            terminal_dirty: true,
            gallery: gallery::Gallery::new(),
            demo_pixels,
            demo_w,
            demo_h,
            egui_ctx,
            egui_state,
            egui_renderer,
            transfer,
            perf_toggles,
            render_config,
            start_time: Instant::now(),
            last_time_secs: 0.0,
            last_blink_on: true,
            last_preview_overlay_visible: false,
            perf: NativePerfSnapshot {
                smoothed_fps: 60.0,
                window_width: size.width.max(1),
                window_height: size.height.max(1),
                surface_width: initial_surface_width,
                surface_height: initial_surface_height,
                terminal_width: initial_terminal_width,
                terminal_height: initial_terminal_height,
                scale_factor: initial_scale_factor,
                last_screen_redrew: true,
                ..Default::default()
            },
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.reconfigure_surface(size);
    }

    fn reconfigure_surface(&mut self, size: PhysicalSize<u32>) {
        let scale = if self.gallery.is_previewing() {
            self.render_config.preview_canvas_scale
        } else {
            self.render_config.gallery_canvas_scale
        };
        self.config.width = ((size.width as f32) * scale).round().max(1.0) as u32;
        self.config.height = ((size.height as f32) * scale).round().max(1.0) as u32;
        self.surface.configure(self.renderer.device(), &self.config);
    }

    fn handle_key_event(&mut self, event: &winit::event::KeyEvent) {
        if !event.state.is_pressed() {
            return;
        }
        let action = match &event.logical_key {
            Key::Named(NamedKey::ArrowUp) => Some(gallery::KeyAction::Up),
            Key::Named(NamedKey::ArrowDown) => Some(gallery::KeyAction::Down),
            Key::Named(NamedKey::Enter) => Some(gallery::KeyAction::Enter),
            Key::Named(NamedKey::Escape) => Some(gallery::KeyAction::Escape),
            Key::Named(NamedKey::Backspace) => Some(gallery::KeyAction::Backspace),
            Key::Character(text) => {
                let mut chars = text.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) if ch.is_ascii_graphic() || ch == ' ' => {
                        Some(gallery::KeyAction::Char(ch))
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(action) = action {
            self.gallery.handle_key(action);
            self.terminal_dirty = true;
            self.screen_dirty = true;
        }
    }

    fn frame(&mut self) -> Result<()> {
        let frame_start = Instant::now();
        let raw_input = self.egui_state.take_egui_input(&self.window);

        let now = self.start_time.elapsed().as_secs_f64();
        let dt_secs = (now - self.last_time_secs).max(0.0);
        self.last_time_secs = now;
        self.gallery.tick(dt_secs as f32);
        if dt_secs > 0.0 {
            let fps = (1.0 / dt_secs) as f32;
            self.perf.smoothed_fps = self.perf.smoothed_fps * 0.9 + fps * 0.1;
            self.perf.smoothed_frame_interval_ms =
                self.perf.smoothed_frame_interval_ms * 0.9 + (dt_secs as f32 * 1000.0) * 0.1;
        }

        let blink_on = gallery::cursor_blink_on(now);
        let preview_overlay_visible = gallery::show_preview_overlay(&self.gallery);
        if blink_on != self.last_blink_on
            || preview_overlay_visible != self.last_preview_overlay_visible
        {
            self.last_blink_on = blink_on;
            self.last_preview_overlay_visible = preview_overlay_visible;
            self.terminal_dirty = true;
            self.screen_dirty = true;
        }

        let mut transfer = self.transfer;
        let mut render_config = self.render_config;
        let perf = self.perf;
        let egui_ui_start = Instant::now();
        let panel_response = self.egui_ctx.run(raw_input, |ctx| {
            Self::draw_egui(
                ctx,
                &mut transfer,
                &mut render_config,
                perf,
            )
        });
        let mut egui_ms = egui_ui_start.elapsed().as_secs_f32() * 1000.0;
        self.transfer = transfer;
        self.render_config = render_config;
        self.egui_state
            .handle_platform_output(&self.window, panel_response.platform_output.clone());

        let desired_surface_scale = if self.gallery.is_previewing() {
            self.render_config.preview_canvas_scale
        } else {
            self.render_config.gallery_canvas_scale
        };
        let actual_surface_scale =
            self.config.width as f32 / self.window.inner_size().width.max(1) as f32;
        if (desired_surface_scale - actual_surface_scale).abs() > 0.01 {
            self.reconfigure_surface(self.window.inner_size());
        }

        let term_start = Instant::now();
        if self.terminal_dirty {
            gallery::render_to_grid(&mut self.terminal_grid, &self.gallery, now);
            self.terminal_renderer
                .render(self.renderer.device(), self.renderer.queue(), &self.terminal_grid);
            self.terminal_dirty = false;
            self.screen_dirty = true;
        }
        let terminal_ms = term_start.elapsed().as_secs_f32() * 1000.0;

        let previewing = self.gallery.is_previewing();
        let preview_mix = self.gallery.preview_mix();
        if self.last_screen_transfer != self.transfer
            || self.last_screen_perf_toggles != self.perf_toggles
            || self.last_screen_render_config.overlay_filter != self.render_config.overlay_filter
            || (self.last_screen_preview_mix - preview_mix).abs() > 0.001
        {
            self.screen_dirty = true;
        }

        let overlay_w = self.terminal_renderer.pixel_width();
        let overlay_h = self.terminal_renderer.pixel_height();
        let scene_start = Instant::now();
        let billboard_pixel_rect: [f32; 4] = if previewing && self.perf_toggles.billboard {
            if let Some(cell_rect) = self.gallery.billboard_cell_rect(TERM_COLS, TERM_ROWS) {
                let native_w = cell_rect.width as f32;
                let native_h = (cell_rect.height as f32) * 2.0;
                let target_max_dim = self.render_config.preview_max_dim as f32;
                let native_max = native_w.max(native_h).max(1.0);
                let scale = target_max_dim / native_max;
                let render_w = (native_w * scale).round().max(1.0) as u32;
                let render_h = (native_h * scale).round().max(1.0) as u32;

                let texture = emoji_renderer::texture::Texture {
                    pixels: &self.demo_pixels,
                    width: self.demo_w,
                    height: self.demo_h,
                };
                let mut params = emoji_preview_scene_params();
                params.sharpen = Some(0.1);
                params.dither = Some(0.3);
                params.vhs = Some(0.5);
                params.jitter = Some(0.1);
                params.supersample = true;
                params.render_scale = Some(self.render_config.preview_render_scale);
                self.renderer.render_to_offscreen_params(
                    &texture,
                    render_w,
                    render_h,
                    now,
                    &params,
                )?;

                let cell_px_w = overlay_w as f32 / TERM_COLS as f32;
                let cell_px_h = overlay_h as f32 / TERM_ROWS as f32;
                [
                    cell_rect.x as f32 * cell_px_w,
                    cell_rect.y as f32 * cell_px_h,
                    cell_rect.width as f32 * cell_px_w,
                    cell_rect.height as f32 * cell_px_h,
                ]
            } else {
                [0.0; 4]
            }
        } else {
            [0.0; 4]
        };
        let scene_ms = scene_start.elapsed().as_secs_f32() * 1000.0;

        let screen_redrew = self.screen_dirty;
        let screen_start = Instant::now();
        if self.screen_dirty {
            let screen_uniforms = CompositeUniforms {
                output_size: [overlay_w as f32, overlay_h as f32],
                time_secs: now as f32,
                preview_mix,
                terminal_rect: [0.0; 4],
                billboard_rect: [0.0; 4],
                terminal_grid: [
                    TERM_COLS as f32,
                    TERM_ROWS as f32,
                    overlay_w as f32 / TERM_COLS as f32,
                    overlay_h as f32 / TERM_ROWS as f32,
                ],
                transfer_tuning: [
                    self.transfer.linear_gain,
                    self.transfer.gamma,
                    self.transfer.lift,
                    self.transfer.saturation,
                ],
                perf_toggles: [
                    if self.perf_toggles.crt { 1.0 } else { 0.0 },
                    if self.perf_toggles.transfer { 1.0 } else { 0.0 },
                    if self.perf_toggles.overlay_filter && self.render_config.overlay_filter {
                        1.0
                    } else {
                        0.0
                    },
                    0.0,
                ],
            };
            self.renderer.queue().write_buffer(
                &self.screen_uniform_buffer,
                0,
                bytemuck::bytes_of(&screen_uniforms),
            );
            self.ensure_screen_bind_group(self.render_config.overlay_filter);
            let mut encoder =
                self.renderer
                    .device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("screen_effect_encoder"),
                    });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("screen_effect_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.screen_texture_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&self.screen_pipeline);
                pass.set_bind_group(0, &self.screen_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            self.renderer.queue().submit(Some(encoder.finish()));
            self.screen_dirty = false;
            self.last_screen_transfer = self.transfer;
            self.last_screen_perf_toggles = self.perf_toggles;
            self.last_screen_render_config = self.render_config;
            self.last_screen_preview_mix = preview_mix;
        }
        let screen_ms = screen_start.elapsed().as_secs_f32() * 1000.0;
        self.perf.last_screen_redrew = screen_redrew;

        let term_aspect = overlay_w as f32 / overlay_h as f32;
        let canvas_aspect = self.config.width as f32 / self.config.height as f32;
        let (term_w, term_h) = if canvas_aspect > term_aspect {
            let th = self.config.height as f32;
            (th * term_aspect, th)
        } else {
            let tw = self.config.width as f32;
            (tw, tw / term_aspect)
        };
        let term_x = (self.config.width as f32 - term_w) * 0.5;
        let term_y = (self.config.height as f32 - term_h) * 0.5;
        let terminal_rect = [term_x, term_y, term_w, term_h];

        let sx = term_w / overlay_w as f32;
        let sy = term_h / overlay_h as f32;
        let billboard_canvas_rect = [
            term_x + billboard_pixel_rect[0] * sx,
            term_y + billboard_pixel_rect[1] * sy,
            billboard_pixel_rect[2] * sx,
            billboard_pixel_rect[3] * sy,
        ];

        let uniforms = CompositeUniforms {
            output_size: [self.config.width as f32, self.config.height as f32],
            time_secs: now as f32,
            preview_mix,
            terminal_rect,
            billboard_rect: billboard_canvas_rect,
            terminal_grid: [
                TERM_COLS as f32,
                TERM_ROWS as f32,
                term_w / TERM_COLS as f32,
                term_h / TERM_ROWS as f32,
            ],
            transfer_tuning: [
                self.transfer.linear_gain,
                self.transfer.gamma,
                self.transfer.lift,
                self.transfer.saturation,
            ],
            perf_toggles: [
                if self.perf_toggles.crt { 1.0 } else { 0.0 },
                if self.perf_toggles.transfer { 1.0 } else { 0.0 },
                if self.perf_toggles.overlay_filter && self.render_config.overlay_filter {
                    1.0
                } else {
                    0.0
                },
                if self.perf_toggles.billboard { 1.0 } else { 0.0 },
            ],
        };
        self.renderer.queue().write_buffer(
            &self.composite_uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );

        let uses_billboard = previewing && preview_mix > 0.0 && billboard_canvas_rect[2] > 0.0;
        self.ensure_composite_bind_group(
            uses_billboard,
            self.render_config.display_pixelated,
            self.renderer.render_target_generation(),
        );

        let surface_acquire_start = Instant::now();
        let output = self.surface.get_current_texture()?;
        let surface_acquire_ms = surface_acquire_start.elapsed().as_secs_f32() * 1000.0;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };
        let egui_prepare_start = Instant::now();
        let paint_jobs = self
            .egui_ctx
            .tessellate(panel_response.shapes, screen_descriptor.pixels_per_point);
        for (id, delta) in &panel_response.textures_delta.set {
            self.egui_renderer
                .update_texture(self.renderer.device(), self.renderer.queue(), *id, delta);
        }

        let composite_start = Instant::now();
        let mut encoder =
            self.renderer
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("native_composite_encoder"),
                });
        self.egui_renderer.update_buffers(
            self.renderer.device(),
            self.renderer.queue(),
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );
        egui_ms += egui_prepare_start.elapsed().as_secs_f32() * 1000.0;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("native_composite_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &self.composite_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.egui_renderer
                .render(&mut pass, &paint_jobs, &screen_descriptor);
        }

        self.renderer.queue().submit(Some(encoder.finish()));
        output.present();
        for id in &panel_response.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        let composite_ms = composite_start.elapsed().as_secs_f32() * 1000.0;
        let frame_cpu_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
        self.perf.smoothed_surface_acquire_ms =
            self.perf.smoothed_surface_acquire_ms * 0.85 + surface_acquire_ms * 0.15;
        self.perf.smoothed_terminal_ms = self.perf.smoothed_terminal_ms * 0.85 + terminal_ms * 0.15;
        self.perf.smoothed_screen_ms = self.perf.smoothed_screen_ms * 0.85 + screen_ms * 0.15;
        self.perf.smoothed_scene_ms = self.perf.smoothed_scene_ms * 0.85 + scene_ms * 0.15;
        self.perf.smoothed_egui_ms = self.perf.smoothed_egui_ms * 0.85 + egui_ms * 0.15;
        self.perf.smoothed_composite_ms =
            self.perf.smoothed_composite_ms * 0.85 + composite_ms * 0.15;
        self.perf.smoothed_frame_cpu_ms =
            self.perf.smoothed_frame_cpu_ms * 0.85 + frame_cpu_ms * 0.15;
        self.perf.window_width = self.window.inner_size().width.max(1);
        self.perf.window_height = self.window.inner_size().height.max(1);
        self.perf.surface_width = self.config.width;
        self.perf.surface_height = self.config.height;
        self.perf.terminal_width = overlay_w;
        self.perf.terminal_height = overlay_h;
        self.perf.scale_factor = self.window.scale_factor() as f32;
        self.perf.preview_mix = preview_mix;
        self.perf.egui_paint_jobs = paint_jobs.len() as u32;
        self.perf.egui_textures_delta =
            (panel_response.textures_delta.set.len() + panel_response.textures_delta.free.len())
                as u32;
        self.perf.last_previewing = previewing;
        self.perf.last_uses_billboard = uses_billboard;
        self.perf.offscreen_stats = self.renderer.offscreen_perf_stats();

        Ok(())
    }

    fn draw_egui(
        ctx: &egui::Context,
        transfer: &mut TransferTuning,
        render_config: &mut RenderConfig,
        perf: NativePerfSnapshot,
    ) {
        egui::Window::new("Controls")
            .default_open(true)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Transfer");
                ui.add(egui::Slider::new(&mut transfer.linear_gain, 0.70..=1.40).text("Linear Gain"));
                ui.add(egui::Slider::new(&mut transfer.gamma, 0.70..=1.30).text("Gamma"));
                ui.add(egui::Slider::new(&mut transfer.lift, -0.08..=0.08).text("Lift"));
                ui.add(egui::Slider::new(&mut transfer.saturation, 0.50..=1.80).text("Saturation"));

                ui.separator();
                ui.heading("Render");
                ComboBox::from_label("Display Scaling")
                    .selected_text(if render_config.display_pixelated {
                        "Pixelated"
                    } else {
                        "Smooth"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut render_config.display_pixelated, false, "Smooth");
                        ui.selectable_value(&mut render_config.display_pixelated, true, "Pixelated");
                    });
                ComboBox::from_label("Terminal Sampling")
                    .selected_text(if render_config.overlay_filter {
                        "Filtered"
                    } else {
                        "Nearest"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut render_config.overlay_filter, true, "Filtered");
                        ui.selectable_value(&mut render_config.overlay_filter, false, "Nearest");
                    });
                ui.add(
                    egui::Slider::new(&mut render_config.gallery_canvas_scale, 0.25..=1.50)
                        .text("Gallery Canvas Res"),
                );
                ui.add(
                    egui::Slider::new(&mut render_config.preview_canvas_scale, 0.25..=1.50)
                        .text("Preview Canvas Res"),
                );
                ui.add(
                    egui::Slider::new(&mut render_config.preview_max_dim, 96..=320)
                        .step_by(4.0)
                        .text("Preview Res"),
                );
                ui.add(
                    egui::Slider::new(&mut render_config.preview_render_scale, 1.0..=3.0)
                        .step_by(0.25)
                        .text("Preview SSAA"),
                );

                ui.separator();
                ui.heading("Perf");
                ui.monospace(format!(
                    "{:>3} FPS\nFRAME {:>4.1} ms\nINTVL {:>4.1} ms\nACQ   {:>4.1} ms\nTERM  {:>4.1} ms\nSCREEN {:>4.1} ms {}\n3D    {:>4.1} ms\nEGUI  {:>4.1} ms\nCOMP  {:>4.1} ms",
                    perf.smoothed_fps.round().clamp(0.0, 999.0) as u32,
                    perf.smoothed_frame_cpu_ms,
                    perf.smoothed_frame_interval_ms,
                    perf.smoothed_surface_acquire_ms,
                    perf.smoothed_terminal_ms,
                    perf.smoothed_screen_ms,
                    if perf.last_screen_redrew { "*" } else { "-" },
                    perf.smoothed_scene_ms,
                    perf.smoothed_egui_ms,
                    perf.smoothed_composite_ms,
                ));
                ui.separator();
                ui.monospace(format!(
                    "WIN  {}x{} @ {:.2}\nSURF {}x{}\nTERM {}x{}\nMODE {} mix {:.2}\nBILL {}\nEGUI jobs {} tex {}",
                    perf.window_width,
                    perf.window_height,
                    perf.scale_factor,
                    perf.surface_width,
                    perf.surface_height,
                    perf.terminal_width,
                    perf.terminal_height,
                    if perf.last_previewing { "preview" } else { "gallery" },
                    perf.preview_mix,
                    if perf.last_uses_billboard { "on" } else { "off" },
                    perf.egui_paint_jobs,
                    perf.egui_textures_delta,
                ));
                if let Some(stats) = perf.offscreen_stats {
                    ui.separator();
                    ui.monospace(format!(
                        "3D scene {}x{}\n3D out  {}x{}\n3D passes {}\n3D draws  {}\n3D downsample {}",
                        stats.scene_width,
                        stats.scene_height,
                        stats.output_width,
                        stats.output_height,
                        stats.pass_count,
                        stats.draw_call_count,
                        if stats.has_downsample { "yes" } else { "no" },
                    ));
                }
            });

        egui::Area::new("fps_overlay".into())
            .anchor(Align2::RIGHT_BOTTOM, [-12.0, -12.0])
            .show(ctx, |ui| {
                let fps_label = perf.smoothed_fps.round().clamp(0.0, 999.0) as u32;
                ui.label(
                    egui::RichText::new(format!(
                        "{fps_label:>3} FPS\nFRAME {:>4.1} MS\nSCREEN {}\n3D {:>4.1} MS\nCOMP {:>4.1} MS",
                        perf.smoothed_frame_cpu_ms,
                        if perf.last_screen_redrew { "*" } else { "-" },
                        perf.smoothed_scene_ms,
                        perf.smoothed_composite_ms,
                    ))
                    .monospace(),
                );
            });
    }

    fn ensure_composite_bind_group(
        &mut self,
        uses_billboard: bool,
        display_pixelated: bool,
        billboard_generation: u64,
    ) {
        if self.composite_uses_billboard == uses_billboard
            && self.composite_display_pixelated == display_pixelated
            && (!uses_billboard || self.composite_billboard_generation == billboard_generation)
        {
            return;
        }

        let billboard_view = if uses_billboard {
            self.renderer
                .offscreen_view()
                .unwrap_or(&self.placeholder_billboard_view)
        } else {
            &self.placeholder_billboard_view
        };
        let overlay_sampler = if display_pixelated {
            &self.overlay_sampler_nearest
        } else {
            &self.overlay_sampler_linear
        };
        let billboard_sampler = if display_pixelated {
            &self.billboard_sampler_nearest
        } else {
            &self.billboard_sampler_linear
        };

        self.composite_bind_group =
            self.renderer
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("native_composite_bg"),
                    layout: &self.composite_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&self.screen_texture_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(overlay_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: self.composite_uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(billboard_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Sampler(billboard_sampler),
                        },
                    ],
                });
        self.composite_uses_billboard = uses_billboard;
        self.composite_display_pixelated = display_pixelated;
        self.composite_billboard_generation = billboard_generation;
    }

    fn ensure_screen_bind_group(&mut self, overlay_filter: bool) {
        if self.screen_overlay_filter == overlay_filter {
            return;
        }
        let overlay_sampler = if overlay_filter {
            &self.overlay_sampler_linear
        } else {
            &self.overlay_sampler_nearest
        };
        self.screen_bind_group =
            self.renderer
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("native_screen_bg"),
                    layout: &self.screen_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                self.terminal_renderer.texture_view(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(overlay_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: self.screen_uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(&self.placeholder_billboard_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Sampler(overlay_sampler),
                        },
                    ],
                });
        self.screen_overlay_filter = overlay_filter;
    }
}

fn create_sampler(device: &wgpu::Device, filter: wgpu::FilterMode) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: filter,
        min_filter: filter,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    })
}

fn create_rgba_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("native_rgba_texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_render_target_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn preferred_surface_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    for format in formats {
        match format {
            wgpu::TextureFormat::Bgra8Unorm => return *format,
            wgpu::TextureFormat::Rgba8Unorm => return *format,
            _ => {}
        }
    }
    formats[0]
}

fn bgl_texture(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            multisampled: false,
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
        },
        count: None,
    }
}

fn bgl_sampler(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn uniform_bgl_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn demo_texture() -> (Vec<[u8; 4]>, u32, u32) {
    let w = 96u32;
    let h = 96u32;
    let mut pixels = vec![[0u8, 0, 0, 0]; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            let inside = x > 12 && x < 84 && y > 12 && y < 84;
            pixels[idx] = if inside {
                if x < 48 {
                    [230, 90, 50, 255]
                } else {
                    [60, 130, 255, 255]
                }
            } else {
                [0, 0, 0, 0]
            };
        }
    }
    (pixels, w, h)
}

fn main() -> Result<()> {
    ensure_linux_gui_runtime_env()?;
    let mut builder = EventLoop::builder();
    configure_event_loop(&mut builder, BackendChoice::Auto);
    let event_loop = builder.build()?;
    let mut app = NativeBillboardApp::new();
    event_loop.run_app(&mut app)?;
    if let Some(err) = app.exit_error {
        return Err(err);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn ensure_linux_gui_runtime_env() -> Result<()> {
    use std::collections::BTreeSet;
    use std::os::unix::process::CommandExt;

    const REEXEC_MARKER: &str = "SLACKSLACK_EMOJI_BILLBOARD_NATIVE_REEXEC";
    if std::env::var_os(REEXEC_MARKER).is_some() {
        return Ok(());
    }

    let mut paths: BTreeSet<String> = std::env::var("LD_LIBRARY_PATH")
        .ok()
        .into_iter()
        .flat_map(|value| value.split(':').map(str::to_owned).collect::<Vec<_>>())
        .filter(|value| !value.is_empty())
        .collect();

    for dir in linux_gui_library_dirs()? {
        paths.insert(dir);
    }

    if paths.is_empty() {
        return Ok(());
    }

    let joined = paths.into_iter().collect::<Vec<_>>().join(":");
    let current = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
    if joined == current {
        return Ok(());
    }

    let exe = std::env::current_exe().context("failed to resolve current executable")?;
    let mut command = std::process::Command::new(exe);
    command.args(std::env::args_os().skip(1));
    command.env("LD_LIBRARY_PATH", joined);
    command.env(REEXEC_MARKER, "1");
    let err = command.exec();
    Err(anyhow!(err)).context("failed to re-exec emoji_billboard_native with GUI library path")
}

#[cfg(target_os = "linux")]
fn linux_gui_library_dirs() -> Result<Vec<String>> {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    let mut dirs = BTreeSet::new();

    for dir in [
        "/run/current-system/sw/lib",
        "/run/opengl-driver/lib",
        "/usr/lib",
        "/usr/lib64",
        "/lib",
        "/lib64",
    ] {
        if Path::new(dir).exists() {
            dirs.insert(dir.to_string());
        }
    }

    let nix_store = Path::new("/nix/store");
    if nix_store.exists() {
        let wanted = [
            "-wayland-",
            "-libX11-",
            "-libxcb-",
            "-libxkbcommon-",
            "-libXcursor-",
            "-libXi-",
            "-libXrandr-",
            "-libXrender-",
            "-libXext-",
        ];

        for entry in fs::read_dir(nix_store).context("failed to read /nix/store")? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !wanted.iter().any(|needle| name.contains(needle)) {
                continue;
            }

            let lib_dir: PathBuf = entry.path().join("lib");
            if lib_dir.is_dir() {
                dirs.insert(lib_dir.to_string_lossy().into_owned());
            }
        }
    }

    Ok(dirs.into_iter().collect())
}

#[cfg(not(target_os = "linux"))]
fn ensure_linux_gui_runtime_env() -> Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn preferred_backend(choice: BackendChoice) -> Option<BackendChoice> {
    match choice {
        BackendChoice::Auto => {
            let has_x11 = std::env::var_os("DISPLAY").is_some();
            let has_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
            match std::env::var("XDG_SESSION_TYPE").ok().as_deref() {
                Some("wayland") if has_wayland => Some(BackendChoice::Wayland),
                Some("x11") if has_x11 => Some(BackendChoice::X11),
                _ => match (has_wayland, has_x11) {
                    (true, _) => Some(BackendChoice::Wayland),
                    (false, true) => Some(BackendChoice::X11),
                    (false, false) => None,
                },
            }
        }
        explicit => Some(explicit),
    }
}

#[cfg(not(target_os = "linux"))]
fn preferred_backend(choice: BackendChoice) -> Option<BackendChoice> {
    let _ = choice;
    Some(BackendChoice::Auto)
}

fn configure_event_loop(builder: &mut EventLoopBuilder<()>, choice: BackendChoice) {
    #[cfg(target_os = "linux")]
    match preferred_backend(choice) {
        Some(BackendChoice::X11) => {
            use winit::platform::x11::EventLoopBuilderExtX11;
            builder.with_x11();
        }
        Some(BackendChoice::Wayland) => {
            use winit::platform::wayland::EventLoopBuilderExtWayland;
            builder.with_wayland();
        }
        Some(BackendChoice::Auto) | None => {}
    }
}
