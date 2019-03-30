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

/// Quad of points stored as: Top left, bottom left, bottom right, top right
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Quad {
    pub top_left: Point2D,
    pub bottom_left: Point2D,
    pub bottom_right: Point2D,
    pub top_right: Point2D,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Triangle {
    points: [Point2D; 3],
}

impl Quad {
    /*pub fn vertex_attributes(self) -> [Vertex; 4] {
        let Quad {
        top_left,
        bottom_left,
        bottom_right,
        top_right,
    } = self;
        #[cfg_attr(rustfmt, rustfmt_skip)]
        [/*
        X               Y               R    G    B                  U    V                    */
    top_left.x,     top_left.y,     1.0, 0.0, 0.0, /* red     */ 0.0, 1.0, /* bottom left  */
    bottom_left.x,  bottom_left.y,  0.0, 1.0, 0.0, /* green   */ 0.0, 0.0, /* top left     */
    bottom_right.x, bottom_right.y, 0.0, 0.0, 1.0, /* blue    */ 1.0, 0.0, /* bottom right */
    top_right.x,    top_right.y,    1.0, 0.0, 1.0, /* magenta */ 1.0, 1.0, /* top right    */
    ]
    }*/
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Point2D {
    pub x: f32,
    pub y: f32,
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
        Point2D { x, y }
    }
}
