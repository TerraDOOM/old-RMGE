pub use vek::geom::repr_simd::Rect;
pub use vek::mat::repr_simd::column_major::mat2::Mat2;
pub use vek::mat::repr_simd::column_major::mat3::Mat3;
pub use vek::vec::repr_c::vec4::Vec4 as CVec4;
pub use vek::vec::repr_simd::vec2::Vec2;
pub use vek::vec::repr_simd::vec3::Vec3;

impl From<Rect<f32, f32>> for Quad {
    /// Yeah this should probably be used at some point, will remove if it never gets used when the project is becoming more stable
    fn from(rect: Rect<f32, f32>) -> Quad {
        let Rect { x, y, w, h } = rect;
        Quad {
            top_left: Vec2 { x: x, y: y + h },
            bottom_left: Vec2 { x: x, y: y },
            bottom_right: Vec2 { x: x + w, y: y },
            top_right: Vec2 { x: x + w, y: y + h },
        }
    }
}

/// Quad of points
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Quad {
    pub top_left: Vec2<f32>,
    pub bottom_left: Vec2<f32>,
    pub bottom_right: Vec2<f32>,
    pub top_right: Vec2<f32>,
}

impl Into<CVec4<Vec3<f32>>> for Quad {
    fn into(self) -> CVec4<Vec3<f32>> {
        let Quad {
            top_left,
            bottom_left,
            bottom_right,
            top_right,
        } = self;
        CVec4::new(top_left, bottom_left, bottom_right, top_right).map(Vec3::from_point_2d)
    }
}

impl From<CVec4<Vec3<f32>>> for Quad {
    fn from(v: CVec4<Vec3<f32>>) -> Quad {
        let [top_left, bottom_left, bottom_right, top_right] = v.map(conv_homogeneous).into_array();
        Quad {
            top_left,
            bottom_left,
            bottom_right,
            top_right,
        }
    }
}

fn conv_homogeneous(v: Vec3<f32>) -> Vec2<f32> {
    Vec2::new(v.x, v.y) / v.z
}

impl Quad {
    pub fn transform(self, rhs: Mat3<f32>) -> Quad {
        Quad::from(<Self as Into<CVec4<Vec3<f32>>>>::into(self).map(|v| rhs * v))
    }

    pub fn rotate_around_center_matrix(&self, degrees: f64) -> Mat3<f32> {
        let center_point = ((self.top_left + self.bottom_right) / 2.0
            + (self.bottom_left + self.top_right) / 2.0)
            / 2.0;
        let t_1: Mat3<f32> = Mat3::identity().translated_2d(center_point);
        let t_2: Mat3<f32> = Mat3::identity().translated_2d(-center_point);
        let r = Mat2::rotation_z((degrees / 360.0 * (std::f64::consts::PI * 2.0)) as f32);
        t_1 * Mat3::from(r) * t_2
    }

    pub fn rotate_180_around_center(self) -> Quad {
        let center_point = ((self.top_left + self.bottom_right) / 2.0
            + (self.bottom_left + self.top_right) / 2.0)
            / 2.0;
        let t_1: Mat3<f32> = Mat3::identity().translated_2d(center_point);
        let t_2: Mat3<f32> = Mat3::identity().translated_2d(-center_point);
        let r = Mat3::with_diagonal(Vec3::new(-1.0, -1.0, 1.0));
        self.transform(t_1 * r * t_2)
    }

    pub fn invert_y(self) -> Quad {
        self.transform(Mat3::with_diagonal(Vec3::new(1.0, -1.0, 1.0)))
    }
}
