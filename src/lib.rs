extern crate winit;
#[macro_use]
pub extern crate slog;
extern crate slog_stdlog;

pub mod graphics;
pub mod geometry;

pub use graphics::HalState;

use gfx_hal::Device;
use slog::Drain;
use winit::{EventsLoop, WindowBuilder, Window, dpi::LogicalSize, Event, DeviceEvent, KeyboardInput};
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

struct Renderer {}

pub trait EventHandler {
    fn handle_event(&mut self, RMEvent);
}

pub struct GraphicsHandle {}

#[derive(Debug)]
pub struct RMGfxContext<E: EventHandler> {
    graphics_state: RMGraphics,
    window: Window,
    handler: E,
    event_receiver: Receiver<Event>,
    control_handle: Sender<Signal>,
}

enum Signal {
    Start,
    Quit
}

impl<E> RMGfxContext<E> {
    pub fn new(event_handler: E, window_title: &str, dimensions: (f64, f64)) -> Self {
        let (win_tx, win_rx) = mpsc::channel();
        let (graphics_handle_tx, graphics_handle_rx) = mpsc::channel();
        let (start_tx, start_rx) = mpsc::sync_channel(1);
        let (event_tx, event_rx) = mpsc::channel(); 
        let window_title = window_title.to_string();
        
        let event_thread_handle = thread::spawn(move || {
            // take ownership of window_title and handle_tx
            let handler_tx = handle_tx;
            let window_title = window_title;

            // inner scope because we don't need win_tx after this and just wanna drop it
            let mut event_loop = {
                let win_tx = win_tx; // explicitly take ownership again
                
                let event_loop = EventLoop::new();
                let window = WindowBuilder::new()
                    .with_title(window_title)
                    .with_dimensions(LogicalSize {
                        height: dimensions.0,
                        width: dimensions.1,
                    })
                    .build(&event_loop);
                win_tx.send(window);
                event_loop
            };

            match start_rx.recv() {
                Ok(Signal::Start) =>
                    event_loop.run_forever(|event| {
                        let timestamp = Instant::now();
                        handle_tx.send((timestamp, event))
                    }),
                _ => return,
            }
        });
        
        let window = match win_rx.recv() {
            Ok(window) => window,
            e @ Err(_) => {
                start_handle.send(Signal::Stop);
                event_thread_handle.join();
                return e
            }
        }
        
    }

    /// Creates a renderer which isn't dependent on the window
    pub fn make_renderer(&self) -> Renderer {
        unimplemented!()
    }

    pub fn run_forever(&mut self) -> Result<()> {
        start_handle.send(Signal::Start);
        loop {
            match self.event_receiver.recv() {
                Ok(e) => self.event_handler.handle_event(RMEvent::from(e)),
                Err(e) => Err(e),
            }
        }
    }
}
