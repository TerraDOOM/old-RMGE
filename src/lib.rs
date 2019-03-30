extern crate winit;
#[macro_use]
pub extern crate slog;
#[macro_use]
extern crate failure;
extern crate slog_stdlog;

/// NOTE: PLACEHOLDER FOR NOW
static CREATURE_BYTES: &[u8] = include_bytes!("creature-smol.png");

pub mod error;
mod eventhandler;
pub mod geometry;
pub mod graphics;

use crate::eventhandler::{Event, EventHandler, RMEventHandler};
use crate::graphics::HalState;

use crate::error::{ContextError, CreationError};

use failure::Error;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;
use winit::{dpi::LogicalSize, ControlFlow, EventsLoop, Window, WindowBuilder};

#[derive(Debug)]
struct RMGraphics {
    window: Window,
    renderer: HalState,
}

impl RMGraphics {
    fn new(window: Window) -> Self {
        unimplemented!()
    }
}

#[derive(Debug)]
pub struct RMGfxContext<E: EventHandler> {
    logger: slog::Logger,
    graphics_state: RMGraphics,
    event_handler: RMEventHandler<E>,
    event_rx: Receiver<(Instant, winit::Event)>,
    control_handle: Sender<Signal>,
}

#[derive(Debug, Copy, Clone)]
enum Signal {
    Start,
    Stop,
}

impl<E: EventHandler> RMGfxContext<E> {
    pub fn new(
        event_handler: E,
        window_title: &str,
        dimensions: (f64, f64),
        logger: slog::Logger,
    ) -> Result<Self, Error> {
        let (win_tx, win_rx) = mpsc::channel();
        let (start_tx, start_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let window_title = window_title.to_string(); // must create a string to move it into the event thread
        let event_logger = logger.new(o!());

        let event_thread_handle = thread::spawn(move || {
            // take ownership of window_title and handle_tx
            let event_tx = event_tx;
            let window_title = window_title;
            let logger = event_logger;

            // inner scope because we don't need win_tx after this and just wanna drop it
            // Explanation for the event_loop stuff:
            // the winit EventsLoop can't actually be moved across threads due to some OS specific stuff,
            // so if we want it in a different thread, it must be  created there. Since the event loop is
            // needed to create the window (which, unlike the event loop, is `Send`), we create the window
            // in the thread and send it back to the main thread.
            let mut event_loop = {
                let win_tx = win_tx; // explicitly take ownership again

                let event_loop = EventsLoop::new();
                let window = WindowBuilder::new()
                    .with_title(window_title)
                    .with_dimensions(LogicalSize {
                        height: dimensions.0,
                        width: dimensions.1,
                    })
                    .build(&event_loop);
                if let Err(_) = win_tx.send(window) {
                    return;
                };
                event_loop
            };

            info!(logger, "window created, waiting for start signal");

            match start_rx.recv() {
                Ok(Signal::Start) => event_loop.run_forever(|event| {
                    let timestamp = Instant::now();
                    if let Err(e) = event_tx.send((timestamp, event)) {
                        info!(logger, "error while sending data, stopping event loop and exiting thread"; "err" => %e);
                        ControlFlow::Break
                    } else {
                        ControlFlow::Continue
                    }
                }),
                _ => return,
            }
        });

        let window = match win_rx.recv().expect("this shouldn't happen") {
            Ok(window) => window,
            Err(err) => {
                let _ = start_tx.send(Signal::Stop);
                let _ = event_thread_handle.join();
                Err(CreationError::WindowCreationError { err })?
            }
        };

        let graphics_state = RMGraphics::new(window);

        Ok(RMGfxContext {
            logger,
            graphics_state,
            event_handler: RMEventHandler::new(event_handler),
            event_rx,
            control_handle: start_tx,
        })
    }

    pub fn run_forever(&mut self) -> Result<(), Error> {
        info!(self.logger, "started running");
        self.control_handle
            .send(Signal::Start)
            .map_err(|_| ContextError::StartChannelError)?;
        loop {
            let event = self
                .event_rx
                .recv()
                .map_err(|e| ContextError::EventChannelError { err: e })?;
            self.event_handler.handle_event(Event::from(event));
        }
    }
}
