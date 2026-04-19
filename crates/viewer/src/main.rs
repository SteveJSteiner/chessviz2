mod camera;
mod renderer;

use camera::Camera;
use crude_layout::{compute, LayoutConfig};
use family_enum::build_table;
use family_graph::build_graph;
use glam::Vec3;
use renderer::{dominant_edge_color, EdgeVertex, Renderer};
use std::collections::HashSet;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

fn build_edge_vertices(layout: &crude_layout::LayoutTable, graph: &family_graph::FamilyGraph) -> Vec<EdgeVertex> {
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

struct State {
    window: Arc<Window>,
    renderer: Renderer,
    camera: Camera,
    keys: HashSet<KeyCode>,
    mouse_captured: bool,
    last_frame: std::time::Instant,
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

        let edge_verts = build_edge_vertices(&layout, &graph);

        let renderer = Renderer::new(window.clone(), &layout, edge_verts).await;

        // Start camera behind the mass of glyphs, looking in
        let camera = Camera::new(Vec3::new(40.0, 0.0, -60.0));

        State {
            window,
            renderer,
            camera,
            keys: HashSet::new(),
            mouse_captured: false,
            last_frame: std::time::Instant::now(),
        }
    }

    fn tick(&mut self) {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        let speed = if self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight) {
            40.0
        } else {
            10.0
        } * dt;

        let fwd = if self.keys.contains(&KeyCode::KeyW) { 1.0 }
            else if self.keys.contains(&KeyCode::KeyS) { -1.0 }
            else { 0.0 };
        let right = if self.keys.contains(&KeyCode::KeyD) { 1.0 }
            else if self.keys.contains(&KeyCode::KeyA) { -1.0 }
            else { 0.0 };
        let up = if self.keys.contains(&KeyCode::KeyE) || self.keys.contains(&KeyCode::Space) { 1.0 }
            else if self.keys.contains(&KeyCode::KeyQ) || self.keys.contains(&KeyCode::ControlLeft) { -1.0 }
            else { 0.0 };

        if fwd != 0.0 || right != 0.0 || up != 0.0 {
            self.camera.move_local(fwd, right, up, speed);
        }

        self.renderer.update_camera(&self.camera);
    }

    fn capture_mouse(&mut self, capture: bool) {
        self.mouse_captured = capture;
        if capture {
            let _ = self.window.set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_| self.window.set_cursor_grab(CursorGrabMode::Locked));
            self.window.set_cursor_visible(false);
        } else {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
            self.window.set_cursor_visible(true);
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
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state: ks, .. },
                ..
            } => {
                if ks == ElementState::Pressed {
                    state.keys.insert(code);
                    if code == KeyCode::Escape {
                        if state.mouse_captured {
                            state.capture_mouse(false);
                        } else {
                            event_loop.exit();
                        }
                    }
                } else {
                    state.keys.remove(&code);
                }
            }
            WindowEvent::MouseInput { button: MouseButton::Right, state: ks, .. } => {
                if ks == ElementState::Pressed {
                    let captured = !state.mouse_captured;
                    state.capture_mouse(captured);
                }
            }
            WindowEvent::RedrawRequested => {
                state.tick();
                match state.renderer.render() {
                    Ok(_) => {}
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
                state.window.request_redraw();
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
        let Some(state) = self.state.as_mut() else { return };
        if !state.mouse_captured { return; }
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            state.camera.apply_mouse(dx as f32, dy as f32, 0.002);
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
