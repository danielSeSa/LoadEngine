use std::sync::Arc;
use std::time::Duration;
use winit::window::Window;

use crate::editor::EditorState;
use crate::game::GameState;
use crate::modeling::ModelState;
use crate::render::{Link, SceneRenderer};
use crate::ui;

#[derive(Clone, Copy, PartialEq)]
pub enum Workspace {
    GameEngine,
    Modeling,
    CodeEditor,
}

/// داده‌های خالصِ موتور (بدون وابستگی به GPU)
pub struct AppState {
    pub active_workspace: Workspace,
    pub editor: EditorState,
    pub game: GameState,
    pub modeling: ModelState,
    /// پل بین UI و رندر GPU
    pub link: Link,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_workspace: Workspace::GameEngine,
            editor: EditorState::new(),
            game: GameState::new(),
            modeling: ModelState::new(),
            link: Link::new(),
        }
    }
}

/// وضعیت رندر (wgpu + egui) و داده‌های برنامه
pub struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    #[allow(dead_code)]
    size: winit::dpi::PhysicalSize<u32>,
    window: Arc<Window>,

    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    scene: SceneRenderer,
    repaint_delay: Duration,

    pub app: AppState,
}

impl State {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: if cfg!(windows) {
                wgpu::Backends::DX12
            } else {
                wgpu::Backends::PRIMARY
            },
            ..Default::default()
        });
        let surface = instance.create_surface(Arc::clone(&window)).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: if surface_caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::Opaque)
            {
                wgpu::CompositeAlphaMode::Opaque
            } else {
                surface_caps.alpha_modes[0]
            },
            desired_maximum_frame_latency: 1,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            None,
            None,
        );
        let mut egui_renderer = egui_wgpu::Renderer::new(&device, surface_format, None, 1);

        // رندرر GPU صحنه + ثبت Texture آن در egui (شناسه ثابت می‌ماند)
        let scene = SceneRenderer::new(&device);
        let tex = egui_renderer.register_native_texture(
            &device,
            scene.color_view(),
            wgpu::FilterMode::Linear,
        );
        let mut app = AppState::new();
        app.link.tex = tex;

        Self {
            surface,
            device,
            queue,
            config,
            size,
            window,
            egui_ctx,
            egui_state,
            egui_renderer,
            scene,
            repaint_delay: Duration::ZERO,
            app,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    /// true یعنی egui می‌خواهد فریم جدید رسم شود
    pub fn handle_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        self.egui_state.on_window_event(&self.window, event).repaint
    }

    pub fn repaint_delay(&self) -> Duration {
        self.repaint_delay
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let raw_input = self.egui_state.take_egui_input(&self.window);
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            ui::draw(ctx, &mut self.app);
        });
        self.repaint_delay = full_output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |v| v.repaint_delay);

        let clipped_primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Encoder"),
            });

        // ۱) رندر GPU صحنه (فقط اگر UI درخواست داده باشد)
        if self.app.link.req.valid {
            self.app.link.req.valid = false;
            let res = self.scene.render(
                &self.device,
                &self.queue,
                &mut encoder,
                &self.app.link.req,
                &self.app.game.meshes,
            );
            if res.resized {
                self.egui_renderer.update_egui_texture_from_wgpu_texture(
                    &self.device,
                    self.scene.color_view(),
                    wgpu::FilterMode::Linear,
                    self.app.link.tex,
                );
            }
            self.app.link.tris = res.tris;
            self.app.link.draw_calls = res.draw_calls;
        }

        // ۲) رسم egui
        for (id, delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, delta);
        }
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.12,
                            g: 0.12,
                            b: 0.14,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            self.egui_renderer
                .render(&mut render_pass, &clipped_primitives, &screen_descriptor);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        self.egui_state
            .handle_platform_output(&self.window, full_output.platform_output);

        Ok(())
    }
}