extern crate winit;
#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
extern crate rmge;

use slog::Drain;

use winit::{EventsLoop, Event, Window, WindowEvent};
use gfx_hal::window::PresentMode::*;

use rmge::geometry::Rectangle;

use rmge::graphics::HalState;
use rmge::graphics::TexturedQuad;

#[derive(Debug, Clone, Default)]
pub struct UserInput {
    end_requested: bool,
    new_frame_size: Option<(f64, f64)>,
    new_mouse_position: Option<(f64, f64)>,
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
            _ => (),
        });
        output
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LocalState {
    pub frame_width: f64,
    pub frame_height: f64,
    pub mouse_x: f64,
    pub mouse_y: f64,
}
impl LocalState {
    pub fn update_from_input(&mut self, input: UserInput) {
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

fn do_the_quad_render(hal_state: &mut HalState, local_state: &LocalState) -> Result<(), &'static str> {
    let x1 = 100.0;
    let y1 = 100.0;
    let x2 = local_state.mouse_x as f32;
    let y2 = local_state.mouse_y as f32;
    let quad = Rectangle {
        x: (x1 / local_state.frame_width as f32) * 2.0 - 1.0,
        y: (y1 / local_state.frame_height as f32) * 2.0 - 1.0,
        w: ((x2 - x1) / local_state.frame_width as f32) * 2.0,
        h: ((y2 - y1) / local_state.frame_height as f32) * 2.0,
    };
    let quad_2 = {
        let Rectangle { x, y, w, h } = quad;
        Rectangle {
            x: x + 0.5,
            y: y - 0.5,
            w,
            h
        }
    };
    let textured_quad = TexturedQuad {
        quad: quad.to_quad(),
        uv_rect: [80.0, 30.0, 180.0, 70.0],
        tex_num: 0,
    };
    let textured_quad2 = TexturedQuad {
        quad: quad_2.to_quad(),
        uv_rect: [80.0, 0.0, 180.0, 30.0],
        tex_num: 1,
    };

    hal_state.draw_quad_frame(&[textured_quad, textured_quad2])
}

fn main() {
    let mut events_loop = EventsLoop::new();
    // slog setup
    let decorator = slog_term::PlainDecorator::new(std::io::stdout());
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    let log = slog::Logger::root(drain, o!());

    let mut window = Window::new(&events_loop).unwrap();
    let mut hal_state = match HalState::new(&window, "rustmania", 512, [Mailbox, Fifo, Relaxed, Immediate], log.new(o!())) {
        Ok(state) => state,
        Err(e) => panic!(e),
    };
    let (frame_width, frame_height) = window
        .get_inner_size()
        .map(|logical| logical.into())
        .unwrap_or((0.0, 0.0));
    let mut local_state = LocalState {
        frame_width,
        frame_height,
        mouse_x: 0.0,
        mouse_y: 0.0,
    };
    hal_state.load_texture(include_bytes!("creature-smol.png")).unwrap();
    hal_state.load_texture(include_bytes!("judgment.png")).unwrap();
    
    loop {
        let inputs = UserInput::poll_events_loop(&mut events_loop);
        if inputs.end_requested {
            break;
        }
        if let Some(a) = inputs.new_frame_size {
            debug!(&log, "Window changed size"; o!("x" => a.0, "y" => a.1));
            core::mem::drop(hal_state);
            hal_state = match HalState::new(&window, "rustmania", 512, [Mailbox, Fifo, Relaxed, Immediate], log.new(o!())) {
                Ok(state) => state,
                Err(e) => panic!(e),
            };
            hal_state.load_texture(include_bytes!("creature-smol.png")).unwrap();
            hal_state.load_texture(include_bytes!("judgment.png")).unwrap();
        }
        local_state.update_from_input(inputs);
        if let Err(e) = do_the_quad_render(&mut hal_state, &local_state) {
            error!(&log, "render error"; "render_error" => e);
            debug!(&log, "Auto-restarting HalState...");
            hal_state = match HalState::new(&window, "rustmania", 512, [Mailbox, Fifo, Relaxed, Immediate], log.new(o!())) {
                Ok(state) => state,
                Err(e) => panic!(e),
            };
            hal_state.load_texture(include_bytes!("creature-smol.png")).unwrap();
            hal_state.load_texture(include_bytes!("judgment.png")).unwrap();
        }
    }

}

