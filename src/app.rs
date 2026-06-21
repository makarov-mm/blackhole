//! Window, GL context and main loop for the black-hole renderer.
//!
//! Controls:
//!   left-drag    orbit camera
//!   wheel        zoom
//!   Space        pause / resume disk rotation
//!   Up / Down    ray-march steps (quality vs. speed)
//!   R            reset view
//!   Esc          quit

use std::num::NonZeroU32;
use std::time::Instant;

use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextAttributesBuilder, NotCurrentGlContext};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowBuilder;

use crate::camera::OrbitCamera;
use crate::render::Renderer;

pub fn run() {
    let event_loop = EventLoop::new().expect("event loop");
    let window_builder = WindowBuilder::new()
        .with_title("Black Hole — Schwarzschild geodesics")
        .with_inner_size(LogicalSize::new(1280.0, 800.0));

    let template = ConfigTemplateBuilder::new().with_depth_size(0);
    let display_builder = DisplayBuilder::new().with_window_builder(Some(window_builder));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |configs| {
            configs
                .reduce(|a, b| if b.num_samples() > a.num_samples() { b } else { a })
                .unwrap()
        })
        .expect("build display");
    let window = window.expect("window");

    let raw_handle = window.raw_window_handle();
    let gl_display = gl_config.display();
    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_handle));
    let not_current = unsafe {
        gl_display
            .create_context(&gl_config, &context_attributes)
            .expect("create context")
    };

    let attrs = window.build_surface_attributes(Default::default());
    let gl_surface: Surface<WindowSurface> = unsafe {
        gl_display
            .create_window_surface(&gl_config, &attrs)
            .expect("create surface")
    };
    let gl_context = not_current.make_current(&gl_surface).expect("make current");

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            let cs = std::ffi::CString::new(s).unwrap();
            gl_display.get_proc_address(&cs) as *const _
        })
    };

    let _ = gl_surface.set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

    let size = window.inner_size();
    let mut renderer = Renderer::new(&gl);
    renderer.resize(&gl, size.width as i32, size.height as i32);

    let mut cam = OrbitCamera::new(22.0, size.width as f32 / size.height.max(1) as f32);

    let mut dragging = false;
    let mut last_cursor = (0.0f32, 0.0f32);
    let mut paused = false;
    let mut sim_time = 2.0f32;
    let mut last = Instant::now();

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);
            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => elwt.exit(),

                    WindowEvent::Resized(s) => {
                        if let (Some(w), Some(h)) = (NonZeroU32::new(s.width), NonZeroU32::new(s.height)) {
                            gl_surface.resize(&gl_context, w, h);
                            renderer.resize(&gl, s.width as i32, s.height as i32);
                            cam.set_aspect(s.width as f32 / s.height as f32);
                        }
                    }

                    WindowEvent::MouseInput { state, button, .. } => {
                        if button == MouseButton::Left {
                            dragging = state == ElementState::Pressed;
                        }
                    }

                    WindowEvent::CursorMoved { position, .. } => {
                        let p = (position.x as f32, position.y as f32);
                        if dragging {
                            cam.rotate(p.0 - last_cursor.0, p.1 - last_cursor.1);
                        }
                        last_cursor = p;
                    }

                    WindowEvent::MouseWheel { delta, .. } => {
                        let amount = match delta {
                            MouseScrollDelta::LineDelta(_, y) => y,
                            MouseScrollDelta::PixelDelta(d) => d.y as f32 / 50.0,
                        };
                        cam.zoom(amount);
                    }

                    WindowEvent::KeyboardInput { event, .. } => {
                        if event.state == ElementState::Pressed {
                            match event.logical_key {
                                Key::Named(NamedKey::Space) => paused = !paused,
                                Key::Named(NamedKey::ArrowUp) => {
                                    renderer.steps = (renderer.steps + 50).min(900)
                                }
                                Key::Named(NamedKey::ArrowDown) => {
                                    renderer.steps = (renderer.steps - 50).max(100)
                                }
                                Key::Character(ref c) if c.as_str() == "r" || c.as_str() == "R" => {
                                    cam = OrbitCamera::new(22.0, cam.aspect);
                                }
                                Key::Named(NamedKey::Escape) => elwt.exit(),
                                _ => {}
                            }
                        }
                    }

                    WindowEvent::RedrawRequested => {
                        let now = Instant::now();
                        let dt = (now - last).as_secs_f32();
                        last = now;
                        if !paused {
                            sim_time += dt;
                        }
                        let rb = cam.basis();
                        renderer.draw(&gl, &rb, sim_time);
                        gl_surface.swap_buffers(&gl_context).expect("swap");
                    }

                    _ => {}
                },

                Event::AboutToWait => window.request_redraw(),
                _ => {}
            }
        })
        .expect("event loop run");
}
