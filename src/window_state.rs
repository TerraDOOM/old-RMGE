extern crate winit;
#[macro_use]
pub extern crate slog;
extern crate slog_stdlog;

pub use graphics::HalState;

use gfx_hal::Device;
use slog::Drain;
use winit::{EventsLoop, WindowBuilder, Window, dpi::LogicalSize, Event, DeviceEvent, KeyboardInput};
use std::sync::mpsc::{self, Sender};
use std::time::Instant;
use std::marker::PhantomData;

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
