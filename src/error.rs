#[derive(Debug, Fail)]
pub enum ContextError {
    #[fail(display = "Event channel closed, event thread might have been terminated ({:?})", err)]
    EventChannelError {
        err: std::sync::mpsc::RecvError,
    },
    #[fail(display = "Start signal channel closed, event thread might have been terminated")]
    StartChannelError
}

#[derive(Debug, Fail)]
pub enum CreationError {
    #[fail(display = "Failed creating the window: {}", err)]
    WindowCreationError {
        err: winit::CreationError,
    }
}
