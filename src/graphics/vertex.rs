use std::mem;
use gfx_hal::{
    pso::{AttributeDesc, Element, ElemOffset},
    format::Format,
};

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Vertex {
    pub xy: [f32; 2],
    pub rgb: [f32; 3],
    pub uv: [f32; 2],
    pub uv_rect: [f32; 4],
}
impl Vertex {
    pub fn attributes() -> Vec<AttributeDesc> {
        const POSITION_ATTR_SIZE: usize = mem::size_of::<f32>() * 2;
        const COLOR_ATTR_SIZE: usize = mem::size_of::<f32>() * 3;
        const UV_ATTR_SIZE: usize = mem::size_of::<f32>() * 2;

        let position_attribute = AttributeDesc {
            location: 0,
            binding: 0,
            element: Element {
                format: Format::Rg32Float,
                offset: 0,
            },
        };
        let color_attribute = AttributeDesc {
            location: 1,
            binding: 0,
            element: Element {
                format: Format::Rgb32Float,
                offset: POSITION_ATTR_SIZE as ElemOffset,
            },
        };
        let uv_attribute = AttributeDesc {
            location: 2,
            binding: 0,
            element: Element {
                format: Format::Rg32Float,
                offset: (POSITION_ATTR_SIZE + COLOR_ATTR_SIZE) as ElemOffset,
            },
        };
        let uv_rect_attribute = AttributeDesc {
            location: 3,
            binding: 0,
            element: Element {
                format: Format::Rgba32Float,
                offset: (POSITION_ATTR_SIZE + COLOR_ATTR_SIZE + UV_ATTR_SIZE) as ElemOffset,
            }
        };
        vec![position_attribute, color_attribute, uv_attribute, uv_rect_attribute]
    }
    pub fn to_array(self) -> [f32; 2 + 3 + 2] {
        let [x, y] = self.xy;
        let [r, g, b] = self.rgb;
        let [u, v] = self.uv;
        //let [ur_x, ur_y, ur_z, ur_w] = self.uv_rect;
        [x, y, r, g, b, u, v] //ur_x, ur_y, ur_z, ur_w ]
    }
}
