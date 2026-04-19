use bytemuck::{Pod, Zeroable};
use family_enum::FamilyRecord;
use family_graph::EdgeType;
use glam::Mat4;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::camera::{projection_matrix, Camera};
use crate::font::make_font;
use crude_layout::LayoutTable;

const MAX_GLYPHS: u64 = 512;

// ── Color modes ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorMode {
    Depletion,       // blue=opening → red=endgame  (east axis driver)
    MaterialDiff,    // green=white ahead, purple=black ahead, gray=equal
    PhaseEstimate,   // dark=opening → bright=endgame
    WnpBand,         // white non-pawn band 0–8
    BnpBand,         // black non-pawn band 0–8
    WhitePawns,      // WP 0–8
    BlackPawns,      // BP 0–8
}

impl ColorMode {
    pub const ALL: &'static [ColorMode] = &[
        ColorMode::Depletion,
        ColorMode::MaterialDiff,
        ColorMode::PhaseEstimate,
        ColorMode::WnpBand,
        ColorMode::BnpBand,
        ColorMode::WhitePawns,
        ColorMode::BlackPawns,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ColorMode::Depletion     => "depletion (east)",
            ColorMode::MaterialDiff  => "material diff (north)",
            ColorMode::PhaseEstimate => "phase estimate (radial)",
            ColorMode::WnpBand       => "white NP band",
            ColorMode::BnpBand       => "black NP band",
            ColorMode::WhitePawns    => "white pawns",
            ColorMode::BlackPawns    => "black pawns",
        }
    }

    pub fn next(self) -> ColorMode {
        let idx = ColorMode::ALL.iter().position(|&m| m == self).unwrap_or(0);
        ColorMode::ALL[(idx + 1) % ColorMode::ALL.len()]
    }
}

fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

fn band_color(band: u8) -> [f32; 4] {
    // 9 bands mapped through a rainbow
    let t = band as f32 / 8.0;
    let colors: [[f32; 4]; 3] = [
        [0.15, 0.15, 0.85, 0.9],   // band 0: deep blue
        [0.15, 0.85, 0.15, 0.9],   // band 4: green
        [0.85, 0.15, 0.15, 0.9],   // band 8: red
    ];
    if t < 0.5 {
        lerp_color(colors[0], colors[1], t * 2.0)
    } else {
        lerp_color(colors[1], colors[2], (t - 0.5) * 2.0)
    }
}

fn pawn_color(pawns: u8) -> [f32; 4] {
    let t = pawns as f32 / 8.0;
    lerp_color(
        [0.85, 0.85, 0.15, 0.9],   // 0 pawns: yellow
        [0.15, 0.55, 0.85, 0.9],   // 8 pawns: steel blue
        t,
    )
}

fn family_color(rec: &FamilyRecord, mode: ColorMode) -> [f32; 4] {
    match mode {
        ColorMode::Depletion => {
            let t = (rec.features.depletion / 78.0).clamp(0.0, 1.0);
            lerp_color([0.1, 0.2, 0.9, 0.9], [0.9, 0.1, 0.1, 0.9], t)
        }
        ColorMode::MaterialDiff => {
            // diff in [-29, 29], neutral = 0
            let t = (rec.features.material_diff / 29.0).clamp(-1.0, 1.0);
            if t >= 0.0 {
                lerp_color([0.5, 0.5, 0.5, 0.9], [0.1, 0.85, 0.3, 0.9], t)
            } else {
                lerp_color([0.5, 0.5, 0.5, 0.9], [0.7, 0.1, 0.8, 0.9], -t)
            }
        }
        ColorMode::PhaseEstimate => {
            let t = rec.features.phase_estimate.clamp(0.0, 1.0);
            lerp_color([0.1, 0.1, 0.2, 0.9], [0.95, 0.95, 0.95, 0.9], t)
        }
        ColorMode::WnpBand => band_color(rec.key.wnp_band),
        ColorMode::BnpBand => band_color(rec.key.bnp_band),
        ColorMode::WhitePawns => pawn_color(rec.key.wp),
        ColorMode::BlackPawns => pawn_color(rec.key.bp),
    }
}

// ── GPU types ─────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct TextParams {
    x0_px:      f32,
    y0_px:      f32,
    char_w_px:  f32,
    char_h_px:  f32,
    glyph_w_px: f32,
    glyph_h_px: f32,
    screen_w:   f32,
    screen_h:   f32,
}

/// One entry per glyph: column, row, ASCII code, pad.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GlyphEntry {
    col:  u32,
    row:  u32,
    code: u32,
    _pad: u32,
}

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
    pub center_size: [f32; 4],
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
    // One instance buffer per color mode, indexed by ColorMode::ALL position.
    family_instance_bufs: Vec<wgpu::Buffer>,
    family_instance_count: u32,

    edge_pipeline: wgpu::RenderPipeline,
    edge_vertex_buf: wgpu::Buffer,
    edge_vertex_count: u32,
    axis_vertex_buf: wgpu::Buffer,

    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,

    text_pipeline: wgpu::RenderPipeline,
    text_bind_group: wgpu::BindGroup,
    text_glyphs_buf: wgpu::Buffer,
    text_params_buf: wgpu::Buffer,
    text_glyph_count: u32,

    pub color_mode: ColorMode,
    pub show_edges: bool,
}

impl Renderer {
    pub async fn new(
        window: Arc<Window>,
        layout: &LayoutTable,
        families: &[FamilyRecord],
        edge_verts: Vec<EdgeVertex>,
    ) -> Self {
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

        // ── Family instance buffers — one per color mode ────────────────────
        let family_instance_count = layout.layouts.len() as u32;
        let family_instance_bufs: Vec<wgpu::Buffer> = ColorMode::ALL
            .iter()
            .map(|&mode| {
                let instances: Vec<FamilyInstance> = layout
                    .layouts
                    .iter()
                    .zip(families.iter())
                    .map(|(fl, rec)| {
                        let c = fl.center;
                        let size = fl.extent_budget.he * 1.5;
                        FamilyInstance {
                            center_size: [c.x, c.y, c.z, size],
                            color: family_color(rec, mode),
                        }
                    })
                    .collect();
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("family_instances"),
                    contents: bytemuck::cast_slice(&instances),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
            .collect();

        // ── Axis lines ──────────────────────────────────────────────────────
        // Three lines through the actual cloud centroid, scaled to the cloud bounds.
        let n = layout.layouts.len() as f32;
        let centroid = layout.layouts.iter().map(|fl| fl.center)
            .fold(glam::Vec3::ZERO, |a, c| a + c) / n;
        let max_dist = layout.layouts.iter().map(|fl| (fl.center - centroid).length())
            .fold(0.0_f32, f32::max);
        let cx = centroid.x;
        let cy = centroid.y;
        let cz = centroid.z;
        let ext = max_dist * 0.8;
        let red   = [1.0_f32, 0.25, 0.25, 1.0];
        let green = [0.25_f32, 1.0, 0.35, 1.0];
        let blue  = [0.35_f32, 0.55, 1.0, 1.0];
        let axis_verts: &[EdgeVertex] = &[
            EdgeVertex { pos: [cx - ext, cy, cz], color: red   },
            EdgeVertex { pos: [cx + ext, cy, cz], color: red   },
            EdgeVertex { pos: [cx, cy - ext, cz], color: green },
            EdgeVertex { pos: [cx, cy + ext, cz], color: green },
            EdgeVertex { pos: [cx, cy, cz - ext], color: blue  },
            EdgeVertex { pos: [cx, cy, cz + ext], color: blue  },
        ];
        let axis_vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("axis"),
            contents: bytemuck::cast_slice(axis_verts),
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

        // ── Text overlay pipeline ───────────────────────────────────────────
        let font_data = make_font();
        let font_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("font"),
            contents: bytemuck::cast_slice(&font_data),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let text_glyphs_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text_glyphs"),
            size: MAX_GLYPHS * std::mem::size_of::<GlyphEntry>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let text_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text_params"),
            size: std::mem::size_of::<TextParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let text_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let text_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text_bg"),
            layout: &text_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: font_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: text_glyphs_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: text_params_buf.as_entire_binding() },
            ],
        });
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("text_shader.wgsl").into()),
        });
        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&text_bgl],
            push_constant_ranges: &[],
        });
        let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_shader,
                entry_point: "vs_text",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
                entry_point: "fs_text",
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
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Renderer {
            surface,
            device,
            queue,
            config,
            size,
            camera_buf,
            camera_bind_group,
            family_pipeline,
            family_instance_bufs,
            family_instance_count,
            edge_pipeline,
            axis_vertex_buf,
            edge_vertex_buf,
            edge_vertex_count,
            depth_texture,
            depth_view,
            text_pipeline,
            text_bind_group,
            text_glyphs_buf,
            text_params_buf,
            text_glyph_count: 0,
            color_mode: ColorMode::Depletion,
            show_edges: false,
        }
    }

    pub fn set_hover_text(&mut self, text: Option<&str>) {
        let Some(t) = text else {
            self.text_glyph_count = 0;
            return;
        };

        let mut glyphs: Vec<GlyphEntry> = Vec::new();
        let mut col = 0u32;
        let mut row = 0u32;
        for ch in t.chars() {
            if ch == '\n' {
                row += 1;
                col = 0;
                continue;
            }
            let code = ch as u32;
            if (32..128).contains(&code) && glyphs.len() < MAX_GLYPHS as usize {
                glyphs.push(GlyphEntry { col, row, code, _pad: 0 });
                col += 1;
            }
        }

        self.text_glyph_count = glyphs.len() as u32;
        if glyphs.is_empty() {
            return;
        }

        self.queue.write_buffer(&self.text_glyphs_buf, 0, bytemuck::cast_slice(&glyphs));

        let params = TextParams {
            x0_px:      14.0,
            y0_px:      14.0,
            char_w_px:  26.0,  // 24 glyph + 2 gap at 3× scale
            char_h_px:  30.0,  // 24 glyph + 6 line gap
            glyph_w_px: 24.0,
            glyph_h_px: 24.0,
            screen_w:   self.size.width as f32,
            screen_h:   self.size.height as f32,
        };
        self.queue.write_buffer(&self.text_params_buf, 0, bytemuck::bytes_of(&params));
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
        let vp: Mat4 = proj * cam.view_matrix();
        let uniforms = CameraUniforms {
            view_proj: vp.to_cols_array_2d(),
            right: cam.right().to_array(),
            _p0: 0.0,
            up: cam.up().to_array(),
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

            rpass.set_bind_group(0, &self.camera_bind_group, &[]);

            // Axis lines (always on)
            rpass.set_pipeline(&self.edge_pipeline);
            rpass.set_vertex_buffer(0, self.axis_vertex_buf.slice(..));
            rpass.draw(0..6, 0..1);

            // Family edges (optional, 'B' to toggle)
            if self.show_edges {
                rpass.set_pipeline(&self.edge_pipeline);
                rpass.set_vertex_buffer(0, self.edge_vertex_buf.slice(..));
                rpass.draw(0..self.edge_vertex_count, 0..1);
            }

            // Family glyphs — active color mode buffer
            let mode_idx = ColorMode::ALL
                .iter()
                .position(|&m| m == self.color_mode)
                .unwrap_or(0);
            rpass.set_pipeline(&self.family_pipeline);
            rpass.set_vertex_buffer(0, self.family_instance_bufs[mode_idx].slice(..));
            rpass.draw(0..6, 0..self.family_instance_count);
        }

        // Text overlay (second pass — loads 3D scene, no depth)
        if self.text_glyph_count > 0 {
            let mut tpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("text_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            tpass.set_pipeline(&self.text_pipeline);
            tpass.set_bind_group(0, &self.text_bind_group, &[]);
            tpass.draw(0..6, 0..self.text_glyph_count);
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
    let dominant = edge_types
        .iter()
        .max_by(|a, b| a.v0_weight().partial_cmp(&b.v0_weight()).unwrap())
        .copied()
        .unwrap_or(EdgeType::E);
    match dominant {
        D => [0.9, 0.55, 0.15, 0.25],
        BMinor => [0.15, 0.85, 0.85, 0.20],
        BMajor => [0.85, 0.85, 0.15, 0.20],
        C => [0.15, 0.85, 0.35, 0.15],
        E | FWithBMinor | FWithBMajor | FWithC => [0.65, 0.15, 0.85, 0.12],
    }
}
