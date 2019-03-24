extern crate winit;
#[macro_use]
pub extern crate slog;
extern crate slog_stdlog;

mod halstate;

pub use halstate::HalState;

use slog::Drain;
use winit::{EventsLoop, WindowBuilder, Window, dpi::LogicalSize, Event, DeviceEvent, KeyboardInput};
use std::sync::mpsc::{self, Sender};
use std::time::Instant;

#[derive(Debug)]
pub struct WindowState {
    window_name: String,
    events_loop: EventsLoop,
    window: Window,
    logger: slog::Logger,
}

impl WindowState {
    pub fn new(title: &str, size: (f64, f64), logger: Option<slog::Logger>) -> Result<WindowState, winit::CreationError> {
        let events_loop = EventsLoop::new();
        let window = WindowBuilder::new()
            .with_title(title)
            .with_dimensions(LogicalSize {
                width: size.0,
                height: size.1,
            })
            .build(&events_loop)?;
        Ok(WindowState {
            window_name: title.into(),
            events_loop,
            window,
            logger: logger.unwrap_or(slog::Logger::root(slog_stdlog::StdLog.fuse(), o!()))
        })
    }

    pub fn poll_event<F>(&mut self, mut f: F) where F: FnMut(Instant, Event) {
        self.events_loop.poll_events(move |event| {
            let now = Instant::now();
            f(now, event)
        })
    }

    pub fn get_window(&self) -> &Window {
        &self.window
    }

    pub fn get_events_loop_mut(&mut self) -> &mut EventsLoop {
        &mut self.events_loop
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Triangle {
    points: [Point2D; 3]
}

impl Triangle {
    pub fn points_flat(self) -> [f32; 6] {
        let [[a, b], [c, d], [e, f]]: [[f32; 2]; 3] = self.into();
        [a, b, c, d, e, f]
    }

    pub fn vertex_attributes(self) -> [f32; 3 * (2 + 3)] {
        let [[a, b], [c, d], [e, f]]: [[f32; 2]; 3] = self.into();
        [
            a, b, 1.0, 0.0, 0.0,
            c, d, 0.0, 1.0, 0.0,
            e, f, 0.0, 0.0, 1.0,
        ]
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Point2D {
    x: f32,
    y: f32,
}

impl Into<[f32; 2]> for Point2D {
    #[inline]
    fn into(self) -> [f32; 2] {
        [self.x, self.y]
    }
}

impl From<[f32; 2]> for Point2D {
    #[inline]
    fn from(arr: [f32; 2]) -> Point2D {
        let [x, y] = arr;
        Point2D {
            x, y
        }
    }
}

impl Into<[[f32; 2]; 3]> for Triangle {
    #[inline]
    fn into(self) -> [[f32; 2]; 3] {
        let [a, b, c] = self.points;
        [a.into(), b.into(), c.into()]
    }
}

impl From<[[f32; 2]; 3]> for Triangle {
    #[inline]
    fn from(arr: [[f32; 2]; 3]) -> Triangle {
        let [a, b, c] = arr;
        Triangle { points: [a.into(), b.into(), c.into()] }
    }
}

