extern crate slog_term;
extern crate slog_async;
extern crate rmge;
#[macro_use]
extern crate slog;

use slog::Drain;

use rmge::{WindowState, HalState, Triangle, Point2D};
use winit::{EventsLoop, WindowBuilder, Window, WindowEvent, Event};

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

fn do_the_render(hal_state: &mut HalState, local_state: &LocalState) -> Result<(), &'static str> {
    let r = (local_state.mouse_x / local_state.frame_width) as f32;
    let g = (local_state.mouse_y / local_state.frame_height) as f32;
    let b = (r + g) * 0.3;
    let a = 1.0;
    hal_state.draw_clear_frame([r, g, b, a])
}

impl Coords {
    /// coordinates in terms of logical pixels to the [-1, 1] interval that gfx uses
    fn to_gfx_coords(self) -> (f64, f64) {
        let WindowSize { height, width } = self.window_size;
        let (x, y) = self.coordinates;
        (x / width - 1.0, y / height - 1.0)
    }
}

fn do_the_triangle_render(hal_state: &mut HalState, local_state: &LocalState) -> Result<(), &'static str> {
    let x = ((local_state.mouse_x / local_state.frame_width) * 2.0) - 1.0;
    let y = ((local_state.mouse_y / local_state.frame_height) * 2.0) - 1.0;
    let triangle: [[f32; 2]; 3] = [[-0.5, 0.5], [-0.5, -0.5], [x as f32, y as f32]];
    hal_state.draw_triangle_frame(Triangle::from(triangle).vertex_attributes())
}

fn main() {
    // slog setup
    let decorator = slog_term::PlainDecorator::new(std::io::stdout());
    let drain = slog_term::CompactFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    let log = slog::Logger::root(drain, o!("version" => "0.1"));

    let mut winit_state = WindowState::new("rustmania", (800.0, 600.0), Some(log.new(o!("child" => 1)))).expect("failed to create window");
    let mut hal_state = match HalState::new(&winit_state) {
        Ok(state) => state,
        Err(e) => panic!(e),
    };
    let (frame_width, frame_height) = winit_state
        .get_window()
        .get_inner_size()
        .map(|logical| logical.into())
        .unwrap_or((0.0, 0.0));
    let mut local_state = LocalState {
        frame_width,
        frame_height,
        mouse_x: 0.0,
        mouse_y: 0.0,
    };
    loop {
        let inputs = UserInput::poll_events_loop(winit_state.get_events_loop_mut());
        if inputs.end_requested {
            break;
        }
        if let Some(a) = inputs.new_frame_size {
            debug!(log, "Window changed size"; o!("x" => a.0, "y" => a.1));
            core::mem::drop(hal_state);
            hal_state = match HalState::new(&winit_state) {
                Ok(state) => state,
                Err(e) => panic!(e),
            };
        }
        local_state.update_from_input(inputs);
        if let Err(e) = do_the_render(&mut hal_state, &local_state) {
            error!(log, "render error"; "render_error" => e);
            debug!(log, "Auto-restarting HalState...");
            hal_state = match HalState::new(&winit_state) {
                Ok(state) => state,
                Err(e) => panic!(e),
            };
        }
    }
}
