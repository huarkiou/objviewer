// OBJ Viewer — lightweight Wavefront OBJ file viewer
// Ported from Godot/C# to Rust (winit + wgpu + egui)

#![windows_subsystem = "windows"]

use std::sync::Arc;

use clap::Parser;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes},
};

mod camera;
mod mesh;
mod parser;
mod renderer;
mod ui;

use camera::Camera;
use mesh::MeshData;
use renderer::Renderer;
use ui::{UiAction, UiState, ViewPreset};

// ─── CLI ────────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "objviewer", about = "Lightweight Wavefront OBJ file viewer")]
struct Cli {
    /// Path to the .obj file to view
    #[arg()]
    obj_file: Option<String>,
}

// ─── Application state ─────────────────────────────────────────────────────────

struct ObjViewerApp {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    ui_state: UiState,

    // egui
    egui_ctx: egui::Context,
    egui_winit: Option<egui_winit::State>,
    egui_renderer: Option<egui_wgpu::Renderer>,

    // Mouse orbit state
    mouse_pressed: bool,
    last_mouse: Option<(f64, f64)>,

    // OBJ path from CLI
    obj_path: Option<String>,
}

impl ObjViewerApp {
    fn new(obj_path: Option<String>) -> Self {
        Self {
            window: None,
            renderer: None,
            camera: Camera::default(),
            ui_state: UiState::default(),
            egui_ctx: egui::Context::default(),
            egui_winit: None,
            egui_renderer: None,
            mouse_pressed: false,
            last_mouse: None,
            obj_path,
        }
    }
}

impl ApplicationHandler for ObjViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // Create window centered on screen, with title bar
        let window_size = PhysicalSize::new(1024, 768);
        let window_attrs = if let Some(monitor) = event_loop.primary_monitor() {
            let msize = monitor.size();
            let x = (msize.width.saturating_sub(window_size.width) / 2) as i32;
            let y = (msize.height.saturating_sub(window_size.height) / 2) as i32;
            WindowAttributes::default()
                .with_title("OBJ Viewer")
                .with_inner_size(window_size)
                .with_position(winit::dpi::PhysicalPosition::new(x, y))
        } else {
            WindowAttributes::default()
                .with_title("OBJ Viewer")
                .with_inner_size(window_size)
        };
        let window = Arc::new(event_loop.create_window(window_attrs).unwrap());
        self.window = Some(window.clone());

        // Initialize renderer (blocking async)
        let mut renderer = pollster::block_on(Renderer::new(window.clone()));

        // Load OBJ file if path provided
        if let Some(ref path) = self.obj_path {
            match parser::ObjData::load(path) {
                Ok(obj_data) => {
                    let mesh = MeshData::from_obj(&obj_data);
                    renderer.upload_mesh(&mesh);
                    log::info!(
                        "Loaded OBJ: {} vertices, {} faces",
                        obj_data.positions.len(),
                        obj_data.faces.len()
                    );
                }
                Err(e) => {
                    log::error!("Failed to load OBJ file '{}': {}", path, e);
                }
            }
        }

        // Setup egui
        let viewport_id = self.egui_ctx.viewport_id();
        let egui_winit = egui_winit::State::new(
            self.egui_ctx.clone(),
            viewport_id,
            &window,
            None,
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &renderer.device,
            renderer.config.format,
            egui_wgpu::RendererOptions::default(),
        );

        self.renderer = Some(renderer);
        self.egui_winit = Some(egui_winit);
        self.egui_renderer = Some(egui_renderer);

        // Continuous rendering for smooth 3D interaction
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let window = match &self.window {
            Some(w) => w.clone(),
            None => return,
        };

        // Forward event to egui first (so UI can consume it)
        if let Some(ref mut egui_winit) = self.egui_winit {
            let _ = egui_winit.on_window_event(&window, &event);
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::KeyboardInput { event, .. }
                if event.state.is_pressed()
                    && event.physical_key == PhysicalKey::Code(KeyCode::Escape) =>
            {
                event_loop.exit();
            }

            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.mouse_pressed = state == ElementState::Pressed;
                if !self.mouse_pressed {
                    self.last_mouse = None;
                }
            }

            WindowEvent::CursorMoved { position, .. } if self.mouse_pressed => {
                if let Some((lx, ly)) = self.last_mouse {
                    let dx = (position.x - lx) as f32;
                    let dy = (position.y - ly) as f32;
                    self.camera.orbit(dx * 0.005, dy * -0.005);
                }
                self.last_mouse = Some((position.x, position.y));
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 0.05,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.001,
                };
                self.camera.zoom(scroll);
            }

            WindowEvent::Resized(new_size) => {
                if let Some(ref mut renderer) = self.renderer {
                    renderer.resize(new_size);
                }
            }

            WindowEvent::RedrawRequested => {
                self.render_frame(&window);
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Request redraw for continuous rendering
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }
}

impl ObjViewerApp {
    fn render_frame(&mut self, window: &Window) {
        // ─── Phase 1: Prepare egui UI (borrows self.ui_state only) ──────────
        let (raw_input, egui_ctx) = {
            let egui_winit = match &mut self.egui_winit {
                Some(s) => s,
                None => return,
            };
            (
                egui_winit.take_egui_input(window),
                egui_winit.egui_ctx().clone(),
            )
        };

        let mut pending_action = UiAction::None;
        let full_output = {
            let ui_state = &mut self.ui_state;
            egui_ctx.run_ui(raw_input, |ctx| {
                pending_action = ui_state.draw(ctx);
            })
        };

        // Process UI action and sync state
        self.handle_ui_action(pending_action);
        self.camera.orthographic = self.ui_state.orthographic;

        // ─── Phase 2: Rendering (borrows renderer) ──────────────────────────
        let renderer = match &mut self.renderer {
            Some(r) => r,
            None => return,
        };
        let egui_renderer = match &mut self.egui_renderer {
            Some(r) => r,
            None => return,
        };

        // ─── Update camera uniform ──────────────────────────────────────────
        renderer.update_camera(&self.camera);

        // ─── Acquire surface ────────────────────────────────────────────────
        let frame = match renderer.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                renderer.resize(renderer.size);
                return;
            }
            _ => return,
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });

        // ─── Render pass: solid mesh + wireframe (MSAA) ──────────────────────
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &renderer.msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.45,
                            g: 0.47,
                            b: 0.50,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &renderer.msaa_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                multiview_mask: None,
                ..Default::default()
            });

            renderer.draw_mesh(&mut pass);
            renderer.draw_wireframe(&mut pass);
        }

        // ─── Prepare egui textures (for future egui overlay) ────────────────
        let _screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [renderer.config.width, renderer.config.height],
            pixels_per_point: window.scale_factor() as f32,
        };

        for (id, delta) in &full_output.textures_delta.set {
            egui_renderer.update_texture(&renderer.device, &renderer.queue, *id, delta);
        }

        // TODO: egui render pass — needs wgpu lifetime workaround
        // Currently blocked by egui-wgpu requiring RenderPass<'static>

        // ─── Submit and present ─────────────────────────────────────────────
        renderer.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        // Clean up egui textures (renderer not used yet, but keep for future)
        for id in &full_output.textures_delta.free {
            egui_renderer.free_texture(id);
        }
    }

    fn handle_ui_action(&mut self, action: UiAction) {
        match action {
            UiAction::ToggleProjection => {
                self.camera.toggle_projection();
                self.ui_state.orthographic = self.camera.orthographic;
            }
            UiAction::SetView(preset) => match preset {
                ViewPreset::Default => self.camera.reset(),
                ViewPreset::Front => self.camera.view_front(),
                ViewPreset::Back => self.camera.view_back(),
                ViewPreset::Left => self.camera.view_left(),
                ViewPreset::Right => self.camera.view_right(),
                ViewPreset::Top => self.camera.view_top(),
                ViewPreset::Bottom => self.camera.view_bottom(),
            },
            UiAction::Quit => {
                std::process::exit(0);
            }
            UiAction::None => {}
        }
    }
}

// ─── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    let event_loop = EventLoop::new().unwrap();
    let mut app = ObjViewerApp::new(cli.obj_file);
    event_loop.run_app(&mut app).unwrap();
}
