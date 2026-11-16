#![allow(float_literal_f32_fallback)]

mod editor;
mod game;
mod modeling;
mod render;
mod state;
mod ui;

use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::{
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use state::State;

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let window = Arc::new(
        WindowBuilder::new()
            .with_title("LoadEngine - Integrated Studio")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .build(&event_loop)
            .unwrap(),
    );

    let mut state = pollster::block_on(State::new(Arc::clone(&window)));
    let mut wake: Option<Instant> = None;
    window.request_redraw();

    event_loop
        .run(move |event, elwt| {
            match &event {
                Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                    wake = None;
                    window.request_redraw();
                }
                Event::WindowEvent { event, .. } => {
                    let repaint = state.handle_event(event);
                    match event {
                        WindowEvent::CloseRequested => elwt.exit(),
                        WindowEvent::Resized(size) => {
                            state.resize(*size);
                            window.request_redraw();
                        }
                        WindowEvent::RedrawRequested => {
                            match state.render() {
                                Ok(()) => {}
                                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                    state.reconfigure();
                                    window.request_redraw();
                                }
                                Err(e) => eprintln!("Render error: {:?}", e),
                            }
                            // egui می‌گوید کی دوباره رسم شود
                            let d = state.repaint_delay();
                            wake = None;
                            if d.is_zero() {
                                window.request_redraw();
                            } else if d < Duration::from_secs(3600) {
                                wake = Some(Instant::now() + d);
                            }
                        }
                        _ => {
                            if repaint {
                                window.request_redraw();
                            }
                        }
                    }
                }
                _ => {}
            }
            elwt.set_control_flow(match wake {
                Some(t) => ControlFlow::WaitUntil(t),
                None => ControlFlow::Wait,
            });
        })
        .unwrap();
}