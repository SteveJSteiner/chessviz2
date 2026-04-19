mod camera;
mod renderer;

use camera::{projection_matrix, Camera};
use crude_layout::{compute, LayoutConfig, LayoutTable};
use family_enum::{build_table, FamilyRecord};
use family_graph::build_graph;
use glam::{Vec3, Vec4};
use renderer::{dominant_edge_color, EdgeVertex, Renderer};
use std::collections::HashSet;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

fn build_edge_vertices(
    layout: &LayoutTable,
    graph: &family_graph::FamilyGraph,
) -> Vec<EdgeVertex> {
    let mut verts = Vec::with_capacity(graph.edge_count() * 2);
    for e in graph.edge_indices() {
        let (a, b) = graph.edge_endpoints(e).unwrap();
        let meta = graph.edge_weight(e).unwrap();
        let color = dominant_edge_color(&meta.edge_types);
        let ca = layout.layouts[a.index()].center;
        let cb = layout.layouts[b.index()].center;
        verts.push(EdgeVertex { pos: [ca.x, ca.y, ca.z], color });
        verts.push(EdgeVertex { pos: [cb.x, cb.y, cb.z], color });
    }
    verts
}

fn print_layout_diagnostics(layout: &LayoutTable, families: &[FamilyRecord]) {
    use glam::Mat3;

    let all_identity = layout.layouts.iter().all(|fl| fl.orientation == Mat3::IDENTITY);
    let max_frob = layout.layouts.iter().map(|fl| {
        let m = fl.orientation;
        let i = Mat3::IDENTITY;
        (0..3).flat_map(|c| {
            let mc = m.col(c);
            let ic = i.col(c);
            (0..3).map(move |r| { let d = mc[r] - ic[r]; d * d })
        }).sum::<f32>().sqrt()
    }).fold(0.0f32, f32::max);

    let mut xs = Vec::with_capacity(layout.layouts.len());
    let mut ys = Vec::with_capacity(layout.layouts.len());
    let mut zs = Vec::with_capacity(layout.layouts.len());
    for fl in &layout.layouts {
        xs.push(fl.center.x);
        ys.push(fl.center.y);
        zs.push(fl.center.z);
    }
    fn range_std(v: &[f32]) -> (f32, f32, f32, f32) {
        let lo = v.iter().copied().fold(f32::MAX, f32::min);
        let hi = v.iter().copied().fold(f32::MIN, f32::max);
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        let std = (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len() as f32).sqrt();
        (lo, hi, hi - lo, std)
    }
    let (x0, x1, xe, xs_) = range_std(&xs);
    let (y0, y1, ye, ys_) = range_std(&ys);
    let (z0, z1, ze, zs_) = range_std(&zs);

    let corr_xz = {
        let mx = xs.iter().sum::<f32>() / xs.len() as f32;
        let mz = zs.iter().sum::<f32>() / zs.len() as f32;
        let cov: f32 = xs.iter().zip(zs.iter()).map(|(x,z)| (x-mx)*(z-mz)).sum::<f32>();
        let sx: f32 = xs.iter().map(|x| (x-mx).powi(2)).sum::<f32>().sqrt();
        let sz: f32 = zs.iter().map(|z| (z-mz).powi(2)).sum::<f32>().sqrt();
        if sx * sz > 1e-6 { cov / (sx * sz) } else { 0.0 }
    };

    eprintln!("── Layout diagnostics ───────────────────────────────────────");
    eprintln!("  orientations: all_identity={all_identity}  max_frob_diff={max_frob:.4}");
    eprintln!("  east  (x): [{x0:.1}, {x1:.1}]  range={xe:.1}  std={xs_:.2}");
    eprintln!("  north (y): [{y0:.1}, {y1:.1}]  range={ye:.1}  std={ys_:.2}");
    eprintln!("  radial(z): [{z0:.1}, {z1:.1}]  range={ze:.1}  std={zs_:.2}");
    eprintln!("  aspect east:north:radial = 1.00 : {:.2} : {:.2}", ye/xe, ze/xe);
    eprintln!("  east–radial Pearson r = {corr_xz:.4}  (0 = independent, 1 = same axis)");
    eprintln!("────────────────────────────────────────────────────────────");
    let _ = families;
}

fn print_locality_stats(layout: &LayoutTable, graph: &family_graph::FamilyGraph) {
    let mut dists: Vec<f32> = graph
        .edge_indices()
        .map(|e| {
            let (a, b) = graph.edge_endpoints(e).unwrap();
            let ca = layout.layouts[a.index()].center;
            let cb = layout.layouts[b.index()].center;
            (ca - cb).length()
        })
        .collect();
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let n = dists.len();
    let mean = dists.iter().sum::<f32>() / n as f32;
    let p10 = dists[n / 10];
    let p50 = dists[n / 2];
    let p90 = dists[n * 9 / 10];
    let max = dists[n - 1];

    let mut min_c = Vec3::splat(f32::MAX);
    let mut max_c = Vec3::splat(f32::MIN);
    for fl in &layout.layouts {
        min_c = min_c.min(fl.center);
        max_c = max_c.max(fl.center);
    }
    let diagonal = (max_c - min_c).length();

    eprintln!("── Edge locality (DESIGN.md acceptance criterion) ──────────");
    eprintln!("  edges: {n}   cloud diagonal: {diagonal:.1}");
    eprintln!("  edge-length p10={p10:.2}  p50={p50:.2}  p90={p90:.2}  max={max:.2}  mean={mean:.2}");
    eprintln!("  p50/diagonal = {:.3}  (lower → more local)", p50 / diagonal);
    eprintln!("────────────────────────────────────────────────────────────");
}

/// Find the hovered family index given cursor position in physical pixels.
/// Returns None if no family glyph is under the cursor.
fn find_hovered(
    cursor_px: (f64, f64),
    layout: &LayoutTable,
    cam: &Camera,
    win_w: u32,
    win_h: u32,
) -> Option<usize> {
    let aspect = win_w as f32 / win_h as f32;
    let proj = projection_matrix(std::f32::consts::FRAC_PI_3, aspect, 0.1, 2000.0);
    let vp = proj * cam.view_matrix();

    let cx = cursor_px.0 as f32;
    let cy = cursor_px.1 as f32;

    let mut best_idx: Option<usize> = None;
    let mut best_dist_sq = f32::MAX;

    for (i, fl) in layout.layouts.iter().enumerate() {
        let p = fl.center;
        let clip = vp * Vec4::new(p.x, p.y, p.z, 1.0);
        if clip.w <= 0.0 {
            continue; // behind camera
        }
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        let sx = (ndc_x + 1.0) * 0.5 * win_w as f32;
        let sy = (1.0 - ndc_y) * 0.5 * win_h as f32;

        // Billboard screen radius: world half-size projected via perspective
        // proj[1][1] = 1/tan(fov/2). radius_px ≈ world_size * proj[1][1] / clip.w * (height/2)
        let world_radius = fl.extent_budget.he * 1.5;
        let proj11 = proj.col(1)[1]; // = 1/tan(fov_y/2)
        let radius_px = (world_radius * proj11 / clip.w * win_h as f32 * 0.5).max(6.0);

        let dx = sx - cx;
        let dy = sy - cy;
        let dist_sq = dx * dx + dy * dy;
        let r_sq = radius_px * radius_px;

        if dist_sq <= r_sq && dist_sq < best_dist_sq {
            best_dist_sq = dist_sq;
            best_idx = Some(i);
        }
    }

    best_idx
}

struct State {
    window: Arc<Window>,
    renderer: Renderer,
    camera: Camera,
    layout: LayoutTable,
    families: Vec<FamilyRecord>,
    keys: HashSet<KeyCode>,
    mouse_drag: bool,
    last_cursor: Option<(f64, f64)>,
    hovered_idx: Option<usize>,
    last_frame: std::time::Instant,
    frames_rendered: u32,
}

impl State {
    async fn new(window: Arc<Window>) -> Self {
        eprintln!("Building family table...");
        let families = build_table();
        eprintln!("Building family graph...");
        let graph = build_graph(&families);
        eprintln!("Computing layout...");
        let layout = compute(&families, &graph, &LayoutConfig::default());

        eprintln!(
            "Layout ready: {} families, {} graph edges",
            layout.layouts.len(),
            graph.edge_count()
        );

        print_layout_diagnostics(&layout, &families);
        print_locality_stats(&layout, &graph);

        let edge_verts = build_edge_vertices(&layout, &graph);
        let renderer = Renderer::new(window.clone(), &layout, &families, edge_verts).await;

        let focus = Vec3::new(40.0, 0.0, 40.0);
        let camera = Camera::new(focus, 0.3, 0.25, 180.0);

        eprintln!("GPU renderer ready — entering event loop");
        eprintln!("Axes (lines through cloud centroid):");
        eprintln!("  RED   = East   (x) — depletion: opening (low) → endgame (high)");
        eprintln!("  GREEN = North  (y) — material diff: black ahead (−) ↔ white ahead (+)");
        eprintln!("  BLUE  = Radial (z) — pawn total: no pawns (near) → full pawns (far)");
        eprintln!("Controls: A/D orbit  W/S zoom  Q/E tilt  right-drag orbit  Shift=fast");
        eprintln!("          C = cycle color mode  B = toggle edges  Escape = quit");

        State {
            window,
            renderer,
            camera,
            layout,
            families,
            keys: HashSet::new(),
            mouse_drag: false,
            last_cursor: None,
            hovered_idx: None,
            last_frame: std::time::Instant::now(),
            frames_rendered: 0,
        }
    }

    fn tick(&mut self) {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        let speed = if self.keys.contains(&KeyCode::ShiftLeft)
            || self.keys.contains(&KeyCode::ShiftRight)
        {
            4.0
        } else {
            1.0
        };

        let orbit_rate = 1.2 * speed * dt;
        let zoom_rate = self.camera.radius * 0.8 * speed * dt;

        let d_az = if self.keys.contains(&KeyCode::KeyA) {
            orbit_rate
        } else if self.keys.contains(&KeyCode::KeyD) {
            -orbit_rate
        } else {
            0.0
        };

        let d_el = if self.keys.contains(&KeyCode::KeyE)
            || self.keys.contains(&KeyCode::Space)
        {
            orbit_rate
        } else if self.keys.contains(&KeyCode::KeyQ)
            || self.keys.contains(&KeyCode::ControlLeft)
        {
            -orbit_rate
        } else {
            0.0
        };

        let zoom = if self.keys.contains(&KeyCode::KeyW) {
            zoom_rate
        } else if self.keys.contains(&KeyCode::KeyS) {
            -zoom_rate
        } else {
            0.0
        };

        if d_az != 0.0 || d_el != 0.0 {
            self.camera.orbit(d_az, d_el);
        }
        if zoom != 0.0 {
            self.camera.zoom(zoom);
        }

        self.renderer.update_camera(&self.camera);
    }

    fn on_cursor_moved(&mut self, x: f64, y: f64) {
        let pos = (x, y);

        // Orbit when any mouse button held (left drag on trackpad)
        if self.mouse_drag {
            if let Some((lx, ly)) = self.last_cursor {
                let dx = (x - lx) as f32;
                let dy = (y - ly) as f32;
                self.camera.orbit(-dx * 0.005, -dy * 0.005);
            }
        }
        self.last_cursor = Some(pos);

        // Hover detection
        let size = self.renderer.size;
        let new_hovered = find_hovered(pos, &self.layout, &self.camera, size.width, size.height);
        if new_hovered != self.hovered_idx {
            self.hovered_idx = new_hovered;
            if let Some(idx) = new_hovered {
                let rec = &self.families[idx];
                let k = &rec.key;
                let f = &rec.features;
                let c = self.layout.layouts[idx].center;
                eprintln!(
                    "hover  wnp={} bnp={} wp={} bp={}  depletion={:.1}  mat_diff={:.1}  pawns={}  pos=({:.1},{:.1},{:.1})",
                    k.wnp_band, k.bnp_band, k.wp, k.bp,
                    f.depletion, f.material_diff, k.wp + k.bp,
                    c.x, c.y, c.z
                );
            }
        }
    }
}

struct App {
    state: Option<State>,
}

impl App {
    fn new() -> Self {
        App { state: None }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_title("chessviz2 — family manifold")
                        .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32)),
                )
                .expect("create window"),
        );
        self.state = Some(pollster::block_on(State::new(window)));
        if let Some(s) = &self.state {
            s.window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(s) = &self.state {
            s.window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else { return };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                state.renderer.resize(size);
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    physical_key: PhysicalKey::Code(code),
                    state: ks,
                    ..
                },
                ..
            } => {
                if ks == ElementState::Pressed {
                    state.keys.insert(code);
                    match code {
                        KeyCode::Escape => event_loop.exit(),
                        KeyCode::KeyC => {
                            state.renderer.color_mode =
                                state.renderer.color_mode.next();
                            eprintln!(
                                "Color mode: {}",
                                state.renderer.color_mode.label()
                            );
                        }
                        KeyCode::KeyB => {
                            state.renderer.show_edges =
                                !state.renderer.show_edges;
                            eprintln!(
                                "Edges: {}",
                                if state.renderer.show_edges { "on" } else { "off" }
                            );
                        }
                        _ => {}
                    }
                } else {
                    state.keys.remove(&code);
                }
            }
            WindowEvent::MouseInput { button, state: ks, .. } => {
                let pressed = ks == ElementState::Pressed;
                match button {
                    MouseButton::Left | MouseButton::Right => {
                        state.mouse_drag = pressed;
                        if !pressed {
                            state.last_cursor = None;
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.on_cursor_moved(position.x, position.y);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 8.0,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.5,
                };
                state.camera.zoom(-scroll * state.camera.radius * 0.015);
            }
            WindowEvent::RedrawRequested => {
                state.tick();
                match state.renderer.render() {
                    Ok(_) => {
                        state.frames_rendered += 1;
                        if state.frames_rendered == 1 {
                            eprintln!("First frame rendered successfully");
                        }
                    }
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let size = state.renderer.size;
                        state.renderer.resize(size);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        eprintln!("OOM — exiting");
                        event_loop.exit();
                    }
                    Err(e) => eprintln!("render error: {e:?}"),
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        match &event {
            DeviceEvent::MouseMotion { delta } => eprintln!("DeviceEvent::MouseMotion  {delta:?}"),
            DeviceEvent::Button { button, state } => eprintln!("DeviceEvent::Button  btn={button}  state={state:?}"),
            _ => {}
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run app");
}
