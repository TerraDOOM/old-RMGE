#version 450
layout (set = 0, binding = 0) uniform texture2D tex[64];
layout (set = 0, binding = 1) uniform sampler samp;

layout (location = 0) in vec2 position;
layout (location = 1) in vec2 vert_uv;
layout (location = 2) in vec4 uv_rect;
layout (location = 3) in uint tex_num;

layout (location = 0) out gl_PerVertex {
  vec4 gl_Position;
};

layout (location = 0) out vec3 frag_color;
layout (location = 1) out vec2 frag_uv;
layout (location = 3) flat out uint v_tex_num;

void main()
{
  vec2 tex_size = textureSize(sampler2D(tex[tex_num], samp), 0);
  gl_Position = vec4(position, 0.0, 1.0);

  vec2 x_scale = uv_rect.xz / float(tex_size.x);
  vec2 y_scale = uv_rect.yw / float(tex_size.y);

  v_tex_num = tex_num;
  
  frag_uv = vec2(x_scale.x + vert_uv.x*(x_scale.y - x_scale.x), vert_uv.y*(y_scale.y - y_scale.x) + y_scale.x);
}
