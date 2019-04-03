extern crate winit;
#[macro_use]
extern crate slog;
extern crate rmge;
extern crate slog_async;
extern crate slog_term;

use slog::Drain;

use gfx_hal::window::PresentMode::*;
use rmge::geometry::{Mat2, Mat3, Quad, Rect, Vec2, Vec3};
use rmge::graphics::{HalState, SamplingConfig, TexturedQuad};
use std::time::{Duration, Instant};
use winit::{DeviceEvent, Event, EventsLoop, KeyboardInput, VirtualKeyCode, Window, WindowEvent};

#[derive(Debug, Clone, Default)]
pub struct UserInput {
    end_requested: bool,
    new_frame_size: Option<(f64, f64)>,
    new_mouse_position: Option<(f64, f64)>,
    transform_rect: Option<Vec2<f32>>,
    rotate_rect: Option<f64>,
    flip_rect: bool,
}

fn create_halstate(window: &Window, log: &slog::Logger) -> HalState {
    match HalState::new(
        &window,
        "rustmania",
        512,
        [Mailbox, Fifo, Relaxed, Immediate],
        SamplingConfig {
            multisampling: Some(16),
            filter_type: Some(gfx_hal::image::Filter::Linear),
        },
        log.new(o!()),
    ) {
        Ok(state) => state,
        Err(e) => panic!(e),
    }
}

impl UserInput {
    pub fn poll_events_loop(events_loop: &mut EventsLoop) -> Self {
        let mut output = UserInput::default();
        events_loop.poll_events(|event| match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                output.end_requested = true;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(logical),
                ..
            } => {
                output.new_frame_size = Some((logical.width, logical.height));
            }
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                output.new_mouse_position = Some((position.x, position.y));
            }
            Event::DeviceEvent {
                event:
                    DeviceEvent::Key(KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::Tab),
                        state: winit::ElementState::Pressed,
                        ..
                    }),
                ..
            } => {
                output.flip_rect = true;
            }
            Event::DeviceEvent {
                event:
                    DeviceEvent::Key(KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::Down),
                        state: winit::ElementState::Pressed,
                        ..
                    }),
                ..
            } => {
                output.transform_rect = Some(Vec2 { x: 0.0, y: 0.125 });
            }
            Event::DeviceEvent {
                event:
                    DeviceEvent::Key(KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::Up),
                        state: winit::ElementState::Pressed,
                        ..
                    }),
                ..
            } => {
                output.transform_rect = Some(Vec2 { x: 0.0, y: -0.125 });
            }
            Event::DeviceEvent {
                event:
                    DeviceEvent::Key(KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::Right),
                        state: winit::ElementState::Pressed,
                        ..
                    }),
                ..
            } => {
                output.transform_rect = Some(Vec2 { x: 0.125, y: 0.0 });
            }
            Event::DeviceEvent {
                event:
                    DeviceEvent::Key(KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::Left),
                        state: winit::ElementState::Pressed,
                        ..
                    }),
                ..
            } => {
                output.transform_rect = Some(Vec2 { x: -0.125, y: 0.0 });
            }
            Event::DeviceEvent {
                event:
                    DeviceEvent::Key(KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::D),
                        state: winit::ElementState::Pressed,
                        ..
                    }),
                ..
            } => {
                output.rotate_rect = Some(15.0);
            }
            Event::DeviceEvent {
                event:
                    DeviceEvent::Key(KeyboardInput {
                        virtual_keycode: Some(VirtualKeyCode::A),
                        state: winit::ElementState::Pressed,
                        ..
                    }),
                ..
            } => {
                output.rotate_rect = Some(-15.0);
            }
            _ => (),
        });
        output
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LocalState {
    pub frame_width: f64,
    pub frame_height: f64,
    pub mouse_x: f64,
    pub mouse_y: f64,
    pub quad: Quad,
    pub rotation: f64,
}
impl LocalState {
    pub fn update_from_input(&mut self, input: UserInput) {
        if let Some(rotation) = input.rotate_rect {
            self.rotation = (self.rotation + rotation) % 360.0;
            self.quad = self
                .quad
                .transform(self.quad.rotate_around_center_matrix(rotation));
        }

        let rotation =
            Mat2::rotation_z(self.rotation as f32 / 360.0 * (std::f32::consts::PI * 2.0));
        if let Some(translation) = input.transform_rect {
            translation.map(|a| a as f32);
            let new_translation = rotation * translation;
            let matrix =
                Mat3::translation_2d(Vec3::new_point_2d(new_translation.x, new_translation.y));
            self.quad = self.quad.transform(matrix);
        }

        if input.flip_rect {
            self.quad = self.quad.rotate_180_around_center();
            self.rotation += 180.0;
        }
        if let Some(frame_size) = input.new_frame_size {
            self.frame_width = frame_size.0;
            self.frame_height = frame_size.1;
        }
        if let Some(position) = input.new_mouse_position {
            self.mouse_x = position.0;
            self.mouse_y = position.1;
        }
    }
}

fn do_the_quad_render(
    hal_state: &mut HalState,
    local_state: &LocalState,
) -> Result<Instant, &'static str> {
    let textured_quad = TexturedQuad {
        quad: local_state.quad,
        uv_rect: [0.0, 0.0, 300.0, 300.0],
        tex_num: 0,
    };
    /*let textured_quad2 = TexturedQuad {
        quad: Quad::from(quad_2).transform(rotate_90 * ident),
        uv_rect: [80.0, 0.0, 180.0, 30.0],
        tex_num: 1,
    };*/
    hal_state.draw_quad_frame(&[textured_quad])?;
    let after = Instant::now();
    Ok(after)
}

fn main() {
    let mut events_loop = EventsLoop::new();
    // slog setup
    let decorator = slog_term::PlainDecorator::new(std::io::stdout());
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    let log = slog::Logger::root(drain, o!());

    let window = Window::new(&events_loop).unwrap();
    let mut hal_state = create_halstate(&window, &log);

    let (frame_width, frame_height) = window
        .get_inner_size()
        .map(|logical| logical.into())
        .unwrap_or((0.0, 0.0));
    let mut local_state = LocalState {
        frame_width,
        frame_height,
        mouse_x: 0.0,
        mouse_y: 0.0,
        quad: Quad::from(Rect {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        }),
        rotation: 0.0,
    };

    hal_state
        .load_texture(include_bytes!("creature-smol.png"))
        .unwrap();
    hal_state
        .load_texture(include_bytes!("judgment.png"))
        .unwrap();

    let mut start = Instant::now();
    let mut frames_this_second = 0;
    loop {
        let inputs = UserInput::poll_events_loop(&mut events_loop);
        if inputs.end_requested {
            break;
        }
        if let Some(a) = inputs.new_frame_size {
            debug!(&log, "Window changed size"; o!("x" => a.0, "y" => a.1));
            core::mem::drop(hal_state);
            hal_state = create_halstate(&window, &log);
            hal_state
                .load_texture(include_bytes!("creature-smol.png"))
                .unwrap();
            hal_state
                .load_texture(include_bytes!("judgment.png"))
                .unwrap();
        }
        local_state.update_from_input(inputs);
        match do_the_quad_render(&mut hal_state, &local_state) {
            Ok(instant) => {
                if (instant - start) >= Duration::from_secs(1) {
                    info!(log, "one second passed"; "fps" => frames_this_second);
                    frames_this_second = 1;
                    start = instant;
                } else {
                    frames_this_second += 1;
                }
            }
            Err(e) => {
                error!(&log, "render error"; "render_error" => e);
                debug!(&log, "Auto-restarting HalState...");
                hal_state = create_halstate(&window, &log);
                hal_state
                    .load_texture(include_bytes!("creature-smol.png"))
                    .unwrap();
                hal_state
                    .load_texture(include_bytes!("judgment.png"))
                    .unwrap();
            }
        }
    }
}
