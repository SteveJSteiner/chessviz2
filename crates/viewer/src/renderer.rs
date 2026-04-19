use bytemuck::{Pod, Zeroable};
use family_graph::EdgeType;
use glam::Mat4;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::camera::{projection_matrix, Camera};
use crude_layout::LayoutTable;

// ── GPU types ─────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CameraUniforms {
    view_proj: [[f32; 4]; 4],
    right: [f32; 3],
    _p0: f32,
    up: [f32; 3],
    _p1: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct FamilyInstance {
    pub center_size: [f32; 4], // xyz=center, w=half_size
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct EdgeVertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}

// ── Renderer ──────────────────────────────────────────────────────────────────

pub struct Renderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    camera_buf: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    family_pipeline: wgpu::RenderPipeline,
    family_instance_buf: wgpu::Buffer,
    family_instance_count: u32,

    edge_pipeline: wgpu::RenderPipeline,
    edge_vertex_buf: wgpu::Buffer,
    edge_vertex_count: u32,

    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl Renderer {
    pub async fn new(window: Arc<Window>, layout: &LayoutTable, edge_verts: Vec<EdgeVertex>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window).expect("create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no adapter found");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .expect("no device");

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
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // ── Camera uniform ──────────────────────────────────────────────────
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: std::mem::size_of::<CameraUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        // ── Shader ─────────────────────────────────────────────────────────
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[],
        });

        let depth_format = wgpu::TextureFormat::Depth32Float;

        // ── Family pipeline ─────────────────────────────────────────────────
        let family_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("family"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_family",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<FamilyInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            offset: 16,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_family",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Edge pipeline ───────────────────────────────────────────────────
        let edge_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("edge"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_edge",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<EdgeVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 12,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_edge",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Family instance buffer ──────────────────────────────────────────
        let family_instances: Vec<FamilyInstance> = layout
            .layouts
            .iter()
            .enumerate()
            .map(|(_, fl)| {
                let c = fl.center;
                let size = fl.extent_budget.he.max(0.3);
                let t = (c.x / 78.0).clamp(0.0, 1.0); // east = depletion proxy
                let color = [
                    0.2 + 0.6 * t,
                    0.3 * (1.0 - t),
                    0.8 * (1.0 - t) + 0.1,
                    0.85,
                ];
                FamilyInstance {
                    center_size: [c.x, c.y, c.z, size],
                    color,
                }
            })
            .collect();

        let family_instance_count = family_instances.len() as u32;
        let family_instance_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("family_instances"),
            contents: bytemuck::cast_slice(&family_instances),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // ── Edge vertex buffer ──────────────────────────────────────────────
        let edge_vertex_count = edge_verts.len() as u32;
        let edge_vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("edge_verts"),
            contents: bytemuck::cast_slice(&edge_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // ── Depth texture ───────────────────────────────────────────────────
        let (depth_texture, depth_view) =
            make_depth_texture(&device, size.width, size.height, depth_format);

        Renderer {
            surface,
            device,
            queue,
            config,
            size,
            camera_buf,
            camera_bind_group,
            family_pipeline,
            family_instance_buf,
            family_instance_count,
            edge_pipeline,
            edge_vertex_buf,
            edge_vertex_count,
            depth_texture,
            depth_view,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        let fmt = self.depth_texture.format();
        (self.depth_texture, self.depth_view) =
            make_depth_texture(&self.device, new_size.width, new_size.height, fmt);
    }

    pub fn update_camera(&self, cam: &Camera) {
        let aspect = self.size.width as f32 / self.size.height as f32;
        let proj = projection_matrix(std::f32::consts::FRAC_PI_3, aspect, 0.1, 2000.0);
        let view = cam.view_matrix();
        let vp: Mat4 = proj * view;
        let right = cam.right();
        let up = cam.up();
        let uniforms = CameraUniforms {
            view_proj: vp.to_cols_array_2d(),
            right: right.to_array(),
            _p0: 0.0,
            up: up.to_array(),
            _p1: 0.0,
        };
        self.queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&uniforms));
    }

    pub fn render(&self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rpass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.02,
                            b: 0.04,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Draw edges first (behind glyphs)
            rpass.set_pipeline(&self.edge_pipeline);
            rpass.set_bind_group(0, &self.camera_bind_group, &[]);
            rpass.set_vertex_buffer(0, self.edge_vertex_buf.slice(..));
            rpass.draw(0..self.edge_vertex_count, 0..1);

            // Draw family billboards on top
            rpass.set_pipeline(&self.family_pipeline);
            rpass.set_vertex_buffer(0, self.family_instance_buf.slice(..));
            rpass.draw(0..6, 0..self.family_instance_count);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn make_depth_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

// ── Edge color helper ─────────────────────────────────────────────────────────

pub fn dominant_edge_color(edge_types: &[EdgeType]) -> [f32; 4] {
    use EdgeType::*;
    // Pick the type with the highest v0 weight as the display representative
    let dominant = edge_types
        .iter()
        .max_by(|a, b| a.v0_weight().partial_cmp(&b.v0_weight()).unwrap())
        .copied()
        .unwrap_or(EdgeType::E);
    match dominant {
        D => [0.9, 0.55, 0.15, 0.35],
        BMinor => [0.15, 0.85, 0.85, 0.30],
        BMajor => [0.85, 0.85, 0.15, 0.30],
        C => [0.15, 0.85, 0.35, 0.25],
        E | FWithBMinor | FWithBMajor | FWithC => [0.65, 0.15, 0.85, 0.20],
    }
}
