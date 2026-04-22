use std::cell::RefCell;

use emoji_renderer::gpu::{emoji_preview_scene_params, GpuRenderer};
use emoji_renderer::texture::{COLOR_SOURCE_ALPHA_THRESHOLD, fill_transparent_rgb_from_nearest};
use js_sys::{Object, Reflect};
use wasm_bindgen::prelude::*;
use web_sys::{Element, HtmlCanvasElement};

mod gallery;
mod terminal_renderer;

use terminal_renderer::{TERM_COLS, TERM_ROWS, TerminalGrid, TerminalRenderer};

const COMPOSITE_SHADER: &str = include_str!("composite.wgsl");

#[derive(Clone, Copy)]
struct TransferTuning {
    linear_gain: f32,
    gamma: f32,
    lift: f32,
    saturation: f32,
}

#[derive(Clone, Copy)]
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

thread_local! {
    static TRANSFER_TUNING: RefCell<TransferTuning> = RefCell::new(TransferTuning::default());
    static PERF_TOGGLES: RefCell<PerfToggles> = RefCell::new(PerfToggles::default());
}

#[wasm_bindgen]
pub fn set_transfer_tuning(linear_gain: f32, gamma: f32, lift: f32, saturation: f32) {
    TRANSFER_TUNING.with(|t| {
        *t.borrow_mut() = TransferTuning {
            linear_gain,
            gamma,
            lift,
            saturation,
        };
    });
}

#[wasm_bindgen]
pub fn set_perf_toggles(
    crt_enabled: bool,
    transfer_enabled: bool,
    overlay_filter_enabled: bool,
    billboard_enabled: bool,
) {
    PERF_TOGGLES.with(|t| {
        *t.borrow_mut() = PerfToggles {
            crt: crt_enabled,
            transfer: transfer_enabled,
            overlay_filter: overlay_filter_enabled,
            billboard: billboard_enabled,
        };
    });
}

#[wasm_bindgen(start)]
pub fn start() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    wasm_bindgen_futures::spawn_local(run());
}

async fn run() {
    let app = match App::init().await {
        Ok(app) => app,
        Err(err) => {
            web_sys::console::error_1(&format!("init failed: {err}").into());
            return;
        }
    };

    let app = std::rc::Rc::new(std::cell::RefCell::new(app));
    let window = web_sys::window().unwrap();

    {
        let app = app.clone();
        let keydown = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::wrap(Box::new(
            move |event: web_sys::KeyboardEvent| {
                let action = match event.key().as_str() {
                    "ArrowUp" => Some(gallery::KeyAction::Up),
                    "ArrowDown" => Some(gallery::KeyAction::Down),
                    "Enter" => Some(gallery::KeyAction::Enter),
                    "Escape" => Some(gallery::KeyAction::Escape),
                    "Backspace" => Some(gallery::KeyAction::Backspace),
                    key if key.len() == 1 => {
                        let ch = key.chars().next().unwrap();
                        if ch.is_ascii_graphic() || ch == ' ' {
                            Some(gallery::KeyAction::Char(ch))
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(action) = action {
                    event.prevent_default();
                    app.borrow_mut().handle_key(action);
                }
            },
        ));
        window
            .add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())
            .unwrap();
        keydown.forget();
    }

    let cb: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let cb_clone = cb.clone();

    *cb_clone.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        {
            let mut app = app.borrow_mut();
            if let Err(err) = app.frame() {
                web_sys::console::error_1(&format!("frame error: {err}").into());
                return;
            }
        }
        window
            .request_animation_frame(cb.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }) as Box<dyn FnMut()>));

    let window2 = web_sys::window().unwrap();
    window2
        .request_animation_frame(
            cb_clone
                .borrow()
                .as_ref()
                .unwrap()
                .as_ref()
                .unchecked_ref(),
        )
        .unwrap();
}

struct App {
    renderer: GpuRenderer,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    canvas: HtmlCanvasElement,
    composite_pipeline: wgpu::RenderPipeline,
    composite_bind_group_layout: wgpu::BindGroupLayout,
    composite_bind_group: wgpu::BindGroup,
    composite_uses_billboard: bool,
    composite_uniform_buffer: wgpu::Buffer,
    overlay_sampler: wgpu::Sampler,
    start_time: f64,
    demo_pixels: Vec<[u8; 4]>,
    demo_w: u32,
    demo_h: u32,
    gallery: gallery::Gallery,
    terminal_renderer: TerminalRenderer,
    terminal_grid: TerminalGrid,
    terminal_dirty: bool,
    billboard_sampler: wgpu::Sampler,
    placeholder_billboard_view: wgpu::TextureView,
    fps_overlay: Element,
    last_time_secs: f64,
    smoothed_fps: f32,
    last_fps_label: u32,
    last_blink_on: bool,
    last_preview_overlay_visible: bool,
    smoothed_terminal_ms: f32,
    smoothed_scene_ms: f32,
    smoothed_composite_ms: f32,
    last_perf_label: String,
    frame_counter: u32,
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

impl App {
    async fn init() -> anyhow::Result<Self> {
        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        let canvas: HtmlCanvasElement = document
            .get_element_by_id("emoji-canvas")
            .expect("no #emoji-canvas element")
            .dyn_into()
            .unwrap();
        let fps_overlay = document
            .get_element_by_id("fps-counter")
            .expect("no #fps-counter element");

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });

        let surface_target = wgpu::SurfaceTarget::Canvas(canvas.clone());
        let surface = instance.create_surface(surface_target)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("no WebGPU adapter"))?;

        let adapter_limits = adapter.limits();
        let features = wgpu::Features::empty();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("emoji_web"),
                    required_features: features,
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
            GpuRenderer::from_device_queue(device, queue, features, linear_depth_format)?;

        let caps = surface.get_capabilities(&adapter);
        let format = preferred_surface_format(&caps.formats);

        let w = canvas.client_width().max(1) as u32;
        let h = canvas.client_height().max(1) as u32;
        canvas.set_width(w);
        canvas.set_height(h);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(renderer.device(), &config);

        let shader = renderer
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("composite_shader"),
                source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
            });

        let composite_bind_group_layout =
            renderer
                .device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("composite_bgl"),
                    entries: &[
                        bgl_texture(0),
                        bgl_sampler(1),
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
                        bgl_texture(3),
                        bgl_sampler(4),
                    ],
                });

        let composite_uniform_buffer =
            renderer
                .device()
                .create_buffer(&wgpu::BufferDescriptor {
                    label: Some("composite_uniforms"),
                    size: std::mem::size_of::<CompositeUniforms>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });

        let pipeline_layout =
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
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
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

        let overlay_sampler =
            renderer
                .device()
                .create_sampler(&wgpu::SamplerDescriptor {
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    ..Default::default()
                });

        let billboard_sampler =
            renderer
                .device()
                .create_sampler(&wgpu::SamplerDescriptor {
                    mag_filter: wgpu::FilterMode::Nearest,
                    min_filter: wgpu::FilterMode::Nearest,
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    ..Default::default()
                });

        let terminal_renderer = TerminalRenderer::new(renderer.device(), renderer.queue())?;
        let terminal_grid = TerminalGrid::new();
        let (_placeholder_billboard_tex, placeholder_billboard_view) =
            create_rgba_texture(renderer.device(), 1, 1);
        let composite_bind_group =
            renderer
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("composite_bg"),
                    layout: &composite_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                terminal_renderer.texture_view(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&overlay_sampler),
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
                            resource: wgpu::BindingResource::Sampler(&billboard_sampler),
                        },
                    ],
                });
        let (demo_pixels, demo_w, demo_h) = demo_texture();
        let gallery = gallery::Gallery::new();
        let start_time = web_sys::window().unwrap().performance().unwrap().now();

        Ok(Self {
            renderer,
            surface,
            config,
            canvas,
            composite_pipeline,
            composite_bind_group_layout,
            composite_bind_group,
            composite_uses_billboard: false,
            composite_uniform_buffer,
            overlay_sampler,
            start_time,
            demo_pixels,
            demo_w,
            demo_h,
            gallery,
            terminal_renderer,
            terminal_grid,
            terminal_dirty: true,
            billboard_sampler,
            placeholder_billboard_view,
            fps_overlay,
            last_time_secs: 0.0,
            smoothed_fps: 60.0,
            last_fps_label: 60,
            last_blink_on: true,
            last_preview_overlay_visible: false,
            smoothed_terminal_ms: 0.0,
            smoothed_scene_ms: 0.0,
            smoothed_composite_ms: 0.0,
            last_perf_label: String::new(),
            frame_counter: 0,
        })
    }

    fn handle_key(&mut self, action: gallery::KeyAction) {
        self.gallery.handle_key(action);
        self.terminal_dirty = true;
    }

    fn frame(&mut self) -> anyhow::Result<()> {
        self.frame_counter = self.frame_counter.wrapping_add(1);
        let perf = web_sys::window().unwrap().performance().unwrap();
        let now = perf.now();
        let elapsed_ms = now - self.start_time;
        let time_secs = elapsed_ms / 1000.0;
        let dt_secs = (time_secs - self.last_time_secs).max(0.0);
        self.last_time_secs = time_secs;
        self.gallery.tick(dt_secs as f32);

        if dt_secs > 0.0 {
            let fps = (1.0 / dt_secs) as f32;
            self.smoothed_fps = self.smoothed_fps * 0.9 + fps * 0.1;
        }
        let fps_label = self.smoothed_fps.round().clamp(0.0, 999.0) as u32;
        if fps_label != self.last_fps_label {
            self.last_fps_label = fps_label;
        }

        let w = self.canvas.client_width().max(1) as u32;
        let h = self.canvas.client_height().max(1) as u32;
        if w != self.config.width || h != self.config.height {
            self.canvas.set_width(w);
            self.canvas.set_height(h);
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(self.renderer.device(), &self.config);
        }

        let blink_on = gallery::cursor_blink_on(time_secs);
        let preview_overlay_visible = gallery::show_preview_overlay(&self.gallery);
        if blink_on != self.last_blink_on
            || preview_overlay_visible != self.last_preview_overlay_visible
        {
            self.last_blink_on = blink_on;
            self.last_preview_overlay_visible = preview_overlay_visible;
            self.terminal_dirty = true;
        }

        let terminal_redrew = self.terminal_dirty;
        let term_start = perf.now();
        if self.terminal_dirty {
            gallery::render_to_grid(&mut self.terminal_grid, &self.gallery, time_secs);
            self.terminal_renderer
                .render(self.renderer.device(), self.renderer.queue(), &self.terminal_grid);
            self.terminal_dirty = false;
        }
        let terminal_ms = (perf.now() - term_start) as f32;

        let previewing = self.gallery.is_previewing();
        let preview_mix = self.gallery.preview_mix();
        let terminal_cols = TERM_COLS as f32;
        let terminal_rows = TERM_ROWS as f32;
        let transfer = TRANSFER_TUNING.with(|t| *t.borrow());
        let perf_toggles = PERF_TOGGLES.with(|t| *t.borrow());

        let overlay_w = self.terminal_renderer.pixel_width();
        let overlay_h = self.terminal_renderer.pixel_height();
        let scene_start = perf.now();
        let billboard_pixel_rect: [f32; 4] = if previewing && perf_toggles.billboard {
            if let Some(cell_rect) = self.gallery.billboard_cell_rect(TERM_COLS, TERM_ROWS) {
                if overlay_w > 4 && overlay_h > 4 {
                    let native_w = cell_rect.width as f32;
                    let native_h = (cell_rect.height as f32) * 2.0;
                    let target_max_dim = 180.0f32;
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
                    params.render_scale = Some(2.0);
                    self.renderer.render_to_offscreen_params(
                        &texture,
                        render_w,
                        render_h,
                        time_secs,
                        &params,
                    )?;
                }

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
        let scene_ms = (perf.now() - scene_start) as f32;

        let term_aspect = overlay_w as f32 / overlay_h as f32;
        let canvas_aspect = w as f32 / h as f32;
        let (term_w, term_h) = if canvas_aspect > term_aspect {
            let th = h as f32;
            (th * term_aspect, th)
        } else {
            let tw = w as f32;
            (tw, tw / term_aspect)
        };
        let term_x = (w as f32 - term_w) * 0.5;
        let term_y = (h as f32 - term_h) * 0.5;
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
            output_size: [w as f32, h as f32],
            time_secs: time_secs as f32,
            preview_mix,
            terminal_rect,
            billboard_rect: billboard_canvas_rect,
            terminal_grid: [
                terminal_cols,
                terminal_rows,
                term_w / terminal_cols,
                term_h / terminal_rows,
            ],
            transfer_tuning: [
                transfer.linear_gain,
                transfer.gamma,
                transfer.lift,
                transfer.saturation,
            ],
            perf_toggles: [
                if perf_toggles.crt { 1.0 } else { 0.0 },
                if perf_toggles.transfer { 1.0 } else { 0.0 },
                if perf_toggles.overlay_filter { 1.0 } else { 0.0 },
                if perf_toggles.billboard { 1.0 } else { 0.0 },
            ],
        };
        self.renderer.queue().write_buffer(
            &self.composite_uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );

        let uses_billboard = previewing && preview_mix > 0.0 && billboard_canvas_rect[2] > 0.0;
        self.ensure_composite_bind_group(uses_billboard);

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let composite_start = perf.now();
        let mut encoder =
            self.renderer
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("composite_encoder"),
                });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite_pass"),
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

        self.renderer.queue().submit(Some(encoder.finish()));
        output.present();
        let composite_ms = (perf.now() - composite_start) as f32;

        self.smoothed_terminal_ms = self.smoothed_terminal_ms * 0.85 + terminal_ms * 0.15;
        self.smoothed_scene_ms = self.smoothed_scene_ms * 0.85 + scene_ms * 0.15;
        self.smoothed_composite_ms = self.smoothed_composite_ms * 0.85 + composite_ms * 0.15;

        let perf_label = format!(
            "{fps_label:>3} FPS\nTERM {:>4.1} MS{}\n3D   {:>4.1} MS{}\nCOMP {:>4.1} MS",
            self.smoothed_terminal_ms,
            if terminal_redrew { " *" } else { "  " },
            self.smoothed_scene_ms,
            if previewing { " *" } else { "  " },
            self.smoothed_composite_ms,
        );
        if perf_label != self.last_perf_label {
            self.last_perf_label = perf_label.clone();
            self.fps_overlay.set_text_content(Some(&perf_label));
        }
        self.publish_perf_metrics(
            fps_label,
            previewing,
            terminal_redrew,
            overlay_w,
            overlay_h,
            terminal_ms,
            scene_ms,
            composite_ms,
            perf_toggles,
        );

        Ok(())
    }

    fn publish_perf_metrics(
        &self,
        fps_label: u32,
        previewing: bool,
        terminal_redrew: bool,
        overlay_w: u32,
        overlay_h: u32,
        terminal_ms: f32,
        scene_ms: f32,
        composite_ms: f32,
        perf_toggles: PerfToggles,
    ) {
        let perf = Object::new();
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("frame"),
            &JsValue::from_f64(self.frame_counter as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("fps"),
            &JsValue::from_f64(self.smoothed_fps as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("fpsLabel"),
            &JsValue::from_f64(fps_label as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("previewing"),
            &JsValue::from_bool(previewing),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("terminalRedrew"),
            &JsValue::from_bool(terminal_redrew),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("terminalMs"),
            &JsValue::from_f64(terminal_ms as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("sceneMs"),
            &JsValue::from_f64(scene_ms as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("compositeMs"),
            &JsValue::from_f64(composite_ms as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("smoothedTerminalMs"),
            &JsValue::from_f64(self.smoothed_terminal_ms as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("smoothedSceneMs"),
            &JsValue::from_f64(self.smoothed_scene_ms as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("smoothedCompositeMs"),
            &JsValue::from_f64(self.smoothed_composite_ms as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("canvasWidth"),
            &JsValue::from_f64(self.config.width as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("canvasHeight"),
            &JsValue::from_f64(self.config.height as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("terminalWidth"),
            &JsValue::from_f64(overlay_w as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("terminalHeight"),
            &JsValue::from_f64(overlay_h as f64),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("crtEnabled"),
            &JsValue::from_bool(perf_toggles.crt),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("transferEnabled"),
            &JsValue::from_bool(perf_toggles.transfer),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("overlayFilterEnabled"),
            &JsValue::from_bool(perf_toggles.overlay_filter),
        );
        let _ = Reflect::set(
            &perf,
            &JsValue::from_str("billboardEnabled"),
            &JsValue::from_bool(perf_toggles.billboard),
        );

        if let Some(window) = web_sys::window() {
            let _ = Reflect::set(window.as_ref(), &JsValue::from_str("__emojiPerf"), &perf);
        }
    }

    fn ensure_composite_bind_group(&mut self, uses_billboard: bool) {
        if self.composite_uses_billboard == uses_billboard {
            return;
        }

        let billboard_view = if uses_billboard {
            self.renderer
                .offscreen_view()
                .unwrap_or(&self.placeholder_billboard_view)
        } else {
            &self.placeholder_billboard_view
        };

        self.composite_bind_group =
            self.renderer
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("composite_bg"),
                    layout: &self.composite_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                self.terminal_renderer.texture_view(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.overlay_sampler),
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
                            resource: wgpu::BindingResource::Sampler(&self.billboard_sampler),
                        },
                    ],
                });
        self.composite_uses_billboard = uses_billboard;
    }
}

fn create_rgba_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("overlay_texture"),
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

fn preferred_surface_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    for format in formats {
        match format {
            wgpu::TextureFormat::Bgra8Unorm => return *format,
            wgpu::TextureFormat::Rgba8Unorm => return *format,
            _ => {}
        }
    }

    for format in formats {
        match format {
            wgpu::TextureFormat::Bgra8UnormSrgb => return wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb => return wgpu::TextureFormat::Rgba8Unorm,
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

fn demo_texture() -> (Vec<[u8; 4]>, u32, u32) {
    let w = 96u32;
    let h = 96u32;
    let mut pixels = vec![[0u8, 0, 0, 0]; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            let inside = x > 12 && x < 84 && y > 12 && y < 84;
            pixels[idx] = if inside {
                let dx = x as f32 / w as f32;
                let dy = y as f32 / h as f32;
                if x < w / 2 {
                    [255, (80.0 + dy * 80.0) as u8, 70, 255]
                } else {
                    [70, (120.0 + dx * 40.0) as u8, 255, 255]
                }
            } else {
                [0, 0, 0, 0]
            };
        }
    }
    fill_transparent_rgb_from_nearest(&mut pixels, w, h, COLOR_SOURCE_ALPHA_THRESHOLD);
    (pixels, w, h)
}

mod console_error_panic_hook {
    use std::panic;

    pub fn hook(info: &panic::PanicHookInfo<'_>) {
        web_sys::console::error_1(&format!("{info}").into());
    }
}
