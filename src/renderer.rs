// WGPU renderer — base infrastructure + mesh rendering with Blinn-Phong lighting.
// The clear-colour loop is kept for fallback; real rendering goes through render_mesh().

use std::sync::Arc;

use glam::Mat4;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::camera::Camera;
use crate::mesh::{MeshData, Vertex};

// ─── GPU-compatible uniform structs ───────────────────────────────────────────
// Each mirrors a WGSL struct in shader.wgsl.  #[repr(C)] and the explicit
// _pad fields guarantee the exact byte-level layout the GPU expects.

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub camera_position: [f32; 3],
    pub _pad: [f32; 1],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightData {
    pub position: [f32; 3],
    pub _pad1: [f32; 1],
    pub color: [f32; 3],
    pub energy: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform {
    pub lights: [LightData; 3],
    pub num_lights: u32,
    pub _pad2: [f32; 3],
    pub ambient: [f32; 3],
    pub _pad3: [f32; 1],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform {
    pub albedo: [f32; 3],
    pub _pad1: [f32; 1],
    pub metallic: f32,
    pub roughness: f32,
    pub _pad2: [f32; 2],
}

// ─── Renderer ─────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct Renderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub window: Arc<Window>,

    // Mesh rendering
    pub render_pipeline: wgpu::RenderPipeline,
    pub line_pipeline: wgpu::RenderPipeline,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub msaa_texture: wgpu::Texture,
    pub msaa_view: wgpu::TextureView,
    pub msaa_depth: wgpu::Texture,
    pub msaa_depth_view: wgpu::TextureView,
    pub sample_count: u32,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub light_buffer: wgpu::Buffer,
    pub light_bind_group: wgpu::BindGroup,
    pub material_buffer: wgpu::Buffer,
    pub material_bind_group: wgpu::BindGroup,
    pub vertex_buffer: Option<wgpu::Buffer>,
    pub index_buffer: Option<wgpu::Buffer>,
    pub index_count: u32,
    pub line_index_buffer: Option<wgpu::Buffer>,
    pub line_index_count: u32,
}

impl Renderer {
    /// Create a WGPU renderer attached to `window`.
    ///
    /// This is async because adapter and device requests are non-blocking futures.
    /// Callers should use `pollster::block_on(Renderer::new(window))`.
    pub async fn new(window: Arc<Window>) -> Self {
        // ---- wgpu instance & surface ----
        #[cfg(target_os = "windows")]
        let backends = wgpu::Backends::DX12;
        #[cfg(not(target_os = "windows"))]
        let backends = wgpu::Backends::VULKAN;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create wgpu surface — is the window backend supported?");

        // ---- adapter (GPU selection) ----
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await
            .expect("Failed to find a suitable GPU adapter — is a GPU available?");

        // ---- device + queue ----
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .expect("Failed to create GPU device");

        // ---- MSAA sample count ----
        let sample_count = 4;
        let multisample = wgpu::MultisampleState {
            count: sample_count,
            ..Default::default()
        };

        // ---- surface configuration ----
        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps.formats[0];
        let alpha_mode = surface_caps
            .alpha_modes
            .iter()
            .copied()
            .find(|&m| m == wgpu::CompositeAlphaMode::PreMultiplied)
            .unwrap_or(surface_caps.alpha_modes[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        // ---- shader ----
        let shader_source = include_str!("shader.wgsl");
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Blinn-Phong shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        // ---- bind-group layouts (one per uniform block) ----
        let uniform_bgl_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera BGL"),
            entries: &[uniform_bgl_entry],
        });

        let light_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Light BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                ..uniform_bgl_entry
            }],
        });

        let material_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Material BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                ..uniform_bgl_entry
            }],
        });

        // ---- pipeline layout ----
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline layout"),
            bind_group_layouts: &[Some(&camera_bgl), Some(&light_bgl), Some(&material_bgl)],
            immediate_size: 0,
        });

        // ---- render pipeline ----
        let render_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Mesh render pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader_module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            // position @ location 0, offset 0
                            wgpu::VertexAttribute {
                                offset: 0,
                                format: wgpu::VertexFormat::Float32x3,
                                shader_location: 0,
                            },
                            // normal @ location 1, offset 12 (3 × f32)
                            wgpu::VertexAttribute {
                                offset: std::mem::size_of::<[f32; 3]>()
                                    as wgpu::BufferAddress,
                                format: wgpu::VertexFormat::Float32x3,
                                shader_location: 1,
                            },
                        ],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader_module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample,
                multiview_mask: None,
                cache: None,
            });

        // ---- line (wireframe overlay) pipeline ----
        let line_shader_source = include_str!("shader_line.wgsl");
        let line_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Line shader"),
            source: wgpu::ShaderSource::Wgsl(line_shader_source.into()),
        });

        // Line pipeline only needs camera bind group (group 0)
        let line_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Line pipeline layout"),
                bind_group_layouts: &[Some(&camera_bgl)],
                immediate_size: 0,
            });

        let line_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Line pipeline"),
                layout: Some(&line_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &line_shader_module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>()
                            as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                offset: 0,
                                format: wgpu::VertexFormat::Float32x3,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                offset: std::mem::size_of::<[f32; 3]>()
                                    as wgpu::BufferAddress,
                                format: wgpu::VertexFormat::Float32x3,
                                shader_location: 1,
                            },
                        ],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &line_shader_module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::LineList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample,
                multiview_mask: None,
                cache: None,
            });

        // ---- depth texture ----
        let (depth_texture, depth_view) = Self::create_depth_texture(&device, &config, 1);

        // ---- MSAA textures ----
        let (msaa_texture, msaa_view) =
            Self::create_msaa_color(&device, &config, sample_count);
        let (msaa_depth, msaa_depth_view) =
            Self::create_depth_texture(&device, &config, sample_count);

        // ---- uniform buffers ----
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera buffer"),
            size: std::mem::size_of::<CameraUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let light_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Light buffer"),
            size: std::mem::size_of::<LightUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let material_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Material buffer"),
            size: std::mem::size_of::<MaterialUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ---- bind groups ----
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let light_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Light bind group"),
            layout: &light_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buffer.as_entire_binding(),
            }],
        });

        let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material bind group"),
            layout: &material_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: material_buffer.as_entire_binding(),
            }],
        });

        // ---- initialise material uniform (NX-style amber) ----
        {
            let material = MaterialUniform {
                albedo: [0.88, 0.68, 0.22],
                _pad1: [0.0],
                metallic: 0.02,
                roughness: 0.35,
                _pad2: [0.0, 0.0],
            };
            queue.write_buffer(&material_buffer, 0, bytemuck::cast_slice(&[material]));
        }

        // ---- initialise light uniform (CAD-style flat even lighting) ----
        {
            let light = LightUniform {
                lights: [
                    // Key: soft top-front-right
                    LightData {
                        position: [3.0, 3.0, 2.0],
                        _pad1: [0.0],
                        color: [1.0, 0.98, 0.95],
                        energy: 0.45,
                    },
                    // Fill: top-back-left
                    LightData {
                        position: [-2.0, 2.5, -2.0],
                        _pad1: [0.0],
                        color: [0.95, 0.97, 1.0],
                        energy: 0.35,
                    },
                    // Bottom fill: reduce harsh shadows
                    LightData {
                        position: [1.0, -2.0, 1.0],
                        _pad1: [0.0],
                        color: [1.0, 1.0, 1.0],
                        energy: 0.25,
                    },
                ],
                num_lights: 3,
                _pad2: [0.0, 0.0, 0.0],
                ambient: [0.65, 0.65, 0.65],
                _pad3: [0.0],
            };
            queue.write_buffer(&light_buffer, 0, bytemuck::cast_slice(&[light]));
        }

        Self {
            surface,
            device,
            queue,
            config,
            size,
            window,
            render_pipeline,
            line_pipeline,
            depth_texture,
            depth_view,
            msaa_texture,
            msaa_view,
            msaa_depth,
            msaa_depth_view,
            sample_count,
            camera_buffer,
            camera_bind_group,
            light_buffer,
            light_bind_group,
            material_buffer,
            material_bind_group,
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            line_index_buffer: None,
            line_index_count: 0,
        }
    }

    /// Handle window resize.
    ///
    /// Ignores zero-area sizes (minimised) to avoid validation errors.
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);

        let (t, v) = Self::create_depth_texture(&self.device, &self.config, 1);
        self.depth_texture = t;
        self.depth_view = v;

        let (t, v) = Self::create_msaa_color(&self.device, &self.config, self.sample_count);
        self.msaa_texture = t;
        self.msaa_view = v;

        let (t, v) = Self::create_depth_texture(&self.device, &self.config, self.sample_count);
        self.msaa_depth = t;
        self.msaa_depth_view = v;
    }

    /// Clear the surface to a dark-grey colour and present it.
    ///
    /// This is the simplest possible render loop — no mesh, no shader, no egui.
    /// Returns `Err(wgpu::SurfaceError)` when the surface is lost or out of date,
    /// which the caller (main loop) should handle by requesting a redraw.
    #[allow(dead_code)]
    pub fn render_clear(&mut self) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => return,
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("clear encoder"),
            });

        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
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
                depth_stencil_attachment: None,
                multiview_mask: None,
                ..Default::default()
            });
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }

    /// Upload mesh geometry to GPU buffers.
    ///
    /// Creates vertex and index buffers from `MeshData` and stores the index
    /// count.  Safe to call multiple times (old buffers are replaced).
    pub fn upload_mesh(&mut self, mesh: &MeshData) {
        self.vertex_buffer = Some(self.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Vertex buffer"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            },
        ));
        self.index_buffer = Some(self.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Index buffer"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            },
        ));
        self.index_count = mesh.indices.len() as u32;

        // Store mesh data for silhouette recomputation
        let mesh_vertices: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
        let mesh_indices = mesh.indices.clone();

        // Build sharp edge indices (dihedral angle threshold)
        if mesh_indices.len() >= 3 {
            use std::collections::HashMap;

            let threshold_rad = 30.0_f32.to_radians();

            let mut face_normals: Vec<[f32; 3]> = Vec::new();
            for tri in mesh_indices.chunks(3) {
                if tri.len() == 3 {
                    let p0 = mesh_vertices[tri[0] as usize];
                    let p1 = mesh_vertices[tri[1] as usize];
                    let p2 = mesh_vertices[tri[2] as usize];
                    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
                    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
                    let nx = e1[1] * e2[2] - e1[2] * e2[1];
                    let ny = e1[2] * e2[0] - e1[0] * e2[2];
                    let nz = e1[0] * e2[1] - e1[1] * e2[0];
                    let len = (nx * nx + ny * ny + nz * nz).sqrt();
                    if len > 1e-10 {
                        face_normals.push([nx / len, ny / len, nz / len]);
                    } else {
                        face_normals.push([0.0, 1.0, 0.0]);
                    }
                }
            }

            let mut edge_faces: HashMap<(u32, u32), Vec<[f32; 3]>> = HashMap::new();
            for (face_idx, tri) in mesh_indices.chunks(3).enumerate() {
                if tri.len() < 3 || face_idx >= face_normals.len() {
                    continue;
                }
                let fnorm = face_normals[face_idx];
                let edges = [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])];
                for &(a, b) in &edges {
                    let key = if a < b { (a, b) } else { (b, a) };
                    edge_faces.entry(key).or_default().push(fnorm);
                }
            }

            let mut line_indices: Vec<u32> = Vec::new();
            for ((a, b), fnorms) in &edge_faces {
                let is_sharp = match fnorms.len() {
                    0 => false,
                    1 => true,
                    2 => {
                        let n0 = fnorms[0];
                        let n1 = fnorms[1];
                        let dot = (n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2]).clamp(-1.0, 1.0);
                        dot.acos() > threshold_rad
                    }
                    _ => true,
                };
                if is_sharp {
                    line_indices.push(*a);
                    line_indices.push(*b);
                }
            }

            if !line_indices.is_empty() {
                self.line_index_buffer = Some(self.device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Line index buffer"),
                        contents: bytemuck::cast_slice(&line_indices),
                        usage: wgpu::BufferUsages::INDEX,
                    },
                ));
                self.line_index_count = line_indices.len() as u32;
            } else {
                self.line_index_buffer = None;
                self.line_index_count = 0;
            }
        }
    }

    /// Update the camera uniform buffer for the current frame.
    ///
    /// Computes the combined view-projection matrix (with OpenGL→WGPU NDC
    /// conversion) and writes it (plus the camera world position) to the GPU
    /// buffer.
    pub fn update_camera(&mut self, camera: &Camera) {
        // Camera world position = eye computed from orbit parameters.
        let eye = glam::Vec3::new(
            camera.distance * camera.pitch.cos() * camera.yaw.sin(),
            camera.distance * camera.pitch.sin(),
            camera.distance * camera.pitch.cos() * camera.yaw.cos(),
        );

        // glam's perspective/orthographic matrices produce OpenGL NDC (z ∈ [-1, 1]).
        // wgpu expects z ∈ [0, 1] — convert before uploading.
        let opengl_to_wgpu = Mat4::from_cols_array_2d(&[
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.5, 0.0],
            [0.0, 0.0, 0.5, 1.0],
        ]);

        let view_proj = opengl_to_wgpu * camera.projection_matrix() * camera.view_matrix();

        let camera_uniform = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            camera_position: [eye.x, eye.y, eye.z],
            _pad: [0.0],
        };

        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[camera_uniform]));
    }

    /// Render the currently loaded mesh with Blinn-Phong lighting.
    ///
    /// If no mesh has been uploaded, this only clears the screen (same
    /// appearance as `render_clear`).  Otherwise, it draws the mesh using all
    /// three bind groups (camera, lights, material) and a depth buffer.
    ///
    /// Returns `Err(wgpu::SurfaceError)` when the surface is lost or out of
    /// date, which the caller should handle by requesting a redraw.
    #[allow(dead_code)]
    pub fn render_mesh(&mut self) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => return,
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mesh render encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mesh render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
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
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                multiview_mask: None,
                ..Default::default()
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_bind_group(1, &self.light_bind_group, &[]);
            render_pass.set_bind_group(2, &self.material_bind_group, &[]);

            if let (Some(vb), Some(ib)) = (&self.vertex_buffer, &self.index_buffer) {
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.index_count, 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }

    /// Draw the mesh into an existing render pass (caller manages the encoder/surface lifecycle).
    pub fn draw_mesh<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_bind_group(1, &self.light_bind_group, &[]);
        render_pass.set_bind_group(2, &self.material_bind_group, &[]);

        if let (Some(vb), Some(ib)) = (&self.vertex_buffer, &self.index_buffer) {
            render_pass.set_vertex_buffer(0, vb.slice(..));
            render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.index_count, 0, 0..1);
        }
    }

    /// Draw wireframe edge overlay (call after draw_mesh in same render pass).
    pub fn draw_wireframe<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if self.vertex_buffer.is_none() || self.line_index_buffer.is_none() {
            return;
        }
        render_pass.set_pipeline(&self.line_pipeline);
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_vertex_buffer(
            0,
            self.vertex_buffer.as_ref().unwrap().slice(..),
        );
        render_pass.set_index_buffer(
            self.line_index_buffer.as_ref().unwrap().slice(..),
            wgpu::IndexFormat::Uint32,
        );
        render_pass.draw_indexed(0..self.line_index_count, 0, 0..1);
    }

    // ── helpers ────────────────────────────────────────────────────────────

    /// Create a depth texture and its default texture view.
    fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        sample_count: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth texture"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[wgpu::TextureFormat::Depth32Float],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Depth32Float),
            ..Default::default()
        });
        (texture, view)
    }

    /// Create a multisampled color texture.
    fn create_msaa_color(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        sample_count: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("MSAA color"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }
}
