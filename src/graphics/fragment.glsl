#version 450

layout(set = 0, binding = 0) uniform texture2D tex[64];
layout(set = 0, binding = 1) uniform sampler samp;

layout (location = 1) in vec2 frag_uv;
layout (location = 0) out vec4 color;
layout (location = 3) flat in uint tex_num;

void main()
{
  vec4 tex_color = texture(sampler2D(tex[tex_num], samp), frag_uv);
  color = tex_color; 
}
