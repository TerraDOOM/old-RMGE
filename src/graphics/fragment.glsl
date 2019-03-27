#version 450

layout(set = 0, binding = 0) uniform texture2D tex;
layout(set = 0, binding = 1) uniform sampler samp;

layout (location = 1) in vec3 frag_color;
layout (location = 2) in vec2 frag_uv;

layout (location = 0) out vec4 color;

void main()
{
  vec4 tex_color = texture(sampler2D(tex, samp), frag_uv);
  color = tex_color;
}
