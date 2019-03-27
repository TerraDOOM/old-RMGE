#version 450
layout (set = 0, binding = 0) uniform texture2D tex;
layout (set = 0, binding = 1) uniform sampler samp;
layout (push_constant) uniform PushConsts {
  vec4 uv_rect;
} push;

layout (location = 0) in vec2 position;
layout (location = 1) in vec3 color;
layout (location = 2) in vec2 vert_uv;
layout (location = 3) in vec4 uv_rect;

layout (location = 0) out gl_PerVertex {
  vec4 gl_Position;
};

layout (location = 1) out vec3 frag_color;
layout (location = 2) out vec2 frag_uv;

void main()
{
  vec2 tex_size = textureSize(sampler2D(tex, samp), 0);
  gl_Position = vec4(position, 0.0, 1.0);
  frag_color = color;

  vec4 uv_rect = uv_rect;

  if (uv_rect == vec4(0.0, 0.0, 0.0, 0.0)) {
    uv_rect = vec4(0.0, 0.0, float(tex_size.x), float(tex_size.y));
  }

  vec2 x_scale = vec2(uv_rect.x, uv_rect.z) / float(tex_size.x);
  vec2 y_scale = vec2(uv_rect.y, uv_rect.w) / float(tex_size.y);

  frag_uv = vec2(x_scale.x + vert_uv.x*(x_scale.y - x_scale.x), vert_uv.y*(y_scale.y - y_scale.x) + y_scale.x);
}
