use std::time::Instant;
use winit::ButtonId;

impl From<(Instant, winit::Event)> for Event {
    fn from(arg: (Instant, winit::Event)) -> Event {
        let (time, event) = arg;
        Event {
            time,
            event: RMEvent::from(event),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Event {
    pub time: Instant,
    pub event: RMEvent,
}

#[derive(Debug, Copy, Clone)]
pub enum RMEvent {}

impl From<winit::Event> for RMEvent {
    fn from(_event: winit::Event) -> RMEvent {
        unimplemented!()
    }
}

/// Internal struct for mapping WindowEvents to easier-to-handle game events (I think for now at least)
#[derive(Debug, Clone)]
pub struct RMEventHandler<E: EventHandler> {
    event_handler: E,
    button_map: ButtonMap,
}

impl<E: EventHandler> RMEventHandler<E> {
    pub fn new(_e: E) -> Self {
        unimplemented!()
    }

    pub fn handle_event(&mut self, event: Event) {
        // do some processing on the event...
        self.event_handler.handle_event(event)
    }
}

#[derive(Debug, Copy, Clone)]
enum VirtualKeyCode {}

#[derive(Debug, Clone)]
struct ButtonMap {
    map: Vec<Option<VirtualKeyCode>>,
}

pub trait EventHandler {
    fn handle_event(&mut self, event: Event);
}
