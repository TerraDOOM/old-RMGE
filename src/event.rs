use std::time::Instant;
use winit::{ButtonId, DeviceId, MouseScrollDelta, ScanCode, VirtualKeyCode};

pub trait EventHandler {
    fn draw(&mut self);
    fn update(&mut self);
    fn key_down(&mut self, _time: Instant, _key: Key) {}
    fn key_up(&mut self, _time: Instant, _key: Key) {}
    fn device_button_down(&mut self, _time: Instant, _button: DeviceButton) {}
    fn device_button_up(&mut self, _time: Instant, _button: DeviceButton) {}
    fn mouse_move(&mut self, _time: Instant, _motion: MouseMove) {}
    fn mouse_wheel(&mut self, _time: Instant, _scroll: MouseScrollDelta) {}
    /// This function is run whenever the user changes focus. The return value is whether to suspend the event loop while unfocused.
    /// Default is to suspend the eventloop
    fn window_focused(&mut self, time: Instant, focused: bool) -> bool {
        !focused
    }
    fn quit(&mut self) -> bool {
        true
    }
}

pub struct MouseMove {/* no fields yet */}

pub struct DeviceButton {
    pub device: DeviceId,
    pub button: ButtonId,
}

pub struct Key {
    pub device: DeviceId,
    pub scancode: ScanCode,
    pub virtual_keycode: Option<VirtualKeyCode>,
    pub modifiers: KeyModifiers,
}

pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
}
