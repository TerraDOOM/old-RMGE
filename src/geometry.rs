#[derive(Debug, Clone, Copy)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rectangle {
    fn to_quad(self) -> Quad {
        let Rectangle { x, y, w, h } = self;
        Quad {
            top_left: Point2D {
                x: x,
                    y: y+h,
                },
            bottom_left: Point2D {
                x: x,
                y: y,
            },
            bottom_right: Point2D {
                x: x+w,
                y: y,
            },
            top_right: Point2D {
                x: x+w,
                y: y+h,
            },
        }
    }
}

/// Quad of points stored as: Top left, bottom left, bottom right, top right
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Quad {
    top_left: Point2D,
    bottom_left  : Point2D,
    bottom_right : Point2D,
    top_right    : Point2D,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Triangle {
    points: [Point2D; 3]
}

impl Quad {
    pub fn vertex_attributes(self) -> [f32; 4 * (2 + 3 + 2)] {
        let Quad { top_left, bottom_left, bottom_right, top_right } = self;
        #[cfg_attr(rustfmt, rustfmt_skip)]
        [/*
         X               Y               R    G    B                  U    V                    */
         top_left.x,     top_left.y,     1.0, 0.0, 0.0, /* red     */ 0.0, 1.0, /* bottom left  */
         bottom_left.x,  bottom_left.y,  0.0, 1.0, 0.0, /* green   */ 0.0, 0.0, /* top left     */
         bottom_right.x, bottom_right.y, 0.0, 0.0, 1.0, /* blue    */ 1.0, 0.0, /* bottom right */
         top_right.x,    top_right.y,    1.0, 0.0, 1.0, /* magenta */ 1.0, 1.0, /* top right    */
        ]
    }
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
