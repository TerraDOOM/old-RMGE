#[derive(Debug, Clone, Copy)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rectangle {
    /// Yeah this should probably be used at some point, will remove if it never gets used when the project is becoming more stable
    pub fn to_quad(self) -> Quad {
        let Rectangle { x, y, w, h } = self;
        Quad {
            top_left: Point2D { x: x, y: y + h },
            bottom_left: Point2D { x: x, y: y },
            bottom_right: Point2D { x: x + w, y: y },
            top_right: Point2D { x: x + w, y: y + h },
        }
    }
}

/// Quad of points
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Quad {
    pub top_left: Point2D,
    pub bottom_left: Point2D,
    pub bottom_right: Point2D,
    pub top_right: Point2D,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Point2D {
    pub x: f32,
    pub y: f32,
}
